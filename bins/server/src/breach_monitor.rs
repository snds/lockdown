//! Email breach monitoring (docs/plan/13-security-center.md): a job that
//! asks a breach feed about each account's email address and records an
//! account alert ([`crate::alerts`]) for every breach it hasn't reported
//! to that account before.
//!
//! The feed is injectable ([`BreachFeed`]). The one built in,
//! [`HttpBreachFeed`], speaks the Have I Been Pwned v3 `breachedaccount`
//! shape, which **needs a paid API key**: the job is off unless the
//! server is started with `--breach-feed-url` (and, for that service,
//! `--breach-feed-key`). With no feed configured nothing is sent anywhere.
//!
//! What leaves the server: the account's login name, when it is an email
//! address, to the configured feed, which is how such feeds work (there
//! is no public k-anonymity lookup for addresses). Nothing from any vault
//! is involved; the server has none of it to send. Aliases are not
//! monitored yet, because Lockdown has no aliases yet.

use std::sync::Arc;
use std::time::Duration;

use lockdown_sync::wire::{AlertKind, BreachNotice};
use serde::Deserialize;

use crate::AppState;
use crate::alerts::AlertStore;

/// The breaches a feed knows for one address.
pub trait BreachFeed: Send + Sync {
    /// Every breach `address` is in (empty when none).
    fn breaches_for(&self, address: &str) -> Result<Vec<BreachNotice>, String>;
}

/// Whether a login name is an email address worth asking about.
pub fn looks_like_email(login: &str) -> bool {
    let Some((local, domain)) = login.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !login.chars().any(char::is_whitespace)
}

/// What one pass did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Accounts whose address was asked about.
    pub checked: usize,
    /// Alerts recorded.
    pub alerts: usize,
    /// Accounts the feed failed for (they are asked again next pass).
    pub failed: usize,
}

/// One pass over every account. `pause` is waited between accounts, to
/// keep under the feed's rate limit.
pub fn scan_once(
    store: &dyn AlertStore,
    feed: &dyn BreachFeed,
    now: i64,
    pause: Duration,
) -> Result<ScanReport, String> {
    let mut report = ScanReport::default();
    let accounts = store.all_accounts().map_err(|e| e.to_string())?;
    for (n, account) in accounts.iter().enumerate() {
        if !looks_like_email(&account.login) {
            continue;
        }
        if n > 0 && !pause.is_zero() {
            std::thread::sleep(pause);
        }
        report.checked += 1;
        let breaches = match feed.breaches_for(&account.login) {
            Ok(b) => b,
            Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        for breach in breaches {
            let seen = store
                .breach_seen(account.account_id, &breach.name)
                .map_err(|e| e.to_string())?;
            if seen {
                continue;
            }
            store
                .record_alert(
                    account.account_id,
                    AlertKind::EmailBreach,
                    now,
                    None,
                    Some(&breach),
                )
                .map_err(|e| e.to_string())?;
            store
                .mark_breach_seen(account.account_id, &breach.name)
                .map_err(|e| e.to_string())?;
            report.alerts += 1;
        }
    }
    Ok(report)
}

/// Runs [`scan_once`] every `every` on a thread of its own, for as long as
/// the process lives. Errors are counted, never logged with an address.
pub fn spawn(
    state: Arc<AppState>,
    feed: Box<dyn BreachFeed>,
    every: Duration,
    pause: Duration,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("breach-monitor".into())
        .spawn(move || {
            loop {
                if let Some(store) = state.storage.alerts() {
                    match scan_once(store, feed.as_ref(), state.now(), pause) {
                        Ok(r) => eprintln!(
                            "lockdown-server: breach check of {} accounts, {} new alerts, {} to retry",
                            r.checked, r.alerts, r.failed
                        ),
                        Err(_) => eprintln!("lockdown-server: the breach check couldn't read the accounts"),
                    }
                }
                std::thread::sleep(every);
            }
        })
}

/// One breach as the HIBP v3 API writes it.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HibpBreach {
    name: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    breach_date: Option<String>,
    #[serde(default)]
    data_classes: Vec<String>,
}

/// A feed that speaks the HIBP v3 `breachedaccount` shape:
/// `GET <endpoint><address>?truncateResponse=false` with the key in the
/// `hibp-api-key` header; 404 means no breach.
pub struct HttpBreachFeed {
    endpoint: String,
    key: Option<String>,
    client: reqwest::blocking::Client,
}

impl std::fmt::Debug for HttpBreachFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HttpBreachFeed({})", self.endpoint)
    }
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn encode_path_part(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl HttpBreachFeed {
    /// A feed at `endpoint` (the address is appended) with `key`. Only
    /// https, or http to this machine for a local mirror or a test.
    pub fn new(endpoint: &str, key: Option<String>) -> Result<Self, String> {
        let endpoint = endpoint.trim();
        let local =
            endpoint.starts_with("http://127.0.0.1") || endpoint.starts_with("http://localhost");
        if !endpoint.starts_with("https://") && !local {
            return Err("the breach feed must be an https address".into());
        }
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("lockdown-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            endpoint: if endpoint.ends_with('/') {
                endpoint.to_string()
            } else {
                format!("{endpoint}/")
            },
            key,
            client,
        })
    }
}

impl BreachFeed for HttpBreachFeed {
    fn breaches_for(&self, address: &str) -> Result<Vec<BreachNotice>, String> {
        let url = format!(
            "{}{}?truncateResponse=false",
            self.endpoint,
            encode_path_part(address)
        );
        let mut req = self.client.get(url);
        if let Some(key) = &self.key {
            req = req.header("hibp-api-key", key);
        }
        let res = req
            .send()
            .map_err(|_| "the breach feed didn't answer".to_string())?;
        match res.status().as_u16() {
            404 => Ok(Vec::new()),
            200 => {
                let bytes = res
                    .bytes()
                    .map_err(|_| "the breach feed's answer was cut off".to_string())?;
                let list: Vec<HibpBreach> = serde_json::from_slice(&bytes)
                    .map_err(|_| "the breach feed's answer didn't read".to_string())?;
                Ok(list
                    .into_iter()
                    .map(|b| BreachNotice {
                        address: address.to_string(),
                        name: b.name,
                        domain: b.domain.filter(|d| !d.is_empty()),
                        breach_date: b.breach_date,
                        data: b.data_classes,
                    })
                    .collect())
            }
            401 | 403 => Err("the breach feed refused the key".into()),
            429 => Err("the breach feed asked us to slow down".into()),
            status => Err(format!("the breach feed answered {status}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{SqliteStorage, Storage};
    use lockdown_vault::Id;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::Mutex;

    struct Fake {
        asked: Mutex<Vec<String>>,
    }

    impl BreachFeed for Fake {
        fn breaches_for(&self, address: &str) -> Result<Vec<BreachNotice>, String> {
            self.asked.lock().unwrap().push(address.to_string());
            if address.starts_with("down") {
                return Err("down".into());
            }
            Ok(vec![BreachNotice {
                address: address.into(),
                name: "ExampleShop".into(),
                domain: Some("shop.example".into()),
                breach_date: Some("2026-09-01".into()),
                data: vec!["Email addresses".into()],
            }])
        }
    }

    #[test]
    fn each_breach_is_reported_once_and_only_email_logins_are_asked() {
        let s = SqliteStorage::open_in_memory().unwrap();
        let sean = s
            .create_account("sean@example.com", Id::new(), b"pf")
            .unwrap();
        s.create_account("just-a-name", Id::new(), b"pf").unwrap();
        s.create_account("down@example.com", Id::new(), b"pf")
            .unwrap();
        let device = Id::new();
        s.register_device(sean.account_id, device, &[1; 32])
            .unwrap();
        let feed = Fake {
            asked: Mutex::new(Vec::new()),
        };
        let r = scan_once(&s, &feed, 100, Duration::ZERO).unwrap();
        assert_eq!(
            r,
            ScanReport {
                checked: 2,
                alerts: 1,
                failed: 1
            }
        );
        assert_eq!(
            *feed.asked.lock().unwrap(),
            vec![
                "sean@example.com".to_string(),
                "down@example.com".to_string()
            ]
        );
        let again = scan_once(&s, &feed, 200, Duration::ZERO).unwrap();
        assert_eq!(again.alerts, 0, "the same breach isn't reported twice");
        let alerts = s.alerts_for(sean.account_id, device, 0).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::EmailBreach);
        assert_eq!(alerts[0].at, 100);
    }

    #[test]
    fn email_addresses_are_told_from_login_names() {
        for yes in ["a@b.co", "sean.sands+lockdown@example.co.uk"] {
            assert!(looks_like_email(yes), "{yes}");
        }
        for no in [
            "sean",
            "@example.com",
            "a@b",
            "a@@b.com",
            "a @b.com",
            "a@.com",
            "a@b.com.",
        ] {
            assert!(!looks_like_email(no), "{no}");
        }
        assert_eq!(encode_path_part("a+b@c.d"), "a%2Bb%40c.d");
    }

    #[test]
    fn the_http_feed_speaks_the_hibp_shape_against_a_local_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(3) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                log.lock().unwrap().push(head.clone());
                let (status, body) = if !head.contains("hibp-api-key: k3y") {
                    ("401 No", String::new())
                } else if head.contains("/nobody%40example.com") {
                    ("404 Not Found", String::new())
                } else {
                    (
                        "200 OK",
                        r#"[{"Name":"Adobe","Domain":"adobe.com","BreachDate":"2013-10-04","DataClasses":["Email addresses","Passwords"],"IsVerified":true}]"#.to_string(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let base = format!("http://{addr}/api/v3/breachedaccount");
        let feed = HttpBreachFeed::new(&base, Some("k3y".into())).unwrap();
        let found = feed.breaches_for("sean@example.com").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Adobe");
        assert_eq!(found[0].data, vec!["Email addresses", "Passwords"]);
        assert!(feed.breaches_for("nobody@example.com").unwrap().is_empty());
        let no_key = HttpBreachFeed::new(&base, None).unwrap();
        assert!(
            no_key
                .breaches_for("sean@example.com")
                .unwrap_err()
                .contains("key")
        );
        let seen = seen.lock().unwrap();
        assert!(
            seen[0].starts_with(
                "GET /api/v3/breachedaccount/sean%40example.com?truncateResponse=false "
            )
        );
        assert!(HttpBreachFeed::new("http://feed.example/", None).is_err());
    }
}
