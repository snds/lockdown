//! The desktop's endpoint for the browser extension
//! (docs/plan/16-browser-extension.md). The browser starts
//! `lockdown-native-host`; the host connects to the socket this module
//! listens on; each side checks the other's process identity (D4, D33);
//! the extension proves its pairing; then the core answers.
//!
//! - [`pairing`]: pairing a browser with a code the person compares, and
//!   the per-connection proof.
//! - [`handler`]: one request, answered.
//! - [`serve`]: one connection, from the host's hello to its close.
//! - [`Endpoint::start`] (Unix): the socket, the endpoint file, the accept
//!   loop.
//!
//! Nothing here needs Tauri: the shell gives it the [`App`] and a
//! [`CopySink`] for its clipboard path, and its commands call
//! [`Endpoint`]'s methods.

pub mod handler;
pub mod pairing;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ecosystem_identity::handshake::{PeerChannel, PeerVerifier};
use lockdown_app::{App, ApprovalKind, GateInput, Revealed};
use lockdown_native_messaging::caller::HostHello;
use lockdown_native_messaging::manifest::{self, Browser, Homes, Os, Target};
use serde::Serialize;
use serde_json::json;
use zeroize::{Zeroize, Zeroizing};

use handler::{Conn, Refusal, error_frame, handle};
use pairing::{PairedView, Pairings, PendingView, random};

/// The native host's file name beside the app's executable.
pub const HOST_BINARY: &str = if cfg!(windows) {
    "lockdown-native-host.exe"
} else {
    "lockdown-native-host"
};

/// How long a save offer waits for the person's answer.
pub const OFFER_FOR: Duration = Duration::from_secs(180);

/// How many save offers one browser may hold at once.
pub const MAX_OFFERS: usize = 8;

/// The shell's clipboard path: write the value, start the clear timer,
/// answer the seconds until it clears. The desktop's `copy_secret` rule
/// (D85, D119) is what stands behind it.
pub trait CopySink: Send + Sync {
    fn copy(&self, value: Revealed) -> Result<u64, String>;
}

/// A sign-in the person may save, held in memory until they answer.
pub struct Offer {
    client: String,
    pub page: String,
    pub username: String,
    pub password: Zeroizing<Vec<u8>>,
    pub update: Option<String>,
    created: Instant,
}

/// The save offers waiting.
#[derive(Default)]
pub struct Offers {
    offers: HashMap<String, Offer>,
}

impl Offers {
    fn expire(&mut self) {
        self.offers.retain(|_, o| o.created.elapsed() < OFFER_FOR);
    }

    /// Holds a new offer; answers its id.
    pub fn put(
        &mut self,
        client: String,
        page: String,
        username: String,
        password: Zeroizing<Vec<u8>>,
        update: Option<String>,
    ) -> Result<String, String> {
        self.expire();
        if self.offers.values().filter(|o| o.client == client).count() >= MAX_OFFERS {
            return Err("too many sign-ins are waiting to be saved".into());
        }
        let mut id = [0u8; 16];
        random(&mut id)?;
        let id = data_encoding::HEXLOWER.encode(&id);
        self.offers.insert(
            id.clone(),
            Offer {
                client,
                page,
                username,
                password,
                update,
                created: Instant::now(),
            },
        );
        Ok(id)
    }

    /// Takes the offer `id` made by `client`, if it is still waiting.
    pub fn take(&mut self, client: &str, id: &str) -> Option<Offer> {
        self.expire();
        match self.offers.get(id) {
            Some(o) if o.client == client => self.offers.remove(id),
            _ => None,
        }
    }

    /// Drops the offer `id`.
    pub fn remove(&mut self, id: &str) {
        self.offers.remove(id);
    }
}

/// What every connection shares.
pub struct Shared {
    pub app: App,
    pub copy: Arc<dyn CopySink>,
    pairings: Mutex<Pairings>,
    offers: Mutex<Offers>,
}

impl Shared {
    /// A shared state over `app`, with the pairings kept in `data`.
    pub fn new(app: App, copy: Arc<dyn CopySink>, data: &Path) -> Result<Self, String> {
        Ok(Self {
            app,
            copy,
            pairings: Mutex::new(Pairings::load(data)?),
            offers: Mutex::new(Offers::default()),
        })
    }

    /// The pairings, locked for one use.
    pub fn pairings(&self) -> MutexGuard<'_, Pairings> {
        self.pairings.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The save offers, locked for one use.
    pub fn offers(&self) -> MutexGuard<'_, Offers> {
        self.offers.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// How long a connection may sit idle between requests before the desktop
/// closes it (the extension reconnects on its next request).
pub const IDLE_CLOSE: Duration = Duration::from_secs(15 * 60);

/// Serves one connection: checks the peer is the installed native host,
/// reads its hello, checks the extension is one Lockdown answers, sends a
/// challenge, then answers requests until the peer goes away.
pub fn serve<C: PeerChannel>(shared: &Shared, verifier: &dyn PeerVerifier, mut ch: C) {
    let refuse = |ch: &mut C, message: &str| {
        let body = json!({ "event": "refused", "message": message }).to_string();
        let _ = ch.send(body.as_bytes());
    };
    if let Err(e) = verifier.verify(&ch.peer()) {
        refuse(
            &mut ch,
            &format!("the program that connected isn't Lockdown's browser host ({e})"),
        );
        return;
    }
    let hello: HostHello = match ch
        .recv(Duration::from_secs(10))
        .ok()
        .and_then(|f| serde_json::from_slice(&f).ok())
    {
        Some(h) => h,
        None => {
            refuse(&mut ch, "the browser host didn't say hello");
            return;
        }
    };
    if hello.v != lockdown_native_messaging::caller::HELLO_V {
        refuse(
            &mut ch,
            "this browser host is a different version from the app",
        );
        return;
    }
    if !hello.caller.is_allowed() {
        refuse(&mut ch, "this extension isn't one Lockdown answers");
        return;
    }
    let mut challenge = [0u8; 32];
    if random(&mut challenge).is_err() {
        refuse(&mut ch, "the system's random source failed");
        return;
    }
    let ready = json!({
        "event": "ready",
        "v": 1,
        "challenge": data_encoding::HEXLOWER.encode(&challenge),
        "app": format!("Lockdown {}", env!("CARGO_PKG_VERSION")),
    });
    if ch.send(ready.to_string().as_bytes()).is_err() {
        return;
    }
    let mut conn = Conn {
        caller: hello.caller,
        challenge,
        client: None,
    };
    loop {
        let mut frame = match ch.recv(IDLE_CLOSE) {
            Ok(f) => f,
            Err(_) => return,
        };
        let answer = if frame.len() > ecosystem_identity::handshake::wire::MAX_FRAME {
            error_frame(
                &serde_json::Value::Null,
                &Refusal {
                    code: "too_large",
                    message: "that message is larger than Lockdown accepts".into(),
                },
            )
        } else {
            handle(shared, &mut conn, &frame)
        };
        frame.zeroize();
        let answer = if answer.len() > ecosystem_identity::handshake::wire::MAX_FRAME {
            let id = serde_json::from_slice::<serde_json::Value>(&answer)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or_default();
            error_frame(
                &id,
                &Refusal {
                    code: "too_large",
                    message: "the answer is larger than the channel carries; narrow the search"
                        .into(),
                },
            )
        } else {
            answer
        };
        if ch.send(&answer).is_err() {
            return;
        }
    }
}

/// What the endpoint is started with.
#[derive(Clone, Debug)]
pub struct EndpointConfig {
    /// Lockdown's data directory (D47).
    pub data: PathBuf,
    /// The installed native host; the only program allowed to connect.
    pub host_binary: PathBuf,
    /// This app's executable, named in the endpoint file for the host.
    pub app_exe: PathBuf,
}

impl EndpointConfig {
    /// The default for a running app: the host sits beside this
    /// executable, unless `LOCKDOWN_NATIVE_HOST` names another file.
    pub fn for_app(data: PathBuf) -> Result<Self, String> {
        let app_exe = std::env::current_exe()
            .map_err(|e| format!("couldn't find the app's own file: {e}"))?;
        let host_binary = match std::env::var_os("LOCKDOWN_NATIVE_HOST").filter(|v| !v.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => app_exe
                .parent()
                .map(|d| d.join(HOST_BINARY))
                .ok_or("the app's folder couldn't be read")?,
        };
        Ok(Self {
            data,
            host_binary,
            app_exe,
        })
    }
}

/// The endpoint's state as the settings screen shows it.
#[derive(Clone, Debug, Serialize)]
pub struct EndpointStatus {
    /// Whether the socket is open.
    pub listening: bool,
    /// Why not, when it isn't.
    pub problem: Option<String>,
    pub host_binary: String,
    /// The browsers whose folders exist for this user, with whether a
    /// manifest for Lockdown is in place.
    pub browsers: Vec<BrowserView>,
}

/// One browser on this machine.
#[derive(Clone, Debug, Serialize)]
pub struct BrowserView {
    pub browser: Browser,
    pub name: String,
    pub connected: bool,
}

/// The running endpoint, as the shell keeps it.
pub struct Endpoint {
    shared: Arc<Shared>,
    config: EndpointConfig,
    problem: Option<String>,
    #[cfg(unix)]
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Endpoint {
    /// An endpoint that answers pairing questions from the app but has no
    /// socket open, with the reason: the host is missing, or the platform
    /// has no transport yet.
    pub fn idle(shared: Arc<Shared>, config: EndpointConfig, problem: String) -> Self {
        Self {
            shared,
            config,
            problem: Some(problem),
            #[cfg(unix)]
            stop: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    /// Opens the socket and starts accepting. Returns an idle endpoint,
    /// not an error, when it can't, so the app runs on and says why.
    pub fn start(
        app: App,
        copy: Arc<dyn CopySink>,
        config: EndpointConfig,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&config.data)
            .map_err(|e| format!("couldn't make the data directory: {e}"))?;
        let shared = Arc::new(Shared::new(app, copy, &config.data)?);
        Ok(match Self::listen(Arc::clone(&shared), &config) {
            Ok(endpoint) => endpoint,
            Err(problem) => Self::idle(shared, config, problem),
        })
    }

    #[cfg(unix)]
    fn listen(shared: Arc<Shared>, config: &EndpointConfig) -> Result<Self, String> {
        use std::sync::atomic::{AtomicBool, Ordering};

        use ecosystem_identity::handshake::unix::UnixSocketTransport;
        use ecosystem_identity::handshake::verify::AllowedBinary;
        use ecosystem_identity::handshake::{PeerTransport, ProcessIdentityVerifier};
        use lockdown_native_messaging::endpoint::{ENDPOINT_V, EndpointFile, socket_path};

        if !config.host_binary.is_file() {
            return Err(format!(
                "the browser host isn't installed at {}",
                config.host_binary.display()
            ));
        }
        let host_hash = ProcessIdentityVerifier::hash_of(&config.host_binary)
            .map_err(|e| format!("couldn't read the browser host: {e}"))?;
        let app_hash = ProcessIdentityVerifier::hash_of(&config.app_exe)
            .map_err(|e| format!("couldn't read the app's own file: {e}"))?;
        let socket = socket_path(&config.data);
        let transport = UnixSocketTransport::at(&socket);
        transport
            .listen()
            .map_err(|e| format!("couldn't open the browser socket: {e}"))?;
        EndpointFile {
            v: ENDPOINT_V,
            socket,
            app_exe: config.app_exe.clone(),
            app_sha256: data_encoding::HEXLOWER.encode(&app_hash),
        }
        .write(&config.data)
        .map_err(|e| format!("couldn't write the endpoint file: {e}"))?;
        let verifier = Arc::new(ProcessIdentityVerifier::new(vec![AllowedBinary {
            app: "native-host".into(),
            path: config.host_binary.clone(),
            sha256: Some(host_hash),
        }]));
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, shared2) = (Arc::clone(&stop), Arc::clone(&shared));
        std::thread::Builder::new()
            .name("lockdown-browser".into())
            .spawn(move || {
                while !stop2.load(Ordering::SeqCst) {
                    match transport.accept(Duration::from_millis(250)) {
                        Ok(ch) => {
                            let (shared, verifier) = (Arc::clone(&shared2), Arc::clone(&verifier));
                            let _ = std::thread::Builder::new()
                                .name("lockdown-browser-conn".into())
                                .spawn(move || serve(&shared, verifier.as_ref(), ch));
                        }
                        Err(ecosystem_identity::Error::Timeout) => {}
                        Err(_) => std::thread::sleep(Duration::from_millis(250)),
                    }
                }
                // Dropping the transport removes the socket file.
                drop(transport);
            })
            .map_err(|e| format!("couldn't start the browser endpoint: {e}"))?;
        Ok(Self {
            shared,
            config: config.clone(),
            problem: None,
            stop,
        })
    }

    #[cfg(not(unix))]
    fn listen(_shared: Arc<Shared>, _config: &EndpointConfig) -> Result<Self, String> {
        Err("the browser extension can't reach the app on this system yet (no named-pipe transport)".into())
    }

    /// Requests waiting for the person.
    pub fn pending(&self) -> Vec<PendingView> {
        self.shared.pairings().pending()
    }

    /// The person's answer to a pairing request. An approval goes through
    /// the `Approve` gate first (D101): pairing lets a browser ask for
    /// fills.
    pub fn answer(
        &self,
        request_id: &str,
        approve: bool,
        gate: GateInput,
    ) -> Result<(), lockdown_app::AppError> {
        if approve {
            self.shared.app.approve(
                ApprovalKind::Approve,
                "connect this browser to Lockdown",
                gate,
            )?;
        }
        self.shared
            .pairings()
            .answer(request_id, approve)
            .map_err(lockdown_app::AppError::refused)
    }

    /// Every paired browser.
    pub fn paired(&self) -> Vec<PairedView> {
        self.shared.pairings().paired()
    }

    /// Removes a paired browser; its open connections stop being answered.
    pub fn unpair(&self, client_id: &str) -> Result<(), String> {
        self.shared.pairings().unpair(client_id)
    }

    /// The socket's state and the browsers on this machine.
    pub fn status(&self) -> EndpointStatus {
        let os = Os::current();
        let browsers = Homes::from_env()
            .map(|homes| {
                Browser::ALL
                    .iter()
                    .filter(|b| manifest::detected(os, **b, &homes))
                    .map(|b| BrowserView {
                        browser: *b,
                        name: b.name().into(),
                        connected: manifest::target(os, *b, &homes).manifest.is_file(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        EndpointStatus {
            listening: self.problem.is_none(),
            problem: self.problem.clone(),
            host_binary: self.config.host_binary.display().to_string(),
            browsers,
        }
    }

    /// Writes the host manifest for `browsers` (every detected browser when
    /// empty), naming the installed host.
    pub fn install(&self, browsers: &[Browser]) -> Result<Vec<Target>, String> {
        let os = Os::current();
        let homes = Homes::from_env().ok_or("no home directory")?;
        let wanted: Vec<Browser> = if browsers.is_empty() {
            Browser::ALL
                .into_iter()
                .filter(|b| manifest::detected(os, *b, &homes))
                .collect()
        } else {
            browsers.to_vec()
        };
        wanted
            .into_iter()
            .map(|b| {
                manifest::install(os, b, &homes, &self.config.host_binary)
                    .map_err(|e| format!("couldn't connect {}: {e}", b.name()))
            })
            .collect()
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            if self.problem.is_none() {
                let _ = std::fs::remove_file(
                    self.config
                        .data
                        .join(lockdown_native_messaging::endpoint::ENDPOINT_FILE),
                );
            }
        }
    }
}
