//! Key transparency on the client (04-security §6; `lockdown_vault::transparency`
//! for the shapes). Three jobs, in the order a sync runs them:
//!
//! 1. [`KtSession::begin`] fetches the server's signed tree head, checks
//!    its signature under the log key pinned for this server (first sight
//!    pins it), and proves it continues the last STH this device saw with
//!    a consistency proof. An STH that is smaller, the same size with
//!    another root, or not provably a continuation is refused and nothing
//!    else happens this sync.
//! 2. [`KtSession::audit`] walks the account's own log entries since the
//!    last audit, verifies each one's inclusion under the STH, and holds
//!    each against what this device knows ([`AuditExpectations`]): a device
//!    key the identity doesn't list, an identity manifest or a vault head
//!    with another hash than this device holds at that `seq`, an identity
//!    manifest newer than this device was ever given — each is a refusal
//!    with a plain sentence. The device's own key and its newest identity
//!    manifest must appear.
//! 3. [`KtSession::verify_citation`] checks the STH another device cited in
//!    a vault head against this device's view with a consistency proof:
//!    gossip through the vault head, so a server that showed two devices
//!    two different logs is caught by the second device to sync.
//!
//! The wire types for the four log endpoints live here too, shared with
//! `lockdown-server`:
//!
//! | Method and path | Body out | Needs a session |
//! |---|---|---|
//! | `GET /v1/kt/sth` | [`SthWire`] (the STH and the log key) | no |
//! | `GET /v1/kt/inclusion?index=I&size=N` | [`InclusionProofWire`] | no |
//! | `GET /v1/kt/consistency?from=M&to=N` | [`ConsistencyProofWire`] | no |
//! | `GET /v1/kt/entries?from=I` | [`AccountEntries`] (the account's leaves at index ≥ I) | yes |

use ecosystem_identity::Identity;
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::merkle;
use lockdown_store::{ServerView, SyncedIdentity};
use lockdown_vault::transparency::{LogEntry, LogLeaf, SignedTreeHead, SthRef};
use lockdown_vault::{Hash32, Id, Timestamp, Vault};
use serde::{Deserialize, Serialize};

use crate::SyncError;
use crate::engine::session_or;
use crate::transport::Transport;

/// `GET /v1/kt/sth`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SthWire {
    /// The newest signed tree head.
    pub sth: SignedTreeHead,
    /// The server's log key (Ed25519 public). Pinned by the client on
    /// first sight; a change is refused.
    #[serde(with = "serde_bytes")]
    pub log_key: [u8; 32],
}

/// `GET /v1/kt/inclusion?index=I&size=N`: the path for leaf `I` in the
/// tree of the first `N` leaves.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct InclusionProofWire {
    /// The leaf.
    pub index: u64,
    /// The tree size the proof is for.
    pub size: u64,
    /// The sibling hashes, leaf to root.
    pub path: Vec<Hash32>,
}

/// `GET /v1/kt/consistency?from=M&to=N`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConsistencyProofWire {
    /// The older size.
    pub from: u64,
    /// The newer size.
    pub to: u64,
    /// The proof.
    pub path: Vec<Hash32>,
}

/// One logged leaf with its index.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LoggedEntry {
    /// Its position in the log.
    pub index: u64,
    /// The leaf's bytes exactly as hashed.
    #[serde(with = "serde_bytes")]
    pub leaf: Vec<u8>,
}

/// `GET /v1/kt/entries?from=I`: the session's account's leaves at index
/// `I` or later, oldest first, and the log size now.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountEntries {
    /// The account the session stands for, as the server files it. Every
    /// leaf must name it, and this device's own key must be among them.
    pub account_id: Id,
    /// The entries.
    pub entries: Vec<LoggedEntry>,
    /// How many leaves the log has now.
    pub size: u64,
}

/// What this device knows, for the audit to hold the log against.
pub trait AuditExpectations {
    /// The public key of a device of this identity, or `None` for a device
    /// the identity doesn't list.
    fn device_key(&self, device_id: Id) -> Option<[u8; 32]>;
    /// This device's id.
    fn own_device(&self) -> Id;
    /// The newest identity manifest this device holds: `seq` and hash.
    fn identity_head(&self) -> Option<(u64, Hash32)>;
    /// The hash of the head at `seq` of a vault this device holds, or
    /// `None` when it holds no such head (a vault or a `seq` it hasn't
    /// pulled yet; checked on the next audit).
    fn vault_head(&self, vault_id: Id, seq: u64) -> Option<Hash32>;
}

/// What a device that holds an ecosystem identity knows, as the audit
/// needs it: the identity's devices (its own records and the set last
/// accepted from the server), the identity manifest it accepted, and the
/// vault it has open. The one [`AuditExpectations`] the command line and
/// the app both audit with, so the two never disagree about what counts.
pub struct DeviceExpectations<'a> {
    /// The identity on this device.
    pub identity: &'a Identity,
    /// The identity's signed record set as last accepted from the server.
    pub synced: &'a SyncedIdentity,
    /// The vault this device holds, with the newest `seq` of it the server
    /// confirmed to this device; `None` before a vault is pulled.
    pub vault: Option<(&'a Vault, u64)>,
}

impl AuditExpectations for DeviceExpectations<'_> {
    fn device_key(&self, device_id: Id) -> Option<[u8; 32]> {
        if let Some(d) = self.identity.devices().find(|d| d.device_id == device_id) {
            return Some(d.device_pub);
        }
        self.synced
            .device_keys()
            .get(&device_id)
            .map(|k| k.to_bytes())
    }

    fn own_device(&self) -> Id {
        self.identity.device_id()
    }

    fn identity_head(&self) -> Option<(u64, Hash32)> {
        let m = self.synced.manifest()?;
        Some((m.seq, m.hash().ok()?))
    }

    /// Only heads the server confirmed to this device count: heads after
    /// `synced_seq` are provisional offline heads that the next sync
    /// replaces with the server's chain (D55), so the log may well name
    /// another device's head at that `seq`.
    fn vault_head(&self, vault_id: Id, seq: u64) -> Option<Hash32> {
        let (vault, synced_seq) = self.vault?;
        if vault.context().vault_id != vault_id || seq > synced_seq {
            return None;
        }
        vault.head_at(seq).and_then(|h| h.hash().ok())
    }
}

/// One audited entry, for `lockdown audit` to print.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AuditedEntry {
    /// Its index.
    pub index: u64,
    /// The leaf.
    pub leaf: LogLeaf,
}

/// What an audit found: every entry checked (all of them passed, or the
/// audit returned an error naming the one that didn't).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AuditReport {
    /// The entries checked this time, oldest first.
    pub checked: Vec<AuditedEntry>,
    /// Entries past the verified STH, left for the next audit.
    pub pending: usize,
}

fn refuse(msg: String) -> SyncError {
    SyncError::Vault(lockdown_vault::Error::Transparency(msg))
}

/// One sync's view of a server's log, verified.
pub struct KtSession<'a> {
    transport: &'a dyn Transport,
    token: &'a str,
    /// The view as verified: the pinned key, the STH, when it was checked,
    /// and the audit position; what the caller remembers afterwards.
    pub view: ServerView,
}

impl std::fmt::Debug for KtSession<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KtSession")
            .field("size", &self.view.sth.size)
            .finish_non_exhaustive()
    }
}

impl<'a> KtSession<'a> {
    /// Fetches and verifies the server's current STH against what this
    /// device remembers (`None` on first contact: the log key is pinned
    /// from this answer).
    pub fn begin(
        transport: &'a dyn Transport,
        token: &'a str,
        remembered: Option<&ServerView>,
    ) -> Result<Self, SyncError> {
        let wire = transport.kt_sth().map_err(session_or)?;
        let log_key = match remembered {
            Some(r) if r.log_key != wire.log_key => {
                return Err(refuse(
                    "the server's log key changed since this device pinned it; refusing to trust the new one"
                        .into(),
                ));
            }
            _ => wire.log_key,
        };
        let key = VerifyingKey::from_bytes(&log_key)
            .map_err(|_| refuse("the server's log key isn't an Ed25519 public key".into()))?;
        wire.sth.verify(&key)?;
        let mut view = ServerView {
            log_key,
            sth: wire.sth,
            audited: 0,
            identity_logged: None,
            own_device_logged: false,
            checked_at: Some(Timestamp::now().0),
        };
        if let Some(r) = remembered {
            view.audited = r.audited;
            view.identity_logged = r.identity_logged;
            view.own_device_logged = r.own_device_logged;
            if view.sth.size < r.sth.size {
                return Err(refuse(format!(
                    "Rollback attempt blocked: the server's log has {} entries but this device already saw {}",
                    view.sth.size, r.sth.size
                )));
            }
            if view.sth.size == r.sth.size {
                if view.sth.root != r.sth.root {
                    return Err(refuse(format!(
                        "Split view: the server's log of {} entries has a different root from the one this device saw at that size",
                        view.sth.size
                    )));
                }
            } else {
                let proof = transport
                    .kt_consistency(r.sth.size, view.sth.size)
                    .map_err(session_or)?;
                if proof.from != r.sth.size
                    || proof.to != view.sth.size
                    || !merkle::verify_consistency(
                        r.sth.size,
                        view.sth.size,
                        r.sth.root,
                        view.sth.root,
                        &proof.path,
                    )
                {
                    return Err(refuse(format!(
                        "the server's log of {} entries doesn't continue the {} this device saw; the history was rewritten",
                        view.sth.size, r.sth.size
                    )));
                }
            }
        }
        Ok(Self {
            transport,
            token,
            view,
        })
    }

    /// The verified STH.
    pub fn sth(&self) -> &SignedTreeHead {
        &self.view.sth
    }

    /// The citation this device puts in the heads it signs.
    pub fn citation(&self) -> SthRef {
        self.view.sth.as_ref()
    }

    /// Checks a citation from another device's head against this view: the
    /// cited tree must be a prefix of this one, or this one of it (the
    /// other device synced later), proven by the server. A citation the
    /// server cannot connect to this view is a split view: refuse.
    pub fn verify_citation(&self, cited: SthRef) -> Result<(), SyncError> {
        let mine = self.view.sth.as_ref();
        let (from, to) = if cited.size <= mine.size {
            (cited, mine)
        } else {
            (mine, cited)
        };
        if from.size == to.size {
            if from.root == to.root {
                return Ok(());
            }
            return Err(refuse(format!(
                "Split view: another device saw a log of {} entries with a different root from the one this device saw; the server showed the two devices different histories",
                from.size
            )));
        }
        let proof = self
            .transport
            .kt_consistency(from.size, to.size)
            .map_err(session_or)?;
        if proof.from != from.size
            || proof.to != to.size
            || !merkle::verify_consistency(from.size, to.size, from.root, to.root, &proof.path)
        {
            return Err(refuse(format!(
                "Split view: the log of {} entries another device cited doesn't connect to the {} this device saw; the server showed the two devices different histories",
                cited.size, mine.size
            )));
        }
        Ok(())
    }

    /// Audits the account's own entries since the last audit against
    /// `expect`. Moves `view.audited` past what was checked; entries past
    /// the verified STH wait for the next one.
    pub fn audit(&mut self, expect: &dyn AuditExpectations) -> Result<AuditReport, SyncError> {
        let entries = self
            .transport
            .kt_entries(self.token, self.view.audited)
            .map_err(session_or)?;
        let account_id = entries.account_id;
        let size = self.view.sth.size;
        let root = self.view.sth.root;
        let mut report = AuditReport::default();
        let mut next = self.view.audited;
        for e in entries.entries {
            if e.index < next {
                return Err(refuse(format!(
                    "the server served log entry {} out of order (expected {} or later)",
                    e.index, next
                )));
            }
            if e.index >= size {
                report.pending += 1;
                continue;
            }
            let leaf = LogLeaf::from_bytes(&e.leaf)?;
            if leaf.account_id != account_id {
                return Err(refuse(format!(
                    "log entry {} belongs to another account",
                    e.index
                )));
            }
            let proof = self
                .transport
                .kt_inclusion(e.index, size)
                .map_err(session_or)?;
            if proof.index != e.index
                || proof.size != size
                || !merkle::verify_inclusion(
                    merkle::leaf_hash(&e.leaf),
                    e.index,
                    size,
                    &proof.path,
                    root,
                )
            {
                return Err(refuse(format!(
                    "log entry {} ({}) isn't in the tree the server signed",
                    e.index,
                    leaf.describe()
                )));
            }
            self.hold_against(&leaf, e.index, expect)?;
            next = e.index + 1;
            report.checked.push(AuditedEntry {
                index: e.index,
                leaf,
            });
        }
        self.view.audited = next;
        // What must be there once the log is caught up: this device's own
        // key, and the newest identity manifest this device holds.
        if report.pending == 0 {
            if !self.view.own_device_logged {
                return Err(refuse(format!(
                    "this device ({}) logged in but the server's log never recorded its key; the server is hiding a device",
                    expect.own_device()
                )));
            }
            if let Some((seq, hash)) = expect.identity_head() {
                match self.view.identity_logged {
                    Some((logged_seq, logged_hash)) if logged_seq == seq && logged_hash == hash => {
                    }
                    Some((logged_seq, _)) if logged_seq > seq => {}
                    _ => {
                        return Err(refuse(format!(
                            "the server accepted identity manifest {seq} but its log doesn't record it; the server is hiding a change"
                        )));
                    }
                }
            }
        }
        Ok(report)
    }

    fn hold_against(
        &mut self,
        leaf: &LogLeaf,
        index: u64,
        expect: &dyn AuditExpectations,
    ) -> Result<(), SyncError> {
        match &leaf.entry {
            LogEntry::Device {
                device_id,
                device_pub,
            } => match expect.device_key(*device_id) {
                Some(known) if known == *device_pub => {
                    if *device_id == expect.own_device() {
                        self.view.own_device_logged = true;
                    }
                }
                Some(_) => {
                    return Err(refuse(format!(
                        "log entry {index}: the server bound device {device_id} to a key that isn't the one your identity lists for it; the server swapped a device key"
                    )));
                }
                None => {
                    return Err(refuse(format!(
                        "log entry {index}: a device ({device_id}) logged in to your account that your identity doesn't list. If it isn't one of yours, your account password is known to someone else: change it and revoke what you don't recognise"
                    )));
                }
            },
            LogEntry::IdentityHead { seq, hash } => {
                if let Some((held_seq, held_hash)) = expect.identity_head() {
                    if *seq == held_seq && *hash != held_hash {
                        return Err(refuse(format!(
                            "log entry {index}: the server's identity manifest {seq} isn't the one this device holds; the server swapped your identity records"
                        )));
                    }
                    if *seq > held_seq {
                        return Err(refuse(format!(
                            "log entry {index}: the server logged identity manifest {seq} but this device was only ever shown {held_seq}; the server is hiding a change"
                        )));
                    }
                }
                if self.view.identity_logged.is_none_or(|(s, _)| s < *seq) {
                    self.view.identity_logged = Some((*seq, *hash));
                }
            }
            LogEntry::VaultHead {
                vault_id,
                seq,
                hash,
            } => {
                if let Some(held) = expect.vault_head(*vault_id, *seq)
                    && held != *hash
                {
                    return Err(refuse(format!(
                        "log entry {index}: the server's head {seq} of vault {vault_id} isn't the one this device holds; the server showed this device another history"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use lockdown_crypto::SigningKey;
    use lockdown_vault::transparency::TreeHead;

    use super::*;
    use crate::transport::{Result as TResult, TransportError};
    use crate::wire;

    /// A log with no server around it: leaves in memory, proofs from the
    /// tree, an STH signed on request. Everything that isn't the log says
    /// "not here".
    struct FakeLog {
        key: SigningKey,
        leaves: RefCell<Vec<Vec<u8>>>,
        /// The account the fake says the session stands for.
        account: RefCell<Id>,
    }

    impl FakeLog {
        fn new() -> Self {
            Self {
                key: SigningKey::generate().unwrap(),
                leaves: RefCell::new(Vec::new()),
                account: RefCell::new(Id::new()),
            }
        }

        fn append(&self, account: Id, entry: LogEntry) -> u64 {
            *self.account.borrow_mut() = account;
            let leaf = LogLeaf::new(account, entry);
            let mut leaves = self.leaves.borrow_mut();
            leaves.push(leaf.to_bytes().unwrap());
            leaves.len() as u64 - 1
        }

        fn hashes(&self) -> Vec<Hash32> {
            self.leaves
                .borrow()
                .iter()
                .map(|b| merkle::leaf_hash(b))
                .collect()
        }

        fn head(&self, size: usize) -> TreeHead {
            let hashes = self.hashes();
            TreeHead {
                size: size as u64,
                root: merkle::root(&hashes[..size]),
                time: 1,
            }
        }

        fn not_here<T>() -> TResult<T> {
            Err(TransportError::Network("not in this test".into()))
        }
    }

    impl Transport for FakeLog {
        fn identity_since(&self, _: &str, _: u64) -> TResult<wire::IdentitySince> {
            Self::not_here()
        }
        fn put_identity(
            &self,
            _: &str,
            _: &wire::IdentityPush,
        ) -> TResult<std::result::Result<wire::HeadAccepted, wire::HeadRefused>> {
            Self::not_here()
        }
        fn list_vaults(&self, _: &str) -> TResult<wire::VaultList> {
            Self::not_here()
        }
        fn put_vault(&self, _: &str, _: Id, _: &[u8]) -> TResult<()> {
            Self::not_here()
        }
        fn heads_since(&self, _: &str, _: Id, _: u64) -> TResult<wire::HeadsSince> {
            Self::not_here()
        }
        fn put_head(
            &self,
            _: &str,
            _: Id,
            _: &[u8],
        ) -> TResult<std::result::Result<wire::HeadAccepted, wire::HeadRefused>> {
            Self::not_here()
        }
        fn items_since(&self, _: &str, _: Id, _: u64) -> TResult<wire::ItemsSince> {
            Self::not_here()
        }
        fn push_item(&self, _: &str, _: Id, _: Id, _: &wire::ItemPush) -> TResult<wire::PushReply> {
            Self::not_here()
        }
        fn put_blob(&self, _: &str, _: Id, _: Hash32, _: &[u8]) -> TResult<wire::BlobAccepted> {
            Self::not_here()
        }
        fn get_blob(&self, _: &str, _: Id, _: Hash32) -> TResult<Option<Vec<u8>>> {
            Self::not_here()
        }
        fn has_blob(&self, _: &str, _: Id, _: Hash32) -> TResult<bool> {
            Self::not_here()
        }
        fn kt_sth(&self) -> TResult<SthWire> {
            let size = self.leaves.borrow().len();
            Ok(SthWire {
                sth: SignedTreeHead::sign(self.head(size), &self.key).unwrap(),
                log_key: self.key.verifying_key().to_bytes(),
            })
        }
        fn kt_inclusion(&self, index: u64, size: u64) -> TResult<InclusionProofWire> {
            let hashes = self.hashes();
            let path = merkle::inclusion_proof(&hashes[..size as usize], index)
                .ok_or_else(|| TransportError::Network("no such leaf".into()))?;
            Ok(InclusionProofWire { index, size, path })
        }
        fn kt_consistency(&self, from: u64, to: u64) -> TResult<ConsistencyProofWire> {
            let hashes = self.hashes();
            let path = merkle::consistency_proof(&hashes[..to as usize], from)
                .ok_or_else(|| TransportError::Network("no such sizes".into()))?;
            Ok(ConsistencyProofWire { from, to, path })
        }
        fn kt_entries(&self, _: &str, from: u64) -> TResult<AccountEntries> {
            let leaves = self.leaves.borrow();
            Ok(AccountEntries {
                account_id: *self.account.borrow(),
                entries: leaves
                    .iter()
                    .enumerate()
                    .skip(from as usize)
                    .map(|(i, b)| LoggedEntry {
                        index: i as u64,
                        leaf: b.clone(),
                    })
                    .collect(),
                size: leaves.len() as u64,
            })
        }
    }

    /// What a device knows, as a table.
    struct Known {
        me: Id,
        devices: BTreeMap<Id, [u8; 32]>,
        identity: Option<(u64, Hash32)>,
        vaults: BTreeMap<(Id, u64), Hash32>,
    }

    impl AuditExpectations for Known {
        fn device_key(&self, device_id: Id) -> Option<[u8; 32]> {
            self.devices.get(&device_id).copied()
        }
        fn own_device(&self) -> Id {
            self.me
        }
        fn identity_head(&self) -> Option<(u64, Hash32)> {
            self.identity
        }
        fn vault_head(&self, vault_id: Id, seq: u64) -> Option<Hash32> {
            self.vaults.get(&(vault_id, seq)).copied()
        }
    }

    fn message(r: Result<impl std::fmt::Debug, SyncError>) -> String {
        match r {
            Err(e) => e.to_string(),
            Ok(v) => panic!("expected a refusal, got {v:?}"),
        }
    }

    /// A log with one account's honest history, and what that account's
    /// device knows about it.
    fn honest() -> (FakeLog, Id, Known, Id) {
        let log = FakeLog::new();
        let account = Id::new();
        let (me, other) = (Id::new(), Id::new());
        let vault = Id::new();
        let (h1, h2) = (Hash32([1; 32]), Hash32([2; 32]));
        let ih = Hash32([7; 32]);
        log.append(
            account,
            LogEntry::Device {
                device_id: me,
                device_pub: [10; 32],
            },
        );
        log.append(account, LogEntry::IdentityHead { seq: 1, hash: ih });
        log.append(
            account,
            LogEntry::VaultHead {
                vault_id: vault,
                seq: 1,
                hash: h1,
            },
        );
        log.append(
            account,
            LogEntry::Device {
                device_id: other,
                device_pub: [20; 32],
            },
        );
        log.append(
            account,
            LogEntry::VaultHead {
                vault_id: vault,
                seq: 2,
                hash: h2,
            },
        );
        let known = Known {
            me,
            devices: BTreeMap::from([(me, [10; 32]), (other, [20; 32])]),
            identity: Some((1, ih)),
            vaults: BTreeMap::from([((vault, 1), h1), ((vault, 2), h2)]),
        };
        (log, account, known, vault)
    }

    #[test]
    fn an_honest_log_audits_clean_and_the_view_moves_forward() {
        let (log, account, known, vault) = honest();
        let _ = account;
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        assert_eq!(session.sth().size, 5);
        let report = session.audit(&known).unwrap();
        assert_eq!(report.checked.len(), 5);
        assert_eq!(report.pending, 0);
        assert_eq!(session.view.audited, 5);
        assert!(session.view.own_device_logged);
        assert_eq!(session.view.identity_logged, Some((1, Hash32([7; 32]))));
        let remembered = session.view.clone();

        // The log grows; the next session proves it continued.
        log.append(
            account,
            LogEntry::VaultHead {
                vault_id: vault,
                seq: 3,
                hash: Hash32([3; 32]),
            },
        );
        let mut session = KtSession::begin(&log, "t", Some(&remembered)).unwrap();
        assert_eq!(session.sth().size, 6);
        // A head this device hasn't pulled yet is left alone.
        let report = session.audit(&known).unwrap();
        assert_eq!(report.checked.len(), 1);
        assert_eq!(session.view.audited, 6);
        // The same size again: nothing new, nothing wrong.
        let again = KtSession::begin(&log, "t", Some(&session.view)).unwrap();
        assert_eq!(again.view.audited, 6);
        // A citation of an earlier or the current size connects; of a
        // later one too once the log has it.
        again.verify_citation(remembered.sth.as_ref()).unwrap();
        again.verify_citation(again.citation()).unwrap();
        log.append(
            account,
            LogEntry::VaultHead {
                vault_id: vault,
                seq: 4,
                hash: Hash32([4; 32]),
            },
        );
        let later = log.head(7);
        again
            .verify_citation(SthRef {
                size: later.size,
                root: later.root,
            })
            .unwrap();
    }

    #[test]
    fn an_older_sth_a_split_view_and_a_rewritten_history_are_refused() {
        let (log, _account, _known, _vault) = honest();
        let session = KtSession::begin(&log, "t", None).unwrap();
        let view = session.view.clone();

        // Remembering a bigger tree than the server now shows: rollback.
        let mut bigger = view.clone();
        bigger.sth = SignedTreeHead::sign(
            TreeHead {
                size: 9,
                root: Hash32([9; 32]),
                time: 1,
            },
            &log.key,
        )
        .unwrap();
        let msg = message(KtSession::begin(&log, "t", Some(&bigger)));
        assert!(msg.contains("Rollback attempt blocked"), "{msg}");

        // The same size with another root: split view.
        let mut other_root = view.clone();
        other_root.sth.root = Hash32([8; 32]);
        let msg = message(KtSession::begin(&log, "t", Some(&other_root)));
        assert!(msg.contains("Split view"), "{msg}");

        // Remembering a smaller tree whose root isn't a prefix: the proof fails.
        let mut forked_past = view.clone();
        forked_past.sth.size = 3;
        forked_past.sth.root = Hash32([6; 32]);
        let msg = message(KtSession::begin(&log, "t", Some(&forked_past)));
        assert!(msg.contains("history was rewritten"), "{msg}");

        // A changed log key is never taken.
        let mut other_key = view.clone();
        other_key.log_key = SigningKey::generate().unwrap().verifying_key().to_bytes();
        let msg = message(KtSession::begin(&log, "t", Some(&other_key)));
        assert!(msg.contains("log key changed"), "{msg}");

        // A citation from another device that this log can't connect to.
        let msg = message(session.verify_citation(SthRef {
            size: 5,
            root: Hash32([5; 32]),
        }));
        assert!(msg.contains("Split view"), "{msg}");
        let msg = message(session.verify_citation(SthRef {
            size: 2,
            root: Hash32([2; 32]),
        }));
        assert!(msg.contains("Split view"), "{msg}");
    }

    #[test]
    fn a_swapped_device_key_an_unknown_device_and_hidden_changes_are_caught() {
        // The server binds the other device to a key the identity doesn't
        // list for it: the inclusion check passes (the log is honest about
        // what the server did) and the audit refuses it by name.
        let (log, _account, mut known, vault) = honest();
        let other = *known.devices.keys().find(|d| **d != known.me).unwrap();
        known.devices.insert(other, [21; 32]);
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("swapped a device key"), "{msg}");

        // A device logged in that the identity doesn't list at all.
        let (log, account, known, _) = honest();
        log.append(
            account,
            LogEntry::Device {
                device_id: Id::new(),
                device_pub: [30; 32],
            },
        );
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("doesn't list"), "{msg}");
        assert!(msg.contains("password"), "{msg}");

        // The identity manifest in the log isn't the one this device holds.
        let (log, _account, mut known, _) = honest();
        known.identity = Some((1, Hash32([77; 32])));
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("swapped your identity records"), "{msg}");

        // A newer identity manifest than this device was ever shown.
        let (log, account, known, _) = honest();
        log.append(
            account,
            LogEntry::IdentityHead {
                seq: 2,
                hash: Hash32([70; 32]),
            },
        );
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("hiding a change"), "{msg}");

        // A vault head with another hash than this device holds at that seq.
        let (log, _account, mut known, vault_id) = honest();
        known.vaults.insert((vault_id, 2), Hash32([22; 32]));
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("another history"), "{msg}");

        // The identity manifest this device holds was never logged.
        let (log, _account, mut known, _) = honest();
        known.identity = Some((2, Hash32([71; 32])));
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("doesn't record it"), "{msg}");

        // This device's own key never appears: the server hid a device.
        let log = FakeLog::new();
        let account = Id::new();
        log.append(
            account,
            LogEntry::VaultHead {
                vault_id: vault,
                seq: 1,
                hash: Hash32([1; 32]),
            },
        );
        let known = Known {
            me: Id::new(),
            devices: BTreeMap::new(),
            identity: None,
            vaults: BTreeMap::new(),
        };
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("hiding a device"), "{msg}");

        // Another account's leaf served as ours: the server says the
        // session is account X and hands over a leaf of account Y.
        let log = FakeLog::new();
        log.append(
            Id::new(),
            LogEntry::Device {
                device_id: known.me,
                device_pub: [1; 32],
            },
        );
        *log.account.borrow_mut() = account;
        let mut session = KtSession::begin(&log, "t", None).unwrap();
        let msg = message(session.audit(&known));
        assert!(msg.contains("another account"), "{msg}");
    }

    #[test]
    fn the_wire_types_round_trip() {
        let entries = AccountEntries {
            account_id: Id::new(),
            entries: vec![LoggedEntry {
                index: 3,
                leaf: vec![1, 2],
            }],
            size: 4,
        };
        let bytes = crate::wire::encode(&entries).unwrap();
        assert_eq!(
            crate::wire::decode::<AccountEntries>(&bytes).unwrap(),
            entries
        );
        let proof = ConsistencyProofWire {
            from: 1,
            to: 2,
            path: vec![Hash32::ZERO],
        };
        let bytes = crate::wire::encode(&proof).unwrap();
        assert_eq!(
            crate::wire::decode::<ConsistencyProofWire>(&bytes).unwrap(),
            proof
        );
    }
}
