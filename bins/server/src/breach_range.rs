//! A breach feed the operator supplies (`email-breach-monitoring`;
//! docs/plan/25-self-host-2.md §7): monitoring without a paid key.
//!
//! Addresses are looked up by **k-anonymity**, the way breached passwords
//! already are (D125): the address is lower-cased, hashed with SHA-1, and
//! only the first [`PREFIX_LEN`] hex digits of the hash leave the server.
//! The source answers with every hash in that range and the breaches each
//! is in; the server compares the rest of the hash itself. A range holds
//! many addresses, so the source can't tell which one was asked about.
//!
//! Two sources:
//!
//! - **A range file** (`--breach-range /path/file`): the operator's own
//!   dataset, read on this machine, so nothing leaves it at all. Each line
//!   is `<40 hex digits of SHA-1(address)>:<Breach>[;<Breach>…]`; blank
//!   lines and lines starting with `#` are skipped. The file is read again
//!   when it changes.
//! - **A range service** (`--breach-range https://…`): `GET <base>/<prefix>`
//!   answers `text/plain` lines `<the other 34 hex digits>:<Breach>[;…]`,
//!   `404` for an empty range. `https://`, or `http://` to this machine.
//!
//! Either way an optional **catalog** (`--breach-catalog FILE`, the JSON
//! list Have I Been Pwned publishes free at `/api/v3/breaches`:
//! `[{"Name", "Domain", "BreachDate", "DataClasses"}]`) gives each breach
//! its site, date and what was taken; without it an alert names the breach
//! only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use lockdown_sync::wire::BreachNotice;
use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::breach_monitor::BreachFeed;

/// How many hex digits of the hash are sent to a range service.
pub const PREFIX_LEN: usize = 6;

/// The largest range answer or range file read.
const MAX_BYTES: u64 = 256 << 20;

/// An address's hash: SHA-1 of the trimmed, lower-cased address, upper-case hex.
pub fn address_hash(address: &str) -> String {
    data_encoding::HEXUPPER.encode(&Sha1::digest(address.trim().to_lowercase().as_bytes()))
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct CatalogEntry {
    name: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    breach_date: Option<String>,
    #[serde(default)]
    data_classes: Vec<String>,
}

/// Breach names to their details.
#[derive(Clone, Debug, Default)]
pub struct Catalog(BTreeMap<String, CatalogEntry>);

impl Catalog {
    /// Reads a catalog file (HIBP's breach list shape).
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("couldn't read the breach catalog {}: {e}", path.display()))?;
        let list: Vec<CatalogEntry> = serde_json::from_slice(&bytes)
            .map_err(|_| "the breach catalog isn't a JSON list of breaches".to_string())?;
        Ok(Self(
            list.into_iter()
                .map(|e| (e.name.to_lowercase(), e))
                .collect(),
        ))
    }

    fn notice(&self, address: &str, name: &str) -> BreachNotice {
        match self.0.get(&name.to_lowercase()) {
            Some(e) => BreachNotice {
                address: address.to_string(),
                name: e.name.clone(),
                domain: e.domain.clone().filter(|d| !d.is_empty()),
                breach_date: e.breach_date.clone(),
                data: e.data_classes.clone(),
            },
            None => BreachNotice {
                address: address.to_string(),
                name: name.to_string(),
                domain: None,
                breach_date: None,
                data: Vec::new(),
            },
        }
    }
}

/// `SUFFIX:Name;Name` lines to (suffix, names); anything else is skipped.
fn parse_lines(text: &str, hash_len: usize) -> Vec<(String, Vec<String>)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|line| {
            let (hash, names) = line.split_once(':')?;
            let hash = hash.trim().to_ascii_uppercase();
            if hash.len() != hash_len || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let names: Vec<String> = names
                .split(';')
                .map(|n| {
                    n.trim()
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(128)
                        .collect::<String>()
                })
                .filter(|n| !n.is_empty())
                .collect();
            (!names.is_empty()).then_some((hash, names))
        })
        .collect()
}

struct FileState {
    stamp: Option<(SystemTime, u64)>,
    by_hash: BTreeMap<String, Vec<String>>,
}

enum Source {
    File {
        path: PathBuf,
        state: Mutex<FileState>,
    },
    Service {
        base: String,
        client: reqwest::blocking::Client,
    },
}

/// The operator's breach feed, looked up by hash range.
pub struct RangeBreachFeed {
    source: Source,
    catalog: Catalog,
}

impl std::fmt::Debug for RangeBreachFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Source::File { path, .. } => write!(f, "RangeBreachFeed(file {})", path.display()),
            Source::Service { base, .. } => write!(f, "RangeBreachFeed({base})"),
        }
    }
}

impl RangeBreachFeed {
    /// A feed over `source`: an `https://` (or local `http://`) range
    /// service, else a range file's path.
    pub fn new(source: &str, catalog: Catalog) -> Result<Self, String> {
        let source = source.trim();
        if source.starts_with("https://") || source.starts_with("http://") {
            let local =
                source.starts_with("http://127.0.0.1") || source.starts_with("http://localhost");
            if source.starts_with("http://") && !local {
                return Err("the breach range service must be an https address".into());
            }
            lockdown_tls::install_default_provider();
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("lockdown-server/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|e| e.to_string())?;
            return Ok(Self {
                source: Source::Service {
                    base: source.trim_end_matches('/').to_string(),
                    client,
                },
                catalog,
            });
        }
        let feed = Self {
            source: Source::File {
                path: PathBuf::from(source),
                state: Mutex::new(FileState {
                    stamp: None,
                    by_hash: BTreeMap::new(),
                }),
            },
            catalog,
        };
        // Read it now, so a wrong path is found at start.
        if let Source::File { path, state } = &feed.source {
            let mut state = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            refresh(path, &mut state)?;
        }
        Ok(feed)
    }

    /// Where the feed looks, for the operator (no secrets in it).
    pub fn describe(&self) -> String {
        match &self.source {
            Source::File { path, .. } => format!("range file {}", path.display()),
            Source::Service { base, .. } => format!("range service {base}"),
        }
    }
}

fn refresh(path: &Path, state: &mut FileState) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| {
        format!(
            "couldn't read the breach range file {}: {e}",
            path.display()
        )
    })?;
    if meta.len() > MAX_BYTES {
        return Err("the breach range file is larger than 256 MiB".into());
    }
    let stamp = (
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        meta.len(),
    );
    if state.stamp == Some(stamp) {
        return Ok(());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|_| "the breach range file isn't readable text".to_string())?;
    state.by_hash = parse_lines(&text, 40).into_iter().collect();
    state.stamp = Some(stamp);
    Ok(())
}

impl BreachFeed for RangeBreachFeed {
    fn breaches_for(&self, address: &str) -> Result<Vec<BreachNotice>, String> {
        let hash = address_hash(address);
        let names: Vec<String> = match &self.source {
            Source::File { path, state } => {
                let mut state = state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                refresh(path, &mut state)?;
                state.by_hash.get(&hash).cloned().unwrap_or_default()
            }
            Source::Service { base, client } => {
                let (prefix, suffix) = hash.split_at(PREFIX_LEN);
                let res = client
                    .get(format!("{base}/{prefix}"))
                    .send()
                    .map_err(|_| "the breach range service didn't answer".to_string())?;
                match res.status().as_u16() {
                    404 => Vec::new(),
                    200 => {
                        if res.content_length().is_some_and(|n| n > MAX_BYTES) {
                            return Err("the breach range answer is too large".into());
                        }
                        let text = res
                            .text()
                            .map_err(|_| "the breach range answer was cut off".to_string())?;
                        parse_lines(&text, 40 - PREFIX_LEN)
                            .into_iter()
                            .find(|(s, _)| s == suffix)
                            .map(|(_, n)| n)
                            .unwrap_or_default()
                    }
                    429 => return Err("the breach range service asked us to slow down".into()),
                    status => return Err(format!("the breach range service answered {status}")),
                }
            }
        };
        let mut names = names;
        names.sort();
        names.dedup();
        Ok(names
            .iter()
            .map(|n| self.catalog.notice(address, n))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::Arc;

    #[test]
    fn the_hash_is_of_the_lowercased_address() {
        // SHA-1("test@example.com").
        assert_eq!(
            address_hash("  Test@Example.com "),
            "567159D622FFBB50B11B0EFD307BE358624A26EE"
        );
    }

    fn catalog(dir: &Path) -> Catalog {
        let path = dir.join("breaches.json");
        std::fs::write(
            &path,
            r#"[{"Name":"ExampleShop","Domain":"shop.example","BreachDate":"2026-09-01","DataClasses":["Email addresses","Passwords"],"IsVerified":true}]"#,
        )
        .unwrap();
        Catalog::load(&path).unwrap()
    }

    #[test]
    fn a_range_file_finds_an_address_and_is_read_again_when_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("range.txt");
        let sean = address_hash("sean@example.com");
        std::fs::write(
            &file,
            format!("# operator's dataset\n{sean}:ExampleShop;OtherLeak;ExampleShop\nnot a line\n"),
        )
        .unwrap();
        let feed = RangeBreachFeed::new(file.to_str().unwrap(), catalog(dir.path())).unwrap();
        let found = feed.breaches_for("Sean@Example.com").unwrap();
        assert_eq!(
            found.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["ExampleShop", "OtherLeak"]
        );
        assert_eq!(found[0].domain.as_deref(), Some("shop.example"));
        assert_eq!(found[0].data, ["Email addresses", "Passwords"]);
        assert!(
            found[1].domain.is_none(),
            "not in the catalog: the name only"
        );
        assert!(feed.breaches_for("nobody@example.com").unwrap().is_empty());
        let nobody = address_hash("nobody@example.com");
        std::fs::write(&file, format!("{nobody}:Third\n")).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(
            feed.breaches_for("nobody@example.com").unwrap()[0].name,
            "Third"
        );
        assert!(feed.breaches_for("sean@example.com").unwrap().is_empty());
        assert!(RangeBreachFeed::new("/no/such/file", Catalog::default()).is_err());
        assert!(RangeBreachFeed::new("http://feed.example/range", Catalog::default()).is_err());
    }

    #[test]
    fn a_range_service_is_sent_the_prefix_only() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = seen.clone();
        let sean = address_hash("sean@example.com");
        let (prefix, suffix) = sean.split_at(PREFIX_LEN);
        let body = format!("{suffix}:ExampleShop\r\n{}:Decoy\r\n", "0".repeat(34));
        let prefix_owned = prefix.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                }
                log.lock().unwrap().push(first.clone());
                let (status, body) = if first.contains(&format!("/range/{prefix_owned} ")) {
                    ("200 OK", body.clone())
                } else {
                    ("404 Not Found", String::new())
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let feed =
            RangeBreachFeed::new(&format!("http://{addr}/range/"), catalog(dir.path())).unwrap();
        let found = feed.breaches_for("sean@example.com").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "ExampleShop");
        assert_eq!(found[0].breach_date.as_deref(), Some("2026-09-01"));
        assert!(feed.breaches_for("nobody@example.com").unwrap().is_empty());
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], format!("GET /range/{prefix} HTTP/1.1\r\n"));
        for line in seen.iter() {
            assert!(!line.contains('@') && !line.contains(suffix), "{line}");
        }
    }
}
