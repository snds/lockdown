//! How the client reaches the server: the [`Transport`] contract over the
//! wire types, and [`HttpTransport`], the HTTPS implementation (blocking
//! `reqwest` over rustls behind the default `http` feature, which is
//! enough for the CLI; the shells get an async one later). A host with
//! its own HTTP client (the web vault, where the browser makes every
//! request) installs it with [`install_http_hook`] and builds without
//! `http`. `HttpTransport` is also the
//! [`AuthRounds`](lockdown_crypto::auth::AuthRounds) the OPAQUE client
//! drives, so one connection does login and sync.

use lockdown_crypto::auth::{AuthRounds, DeviceProof, LoginStarted, Session};
use lockdown_vault::{Hash32, Id};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::BufRead;

use crate::kt::{AccountEntries, ConsistencyProofWire, InclusionProofWire, SthWire};
use crate::wire::{
    self, BEARER, BlobAccepted, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, HeadAccepted,
    HeadRefused, HeadsSince, IdentityPush, IdentitySince, ItemPush, ItemsSince, LoginFinish,
    LoginFinishReply, LoginStart, LoginStartReply, PushReply, RegisterFinish, RegisterStart,
    RegisterStartReply, VaultList,
};

/// What can go wrong between client and server.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// No answer, or not an HTTP one.
    #[error("couldn't reach the server: {0}")]
    Network(String),
    /// An answer that did not decode as the wire type expected.
    #[error("the server's answer didn't decode: {0}")]
    Decode(String),
    /// The server said no.
    #[error("the server refused: {message}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// What the server said to do about it.
        code: ErrorCode,
        /// For the person.
        message: String,
        /// For a refused head, where the server stands.
        head: Option<HeadRefused>,
    },
}

/// The transport result type.
pub type Result<T> = std::result::Result<T, TransportError>;

/// The authenticated calls of the protocol (`token` is the session).
pub trait Transport {
    /// `GET /v1/identity?since=N`.
    fn identity_since(&self, token: &str, since: u64) -> Result<IdentitySince>;
    /// `PUT /v1/identity`. A refusal with a chain code comes back as
    /// `Ok(Err(_))`; anything else is an error.
    fn put_identity(
        &self,
        token: &str,
        push: &IdentityPush,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// `GET /v1/vaults`.
    fn list_vaults(&self, token: &str) -> Result<VaultList>;
    /// `PUT /v1/vaults/{vault}`.
    fn put_vault(&self, token: &str, vault_id: Id, record: &[u8]) -> Result<()>;
    /// `GET /v1/vaults/{vault}/heads?since=N`.
    fn heads_since(&self, token: &str, vault_id: Id, since: u64) -> Result<HeadsSince>;
    /// `PUT /v1/vaults/{vault}/heads`. A refusal with a head code comes
    /// back as `Ok(Err(_))`; anything else is an error.
    fn put_head(
        &self,
        token: &str,
        vault_id: Id,
        head: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// `GET /v1/vaults/{vault}/items?since=R`.
    fn items_since(&self, token: &str, vault_id: Id, since: u64) -> Result<ItemsSince>;
    /// `PUT /v1/vaults/{vault}/items/{item}`. Stale comes back as
    /// [`PushReply::Stale`], not an error.
    fn push_item(
        &self,
        token: &str,
        vault_id: Id,
        item_id: Id,
        push: &ItemPush,
    ) -> Result<PushReply>;
    /// `PUT /v1/vaults/{vault}/blobs/{hash}`.
    fn put_blob(
        &self,
        token: &str,
        vault_id: Id,
        hash: Hash32,
        bytes: &[u8],
    ) -> Result<BlobAccepted>;
    /// `GET /v1/vaults/{vault}/blobs/{hash}`. `None` when the server
    /// doesn't hold it.
    fn get_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>>;
    /// `HEAD /v1/vaults/{vault}/blobs/{hash}`.
    fn has_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<bool>;
    /// `GET /v1/kt/sth`: the log's newest signed tree head and the log key.
    fn kt_sth(&self) -> Result<SthWire>;
    /// `GET /v1/kt/inclusion?index=I&size=N`.
    fn kt_inclusion(&self, index: u64, size: u64) -> Result<InclusionProofWire>;
    /// `GET /v1/kt/consistency?from=M&to=N`.
    fn kt_consistency(&self, from: u64, to: u64) -> Result<ConsistencyProofWire>;
    /// `GET /v1/kt/entries?from=I`: the account's own leaves from index `I`.
    fn kt_entries(&self, token: &str, from: u64) -> Result<AccountEntries>;
}

/// The HTTP methods the protocol uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// `GET`.
    Get,
    /// `PUT`.
    Put,
    /// `POST`.
    Post,
    /// `HEAD`.
    Head,
    /// `DELETE`.
    Delete,
}

impl Method {
    /// The method's name on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Post => "POST",
            Self::Head => "HEAD",
            Self::Delete => "DELETE",
        }
    }
}

/// One request as a host's HTTP client sees it: what [`HttpHook`] is
/// handed when a build has no HTTP client of its own (the web vault, whose
/// requests go through the browser, docs/plan/17-web-vault.md).
#[derive(Clone, Copy, Debug)]
pub struct HttpRequest<'a> {
    /// The method.
    pub method: Method,
    /// The full URL: the transport's base and the path.
    pub url: &'a str,
    /// The `Content-Type` header.
    pub content_type: &'a str,
    /// The `Authorization` header (`Bearer <session token>`), when the
    /// call is authenticated.
    pub authorization: Option<&'a str>,
    /// The body, when the request has one.
    pub body: Option<&'a [u8]>,
}

/// A host's HTTP client: the status and the body, or why there was no
/// answer (for the person, never with a secret in it).
pub type HttpHook = fn(&HttpRequest<'_>) -> std::result::Result<(u16, Vec<u8>), String>;

static HTTP_HOOK: std::sync::OnceLock<HttpHook> = std::sync::OnceLock::new();

/// Installs the host's HTTP client, once, before any transport is made.
/// Every [`HttpTransport`] then sends through it, in place of the built-in
/// client (which a build without the `http` feature does not have).
/// Returns `false` when one was installed already; the first one stays.
pub fn install_http_hook(hook: HttpHook) -> bool {
    HTTP_HOOK.set(hook).is_ok()
}

/// The reserved base of the server the app runs inside itself for a local
/// account (docs/plan/39-first-run.md §1.1): no socket, no port, no TLS.
/// Requests to it go to the hook a host installs with
/// [`install_local_server`], never to the network.
pub const LOCAL_BASE: &str = "lockdown-local://this-device";

/// The in-process server: handed the data directory whose server is meant
/// (see [`set_local_home`]) and the request; answers like [`HttpHook`].
pub type LocalHook = Box<
    dyn Fn(&std::path::Path, &HttpRequest<'_>) -> std::result::Result<(u16, Vec<u8>), String>
        + Send
        + Sync,
>;

static LOCAL_HOOK: std::sync::OnceLock<LocalHook> = std::sync::OnceLock::new();
static LOCAL_HOME: std::sync::RwLock<Option<std::path::PathBuf>> = std::sync::RwLock::new(None);

thread_local! {
    static THREAD_HOME: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Installs the in-process server, once per process, before any transport
/// to [`LOCAL_BASE`] is made. The desktop app and the command line do; the
/// web vault never does. Returns `false` when one was installed already.
pub fn install_local_server(hook: LocalHook) -> bool {
    LOCAL_HOOK.set(hook).is_ok()
}

/// Whether a local server is installed in this process.
pub fn local_server_installed() -> bool {
    LOCAL_HOOK.get().is_some()
}

/// Names the data directory whose local server this thread's requests
/// reach (the app's worker thread names its own; the command line names
/// its one). `None` clears it.
pub fn set_local_home(home: Option<std::path::PathBuf>) {
    THREAD_HOME.with(|h| *h.borrow_mut() = home);
}

/// The process-wide fallback for [`set_local_home`], for a host with one
/// data directory and requests on more than one thread.
pub fn set_default_local_home(home: Option<std::path::PathBuf>) {
    match LOCAL_HOME.write() {
        Ok(mut h) => *h = home,
        Err(poisoned) => *poisoned.into_inner() = home,
    }
}

fn local_home() -> Option<std::path::PathBuf> {
    THREAD_HOME
        .with(|h| h.borrow().clone())
        .or_else(|| match LOCAL_HOME.read() {
            Ok(h) => h.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        })
}

/// Whether `base` is the reserved address of the in-process server.
pub fn is_local(base: &str) -> bool {
    base.trim_end_matches('/') == LOCAL_BASE
}

fn local_exchange(request: &HttpRequest<'_>) -> Result<(u16, Vec<u8>)> {
    let hook = LOCAL_HOOK.get().ok_or_else(|| {
        TransportError::Network("this build can't keep a vault on this computer alone".into())
    })?;
    let home = local_home().ok_or_else(|| {
        TransportError::Network("the vault on this computer isn't open on this thread".into())
    })?;
    hook(&home, request).map_err(TransportError::Network)
}

static HOLD: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Holds every request this process makes through [`HttpTransport`],
/// already made or not, until called again with `None`. `reason` is the
/// plain sentence each held request fails with. A host uses it for a
/// session that must stay off the network (docs/plan/40-devtools.md §3).
pub fn hold_network(reason: Option<String>) {
    match HOLD.write() {
        Ok(mut held) => *held = reason,
        Err(poisoned) => *poisoned.into_inner() = reason,
    }
}

/// Why requests are held, while they are.
pub fn network_held() -> Option<String> {
    match HOLD.read() {
        Ok(held) => held.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn not_held() -> Result<()> {
    match network_held() {
        Some(reason) => Err(TransportError::Network(reason)),
        None => Ok(()),
    }
}

/// The HTTPS transport.
pub struct HttpTransport {
    base: String,
    /// Whether a call carrying a session token was answered 401: the token
    /// was refused, so the caller may renew it and try once more
    /// (docs/plan/33-session-renewal.md).
    turned_away: std::sync::atomic::AtomicBool,
    #[cfg(feature = "http")]
    client: reqwest::blocking::Client,
    #[cfg(feature = "http")]
    trust: lockdown_tls::Trust,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HttpTransport({})", self.base)
    }
}

/// The `User-Agent` every Lockdown client sends: the version and the
/// platform, which the server shows in the account's session list.
pub fn user_agent() -> String {
    format!(
        "lockdown/{} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )
}

/// Whether `base` may be used: `https://` anywhere, plain `http://` only to
/// this machine (a session token is a bearer credential and must not cross
/// a network in the clear; 05-self-host §3).
pub fn check_base(base: &str) -> Result<()> {
    if is_local(base) {
        return if local_server_installed() {
            Ok(())
        } else {
            Err(TransportError::Network(
                "this build can't keep a vault on this computer alone".into(),
            ))
        };
    }
    let lower = base.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    let Some(rest) = lower.strip_prefix("http://") else {
        return Err(TransportError::Network(format!(
            "{base} isn't an https:// address"
        )));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    let loopback = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if loopback {
        Ok(())
    } else {
        Err(TransportError::Network(format!(
            "{base} is plain http to another machine; Lockdown only talks to a server there over https"
        )))
    }
}

impl HttpTransport {
    /// A transport for the server at `base` (`https://host[:port]`, no
    /// trailing slash needed; `http://` only to this machine). TLS is 1.3
    /// with the hybrid X25519MLKEM768 key exchange first (`lockdown-tls`);
    /// the certificate is checked against the system's store, or only
    /// against the certificates in `LOCKDOWN_CA_CERT` when that is set.
    pub fn new(base: &str) -> Result<Self> {
        #[cfg(feature = "http")]
        {
            let trust = lockdown_tls::Trust::from_env()
                .map_err(|e| TransportError::Network(e.to_string()))?;
            Self::with_trust(base, &trust)
        }
        #[cfg(not(feature = "http"))]
        {
            check_base(base)?;
            if HTTP_HOOK.get().is_none() && !is_local(base) {
                return Err(TransportError::Network(
                    "this build has no HTTP client and the host installed none".into(),
                ));
            }
            Ok(Self {
                base: base.trim_end_matches('/').to_string(),
                turned_away: std::sync::atomic::AtomicBool::new(false),
            })
        }
    }

    /// [`HttpTransport::new`] with the certificates to trust given here.
    #[cfg(feature = "http")]
    pub fn with_trust(base: &str, trust: &lockdown_tls::Trust) -> Result<Self> {
        Self::build(base, trust, Some(std::time::Duration::from_secs(30)))
    }

    #[cfg(feature = "http")]
    fn build(
        base: &str,
        trust: &lockdown_tls::Trust,
        timeout: Option<std::time::Duration>,
    ) -> Result<Self> {
        check_base(base)?;
        lockdown_tls::install_default_provider();
        let tls = lockdown_tls::client_config(trust)
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .connect_timeout(std::time::Duration::from_secs(15))
            .user_agent(user_agent())
            .tls_backend_preconfigured(tls)
            .build()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            turned_away: std::sync::atomic::AtomicBool::new(false),
            client,
            trust: trust.clone(),
        })
    }

    /// The server's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Whether any call through this transport that carried a session
    /// token was answered 401 (the token refused, run out or ended).
    pub fn session_refused(&self) -> bool {
        self.turned_away.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn note(&self, token: Option<&str>, answer: Result<(u16, Vec<u8>)>) -> Result<(u16, Vec<u8>)> {
        if token.is_some() && matches!(answer, Ok((401, _))) {
            self.turned_away
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        answer
    }

    fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        self.note(
            token,
            self.exchange(method, path, CONTENT_TYPE, token, body),
        )
    }

    /// Like `send`, with an `application/octet-stream` body: blobs.
    fn send_raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        self.note(
            token,
            self.exchange(method, path, "application/octet-stream", token, body),
        )
    }

    /// One round trip: through the host's hook when one is installed,
    /// else the built-in client.
    fn exchange(
        &self,
        method: Method,
        path: &str,
        content_type: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        not_held()?;
        let url = format!("{}{path}", self.base);
        if is_local(&self.base) {
            let authorization = token.map(|t| format!("{BEARER}{t}"));
            return local_exchange(&HttpRequest {
                method,
                url: &url,
                content_type,
                authorization: authorization.as_deref(),
                body: body.as_deref(),
            });
        }
        if let Some(hook) = HTTP_HOOK.get() {
            let authorization = token.map(|t| format!("{BEARER}{t}"));
            return hook(&HttpRequest {
                method,
                url: &url,
                content_type,
                authorization: authorization.as_deref(),
                body: body.as_deref(),
            })
            .map_err(TransportError::Network);
        }
        #[cfg(feature = "http")]
        {
            let method = match method {
                Method::Get => reqwest::Method::GET,
                Method::Put => reqwest::Method::PUT,
                Method::Post => reqwest::Method::POST,
                Method::Head => reqwest::Method::HEAD,
                Method::Delete => reqwest::Method::DELETE,
            };
            let mut req = self
                .client
                .request(method, url)
                .header(reqwest::header::CONTENT_TYPE, content_type);
            if let Some(t) = token {
                req = req.header(reqwest::header::AUTHORIZATION, format!("{BEARER}{t}"));
            }
            if let Some(b) = body {
                req = req.body(b);
            }
            let res = req
                .send()
                .map_err(|e| TransportError::Network(e.to_string()))?;
            let status = res.status().as_u16();
            let bytes = res
                .bytes()
                .map_err(|e| TransportError::Network(e.to_string()))?;
            Ok((status, bytes.to_vec()))
        }
        #[cfg(not(feature = "http"))]
        {
            let _ = (url, content_type, token, body);
            Err(TransportError::Network(
                "this build has no HTTP client and the host installed none".into(),
            ))
        }
    }

    fn refused(status: u16, bytes: &[u8]) -> TransportError {
        match wire::decode::<ErrorBody>(bytes) {
            Ok(body) => TransportError::Refused {
                status,
                code: body.code,
                // A revoked device's sign-in is told in this client's own
                // words, not the server's (docs/plan/35-revoke-split.md §1).
                message: if body.code == ErrorCode::RevokedSignIn {
                    crate::renew::SignInReason::Revoked.message().to_string()
                } else {
                    body.message
                },
                head: wire::decode::<HeadRefused>(bytes).ok(),
            },
            Err(_) => TransportError::Refused {
                status,
                code: ErrorCode::Internal,
                message: format!(
                    "HTTP {status}: {}",
                    String::from_utf8_lossy(bytes)
                        .chars()
                        .take(200)
                        .collect::<String>()
                ),
                head: None,
            },
        }
    }

    pub(crate) fn call<T: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
    ) -> Result<R> {
        let body = match body {
            Some(b) => Some(wire::encode(b).map_err(|e| TransportError::Decode(e.to_string()))?),
            None => None,
        };
        let (status, bytes) = self.send(method, path, token, body)?;
        if (200..300).contains(&status) {
            wire::decode(&bytes).map_err(|e| TransportError::Decode(format!("{path}: {e}")))
        } else {
            Err(Self::refused(status, &bytes))
        }
    }

    pub(crate) fn call_raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        let (status, bytes) = self.send(method, path, token, body)?;
        if (200..300).contains(&status) {
            Ok((status, bytes))
        } else {
            Err(Self::refused(status, &bytes))
        }
    }
}

fn auth_error(e: TransportError) -> lockdown_crypto::Error {
    match e {
        TransportError::Refused {
            code: ErrorCode::LoginFailed,
            ..
        } => lockdown_crypto::Error::LoginFailed,
        // A device the identity revoked: the words are this client's own,
        // whatever the server wrote (docs/plan/35-revoke-split.md §1).
        TransportError::Refused {
            code: ErrorCode::RevokedSignIn,
            ..
        } => lockdown_crypto::Error::Invalid(
            crate::renew::SignInReason::Revoked.message().to_string(),
        ),
        other => lockdown_crypto::Error::Invalid(other.to_string()),
    }
}

impl AuthRounds for HttpTransport {
    fn register_start(&mut self, login: &str, request: &[u8]) -> lockdown_crypto::Result<Vec<u8>> {
        let reply: RegisterStartReply = self
            .call(
                Method::Post,
                "/v1/account/register/start",
                None,
                Some(&RegisterStart {
                    login: login.into(),
                    request: request.to_vec(),
                }),
            )
            .map_err(auth_error)?;
        Ok(reply.response)
    }

    fn register_finish(
        &mut self,
        login: &str,
        profile_id: &Id,
        upload: &[u8],
    ) -> lockdown_crypto::Result<()> {
        let body = wire::encode(&RegisterFinish {
            login: login.into(),
            profile_id: *profile_id,
            upload: upload.to_vec(),
        })?;
        self.call_raw(
            Method::Post,
            "/v1/account/register/finish",
            None,
            Some(body),
        )
        .map_err(auth_error)?;
        Ok(())
    }

    fn login_start(
        &mut self,
        login: &str,
        request: &[u8],
    ) -> lockdown_crypto::Result<LoginStarted> {
        let reply: LoginStartReply = self
            .call(
                Method::Post,
                "/v1/account/login/start",
                None,
                Some(&LoginStart {
                    login: login.into(),
                    request: request.to_vec(),
                }),
            )
            .map_err(auth_error)?;
        Ok(LoginStarted {
            login_id: reply.login_id,
            response: reply.response,
            ksf: reply.ksf,
        })
    }

    fn login_finish(
        &mut self,
        login_id: &Id,
        finalization: &[u8],
        device: &DeviceProof,
    ) -> lockdown_crypto::Result<Session> {
        let reply: LoginFinishReply = self
            .call(
                Method::Post,
                "/v1/account/login/finish",
                None,
                Some(&LoginFinish {
                    login_id: *login_id,
                    finalization: finalization.to_vec(),
                    device: DeviceProofWire {
                        device_id: device.device_id,
                        device_pub: device.device_pub,
                        pop: device.pop.clone(),
                    },
                }),
            )
            .map_err(auth_error)?;
        Ok(crate::account_security::session_of(reply))
    }
}

impl Transport for HttpTransport {
    fn identity_since(&self, token: &str, since: u64) -> Result<IdentitySince> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/identity?since={since}"),
            Some(token),
            None,
        )
    }

    fn put_identity(
        &self,
        token: &str,
        push: &IdentityPush,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let body = wire::encode(push).map_err(|e| TransportError::Decode(e.to_string()))?;
        let (status, bytes) = self.send(Method::Put, "/v1/identity", Some(token), Some(body))?;
        if (200..300).contains(&status) {
            return Ok(Ok(
                wire::decode(&bytes).map_err(|e| TransportError::Decode(e.to_string()))?
            ));
        }
        if status == 409
            && let Ok(refused) = wire::decode::<HeadRefused>(&bytes)
        {
            return Ok(Err(refused));
        }
        Err(Self::refused(status, &bytes))
    }

    fn list_vaults(&self, token: &str) -> Result<VaultList> {
        self.call::<(), _>(Method::Get, "/v1/vaults", Some(token), None)
    }

    fn put_vault(&self, token: &str, vault_id: Id, record: &[u8]) -> Result<()> {
        self.call_raw(
            Method::Put,
            &format!("/v1/vaults/{vault_id}"),
            Some(token),
            Some(record.to_vec()),
        )?;
        Ok(())
    }

    fn heads_since(&self, token: &str, vault_id: Id, since: u64) -> Result<HeadsSince> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/vaults/{vault_id}/heads?since={since}"),
            Some(token),
            None,
        )
    }

    fn put_head(
        &self,
        token: &str,
        vault_id: Id,
        head: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let (status, bytes) = self.send(
            Method::Put,
            &format!("/v1/vaults/{vault_id}/heads"),
            Some(token),
            Some(head.to_vec()),
        )?;
        if (200..300).contains(&status) {
            return Ok(Ok(
                wire::decode(&bytes).map_err(|e| TransportError::Decode(e.to_string()))?
            ));
        }
        if status == 409
            && let Ok(refused) = wire::decode::<HeadRefused>(&bytes)
        {
            return Ok(Err(refused));
        }
        Err(Self::refused(status, &bytes))
    }

    fn items_since(&self, token: &str, vault_id: Id, since: u64) -> Result<ItemsSince> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/vaults/{vault_id}/items?since={since}"),
            Some(token),
            None,
        )
    }

    fn push_item(
        &self,
        token: &str,
        vault_id: Id,
        item_id: Id,
        push: &ItemPush,
    ) -> Result<PushReply> {
        let body = wire::encode(push).map_err(|e| TransportError::Decode(e.to_string()))?;
        let (status, bytes) = self.send(
            Method::Put,
            &format!("/v1/vaults/{vault_id}/items/{item_id}"),
            Some(token),
            Some(body),
        )?;
        if ((200..300).contains(&status) || status == 409)
            && let Ok(reply) = wire::decode::<PushReply>(&bytes)
        {
            return Ok(reply);
        }
        Err(Self::refused(status, &bytes))
    }

    fn put_blob(
        &self,
        token: &str,
        vault_id: Id,
        hash: Hash32,
        bytes: &[u8],
    ) -> Result<BlobAccepted> {
        let (status, body) = self.send_raw(
            Method::Put,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            Some(bytes.to_vec()),
        )?;
        if (200..300).contains(&status) {
            return wire::decode(&body).map_err(|e| TransportError::Decode(e.to_string()));
        }
        Err(Self::refused(status, &body))
    }

    fn get_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>> {
        let (status, body) = self.send_raw(
            Method::Get,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            None,
        )?;
        if status == 404 {
            return Ok(None);
        }
        if (200..300).contains(&status) {
            return Ok(Some(body));
        }
        Err(Self::refused(status, &body))
    }

    fn kt_sth(&self) -> Result<SthWire> {
        self.call::<(), _>(Method::Get, "/v1/kt/sth", None, None)
    }

    fn kt_inclusion(&self, index: u64, size: u64) -> Result<InclusionProofWire> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/kt/inclusion?index={index}&size={size}"),
            None,
            None,
        )
    }

    fn kt_consistency(&self, from: u64, to: u64) -> Result<ConsistencyProofWire> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/kt/consistency?from={from}&to={to}"),
            None,
            None,
        )
    }

    fn kt_entries(&self, token: &str, from: u64) -> Result<AccountEntries> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/kt/entries?from={from}"),
            Some(token),
            None,
        )
    }

    fn has_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<bool> {
        let (status, body) = self.send_raw(
            Method::Head,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            None,
        )?;
        if status == 404 {
            return Ok(false);
        }
        if (200..300).contains(&status) {
            return Ok(true);
        }
        Err(Self::refused(status, &body))
    }
}
/// The account's alerts (docs/plan/13-security-center.md). Inherent
/// rather than on [`Transport`], so the sync engine's test transports need
/// nothing new.
impl HttpTransport {
    /// `GET /v1/account/alerts?since=N`.
    pub fn account_alerts(&self, token: &str, since: u64) -> Result<wire::AlertsSince> {
        self.call::<(), _>(
            Method::Get,
            &format!("/v1/account/alerts?since={since}"),
            Some(token),
            None,
        )
    }

    /// `GET /v1/account/storage`: the account's attachment room (D234).
    pub fn storage(&self, token: &str) -> Result<wire::StorageUsage> {
        self.call::<(), _>(Method::Get, "/v1/account/storage", Some(token), None)
    }

    /// `POST /v1/account/alerts/{seq}/dismiss`.
    pub fn dismiss_alert(&self, token: &str, seq: u64) -> Result<()> {
        self.call_raw(
            Method::Post,
            &format!("/v1/account/alerts/{seq}/dismiss"),
            Some(token),
            None,
        )
        .map(|_| ())
    }
}

/// The account's sessions and the live channel
/// (docs/plan/20-self-host-server.md §5–§6). Inherent, like the alerts.
impl HttpTransport {
    /// `GET /v1/account/sessions`.
    pub fn sessions(&self, token: &str) -> Result<wire::SessionList> {
        self.call::<(), _>(Method::Get, "/v1/account/sessions", Some(token), None)
    }

    /// `POST /v1/account/sessions/{id}/end`.
    pub fn end_session(&self, token: &str, id: &str) -> Result<()> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(TransportError::Network(format!("{id} isn't a session id")));
        }
        self.call_raw(
            Method::Post,
            &format!("/v1/account/sessions/{id}/end"),
            Some(token),
            None,
        )
        .map(|_| ())
    }

    /// `POST /v1/account/sessions/end-others`: every session of the
    /// account but this one.
    pub fn end_other_sessions(&self, token: &str) -> Result<wire::SessionsEnded> {
        self.call::<(), _>(
            Method::Post,
            "/v1/account/sessions/end-others",
            Some(token),
            None,
        )
    }

    /// `GET /v1/events`: opens the live channel. Each read waits at most
    /// [`EVENTS_READ_TIMEOUT`]; the server writes a keep-alive line every
    /// 20 s, so a silent connection is a dead one and comes back as an
    /// error.
    #[cfg(feature = "http")]
    pub fn events(&self, token: &str) -> Result<EventStream> {
        not_held()?;
        if is_local(&self.base) {
            return Err(TransportError::Network(
                "the vault on this computer has no live channel".into(),
            ));
        }
        let tls = lockdown_tls::client_config(&self.trust)
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let client = reqwest::blocking::Client::builder()
            .timeout(EVENTS_READ_TIMEOUT)
            .connect_timeout(std::time::Duration::from_secs(15))
            .user_agent(user_agent())
            .tls_backend_preconfigured(tls)
            .build()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let res = client
            .get(format!("{}/v1/events", self.base))
            .header(reqwest::header::AUTHORIZATION, format!("{BEARER}{token}"))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let status = res.status().as_u16();
        if !(200..300).contains(&status) {
            let bytes = res
                .bytes()
                .map_err(|e| TransportError::Network(e.to_string()))?;
            return Err(Self::refused(status, &bytes));
        }
        Ok(EventStream {
            reader: std::io::BufReader::new(res),
        })
    }
}

/// How long one read of the live channel waits before the connection is
/// taken for dead (three keep-alives missed).
pub const EVENTS_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(65);

/// The largest event the client reads; a server sending more is cut off.
const MAX_EVENT_BYTES: usize = 16 * 1024;

/// The live channel, open. Read it with [`EventStream::next_event`].
#[cfg(feature = "http")]
pub struct EventStream {
    reader: std::io::BufReader<reqwest::blocking::Response>,
}

#[cfg(feature = "http")]
impl std::fmt::Debug for EventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EventStream")
    }
}

#[cfg(feature = "http")]
impl EventStream {
    /// The next event; `Ok(None)` when the server closed the stream.
    /// Keep-alives are skipped, and so are events of another name or with
    /// data that isn't a [`wire::LiveEvent`].
    pub fn next_event(&mut self) -> Result<Option<wire::LiveEvent>> {
        loop {
            match read_sse(&mut self.reader)? {
                None => return Ok(None),
                Some(SseItem::Event(e)) => return Ok(Some(e)),
                Some(SseItem::KeepAlive) => {}
            }
        }
    }

    /// The next event or keep-alive (about every 20 s), so a reader can
    /// look up between events; `Ok(None)` when the server closed.
    pub fn next_item(&mut self) -> Result<Option<SseItem>> {
        read_sse(&mut self.reader)
    }
}

/// What the live channel delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseItem {
    /// An event.
    Event(wire::LiveEvent),
    /// A keep-alive comment: the connection is alive, nothing changed.
    KeepAlive,
}

/// Reads server-sent events from `reader` until one named `lockdown`
/// carries a [`wire::LiveEvent`].
pub fn next_sse_event(reader: &mut impl std::io::BufRead) -> Result<Option<wire::LiveEvent>> {
    loop {
        match read_sse(reader)? {
            None => return Ok(None),
            Some(SseItem::Event(e)) => return Ok(Some(e)),
            Some(SseItem::KeepAlive) => {}
        }
    }
}

fn read_sse(reader: &mut impl std::io::BufRead) -> Result<Option<SseItem>> {
    let mut name = String::new();
    let mut data = String::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = std::io::Read::take(&mut *reader, MAX_EVENT_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| TransportError::Network(format!("the live channel stopped: {e}")))?;
        if n == 0 {
            return Ok(None);
        }
        if line.len() > MAX_EVENT_BYTES || data.len() > MAX_EVENT_BYTES {
            return Err(TransportError::Decode(
                "the live channel sent an event too large to read".into(),
            ));
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            // A blank line ends an event.
            let event = (name.is_empty() || name == "lockdown")
                .then(|| serde_json::from_str::<wire::LiveEvent>(&data).ok())
                .flatten();
            name.clear();
            data.clear();
            if let Some(event) = event {
                return Ok(Some(SseItem::Event(event)));
            }
            continue;
        }
        if text.starts_with(':') {
            if name.is_empty() && data.is_empty() {
                return Ok(Some(SseItem::KeepAlive));
            }
            continue;
        }
        let (field, value) = text.split_once(':').unwrap_or((text, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => name = value.to_string(),
            "data" => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
            _ => {}
        }
    }
}

/// One look at the server's clock, for code clock-skew correction
/// (docs/plan/23-bridges.md §8): when the request left and the answer came
/// back, by this device's clock in milliseconds, and the answer's `Date`
/// header as the server wrote it. Checked by `lockdown_codes::skew`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockSample {
    /// The `Date` header, when the answer had one.
    pub date: Option<String>,
    /// This device's clock when the request left (ms since the epoch).
    pub sent_ms: u64,
    /// This device's clock when the answer arrived (ms since the epoch).
    pub received_ms: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The server's clock and the monitored addresses (docs/plan/23-bridges.md).
/// Inherent, like the alerts.
impl HttpTransport {
    /// `GET /healthz`, timed, with the answer's `Date` header. Through a
    /// host's hook there are no headers to read, so it says so.
    pub fn clock_sample(&self) -> Result<ClockSample> {
        if HTTP_HOOK.get().is_some() || is_local(&self.base) {
            return Err(TransportError::Network(
                "this build reads no headers, so it can't check the server's clock".into(),
            ));
        }
        #[cfg(feature = "http")]
        {
            let sent_ms = now_ms();
            let res = self
                .client
                .get(format!("{}/healthz", self.base))
                .send()
                .map_err(|e| TransportError::Network(e.to_string()))?;
            let received_ms = now_ms();
            let date = res
                .headers()
                .get(reqwest::header::DATE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            Ok(ClockSample {
                date,
                sent_ms,
                received_ms,
            })
        }
        #[cfg(not(feature = "http"))]
        {
            let _ = now_ms();
            Err(TransportError::Network(
                "this build has no HTTP client and the host installed none".into(),
            ))
        }
    }

    /// `GET /v1/account/monitored`.
    pub fn monitored(&self, token: &str) -> Result<wire::MonitoredAddresses> {
        self.call::<(), _>(Method::Get, "/v1/account/monitored", Some(token), None)
    }

    /// `PUT /v1/account/monitored`: replaces the list.
    pub fn set_monitored(
        &self,
        token: &str,
        list: &wire::MonitoredAddresses,
    ) -> Result<wire::MonitoredAddresses> {
        self.call(
            Method::Put,
            "/v1/account/monitored",
            Some(token),
            Some(list),
        )
    }
}

/// Log mirrors and the server's restore drill
/// (docs/plan/27-web-server-finish.md §3–§4). Inherent, like the alerts.
impl HttpTransport {
    /// `GET /v1/kt/mirror` on a mirror (this transport's base is the
    /// mirror's): the newest head it copied. No session: a mirror is public.
    pub fn kt_mirror_head(&self) -> Result<crate::mirror::MirrorHead> {
        self.call::<(), _>(Method::Get, crate::mirror::MIRROR_PATH, None, None)
    }

    /// `GET /v1/kt/mirrors`: the mirrors the account server names; none
    /// from a server older than mirrors.
    pub fn kt_mirrors(&self) -> Result<crate::mirror::KtMirrors> {
        let (status, body) = self.send(Method::Get, crate::mirror::MIRRORS_PATH, None, None)?;
        if status == 404 {
            return Ok(crate::mirror::KtMirrors::default());
        }
        if (200..300).contains(&status) {
            return wire::decode(&body).map_err(|e| TransportError::Decode(e.to_string()));
        }
        Err(Self::refused(status, &body))
    }

    /// `GET /v1/server/restore-drill`: the server's own restore drills, as
    /// it reports them; `None` from a server older than the endpoint.
    pub fn restore_drill(&self, token: &str) -> Result<Option<wire::RestoreDrill>> {
        let (status, body) =
            self.send(Method::Get, "/v1/server/restore-drill", Some(token), None)?;
        if status == 404 {
            return Ok(None);
        }
        if (200..300).contains(&status) {
            return wire::decode(&body)
                .map(Some)
                .map_err(|e| TransportError::Decode(e.to_string()));
        }
        Err(Self::refused(status, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{LiveEvent, LiveKind};

    #[test]
    fn the_local_base_reaches_only_the_installed_hook_for_this_threads_home() {
        // Before a host installs one, the reserved address is refused.
        if !local_server_installed() {
            assert!(check_base(LOCAL_BASE).is_err());
        }
        install_local_server(Box::new(|home, req| {
            Ok((
                200,
                format!("{}|{}|{}", home.display(), req.method.as_str(), req.url).into_bytes(),
            ))
        }));
        assert!(check_base(LOCAL_BASE).is_ok());
        assert!(is_local("lockdown-local://this-device/"));
        assert!(!is_local("https://lockdown-local.example"));
        let t = HttpTransport::new(LOCAL_BASE).unwrap();
        // No home named on this thread: nothing is sent anywhere.
        let none = std::thread::spawn(move || {
            t.exchange(Method::Get, "/healthz", CONTENT_TYPE, None, None)
                .is_err()
        });
        assert!(none.join().unwrap());
        set_local_home(Some(std::path::PathBuf::from("/tmp/a")));
        let t = HttpTransport::new(LOCAL_BASE).unwrap();
        let (status, body) = t
            .exchange(Method::Post, "/v1/x", CONTENT_TYPE, None, None)
            .unwrap();
        assert_eq!(status, 200);
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "/tmp/a|POST|lockdown-local://this-device/v1/x"
        );
        assert!(t.events("t").is_err());
        set_local_home(None);
    }

    #[test]
    fn only_loopback_may_be_plain_http() {
        for ok in [
            "https://lockdown.example",
            "https://10.0.0.2:8443/",
            "http://127.0.0.1:8787",
            "http://localhost:8787",
            "HTTP://[::1]:8787/",
            "http://127.9.9.9",
        ] {
            assert!(check_base(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://192.168.1.2:8787",
            "http://lockdown.example",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "ftp://127.0.0.1",
            "lockdown.example",
        ] {
            assert!(check_base(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn server_sent_events_parse_and_noise_is_skipped() {
        let stream = b": keep-alive\n\nevent: lockdown\ndata: {\"v\":1,\"kind\":\"sync\"}\n\nevent: other\ndata: {\"v\":1,\"kind\":\"sync\"}\n\nevent: lockdown\ndata: not json\n\nevent: lockdown\r\ndata: {\"v\":1,\"kind\":\"a_kind_from_the_future\"}\r\n\r\nevent: lockdown\ndata: {\"v\":1,\ndata: \"kind\":\"session_ended\"}\n\n";
        let mut reader = std::io::BufReader::new(&stream[..]);
        let mut got = Vec::new();
        while let Some(e) = next_sse_event(&mut reader).unwrap() {
            got.push(e);
        }
        assert_eq!(
            got,
            vec![
                LiveEvent {
                    v: 1,
                    kind: LiveKind::Sync
                },
                LiveEvent {
                    v: 1,
                    kind: LiveKind::Other
                },
                LiveEvent {
                    v: 1,
                    kind: LiveKind::SessionEnded
                },
            ]
        );
        let huge = format!("data: {}\n\n", "x".repeat(MAX_EVENT_BYTES + 10));
        let mut reader = std::io::BufReader::new(huge.as_bytes());
        assert!(next_sse_event(&mut reader).is_err());
    }
}
