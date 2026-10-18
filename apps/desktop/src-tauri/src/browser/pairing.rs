//! Pairing a browser (docs/plan/16-browser-extension.md §3). Before the
//! desktop answers anything but the pairing calls, the extension proves it
//! is one the person paired here:
//!
//! 1. The extension makes a client id and a 32-byte key, keeps the key
//!    in the browser's IndexedDB as a non-extractable WebCrypto key, and
//!    sends both once in `pair_begin`.
//! 2. The desktop makes a six-digit code from the operating system's
//!    random source, answers it to that request, and shows it in the app
//!    beside the browser's name. The extension shows the code it was given.
//! 3. The person checks the two codes match and approves in the app,
//!    through the `Approve` gate (D101). A request from anything else on
//!    the machine gets a code of its own, which no extension window shows.
//! 4. From then on each connection starts with a fresh 32-byte challenge
//!    from the desktop, and the extension answers
//!    `HMAC-SHA256(key, "lockdown-browser-auth/v1\0" ‖ challenge ‖ client id)`.
//!
//! Paired browsers are kept in `browser-pairings.json` in the data
//! directory, user-only. A pending request lives two minutes, in memory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use lockdown_native_messaging::caller::Caller;
use lockdown_native_messaging::endpoint::write_private;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// The pairings file's name in the data directory.
pub const PAIRINGS_FILE: &str = "browser-pairings.json";

/// How long a pairing request waits for the person.
pub const PENDING_FOR: Duration = Duration::from_secs(120);

/// How many requests may wait at once.
pub const MAX_PENDING: usize = 3;

/// The proof's domain-separation prefix.
const PROOF_PREFIX: &[u8] = b"lockdown-browser-auth/v1\0";

/// A paired browser, as kept on disk.
#[derive(Clone, Serialize, Deserialize)]
pub struct Paired {
    pub client_id: String,
    /// The pairing key, hex. Not a vault secret: it proves an extension is
    /// the one paired here, and is useless away from this machine's socket.
    key: String,
    /// What the extension called itself ("Chrome on Linux").
    pub name: String,
    pub caller: Caller,
    /// Seconds since the Unix epoch.
    pub paired_at: i64,
}

/// A paired browser as the app's settings screen lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PairedView {
    pub client_id: String,
    pub name: String,
    pub family: String,
    pub extension_id: String,
    pub paired_at: i64,
}

/// A request waiting for the person, as the app shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PendingView {
    pub request_id: String,
    pub code: String,
    pub name: String,
    pub family: String,
    pub extension_id: String,
    pub expires_in_secs: u64,
}

/// What `pair_status` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairState {
    Pending,
    Paired,
    Refused,
    Unknown,
}

struct Pending {
    request_id: String,
    client_id: String,
    key: Zeroizing<Vec<u8>>,
    name: String,
    caller: Caller,
    code: String,
    created: Instant,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    v: u32,
    clients: Vec<Paired>,
}

/// The paired browsers and the requests waiting.
pub struct Pairings {
    path: PathBuf,
    paired: Vec<Paired>,
    pending: Vec<Pending>,
    refused: Vec<(String, Instant)>,
}

/// Fills `buf` from the operating system's random source.
pub fn random(buf: &mut [u8]) -> Result<(), String> {
    getrandom::fill(buf).map_err(|_| "the system's random source failed".to_string())
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    data_encoding::HEXLOWER_PERMISSIVE
        .decode(text.as_bytes())
        .ok()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A client id: 32 hex characters.
fn valid_client_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The proof an extension holding `key` gives for `challenge`.
pub fn proof(key: &[u8], challenge: &[u8], client_id: &str) -> Vec<u8> {
    let mut mac =
        <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(PROOF_PREFIX);
    mac.update(challenge);
    mac.update(client_id.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

impl Pairings {
    /// Loads the pairings kept in `data`; none when the file is absent.
    pub fn load(data: &Path) -> Result<Self, String> {
        let path = data.join(PAIRINGS_FILE);
        let paired = match std::fs::read(&path) {
            Ok(bytes) => {
                let f: File = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("the browser pairings file didn't read: {e}"))?;
                f.clients
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("couldn't read the browser pairings: {e}")),
        };
        Ok(Self {
            path,
            paired,
            pending: Vec::new(),
            refused: Vec::new(),
        })
    }

    fn save(&self) -> Result<(), String> {
        let file = File {
            v: 1,
            clients: self.paired.clone(),
        };
        let body = Zeroizing::new(serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?);
        write_private(&self.path, &body).map_err(|e| format!("couldn't save the pairing: {e}"))
    }

    fn expire(&mut self) {
        self.pending.retain(|p| p.created.elapsed() < PENDING_FOR);
        self.refused.retain(|(_, at)| at.elapsed() < PENDING_FOR);
    }

    /// A new request from `caller`. Answers the request id and the code.
    pub fn begin(
        &mut self,
        caller: &Caller,
        client_id: &str,
        key_hex: &str,
        name: &str,
    ) -> Result<(String, String), String> {
        self.expire();
        if !valid_client_id(client_id) {
            return Err("the client id isn't 32 hex characters".into());
        }
        let key = Zeroizing::new(unhex(key_hex).ok_or("the pairing key isn't hex")?);
        if key.len() != 32 {
            return Err("the pairing key isn't 32 bytes".into());
        }
        if self.paired.iter().any(|p| p.client_id == client_id) {
            return Err("this browser is already paired; it should sign in instead".into());
        }
        self.pending.retain(|p| p.client_id != client_id);
        if self.pending.len() >= MAX_PENDING {
            return Err(
                "too many browsers are waiting to pair; answer them in Lockdown first".into(),
            );
        }
        let mut id = [0u8; 16];
        random(&mut id)?;
        let mut n = [0u8; 4];
        random(&mut n)?;
        let code = format!("{:06}", u32::from_le_bytes(n) % 1_000_000);
        let name: String = name.chars().filter(|c| !c.is_control()).take(60).collect();
        let request_id = hex(&id);
        self.pending.push(Pending {
            request_id: request_id.clone(),
            client_id: client_id.to_string(),
            key,
            name: if name.trim().is_empty() {
                "A browser".into()
            } else {
                name
            },
            caller: caller.clone(),
            code: code.clone(),
            created: Instant::now(),
        });
        Ok((request_id, code))
    }

    /// Where a client's pairing stands.
    pub fn state(&mut self, client_id: &str) -> PairState {
        self.expire();
        if self.paired.iter().any(|p| p.client_id == client_id) {
            PairState::Paired
        } else if self.pending.iter().any(|p| p.client_id == client_id) {
            PairState::Pending
        } else if self.refused.iter().any(|(c, _)| c == client_id) {
            PairState::Refused
        } else {
            PairState::Unknown
        }
    }

    /// The person's answer in the app.
    pub fn answer(&mut self, request_id: &str, approve: bool) -> Result<(), String> {
        self.expire();
        let at = self
            .pending
            .iter()
            .position(|p| p.request_id == request_id)
            .ok_or("that pairing request has expired; start it again from the browser")?;
        let p = self.pending.remove(at);
        if !approve {
            self.refused.push((p.client_id, Instant::now()));
            return Ok(());
        }
        self.paired.push(Paired {
            client_id: p.client_id.clone(),
            key: hex(&p.key),
            name: p.name.clone(),
            caller: p.caller.clone(),
            paired_at: now_secs(),
        });
        if let Err(e) = self.save() {
            self.paired.retain(|x| x.client_id != p.client_id);
            return Err(e);
        }
        Ok(())
    }

    /// Whether `proof_hex` proves `client_id` for `challenge`, from an
    /// extension with the same id it paired from.
    pub fn verify(
        &self,
        caller: &Caller,
        client_id: &str,
        challenge: &[u8],
        proof_hex: &str,
    ) -> bool {
        let Some(p) = self.paired.iter().find(|p| p.client_id == client_id) else {
            return false;
        };
        if &p.caller != caller {
            return false;
        }
        let (Some(key), Some(tag)) = (unhex(&p.key).map(Zeroizing::new), unhex(proof_hex)) else {
            return false;
        };
        let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(&key)
            .expect("HMAC takes any key length");
        mac.update(PROOF_PREFIX);
        mac.update(challenge);
        mac.update(client_id.as_bytes());
        mac.verify_slice(&tag).is_ok()
    }

    /// Whether `client_id` is still paired (a removal ends its sessions).
    pub fn is_paired(&self, client_id: &str) -> bool {
        self.paired.iter().any(|p| p.client_id == client_id)
    }

    /// Removes a paired browser.
    pub fn unpair(&mut self, client_id: &str) -> Result<(), String> {
        let before = self.paired.len();
        self.paired.retain(|p| p.client_id != client_id);
        if self.paired.len() == before {
            return Err("that browser isn't paired".into());
        }
        self.save()
    }

    /// Every paired browser.
    pub fn paired(&self) -> Vec<PairedView> {
        self.paired
            .iter()
            .map(|p| PairedView {
                client_id: p.client_id.clone(),
                name: p.name.clone(),
                family: family_name(&p.caller).into(),
                extension_id: p.caller.extension_id.clone(),
                paired_at: p.paired_at,
            })
            .collect()
    }

    /// Every request waiting for the person.
    pub fn pending(&mut self) -> Vec<PendingView> {
        self.expire();
        self.pending
            .iter()
            .map(|p| PendingView {
                request_id: p.request_id.clone(),
                code: p.code.clone(),
                name: p.name.clone(),
                family: family_name(&p.caller).into(),
                extension_id: p.caller.extension_id.clone(),
                expires_in_secs: PENDING_FOR.saturating_sub(p.created.elapsed()).as_secs(),
            })
            .collect()
    }
}

fn family_name(c: &Caller) -> &'static str {
    match c.family {
        lockdown_native_messaging::caller::Family::Chromium => "chromium",
        lockdown_native_messaging::caller::Family::Firefox => "firefox",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_native_messaging::caller::Family;
    use lockdown_native_messaging::{DEV_CHROMIUM_ID, FIREFOX_ID};

    fn chrome() -> Caller {
        Caller {
            family: Family::Chromium,
            extension_id: DEV_CHROMIUM_ID.into(),
        }
    }

    const CLIENT: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_request_waits_with_a_code_and_an_approval_pairs_it_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Pairings::load(dir.path()).unwrap();
        let key = [7u8; 32];
        let (request, code) = p
            .begin(&chrome(), CLIENT, &hex(&key), "Chrome on Linux")
            .unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(p.state(CLIENT), PairState::Pending);
        let pending = p.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].code, code);
        assert_eq!(pending[0].name, "Chrome on Linux");
        p.answer(&request, true).unwrap();
        assert_eq!(p.state(CLIENT), PairState::Paired);
        assert!(p.pending().is_empty());

        let again = Pairings::load(dir.path()).unwrap();
        assert!(again.is_paired(CLIENT));
        let challenge = [9u8; 32];
        let good = hex(&proof(&key, &challenge, CLIENT));
        assert!(again.verify(&chrome(), CLIENT, &challenge, &good));
        assert!(
            !again.verify(&chrome(), CLIENT, &[8u8; 32], &good),
            "another challenge"
        );
        assert!(!again.verify(
            &chrome(),
            CLIENT,
            &challenge,
            &hex(&proof(&[6u8; 32], &challenge, CLIENT))
        ));
        let firefox = Caller {
            family: Family::Firefox,
            extension_id: FIREFOX_ID.into(),
        };
        assert!(
            !again.verify(&firefox, CLIENT, &challenge, &good),
            "another extension"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(PAIRINGS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn refusals_bad_input_and_limits() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Pairings::load(dir.path()).unwrap();
        assert!(p.begin(&chrome(), "short", &hex(&[1; 32]), "x").is_err());
        assert!(p.begin(&chrome(), CLIENT, "zz", "x").is_err());
        assert!(p.begin(&chrome(), CLIENT, &hex(&[1; 16]), "x").is_err());
        let (request, _) = p.begin(&chrome(), CLIENT, &hex(&[1; 32]), "\u{7}").unwrap();
        assert_eq!(p.pending()[0].name, "A browser");
        p.answer(&request, false).unwrap();
        assert_eq!(p.state(CLIENT), PairState::Refused);
        assert!(p.answer(&request, true).is_err(), "answered once");
        for i in 0..MAX_PENDING {
            p.begin(&chrome(), &format!("{i:032x}"), &hex(&[1; 32]), "x")
                .unwrap();
        }
        assert!(
            p.begin(&chrome(), &format!("{:032x}", 99), &hex(&[1; 32]), "x")
                .is_err()
        );
        assert_eq!(
            p.state("ffffffffffffffffffffffffffffffff"),
            PairState::Unknown
        );
    }

    #[test]
    fn unpairing_removes_it_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Pairings::load(dir.path()).unwrap();
        let (request, _) = p.begin(&chrome(), CLIENT, &hex(&[2; 32]), "Edge").unwrap();
        p.answer(&request, true).unwrap();
        assert!(
            p.begin(&chrome(), CLIENT, &hex(&[2; 32]), "Edge").is_err(),
            "already paired"
        );
        assert_eq!(p.paired().len(), 1);
        p.unpair(CLIENT).unwrap();
        assert!(!p.is_paired(CLIENT));
        assert!(p.unpair(CLIENT).is_err());
        assert!(!Pairings::load(dir.path()).unwrap().is_paired(CLIENT));
    }
}
