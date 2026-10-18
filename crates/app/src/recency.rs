//! Which items this device used, and how often (02-model §10: "fill counts
//! and last-fill times stay on the device and never sync"): what ranks the
//! quick-access palette and the list's "Recently used" sort.
//!
//! A use is a deliberate act on an item: a copy, a reveal, a code copied,
//! its site opened. Opening an item to look at it is not a use. The record
//! is one small file per persona, `recency-<persona>.cbor` beside
//! `remembered.cbor`, sealed under the persona's App Data Key with
//! XChaCha20-Poly1305 so even the list of item ids and times is unreadable
//! without the identity; a file that doesn't open (another key, damage) is
//! read as empty and rewritten, since nothing in it matters beyond ranking.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecosystem_identity::keys::AppDataKey;
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};

const AAD: &[u8] = b"lockdown/recency/v1";
const RECENCY_V: u16 = 1;
/// The most items remembered; the least recently used fall off.
pub const MAX_ENTRIES: usize = 2_000;

/// One item's use on this device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Use {
    /// Seconds since the epoch.
    pub last: i64,
    /// How many times.
    pub count: u32,
}

#[derive(Serialize, Deserialize)]
struct Body {
    v: u16,
    uses: BTreeMap<Id, Use>,
}

/// The device's record of uses, for one persona.
#[derive(Debug)]
pub struct Recency {
    path: PathBuf,
    uses: BTreeMap<Id, Use>,
}

impl Recency {
    /// Where the record for `persona` lives in `data`.
    pub fn path_in(data: &Path, persona: Id) -> PathBuf {
        data.join(format!("recency-{persona}.cbor"))
    }

    /// Reads the record, or starts an empty one.
    pub fn open(path: PathBuf, key: &AppDataKey) -> Self {
        let uses = std::fs::read(&path)
            .ok()
            .and_then(|bytes| lockdown_crypto::cbor::from_slice::<Sealed>(&bytes).ok())
            .and_then(|sealed| open(key, AAD, &sealed).ok())
            .and_then(|plain| lockdown_crypto::cbor::from_slice::<Body>(&plain).ok())
            .filter(|b| b.v <= RECENCY_V)
            .map(|b| b.uses)
            .unwrap_or_default();
        Self { path, uses }
    }

    /// One item's use, if any.
    pub fn get(&self, item: Id) -> Option<Use> {
        self.uses.get(&item).copied()
    }

    /// Notes a use at `now` and writes the record.
    pub fn note(&mut self, item: Id, now: i64, key: &AppDataKey) -> Result<(), String> {
        let u = self.uses.entry(item).or_default();
        u.last = u.last.max(now);
        u.count = u.count.saturating_add(1);
        self.save(key)
    }

    /// Moves an item's record to its new id (a move between vaults).
    pub fn rename(&mut self, from: Id, to: Id, key: &AppDataKey) -> Result<(), String> {
        if let Some(u) = self.uses.remove(&from) {
            self.uses.insert(to, u);
            self.save(key)?;
        }
        Ok(())
    }

    /// Drops the records of items that are gone.
    pub fn forget(&mut self, gone: &[Id], key: &AppDataKey) -> Result<(), String> {
        let before = self.uses.len();
        for id in gone {
            self.uses.remove(id);
        }
        if self.uses.len() != before {
            self.save(key)?;
        }
        Ok(())
    }

    fn save(&mut self, key: &AppDataKey) -> Result<(), String> {
        while self.uses.len() > MAX_ENTRIES {
            let oldest = self
                .uses
                .iter()
                .min_by_key(|(_, u)| u.last)
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => self.uses.remove(&id),
                None => break,
            };
        }
        let body = Body {
            v: RECENCY_V,
            uses: self.uses.clone(),
        };
        let plain = zeroize::Zeroizing::new(
            lockdown_crypto::cbor::to_canonical(&body).map_err(|e| e.to_string())?,
        );
        let sealed = seal(key, AAD, &plain).map_err(|e| e.to_string())?;
        let bytes = lockdown_crypto::cbor::to_canonical(&sealed).map_err(|e| e.to_string())?;
        lockdown_store::write_private(&self.path, &bytes)
            .map_err(|e| format!("couldn't write {}: {e}", self.path.display()))
    }
}

/// How strongly a use ranks now: the count, halved for every two weeks
/// since the last use, so last week's daily login outranks last year's.
pub fn score(u: Option<Use>, now: i64) -> f64 {
    let Some(u) = u else { return 0.0 };
    let weeks = ((now - u.last).max(0) as f64) / (14.0 * 86_400.0);
    f64::from(u.count) / 2f64.powf(weeks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::SymmetricKey;

    #[test]
    fn uses_are_sealed_on_disk_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let key = AppDataKey::from_bytes([3; 32]);
        let persona = Id::new();
        let path = Recency::path_in(dir.path(), persona);
        let mut r = Recency::open(path.clone(), &key);
        let a = Id::new();
        r.note(a, 100, &key).unwrap();
        r.note(a, 200, &key).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(16).any(|w| w == a.as_bytes()), "sealed");
        let back = Recency::open(path.clone(), &key);
        assert_eq!(
            back.get(a),
            Some(Use {
                last: 200,
                count: 2
            })
        );
        // Another key reads nothing (and never fails).
        let other = Recency::open(path, &AppDataKey::from_bytes([4; 32]));
        assert_eq!(other.get(a), None);
    }

    #[test]
    fn the_score_fades_with_time() {
        let now = 10 * 86_400 * 14;
        let fresh = score(
            Some(Use {
                last: now,
                count: 4,
            }),
            now,
        );
        let old = score(
            Some(Use {
                last: now - 4 * 14 * 86_400,
                count: 20,
            }),
            now,
        );
        assert!(fresh > old, "{fresh} {old}");
        assert_eq!(score(None, now), 0.0);
    }
}
