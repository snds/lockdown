//! `lockdown-server`: the server (03-architecture §5; 05-self-host.md; D7).
//!
//! One axum binary, the same code for cloud and self-host. It runs the
//! OPAQUE server half for account login, hands out short-lived signed
//! session tokens, keeps each account's identity record set as a chain of
//! signed identity manifests (and refuses writes from devices that chain
//! revoked), and per vault orders item pushes by revision and accepts
//! manifest heads only when they extend the chain by one, name the
//! current head, and are signed by a device the account logged in with. It never holds a vault key, an
//! item key, a password, a plain title or URL, or a device signing key;
//! it orders writes and keeps receipts.
//!
//! - [`storage`] — what is kept, behind a trait; SQLite today
//! - [`keys`] — the server key file (token key, OPAQUE setup), made on first run
//! - [`token`] — the `lk1` session token
//! - [`ratelimit`] — the brake on the login endpoints
//! - [`api`] — the handlers; the wire types are `lockdown_sync::wire`
//!
//! The key-transparency log, checkpoints, blobs and push arrive in order.

pub mod api;
pub mod keys;
pub mod ratelimit;
pub mod storage;
pub mod token;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use lockdown_crypto::auth::{LoginServerState, OpaqueServer};
use lockdown_vault::Id;
use serde::Serialize;

use crate::keys::ServerKeys;
use crate::ratelimit::RateLimiter;
use crate::storage::{Account, Storage};
use crate::token::TokenSigner;

/// What `/.well-known/lockdown` says about this install.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// The discovery document version.
    pub v: u16,
    /// The API versions this server speaks.
    pub api: Vec<u16>,
    /// Features this install offers. Differ by configuration, never by fork (D7).
    pub features: Features,
}

/// Feature flags a client reads before relying on a server.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Features {
    /// OPAQUE account login (04-security §3).
    pub opaque: bool,
    /// Whether new accounts may register.
    pub signups: bool,
    /// Key-transparency log with published checkpoints (04-security §6).
    pub key_transparency: bool,
    /// Manifest-head anti-rollback checkpoints (02-model §8).
    pub checkpoints: bool,
    /// Attachment blobs.
    pub blobs: bool,
    /// Content-free push pings.
    pub push: bool,
}

/// The discovery document this build serves.
pub fn discovery(signups: bool) -> Discovery {
    Discovery {
        v: 1,
        api: vec![lockdown_sync::wire::API_V],
        features: Features {
            opaque: true,
            signups,
            key_transparency: false,
            checkpoints: false,
            blobs: true,
            push: false,
        },
    }
}

/// How the server is set up. Everything here comes from flags or the
/// environment (05-self-host §1).
#[derive(Clone, Debug)]
pub struct Config {
    /// How long a session token lives.
    pub session_ttl: Duration,
    /// Whether new accounts may register.
    pub signups_open: bool,
    /// Login attempts allowed at once per client address, then per second.
    pub per_address: (u32, f64),
    /// Login attempts allowed at once per login name, then per second.
    pub per_login: (u32, f64),
    /// The largest attachment blob accepted, in bytes.
    pub max_blob_bytes: u64,
    /// How long a blob nothing names is kept before it is collected.
    pub blob_grace: Duration,
}

/// The default blob cap: 64 MiB.
pub const DEFAULT_MAX_BLOB_BYTES: u64 = 64 << 20;

/// The default grace before an unnamed blob is collected: a day.
pub const DEFAULT_BLOB_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

impl Default for Config {
    fn default() -> Self {
        Self {
            session_ttl: Duration::from_secs(8 * 60 * 60),
            signups_open: true,
            per_address: (30, 0.5),
            per_login: (10, 1.0 / 6.0),
            max_blob_bytes: DEFAULT_MAX_BLOB_BYTES,
            blob_grace: DEFAULT_BLOB_GRACE,
        }
    }
}

/// A login between its two round trips.
pub struct PendingLogin {
    /// The login name the attempt is for.
    pub login: String,
    /// The account, when the login names one (a decoy has none).
    pub account: Option<Account>,
    /// The OPAQUE server state.
    pub state: LoginServerState,
    /// When it started; dropped after [`PENDING_TTL`].
    pub started: Instant,
}

/// How long the server waits for the second round trip of a login.
pub const PENDING_TTL: Duration = Duration::from_secs(120);

/// The most logins kept between round trips.
pub const PENDING_MAX: usize = 10_000;

/// Everything the handlers share.
pub struct AppState {
    /// The store.
    pub storage: Box<dyn Storage>,
    /// The OPAQUE server half.
    pub opaque: OpaqueServer,
    /// Issues and checks session tokens.
    pub tokens: TokenSigner,
    /// The settings.
    pub config: Config,
    /// Logins between their round trips.
    pub pending: Mutex<HashMap<Id, PendingLogin>>,
    /// The brake per client address.
    pub per_address: RateLimiter,
    /// The brake per login name.
    pub per_login: RateLimiter,
    /// The clock (seconds since the Unix epoch), so tests can move time.
    pub clock: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// A state over `storage` with `keys`.
    pub fn new(storage: Box<dyn Storage>, keys: ServerKeys, config: Config) -> Self {
        Self {
            storage,
            opaque: keys.opaque,
            tokens: TokenSigner::new(keys.token_key),
            per_address: RateLimiter::new(config.per_address.0, config.per_address.1),
            per_login: RateLimiter::new(config.per_login.0, config.per_login.1),
            config,
            pending: Mutex::new(HashMap::new()),
            clock: Box::new(|| lockdown_vault::Timestamp::now().0),
        }
    }

    /// A state in memory with fresh keys: tests, and a server with no disk.
    pub fn in_memory(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::new(
            Box::new(storage::SqliteStorage::open_in_memory()?),
            ServerKeys::generate()?,
            config,
        ))
    }

    /// The time now, by this state's clock.
    pub fn now(&self) -> i64 {
        (self.clock)()
    }
}

/// The router over `state`. Tests drive it without a socket.
pub fn app(state: Arc<AppState>) -> Router {
    let blob_limit = usize::try_from(state.config.max_blob_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    Router::new()
        .route(
            "/v1/vaults/{vault}/blobs/{hash}",
            get(api::get_blob)
                .put(api::put_blob)
                .layer(DefaultBodyLimit::max(blob_limit)),
        )
        .route("/healthz", get(api::healthz))
        .route("/.well-known/lockdown", get(api::well_known))
        .route("/v1/account/register/start", post(api::register_start))
        .route("/v1/account/register/finish", post(api::register_finish))
        .route("/v1/account/login/start", post(api::login_start))
        .route("/v1/account/login/finish", post(api::login_finish))
        .route(
            "/v1/identity",
            get(api::identity_since).put(api::put_identity),
        )
        .route("/v1/vaults", get(api::list_vaults))
        .route("/v1/vaults/{vault}", put(api::put_vault))
        .route(
            "/v1/vaults/{vault}/heads",
            get(api::heads_since).put(api::put_head),
        )
        .route("/v1/vaults/{vault}/items", get(api::items_since))
        .route(
            "/v1/vaults/{vault}/items/{item}",
            get(api::get_item).put(api::push_item),
        )
        .with_state(state)
}

/// Serves `app(state)` on `listener` until `shutdown` resolves.
pub async fn serve(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

/// A server running on its own thread, for tests and tools that drive it
/// over a real socket. Dropping it stops the server.
pub struct RunningServer {
    /// Where it listens.
    pub addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for RunningServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RunningServer({})", self.addr)
    }
}

impl RunningServer {
    /// The base URL, `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts the server on a free local port, on a thread of its own.
pub fn run_in_background(state: Arc<AppState>) -> std::io::Result<RunningServer> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    std_listener.set_nonblocking(true)?;
    let addr = std_listener.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::io::Result<()>>();
    let thread = std::thread::Builder::new()
        .name("lockdown-server".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(std_listener) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let _ = serve(listener, state, async {
                    let _ = rx.await;
                })
                .await;
            });
        })?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(RunningServer {
            addr,
            shutdown: Some(tx),
            thread: Some(thread),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(std::io::Error::other(
            "the server thread stopped before it listened",
        )),
    }
}
