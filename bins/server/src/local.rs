//! The server the app runs inside itself for a local account
//! (docs/plan/39-first-run.md §1.1, D460).
//!
//! The same router as the server binary ([`crate::app`]), over a SQLite
//! store and a key file under `<data>/local-server/`, driven as a tower
//! service: each request is one `oneshot` on a small runtime this module
//! owns. There is no listening socket, no port and no TLS, so nothing on
//! the network or on this computer can reach it but the app and the
//! command line that installed it. The person never sees or configures it.
//!
//! The desktop app and the command line share the data directory (D86),
//! so both may run this server over one store: every request holds an
//! exclusive lock on `<data>/local-server/lock` for its duration, and the
//! store keeps no state in memory between requests that another process
//! could leave stale.
//!
//! Hosts call [`install`] once at start, before the core opens anything;
//! `lockdown-sync` then routes [`lockdown_sync::LOCAL_BASE`] here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use lockdown_sync::{HttpRequest, Method};
use tower::ServiceExt;

use crate::keys::ServerKeys;
use crate::storage::SqlStorage;
use crate::{AppState, Config};

/// The folder under the data directory that holds the local server.
pub const DIR: &str = "local-server";
const DB_FILE: &str = "server.sqlite";
const KEY_FILE: &str = "server-key.cbor";
const LOCK_FILE: &str = "lock";
/// The largest answer handed back (the largest attachment and some room).
const MAX_ANSWER: usize = (crate::DEFAULT_MAX_BLOB_BYTES as usize) + (1 << 20);

/// The local server's settings: signups open (one account in practice),
/// the same session limits as any server (D401) with a long idle window,
/// no brakes (there is no address to brake), attachments without a quota,
/// and nothing else.
pub fn config() -> Config {
    Config {
        session_ttl: Duration::from_secs(8 * 60 * 60),
        // Renewal by the device's key keeps working for a year without
        // use; the master password, not this, is what opens the vault.
        session_idle: Duration::from_secs(365 * 24 * 60 * 60),
        signups_open: true,
        per_address: (u32::MAX, 1_000_000.0),
        per_login: (u32::MAX, 1_000_000.0),
        max_blob_bytes: crate::DEFAULT_MAX_BLOB_BYTES,
        blob_grace: crate::DEFAULT_BLOB_GRACE,
        attachment_quota: None,
        trusted_proxies: Vec::new(),
        web_vault: None,
    }
}

/// One data directory's server.
struct LocalServer {
    router: Router,
    runtime: tokio::runtime::Runtime,
    lock: PathBuf,
}

static SERVERS: Mutex<BTreeMap<PathBuf, Arc<LocalServer>>> = Mutex::new(BTreeMap::new());

/// Where a data directory's local server keeps its files.
pub fn dir(data: &Path) -> PathBuf {
    data.join(DIR)
}

impl LocalServer {
    fn open(data: &Path) -> Result<Self, String> {
        let dir = dir(data);
        crate::keys::create_private_dir(&dir)
            .map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
        let lock = dir.join(LOCK_FILE);
        let guard = Self::take_lock(&lock)?;
        let storage = SqlStorage::open(&dir.join(DB_FILE)).map_err(|e| e.to_string())?;
        let keys = ServerKeys::load_or_create(&dir.join(KEY_FILE)).map_err(|e| e.to_string())?;
        drop(guard);
        let state = Arc::new(AppState::new(Box::new(storage), keys, config()));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            router: crate::app(state),
            runtime,
            lock,
        })
    }

    fn take_lock(path: &Path) -> Result<std::fs::File, String> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|e| format!("couldn't open {}: {e}", path.display()))?;
        file.lock()
            .map_err(|e| format!("couldn't lock {}: {e}", path.display()))?;
        Ok(file)
    }

    fn handle(&self, req: &HttpRequest<'_>) -> Result<(u16, Vec<u8>), String> {
        let path = req
            .url
            .strip_prefix(lockdown_sync::LOCAL_BASE)
            .filter(|p| p.starts_with('/'))
            .ok_or("that address isn't the vault on this computer")?;
        let method = match req.method {
            Method::Get => axum::http::Method::GET,
            Method::Put => axum::http::Method::PUT,
            Method::Post => axum::http::Method::POST,
            Method::Head => axum::http::Method::HEAD,
            Method::Delete => axum::http::Method::DELETE,
        };
        let mut builder = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header(axum::http::header::CONTENT_TYPE, req.content_type)
            .header(
                axum::http::header::USER_AGENT,
                lockdown_sync::transport::user_agent(),
            );
        if let Some(auth) = req.authorization {
            builder = builder.header(axum::http::header::AUTHORIZATION, auth);
        }
        let request = builder
            .body(Body::from(req.body.map(<[u8]>::to_vec).unwrap_or_default()))
            .map_err(|e| e.to_string())?;
        let _guard = Self::take_lock(&self.lock)?;
        let router = self.router.clone();
        self.runtime.block_on(async move {
            let response = router.oneshot(request).await.map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            let bytes = axum::body::to_bytes(response.into_body(), MAX_ANSWER)
                .await
                .map_err(|e| e.to_string())?;
            Ok((status, bytes.to_vec()))
        })
    }
}

fn server_for(data: &Path) -> Result<Arc<LocalServer>, String> {
    let mut servers = SERVERS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(s) = servers.get(data) {
        return Ok(s.clone());
    }
    let server = Arc::new(LocalServer::open(data)?);
    servers.insert(data.to_path_buf(), server.clone());
    Ok(server)
}

/// Answers one request for the local server of the data directory `data`,
/// opening it the first time.
pub fn handle(data: &Path, req: &HttpRequest<'_>) -> Result<(u16, Vec<u8>), String> {
    server_for(data)?.handle(req)
}

/// Forgets the open server of `data` (after its folder was removed, by a
/// move to a real server or a reset), so the next request opens it again.
pub fn close(data: &Path) {
    let mut servers = SERVERS.lock().unwrap_or_else(|p| p.into_inner());
    servers.remove(data);
}

/// Installs this module as the process's local server. Returns `false`
/// when one was installed already.
pub fn install() -> bool {
    lockdown_sync::install_local_server(Box::new(handle))
}
