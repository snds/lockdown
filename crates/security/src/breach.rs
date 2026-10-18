//! The breached-password check, by k-anonymity.
//!
//! For each password the device works out its SHA-1 (forty hex
//! characters) and sends **only the first five** to a range source. The
//! source answers with every known suffix under that prefix and how often
//! it was seen; the device looks for its own suffix in the answer. The
//! source learns a five-character prefix shared by hundreds of thousands
//! of passwords, never the password and never its full hash. Asking with
//! `Add-Padding: true` makes the public API pad every answer with decoy
//! suffixes counted zero, so even the answer's size says nothing; decoys
//! are ignored here.
//!
//! Sources ([`RangeSource`]):
//!
//! - [`HttpRange`]: the public Pwned Passwords range API by default
//!   ([`DEFAULT_RANGE_ENDPOINT`]), or any endpoint that answers the same
//!   way (a self-hosted mirror); the prefix is appended to the address.
//! - [`DirRange`]: a directory of range files named `<PREFIX>.txt` (the
//!   layout the public downloader writes), for a computer that is never
//!   online. The address is `file:///path/to/dir`.
//!
//! One request per distinct prefix, however many passwords share it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

use crate::http::{Fetch, FetchError, allowed_address, host_of};

/// The public Pwned Passwords range API.
pub const DEFAULT_RANGE_ENDPOINT: &str = "https://api.pwnedpasswords.com/range/";

/// How many hex characters of the hash leave the device.
pub const PREFIX_LEN: usize = 5;

/// What can go wrong.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BreachError {
    /// The configured source isn't usable.
    #[error("{0}")]
    BadSource(String),
    /// The source didn't answer.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// The source answered with an error status.
    #[error("{host} answered {status}, so this check didn't finish")]
    Status {
        /// The host.
        host: String,
        /// The HTTP status.
        status: u16,
    },
    /// A range file was missing or unreadable.
    #[error("the breach list on this computer has no file for one of the checks ({0})")]
    MissingFile(String),
}

/// Where ranges come from.
pub trait RangeSource: Send + Sync {
    /// The range for a five-character upper-case hex `prefix`: lines of
    /// `SUFFIX:COUNT`.
    fn range(&self, prefix: &str) -> Result<String, BreachError>;
    /// The source in a few words, for "Checked against …".
    fn describe(&self) -> String;
}

/// A range API over HTTPS.
pub struct HttpRange {
    endpoint: String,
    fetch: Box<dyn Fetch>,
}

impl HttpRange {
    /// A source at `endpoint` (the prefix is appended) through `fetch`.
    pub fn new(endpoint: &str, fetch: Box<dyn Fetch>) -> Result<Self, BreachError> {
        let endpoint = endpoint.trim();
        allowed_address(endpoint).map_err(|e| BreachError::BadSource(e.to_string()))?;
        let endpoint = if endpoint.ends_with('/') {
            endpoint.to_string()
        } else {
            format!("{endpoint}/")
        };
        Ok(Self { endpoint, fetch })
    }
}

impl RangeSource for HttpRange {
    fn range(&self, prefix: &str) -> Result<String, BreachError> {
        check_prefix(prefix)?;
        let url = format!("{}{prefix}", self.endpoint);
        let got = self.fetch.get(&url, &[("Add-Padding", "true")])?;
        if got.status != 200 {
            return Err(BreachError::Status {
                host: host_of(&url),
                status: got.status,
            });
        }
        Ok(String::from_utf8_lossy(&got.body).into_owned())
    }

    fn describe(&self) -> String {
        host_of(&self.endpoint)
    }
}

/// Range files on this computer.
#[derive(Clone, Debug)]
pub struct DirRange {
    dir: PathBuf,
}

impl DirRange {
    /// Files in `dir`, one `<PREFIX>.txt` per prefix.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, BreachError> {
        let dir = dir.into();
        if !dir.is_dir() {
            return Err(BreachError::BadSource(format!(
                "{} isn't a folder of breach files",
                dir.display()
            )));
        }
        Ok(Self { dir })
    }
}

impl RangeSource for DirRange {
    fn range(&self, prefix: &str) -> Result<String, BreachError> {
        check_prefix(prefix)?;
        std::fs::read_to_string(self.dir.join(format!("{prefix}.txt")))
            .map_err(|_| BreachError::MissingFile(prefix.to_string()))
    }

    fn describe(&self) -> String {
        "the breach list on this computer".into()
    }
}

/// The source an address names: `file:///…` is a folder of range files,
/// anything else an HTTPS range API reached through `fetch`.
pub fn source_for(
    endpoint: &str,
    fetch: Box<dyn Fetch>,
) -> Result<Box<dyn RangeSource>, BreachError> {
    let endpoint = endpoint.trim();
    if let Some(path) = endpoint.strip_prefix("file://") {
        return Ok(Box::new(DirRange::new(path)?));
    }
    Ok(Box::new(HttpRange::new(endpoint, fetch)?))
}

fn check_prefix(prefix: &str) -> Result<(), BreachError> {
    if prefix.len() == PREFIX_LEN
        && prefix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
    {
        Ok(())
    } else {
        Err(BreachError::BadSource(
            "a range is asked for by five hex characters only".into(),
        ))
    }
}

/// One password to check, held as its hash only. `key` is the caller's
/// name for it (an item and field); it never leaves the device.
pub struct Candidate {
    /// The caller's name for this password.
    pub key: String,
    hex: Zeroizing<String>,
}

impl std::fmt::Debug for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The prefix is what leaves the device anyway; the rest never prints.
        write!(f, "Candidate({}, {}…)", self.key, self.prefix())
    }
}

impl Candidate {
    /// Hashes `password` (bytes the caller wipes) under `key`.
    pub fn new(key: impl Into<String>, password: &[u8]) -> Self {
        let digest = Sha1::digest(password);
        let mut hex = Zeroizing::new(String::with_capacity(40));
        for b in digest.iter() {
            hex.push(char::from(HEX[usize::from(b >> 4)]));
            hex.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
        Self {
            key: key.into(),
            hex,
        }
    }

    /// The five characters that are sent.
    pub fn prefix(&self) -> &str {
        &self.hex[..PREFIX_LEN]
    }

    fn suffix(&self) -> &str {
        &self.hex[PREFIX_LEN..]
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// The result of a check over many passwords.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Checked {
    /// Keys found in a breach, with how often the password was seen.
    pub found: Vec<(String, u64)>,
    /// Keys checked and not found.
    pub clear: Vec<String>,
    /// Keys not checked because their range failed, and why (the first failure).
    pub unchecked: Vec<String>,
    /// The first failure, in words.
    pub error: Option<BreachError>,
}

/// How often `suffix` appears in a range answer; decoys (count zero) and
/// lines that don't parse count as absent.
pub fn count_in(range: &str, suffix: &str) -> u64 {
    range
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find(|(s, _)| s.trim().eq_ignore_ascii_case(suffix))
        .and_then(|(_, n)| n.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// Checks every candidate against `source`, one request per distinct
/// prefix. A failed range leaves its candidates unchecked and the rest go
/// on; the first failure is reported.
pub fn check(source: &dyn RangeSource, candidates: &[Candidate]) -> Checked {
    let mut by_prefix: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for c in candidates {
        by_prefix.entry(c.prefix()).or_default().push(c);
    }
    let mut out = Checked::default();
    for (prefix, group) in by_prefix {
        match source.range(prefix) {
            Ok(range) => {
                for c in group {
                    match count_in(&range, c.suffix()) {
                        0 => out.clear.push(c.key.clone()),
                        n => out.found.push((c.key.clone(), n)),
                    }
                }
            }
            Err(e) => {
                out.unchecked.extend(group.iter().map(|c| c.key.clone()));
                if out.error.is_none() {
                    out.error = Some(e);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
    const PASSWORD_SUFFIX: &str = "1E4C9B93F3F0682250B6CF8331B7EE68FD8";

    struct Fake {
        asked: Mutex<Vec<String>>,
        fail: Option<&'static str>,
    }

    impl RangeSource for Fake {
        fn range(&self, prefix: &str) -> Result<String, BreachError> {
            self.asked.lock().unwrap().push(prefix.to_string());
            if Some(prefix) == self.fail {
                return Err(BreachError::Status {
                    host: "fake".into(),
                    status: 503,
                });
            }
            Ok(match prefix {
                "5BAA6" => format!(
                    "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n{PASSWORD_SUFFIX}:10437277\r\nFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF:0\r\n"
                ),
                _ => "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n".into(),
            })
        }
        fn describe(&self) -> String {
            "fake".into()
        }
    }

    #[test]
    fn only_five_characters_of_the_hash_are_asked_for() {
        let c = Candidate::new("a", b"password");
        assert_eq!(c.prefix(), "5BAA6");
        assert_eq!(c.suffix(), PASSWORD_SUFFIX);
        let debug = format!("{c:?}");
        assert!(!debug.contains(PASSWORD_SUFFIX), "{debug}");
        assert!(!debug.contains("password"), "{debug}");
    }

    #[test]
    fn found_clear_and_one_request_per_prefix() {
        let fake = Fake {
            asked: Mutex::new(Vec::new()),
            fail: None,
        };
        let candidates = vec![
            Candidate::new("github/pw", b"password"),
            Candidate::new("mail/pw", b"password"),
            Candidate::new("bank/pw", b"k7#Qm2^vX9@pL4&wR8!z"),
        ];
        let out = check(&fake, &candidates);
        assert_eq!(
            out.found,
            vec![
                ("github/pw".into(), 10_437_277),
                ("mail/pw".into(), 10_437_277)
            ]
        );
        assert_eq!(out.clear, vec!["bank/pw".to_string()]);
        assert!(out.error.is_none());
        let asked = fake.asked.lock().unwrap();
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(asked.iter().all(|p| p.len() == 5));
    }

    #[test]
    fn padding_lines_count_as_absent_and_a_failure_leaves_only_its_prefix_unchecked() {
        assert_eq!(count_in("ABC:0\nDEF:3", "abc"), 0);
        assert_eq!(count_in("ABC:0\nDEF:3", "DEF"), 3);
        assert_eq!(count_in("garbage\n:\nDEF:x", "DEF"), 0);
        let fake = Fake {
            asked: Mutex::new(Vec::new()),
            fail: Some("5BAA6"),
        };
        let out = check(
            &fake,
            &[
                Candidate::new("a", b"password"),
                Candidate::new("b", b"something else entirely"),
            ],
        );
        assert_eq!(out.unchecked, vec!["a".to_string()]);
        assert_eq!(out.clear, vec!["b".to_string()]);
        assert!(matches!(
            out.error,
            Some(BreachError::Status { status: 503, .. })
        ));
    }

    #[test]
    fn a_folder_of_range_files_works_offline() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("5BAA6.txt"),
            format!("{PASSWORD_SUFFIX}:42\n"),
        )
        .unwrap();
        let fetch = crate::http::HttpFetch::new(std::time::Duration::from_secs(1)).unwrap();
        let source =
            source_for(&format!("file://{}", dir.path().display()), Box::new(fetch)).unwrap();
        let out = check(
            source.as_ref(),
            &[
                Candidate::new("a", b"password"),
                Candidate::new("b", b"not in the files"),
            ],
        );
        assert_eq!(out.found, vec![("a".into(), 42)]);
        assert_eq!(out.unchecked, vec!["b".to_string()]);
        assert!(matches!(out.error, Some(BreachError::MissingFile(_))));
        assert!(DirRange::new(dir.path().join("nope")).is_err());
    }

    #[test]
    fn prefixes_are_checked_before_anything_is_asked() {
        let dir = tempfile::tempdir().unwrap();
        let source = DirRange::new(dir.path()).unwrap();
        for bad in ["../..", "5baa6", "5BAA", "5BAA6X", "ZZZZZ"] {
            assert!(
                matches!(source.range(bad), Err(BreachError::BadSource(_))),
                "{bad}"
            );
        }
    }
}
