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
//! - [`alerts`] — account alerts: a new device's sign-in, an email breach
//! - [`breach_monitor`] — the email breach job over an injectable feed
//! - [`sessions`] — one row per session token, so a session can be ended
//! - [`live`] — the live channel: content-free "something changed" events
//! - [`tls`] — TLS on the server itself, hybrid post-quantum first
//! - [`client_addr`] — the client's address behind trusted reverse proxies
//! - [`backup`] — the encrypted snapshot under the person's Backup Key
//! - [`sharing`] / [`sharing_api`] — shared vaults and their members, items
//!   shared on their own, share links, request links, emergency access
//! - [`web_vault`] — the built web vault at `/vault/`, with its headers
//!
//! The key-transparency log (`storage::kt_log`, served under `/v1/kt/`) is
//! appended inside the same transactions that accept identity manifests,
//! vault heads and device logins. The mobile push relay is not built
//! (D122); desktop and web devices hear of changes on [`live`].

pub mod alerts;
pub mod api;
pub mod backup;
pub mod breach_monitor;
pub mod client_addr;
pub mod keys;
pub mod live;
pub mod ratelimit;
pub mod sessions;
pub mod sharing;
pub mod sharing_api;
pub mod storage;
pub mod tls;
pub mod token;
pub mod web_vault;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};
use lockdown_crypto::auth::{LoginServerState, OpaqueServer};
use lockdown_vault::Id;
use serde::Serialize;

use crate::keys::ServerKeys;
use crate::ratelimit::RateLimiter;
use crate::storage::{Account, Storage};
use crate::token::TokenSigner;
use lockdown_crypto::SigningKey;

/// What `/.well-known/lockdown` says about this install.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// The discovery document version.
    pub v: u16,
    /// This server's release (05-self-host §7: a client names the release
    /// it needs when it is too new for this one).
    pub version: String,
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
    /// The key-transparency log's public key, lowercase hex, for outside
    /// auditors (clients pin it from `/v1/kt/sth` on first sight).
    pub kt_log_key: String,
    /// The live channel at `/v1/events` (content-free change events).
    pub live: bool,
    /// The account's session list, with ending a session.
    pub sessions: bool,
    /// `/v1/account/storage`: the attachment room per account (D234).
    pub storage: bool,
}

/// The discovery document this build serves.
pub fn discovery(signups: bool, log_key: &lockdown_crypto::VerifyingKey) -> Discovery {
    Discovery {
        v: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
        api: vec![lockdown_sync::wire::API_V],
        features: Features {
            opaque: true,
            signups,
            key_transparency: true,
            checkpoints: true,
            blobs: true,
            push: false,
            kt_log_key: lockdown_crypto::Hash32(log_key.to_bytes()).to_hex(),
            live: true,
            sessions: true,
            storage: true,
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
    /// How many bytes of attachments the vaults one account owns may hold
    /// (D234); `None` is no limit.
    pub attachment_quota: Option<u64>,
    /// Reverse proxies whose `X-Forwarded-For` is believed (`client_addr`).
    pub trusted_proxies: Vec<client_addr::Cidr>,
    /// The built web vault (`apps/web/dist`) to serve at `/vault/`; `None`
    /// serves none (docs/plan/17-web-vault.md).
    pub web_vault: Option<std::path::PathBuf>,
}

/// The default blob cap: 64 MiB.
pub const DEFAULT_MAX_BLOB_BYTES: u64 = 64 << 20;

/// The default attachment room per account: 5 GB (D234).
pub const DEFAULT_ATTACHMENT_QUOTA: u64 = 5_000_000_000;

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
            attachment_quota: Some(DEFAULT_ATTACHMENT_QUOTA),
            trusted_proxies: Vec::new(),
            web_vault: None,
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
    /// Signs the key-transparency log's tree heads.
    pub log_key: SigningKey,
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
    /// The live channel's fan-out.
    pub live: Arc<live::Notifier>,
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
            log_key: keys.log_key,
            per_address: RateLimiter::new(config.per_address.0, config.per_address.1),
            per_login: RateLimiter::new(config.per_login.0, config.per_login.1),
            config,
            pending: Mutex::new(HashMap::new()),
            clock: Box::new(|| lockdown_vault::Timestamp::now().0),
            live: Arc::new(live::Notifier::new()),
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
        .route("/v1/kt/sth", get(api::kt_sth))
        .route("/v1/kt/inclusion", get(api::kt_inclusion))
        .route("/v1/kt/consistency", get(api::kt_consistency))
        .route("/v1/kt/entries", get(api::kt_entries))
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
        .route("/v1/account/alerts", get(api::account_alerts))
        .route("/v1/account/alerts/{seq}/dismiss", post(api::dismiss_alert))
        .route("/v1/directory", get(sharing_api::directory))
        .route(
            "/v1/directory/accounts/{account}",
            get(sharing_api::directory_account),
        )
        .route("/v1/shared", get(sharing_api::shared_vaults))
        .route(
            "/v1/vaults/{vault}/members",
            get(sharing_api::members_since).post(sharing_api::push_membership),
        )
        .route("/v1/vaults/{vault}/key", get(sharing_api::vault_key))
        .route(
            "/v1/shares/items",
            get(sharing_api::received_items).post(sharing_api::share_item),
        )
        .route("/v1/shares/items/sent", get(sharing_api::sent_items))
        .route(
            "/v1/shares/items/{share}",
            delete(sharing_api::unshare_item),
        )
        .route(
            "/v1/sends",
            get(sharing_api::sends)
                .post(sharing_api::create_send)
                .layer(DefaultBodyLimit::max(blob_limit)),
        )
        .route("/v1/sends/{send}", delete(sharing_api::delete_send))
        .route("/v1/sends/{send}/meta", get(sharing_api::send_meta))
        .route("/v1/sends/{send}/open", post(sharing_api::open_send))
        .route(
            "/v1/requests",
            get(sharing_api::requests).post(sharing_api::create_request),
        )
        .route(
            "/v1/requests/{request}",
            delete(sharing_api::delete_request),
        )
        .route("/v1/requests/{request}/key", get(sharing_api::request_key))
        .route(
            "/v1/requests/{request}/answer",
            post(sharing_api::answer_request),
        )
        .route(
            "/v1/emergency",
            get(sharing_api::emergency).post(sharing_api::create_emergency),
        )
        .route(
            "/v1/emergency/{grant}/step",
            post(sharing_api::emergency_step),
        )
        .route(
            "/v1/emergency/{grant}/envelopes",
            post(sharing_api::emergency_envelopes),
        )
        .route(
            "/v1/emergency/{grant}/release",
            get(sharing_api::emergency_release),
        )
        .route("/s/{send}", get(sharing_api::send_page))
        .route("/r/{request}", get(sharing_api::request_page))
        .route("/vault", get(web_vault::redirect))
        .route("/vault/", get(web_vault::index))
        .route("/vault/{*path}", get(web_vault::file))
        .route("/v1/account/sessions", get(api::list_sessions))
        .route("/v1/account/sessions/{id}/end", post(api::end_session))
        .route(
            "/v1/account/sessions/end-others",
            post(api::end_other_sessions),
        )
        .route("/v1/events", get(live::events))
        .route("/v1/account/storage", get(api::account_storage))
        .route(
            "/v1/account/monitored",
            get(api::monitored).put(api::set_monitored),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            live::notify_writes,
        ))
        .with_state(state)
}

/// Serves `app(state)` on `listener` until `shutdown` resolves.
pub async fn serve(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let live = state.live.clone();
    let shutdown = async move {
        shutdown.await;
        live.shut_down();
    };
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

/// TLS for [`serve_tls`]: the configuration and where the handshake
/// counts go.
#[derive(Clone, Debug)]
pub struct TlsServing {
    /// The rustls server configuration (`lockdown_tls::server_config`).
    pub config: Arc<rustls::ServerConfig>,
    /// The `Strict-Transport-Security` max-age; 0 sends none.
    pub hsts_max_age: u64,
    /// Which groups the handshakes used.
    pub stats: Arc<tls::TlsStats>,
}

/// Serves `app(state)` over TLS on `listener` until `shutdown` resolves.
pub async fn serve_tls(
    listener: tokio::net::TcpListener,
    state: Arc<AppState>,
    tls: TlsServing,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let state_live = state.live.clone();
    let mut router = app(state);
    if tls.hsts_max_age > 0 {
        let value = tls::hsts_value(tls.hsts_max_age);
        router = router.layer(axum::middleware::from_fn(move |req, next| {
            tls::hsts(value.clone(), req, next)
        }));
    }
    let listener = tls::TlsListener::new(listener, tls.config, tls.stats)?;
    let live = state_live;
    let shutdown = async move {
        shutdown.await;
        live.shut_down();
    };
    axum::serve(
        axum::serve::ListenerExt::tap_io(listener, |_| {}),
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

/// A server running on its own thread, for tests and tools that drive it
/// over a real socket. Dropping it stops the server.
pub struct RunningServer {
    /// Where it listens.
    pub addr: SocketAddr,
    /// With TLS: which groups the handshakes used.
    pub tls_stats: Option<Arc<tls::TlsStats>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for RunningServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RunningServer({})", self.addr)
    }
}

impl RunningServer {
    /// The base URL, `http://127.0.0.1:<port>` (`https://` with TLS).
    pub fn url(&self) -> String {
        let scheme = if self.tls_stats.is_some() {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://{}", self.addr)
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
    run_on_thread(state, None)
}

/// [`run_in_background`] over TLS with `config` (HSTS on).
pub fn run_tls_in_background(
    state: Arc<AppState>,
    config: Arc<rustls::ServerConfig>,
) -> std::io::Result<RunningServer> {
    run_on_thread(
        state,
        Some(TlsServing {
            config,
            hsts_max_age: 31_536_000,
            stats: Arc::new(tls::TlsStats::default()),
        }),
    )
}

fn run_on_thread(state: Arc<AppState>, tls: Option<TlsServing>) -> std::io::Result<RunningServer> {
    let tls_stats = tls.as_ref().map(|t| t.stats.clone());
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
                let shutdown = async {
                    let _ = rx.await;
                };
                let _ = match tls {
                    Some(tls) => serve_tls(listener, state, tls, shutdown).await,
                    None => serve(listener, state, shutdown).await,
                };
            });
        })?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(RunningServer {
            addr,
            tls_stats,
            shutdown: Some(tx),
            thread: Some(thread),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(std::io::Error::other(
            "the server thread stopped before it listened",
        )),
    }
}
