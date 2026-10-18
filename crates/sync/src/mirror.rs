//! Log mirrors (`key-transparency`; 04-security §6 point 4, 05-self-host
//! §6; docs/plan/27-web-server-finish.md §4).
//!
//! A **mirror** is a second server, or any static file host, that copies
//! an account server's signed tree heads and serves the newest one it has
//! verified. It never holds a leaf: it checks each head it fetches is
//! signed by the log key it pinned on first sight and, by a consistency
//! proof from the account server, continues the last head it copied. A
//! server that rewrites its log stops continuing what the mirror holds, and
//! the mirror says so (`problem`) and keeps serving its last good head.
//!
//! A **client** that was told about mirrors (by the account server's
//! `/v1/kt/mirrors`, remembered once seen, or by its own settings) fetches
//! each one's head and holds it against the head it just verified itself:
//! the same log key, a valid signature, and a consistency proof between the
//! two from the account server, smaller to larger. Two signed heads the
//! account server can't connect are proof it signed two histories: a
//! **split view**, shown in words. A mirror that can't be reached, copies
//! another server or has stopped copying is said to be so, never counted as
//! agreement.
//!
//! | Method and path | Body | Served by |
//! |---|---|---|
//! | `GET /v1/kt/mirror` | [`MirrorHead`] (CBOR) | a mirror (or a file at that path) |
//! | `GET /v1/kt/mirrors` | [`KtMirrors`] (CBOR) | the account server: the mirrors it names |

use lockdown_crypto::VerifyingKey;
use lockdown_crypto::merkle;
use lockdown_vault::transparency::{SignedTreeHead, SthRef};
use serde::{Deserialize, Serialize};

use crate::kt::KtSession;

/// Where a mirror serves its head (and where a static host puts the file).
pub const MIRROR_PATH: &str = "/v1/kt/mirror";
/// Where an account server lists its mirrors.
pub const MIRRORS_PATH: &str = "/v1/kt/mirrors";
/// The mirror document's version.
pub const MIRROR_V: u16 = 1;
/// A mirror whose newest copy is older than this has stopped copying.
pub const STALE_SECS: i64 = 7 * 86_400;
/// The most mirrors a client checks.
pub const MAX_MIRRORS: usize = 8;

/// `GET /v1/kt/mirror`: the newest head a mirror copied.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MirrorHead {
    /// [`MIRROR_V`].
    pub v: u16,
    /// The account server it copies (its base URL).
    pub upstream: String,
    /// The log key it pinned on first sight.
    #[serde(with = "serde_bytes")]
    pub log_key: [u8; 32],
    /// The newest head it verified as a continuation of every head before.
    pub sth: SignedTreeHead,
    /// When it last fetched the account server's head (seconds).
    pub checked_at: i64,
    /// How many heads it has copied since it began.
    pub copied: u64,
    /// What the mirror itself caught: the account server's log stopped
    /// continuing the one it copied, in words. Sticky until the operator
    /// starts the mirror over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// `GET /v1/kt/mirrors`: the mirrors an account server names.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KtMirrors {
    /// Their base URLs.
    pub mirrors: Vec<String>,
}

/// What one mirror's head said about the log this device verified.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MirrorVerdict {
    /// The mirror's head and this device's are one history.
    Agrees {
        /// The mirror's head's size.
        size: u64,
        /// When the mirror last copied.
        checked_at: i64,
    },
    /// The server signed two histories: this device's and the mirror's.
    Split {
        /// In words.
        message: String,
    },
    /// The mirror can't vouch for anything: another server's, stopped
    /// copying, or a head this server never signed.
    Unusable {
        /// In words.
        message: String,
    },
    /// The mirror (or the proof between the two heads) couldn't be had.
    Unreachable {
        /// In words.
        message: String,
    },
}

impl MirrorVerdict {
    /// Whether this is a split view.
    pub fn is_split(&self) -> bool {
        matches!(self, Self::Split { .. })
    }
}

/// A mirror's base URL as compared and remembered: trimmed, no trailing
/// slash, `https://` (or `http://` to this machine), at most 512 bytes.
pub fn normalise_mirror(url: &str) -> Option<String> {
    let t = url.trim().trim_end_matches('/');
    if t.is_empty() || t.len() > 512 || t.chars().any(char::is_whitespace) {
        return None;
    }
    crate::transport::check_base(t).ok()?;
    Some(t.to_string())
}

fn same_server(a: &str, b: &str) -> bool {
    a.trim()
        .trim_end_matches('/')
        .eq_ignore_ascii_case(b.trim().trim_end_matches('/'))
}

/// Holds `mirror` against the head `kt` verified for `server` at `now`.
pub fn compare(kt: &KtSession<'_>, server: &str, mirror: &MirrorHead, now: i64) -> MirrorVerdict {
    let unusable = |message: String| MirrorVerdict::Unusable { message };
    if mirror.v != MIRROR_V {
        return unusable(format!(
            "it speaks mirror format {}, which this build doesn't read",
            mirror.v
        ));
    }
    if !same_server(&mirror.upstream, server) {
        return unusable(format!("it copies {}, not this server", mirror.upstream));
    }
    if mirror.log_key != kt.view.log_key {
        return MirrorVerdict::Split {
            message: "Split view: the mirror saw this server sign its log with another key than the one this device pinned".into(),
        };
    }
    let Ok(key) = VerifyingKey::from_bytes(&mirror.log_key) else {
        return unusable("its log key isn't an Ed25519 public key".into());
    };
    if mirror.sth.verify(&key).is_err() {
        return unusable("its copy isn't signed by this server's log key".into());
    }
    let mine = kt.sth().as_ref();
    let theirs = mirror.sth.as_ref();
    let (from, to): (SthRef, SthRef) = if theirs.size <= mine.size {
        (theirs, mine)
    } else {
        (mine, theirs)
    };
    if from.size == to.size {
        if from.root != to.root {
            return MirrorVerdict::Split {
                message: format!(
                    "Split view: the mirror copied a log of {} entries with a different root from the one this server showed this device; the server keeps two histories",
                    from.size
                ),
            };
        }
    } else {
        let proof = match kt.transport().kt_consistency(from.size, to.size) {
            Ok(p) => p,
            Err(crate::TransportError::Refused { status, .. }) if status < 500 => {
                return MirrorVerdict::Split {
                    message: format!(
                        "Split view: this server wouldn't connect its log of {} entries to the {} the mirror copied; the server keeps two histories",
                        mine.size, theirs.size
                    ),
                };
            }
            Err(e) => {
                return MirrorVerdict::Unreachable {
                    message: format!("the proof between the two heads couldn't be had: {e}"),
                };
            }
        };
        if proof.from != from.size
            || proof.to != to.size
            || !merkle::verify_consistency(from.size, to.size, from.root, to.root, &proof.path)
        {
            return MirrorVerdict::Split {
                message: format!(
                    "Split view: the log of {} entries the mirror copied doesn't connect to the {} this server showed this device; the server keeps two histories",
                    theirs.size, mine.size
                ),
            };
        }
    }
    if let Some(problem) = &mirror.problem {
        return MirrorVerdict::Split {
            message: format!("Split view, seen by the mirror: {problem}"),
        };
    }
    if now - mirror.checked_at > STALE_SECS {
        return unusable(format!(
            "it hasn't copied this server's log since {}",
            crate::engine::civil_date(mirror.checked_at)
        ));
    }
    MirrorVerdict::Agrees {
        size: theirs.size,
        checked_at: mirror.checked_at,
    }
}

/// What the mirror side does with a newly fetched head: whether it
/// continues the last one copied, proven by `proof` (from the account
/// server, `None` when the sizes are equal).
pub fn continues(
    last: &SignedTreeHead,
    next: &SignedTreeHead,
    proof: Option<&crate::kt::ConsistencyProofWire>,
) -> Result<(), String> {
    if next.size < last.size {
        return Err(format!(
            "the server's log went back from {} entries to {}",
            last.size, next.size
        ));
    }
    if next.size == last.size {
        return if next.root == last.root {
            Ok(())
        } else {
            Err(format!(
                "the server signed two different logs of {} entries",
                next.size
            ))
        };
    }
    let Some(p) = proof else {
        return Err("no proof that the server's log continues the one copied".into());
    };
    if p.from != last.size
        || p.to != next.size
        || !merkle::verify_consistency(last.size, next.size, last.root, next.root, &p.path)
    {
        return Err(format!(
            "the server's log of {} entries doesn't continue the {} copied; its history was rewritten",
            next.size, last.size
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_crypto::SigningKey;
    use lockdown_vault::Hash32;
    use lockdown_vault::transparency::TreeHead;

    fn tree(n: u64) -> Vec<Vec<u8>> {
        (0..n).map(|i| format!("leaf {i}").into_bytes()).collect()
    }

    fn sth(key: &SigningKey, leaves: &[Vec<u8>]) -> SignedTreeHead {
        let hashes: Vec<Hash32> = leaves.iter().map(|l| merkle::leaf_hash(l)).collect();
        let head = TreeHead {
            size: leaves.len() as u64,
            root: merkle::root(&hashes),
            time: 1_792_000_000,
        };
        SignedTreeHead::sign(head, key).unwrap()
    }

    #[test]
    fn a_mirror_follows_a_log_that_grows_and_refuses_one_that_was_rewritten() {
        let key = SigningKey::generate().unwrap();
        let five = tree(5);
        let mut eight = five.clone();
        eight.extend(tree(8).split_off(5));
        let a = sth(&key, &five);
        let b = sth(&key, &eight);
        let hashes: Vec<Hash32> = eight.iter().map(|l| merkle::leaf_hash(l)).collect();
        let proof = crate::kt::ConsistencyProofWire {
            from: 5,
            to: 8,
            path: merkle::consistency_proof(&hashes, 5).unwrap(),
        };
        assert_eq!(continues(&a, &b, Some(&proof)), Ok(()));
        assert_eq!(continues(&a, &a, None), Ok(()));
        assert!(continues(&b, &a, None).unwrap_err().contains("went back"));
        // A log of eight whose first five aren't the five copied.
        let mut other = tree(3);
        other.extend((0..5).map(|i| format!("forged {i}").into_bytes()));
        let forged = sth(&key, &other);
        let other_hashes: Vec<Hash32> = other.iter().map(|l| merkle::leaf_hash(l)).collect();
        let forged_proof = crate::kt::ConsistencyProofWire {
            from: 5,
            to: 8,
            path: merkle::consistency_proof(&other_hashes, 5).unwrap(),
        };
        assert!(
            continues(&a, &forged, Some(&forged_proof))
                .unwrap_err()
                .contains("rewritten")
        );
        let same_size = sth(&key, &other[..5]);
        assert!(
            continues(&a, &same_size, None)
                .unwrap_err()
                .contains("two different")
        );
    }

    #[test]
    fn mirror_addresses_are_checked_before_they_are_kept() {
        assert_eq!(
            normalise_mirror(" https://mirror.example/ ").as_deref(),
            Some("https://mirror.example")
        );
        assert_eq!(
            normalise_mirror("http://127.0.0.1:9000").as_deref(),
            Some("http://127.0.0.1:9000")
        );
        assert!(normalise_mirror("http://mirror.example").is_none());
        assert!(normalise_mirror("ftp://mirror.example").is_none());
        assert!(normalise_mirror("https://a b").is_none());
        assert!(normalise_mirror("").is_none());
        assert!(same_server(
            "https://Vault.example/",
            "https://vault.example"
        ));
    }

    #[test]
    fn the_verdicts_serialise_as_tagged_words() {
        let v = MirrorVerdict::Split {
            message: "Split view: x".into(),
        };
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["state"], "split");
        assert!(v.is_split());
        assert!(
            !MirrorVerdict::Agrees {
                size: 1,
                checked_at: 0
            }
            .is_split()
        );
    }
}
