//! TLS on the server itself (docs/plan/20-self-host-server.md §2–§3):
//! `--tls-cert` and `--tls-key` turn it on; the key exchange is hybrid
//! X25519MLKEM768 first (`lockdown-tls`), TLS 1.3 only.
//!
//! - [`TlsListener`] accepts TCP, runs each handshake on its own task with a
//!   time limit (so a slow client holds up nobody), counts which group each
//!   handshake used ([`TlsStats`]), and hands finished connections to
//!   axum's server loop.
//! - [`spawn_reloader`] swaps in a renewed certificate on SIGHUP and when
//!   the files change (checked every `--tls-reload-secs`).
//! - [`hsts`] adds `Strict-Transport-Security` to every response over TLS.
//! - [`redirect_app`] answers plain HTTP with a permanent redirect to the
//!   same path over HTTPS (`--redirect-http`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use lockdown_tls::ReloadingCert;
use rustls::ServerConfig;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

/// How long a handshake may take.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Handshakes running at once, at most.
pub const MAX_HANDSHAKES: usize = 512;

/// What the handshakes negotiated since the server started.
#[derive(Debug, Default)]
pub struct TlsStats {
    hybrid: AtomicU64,
    classical: AtomicU64,
    failed: AtomicU64,
}

/// A copy of [`TlsStats`] at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TlsCounts {
    /// Handshakes on X25519MLKEM768.
    pub hybrid: u64,
    /// Handshakes on a classical group (a client without the hybrid one).
    pub classical: u64,
    /// Handshakes that failed or timed out.
    pub failed: u64,
}

impl TlsStats {
    /// The counts now.
    pub fn counts(&self) -> TlsCounts {
        TlsCounts {
            hybrid: self.hybrid.load(Ordering::Relaxed),
            classical: self.classical.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
        }
    }
}

/// A listener that hands axum finished TLS connections.
pub struct TlsListener {
    rx: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    local: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for TlsListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TlsListener({})", self.local)
    }
}

impl Drop for TlsListener {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl TlsListener {
    /// Starts accepting on `tcp` with `config`; needs a Tokio runtime.
    pub fn new(
        tcp: TcpListener,
        config: Arc<ServerConfig>,
        stats: Arc<TlsStats>,
    ) -> std::io::Result<Self> {
        let local = tcp.local_addr()?;
        let (tx, rx) = mpsc::channel(64);
        let acceptor = TlsAcceptor::from(config);
        let permits = Arc::new(Semaphore::new(MAX_HANDSHAKES));
        let task = tokio::spawn(async move {
            loop {
                let (sock, addr) = match tcp.accept().await {
                    Ok(pair) => pair,
                    Err(_) => {
                        // Out of file descriptors and the like: back off.
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    return;
                };
                let (acceptor, tx, stats) = (acceptor.clone(), tx.clone(), stats.clone());
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = sock.set_nodelay(true);
                    match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(sock)).await {
                        Ok(Ok(stream)) => {
                            let group = stream
                                .get_ref()
                                .1
                                .negotiated_key_exchange_group()
                                .map(|g| g.name());
                            if group.is_some_and(lockdown_tls::is_hybrid) {
                                stats.hybrid.fetch_add(1, Ordering::Relaxed);
                            } else {
                                stats.classical.fetch_add(1, Ordering::Relaxed);
                            }
                            let _ = tx.send((stream, addr)).await;
                        }
                        _ => {
                            stats.failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        Ok(Self { rx, local, task })
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(pair) => pair,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

/// Reloads `cert` on SIGHUP (Unix) and, with `every`, when its files
/// change. A bad renewal is reported and the pair in use stays.
pub fn spawn_reloader(cert: Arc<ReloadingCert>, every: Option<Duration>) {
    #[cfg(unix)]
    {
        let cert = cert.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let Ok(mut hup) = signal(SignalKind::hangup()) else {
                eprintln!("lockdown-server: SIGHUP can't be watched; certificates reload on change only");
                return;
            };
            while hup.recv().await.is_some() {
                match cert.reload() {
                    Ok(()) => eprintln!("lockdown-server: certificate reloaded (SIGHUP)"),
                    Err(e) => eprintln!(
                        "lockdown-server: the certificate wasn't reloaded, the old one stays: {e}"
                    ),
                }
            }
        });
    }
    if let Some(every) = every.filter(|d| !d.is_zero()) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.tick().await;
            loop {
                tick.tick().await;
                match cert.reload_if_changed() {
                    Ok(true) => eprintln!("lockdown-server: certificate reloaded (the files changed)"),
                    Ok(false) => {}
                    Err(e) => eprintln!(
                        "lockdown-server: the certificate files changed but didn't load, the old one stays: {e}"
                    ),
                }
            }
        });
    }
}

/// The `Strict-Transport-Security` value for `max_age` seconds.
pub fn hsts_value(max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!("max-age={max_age}"))
        .unwrap_or_else(|_| HeaderValue::from_static("max-age=31536000"))
}

/// Adds `Strict-Transport-Security` to every response: once a browser has
/// reached the server over HTTPS it never tries plain HTTP for it again.
pub async fn hsts(value: HeaderValue, req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(header::STRICT_TRANSPORT_SECURITY, value);
    response
}

/// Whether `host` is a plain host name or address (no user part, path or
/// other surprise), as a redirect target may be.
fn safe_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

/// The HTTPS address for a plain-HTTP request to `host` and `uri`, with the
/// TLS listener on `https_port`.
pub fn redirect_target(host: Option<&str>, uri: &Uri, https_port: u16) -> Option<String> {
    let host = host?;
    if !safe_host(host) {
        return None;
    }
    // Drop the plain port; keep an IPv6 literal whole.
    let name = if let Some(rest) = host.strip_prefix('[') {
        format!("[{}]", rest.split(']').next()?)
    } else {
        host.rsplit_once(':')
            .map_or(host, |(h, _)| h)
            .to_string()
    };
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    Some(if https_port == 443 {
        format!("https://{name}{path}")
    } else {
        format!("https://{name}:{https_port}{path}")
    })
}

/// The plain-HTTP app: every request is sent to HTTPS (308 keeps the
/// method and body), with `/healthz` answered in place for load balancers.
pub fn redirect_app(https_port: u16) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .fallback(move |req: Request| async move {
            let host = req
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok());
            match redirect_target(host, req.uri(), https_port) {
                Some(to) => match HeaderValue::from_str(&to) {
                    Ok(v) => (StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, v)]).into_response(),
                    Err(_) => (StatusCode::BAD_REQUEST, "use https").into_response(),
                },
                None => (StatusCode::BAD_REQUEST, "use https").into_response(),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_go_to_the_same_path_over_https_and_refuse_odd_hosts() {
        let uri: Uri = "/v1/vaults?since=3".parse().unwrap();
        assert_eq!(
            redirect_target(Some("lockdown.example"), &uri, 443).as_deref(),
            Some("https://lockdown.example/v1/vaults?since=3")
        );
        assert_eq!(
            redirect_target(Some("lockdown.example:80"), &uri, 8443).as_deref(),
            Some("https://lockdown.example:8443/v1/vaults?since=3")
        );
        assert_eq!(
            redirect_target(Some("[::1]:8080"), &"/".parse().unwrap(), 8443).as_deref(),
            Some("https://[::1]:8443/")
        );
        assert_eq!(redirect_target(None, &uri, 443), None);
        assert_eq!(redirect_target(Some("evil.example/x"), &uri, 443), None);
        assert_eq!(redirect_target(Some("a@b"), &uri, 443), None);
        assert_eq!(redirect_target(Some(""), &uri, 443), None);
    }
}
