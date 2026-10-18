//! The HTTP client the breach check and the change-password probe share,
//! behind [`Fetch`] so a test (or a shell with its own network stack)
//! supplies another.
//!
//! [`HttpFetch`] is blocking `reqwest` over rustls with the `ring`
//! provider, like the sync client. It follows no redirect (the probe needs
//! to see one, and the range API never sends one), keeps no cookies, sends
//! nothing but the URL and the headers a caller names, times out, and caps
//! what it reads.

use std::time::Duration;

/// The most bytes read from one answer. A padded range answer is about
/// 40 kB; a front page we only need the status of.
pub const MAX_BODY: usize = 4 << 20;

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    /// The HTTP status.
    pub status: u16,
    /// The body, at most [`MAX_BODY`] bytes.
    pub body: Vec<u8>,
}

/// What can go wrong on the way.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The address isn't one this client will call.
    #[error("{0}")]
    BadAddress(String),
    /// No answer, or not an HTTP one.
    #[error("couldn't reach {host}: {why}")]
    Network {
        /// The host, never the full address.
        host: String,
        /// What the network stack said.
        why: String,
    },
    /// The answer was larger than [`MAX_BODY`].
    #[error("{0} sent more than expected, so it was ignored")]
    TooLarge(String),
}

/// One GET. Implementations send `url` and `headers` and nothing else.
pub trait Fetch: Send + Sync {
    /// Fetches `url` with `headers`, following no redirect.
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Fetched, FetchError>;
}

/// The host of `url`, for an error message that never repeats a path.
pub(crate) fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or("the server")
        .to_string()
}

/// Whether `url` may be called: `https://`, or `http://` to this machine
/// only (a local mirror or a test server).
pub fn allowed_address(url: &str) -> Result<(), FetchError> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
        if matches!(name, "localhost" | "127.0.0.1" | "[::1]") {
            return Ok(());
        }
    }
    Err(FetchError::BadAddress(format!(
        "{} isn't an https address, so Lockdown won't call it",
        host_of(url)
    )))
}

/// Installs rustls's `ring` provider with the hybrid X25519MLKEM768 group
/// first, once per process (harmless when the sync client already did).
fn ensure_tls_provider() {
    lockdown_tls::install_default_provider();
}

/// The real client.
pub struct HttpFetch {
    client: reqwest::blocking::Client,
}

impl std::fmt::Debug for HttpFetch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpFetch")
    }
}

impl HttpFetch {
    /// A client with a `timeout` per call.
    pub fn new(timeout: Duration) -> Result<Self, FetchError> {
        ensure_tls_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("lockdown/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| FetchError::Network {
                host: "the network".into(),
                why: e.to_string(),
            })?;
        Ok(Self { client })
    }
}

impl Fetch for HttpFetch {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Fetched, FetchError> {
        allowed_address(url)?;
        let host = host_of(url);
        let mut req = self.client.get(url);
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let res = req.send().map_err(|e| FetchError::Network {
            host: host.clone(),
            // reqwest's message names the URL; keep only its kind.
            why: if e.is_timeout() {
                "it took too long".into()
            } else if e.is_connect() {
                "the connection failed".into()
            } else {
                "the request failed".into()
            },
        })?;
        let status = res.status().as_u16();
        if res.content_length().is_some_and(|n| n > MAX_BODY as u64) {
            return Err(FetchError::TooLarge(host));
        }
        let body = res.bytes().map_err(|_| FetchError::Network {
            host: host.clone(),
            why: "the answer was cut off".into(),
        })?;
        if body.len() > MAX_BODY {
            return Err(FetchError::TooLarge(host));
        }
        Ok(Fetched {
            status,
            body: body.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_machine_is_called() {
        assert!(allowed_address("https://api.pwnedpasswords.com/range/ABCDE").is_ok());
        assert!(allowed_address("http://127.0.0.1:8080/range/ABCDE").is_ok());
        assert!(allowed_address("http://localhost/range/").is_ok());
        for bad in [
            "http://example.com/range/",
            "ftp://example.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "",
        ] {
            assert!(allowed_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn errors_name_the_host_never_the_path() {
        assert_eq!(
            host_of("https://example.com/range/ABCDE?x=1"),
            "example.com"
        );
        let e = allowed_address("http://evil.example/secret/path").unwrap_err();
        assert!(!e.to_string().contains("secret"), "{e}");
    }
}
