//! `lockdown-sync`: sync (03-architecture §2 `sync` row and §6).
//!
//! **Status: skeleton with the merge rule real.** The unit of sync is the
//! item; each carries a server-assigned [`Revision`]. A write sends the
//! revision it last saw; if the server's moved, the write is refused with
//! the current item and the client merges field by field. Edits to
//! different fields merge automatically; edits to the same field become a
//! [`ConflictCopy`] and the person decides. Nothing is overwritten silently.
//! That three-way merge is implemented and tested here over decrypted
//! fields. The transport, the offline queue, the key-transparency check and
//! the checkpoint proof are not built yet (they need the server).

use std::collections::BTreeMap;

use lockdown_vault::PlainField;
use serde::{Deserialize, Serialize};

/// A server-assigned, monotonically rising revision of one item.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct Revision(pub u64);

/// What the server says to a push.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PushOutcome {
    /// Stored; this is the new revision.
    Accepted(Revision),
    /// The revision moved; here is what the server holds now.
    Refused {
        /// The server's current revision.
        current: Revision,
    },
}

/// Two edits touched the same field. Both values are kept; the person picks.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConflictCopy {
    /// The field both sides changed.
    pub field_id: String,
    /// Our value.
    pub ours: PlainField,
    /// Their value.
    pub theirs: PlainField,
}

/// The result of a three-way merge.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MergeResult {
    /// The merged fields, ours where only we changed, theirs where only they did.
    pub merged: Vec<PlainField>,
    /// Fields both sides changed differently.
    pub conflicts: Vec<ConflictCopy>,
}

impl MergeResult {
    /// Whether the merge was clean.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

fn by_id(fields: &[PlainField]) -> BTreeMap<&str, &PlainField> {
    fields.iter().map(|f| (f.field_id.as_str(), f)).collect()
}

/// Merges `ours` and `theirs` against the common `base`, field by field.
/// A field is "changed" on a side when its value differs from the base
/// (or it was added or removed). Order follows `ours`, then anything only
/// in `theirs`.
pub fn merge_fields(
    base: &[PlainField],
    ours: &[PlainField],
    theirs: &[PlainField],
) -> MergeResult {
    let b = by_id(base);
    let o = by_id(ours);
    let t = by_id(theirs);
    let mut ids: Vec<&str> = ours.iter().map(|f| f.field_id.as_str()).collect();
    for f in theirs {
        if !o.contains_key(f.field_id.as_str()) {
            ids.push(f.field_id.as_str());
        }
    }
    for f in base {
        if !o.contains_key(f.field_id.as_str()) && !t.contains_key(f.field_id.as_str()) {
            // Removed on both sides: stays removed.
            continue;
        }
    }
    let mut result = MergeResult::default();
    for id in ids {
        let (bv, ov, tv) = (b.get(id), o.get(id), t.get(id));
        let ours_changed = ov != bv;
        let theirs_changed = tv != bv;
        match (ours_changed, theirs_changed) {
            (false, false) | (true, false) => {
                if let Some(f) = ov {
                    result.merged.push((*f).clone());
                }
            }
            (false, true) => {
                if let Some(f) = tv {
                    result.merged.push((*f).clone());
                }
            }
            (true, true) => {
                if ov == tv {
                    if let Some(f) = ov {
                        result.merged.push((*f).clone());
                    }
                } else if let (Some(a), Some(c)) = (ov, tv) {
                    result.merged.push((*a).clone());
                    result.conflicts.push(ConflictCopy {
                        field_id: id.to_string(),
                        ours: (*a).clone(),
                        theirs: (*c).clone(),
                    });
                } else if let Some(f) = ov.or(tv) {
                    // One side removed it, the other changed it: keep the change and flag it.
                    result.merged.push((*f).clone());
                    result.conflicts.push(ConflictCopy {
                        field_id: id.to_string(),
                        ours: ov.map(|f| (*f).clone()).unwrap_or_else(|| (*f).clone()),
                        theirs: tv.map(|f| (*f).clone()).unwrap_or_else(|| (*f).clone()),
                    });
                }
            }
        }
    }
    result
}

/// Remembers the newest manifest sequence a device accepted per vault, and
/// refuses anything older (03-architecture §6 "rollback check on every pull").
#[derive(Default, Debug, Clone)]
pub struct RollbackGuard {
    seen: BTreeMap<lockdown_vault::Id, u64>,
}

/// Why a pull was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SyncError {
    /// The server served an older head than this device has seen.
    #[error(
        "Rollback attempt blocked: the server offered manifest {offered} but this device has already accepted {remembered}"
    )]
    Rollback {
        /// What the server offered.
        offered: u64,
        /// What this device remembers.
        remembered: u64,
    },
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

impl RollbackGuard {
    /// Accepts `seq` for `vault_id` if it is not older than what was seen.
    pub fn accept(&mut self, vault_id: lockdown_vault::Id, seq: u64) -> Result<(), SyncError> {
        let remembered = self.seen.get(&vault_id).copied().unwrap_or(0);
        if seq < remembered {
            return Err(SyncError::Rollback {
                offered: seq,
                remembered,
            });
        }
        self.seen.insert(vault_id, seq);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_vault::{FieldKind, FieldValue};

    fn f(id: &str, v: &str) -> PlainField {
        PlainField::new(id, FieldKind::Text, id, FieldValue::Text(v.into()))
    }

    #[test]
    fn different_fields_merge_and_same_field_conflicts() {
        let base = [f("title", "Bank"), f("user", "me"), f("pw", "old")];
        let ours = [f("title", "Bank"), f("user", "me@new"), f("pw", "old")];
        let theirs = [f("title", "Bank"), f("user", "me"), f("pw", "new")];
        let r = merge_fields(&base, &ours, &theirs);
        assert!(r.is_clean());
        assert_eq!(
            r.merged,
            vec![f("title", "Bank"), f("user", "me@new"), f("pw", "new")]
        );

        let theirs2 = [
            f("title", "Bank"),
            f("user", "someone-else"),
            f("pw", "old"),
        ];
        let r = merge_fields(&base, &ours, &theirs2);
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].field_id, "user");
        assert_eq!(r.conflicts[0].ours, f("user", "me@new"));
        assert_eq!(r.conflicts[0].theirs, f("user", "someone-else"));
        // Ours stays in the merged list until the person picks.
        assert!(r.merged.contains(&f("user", "me@new")));

        // Added on one side only: kept. Same edit on both sides: no conflict.
        let ours3 = [
            f("title", "Bank"),
            f("user", "me"),
            f("pw", "same"),
            f("note", "hi"),
        ];
        let theirs3 = [f("title", "Bank"), f("user", "me"), f("pw", "same")];
        let r = merge_fields(&base, &ours3, &theirs3);
        assert!(r.is_clean());
        assert_eq!(r.merged.len(), 4);
    }

    #[test]
    fn rollback_is_blocked_loudly() {
        let v = lockdown_vault::Id::new();
        let mut g = RollbackGuard::default();
        g.accept(v, 100).unwrap();
        g.accept(v, 100).unwrap();
        g.accept(v, 102).unwrap();
        let err = g.accept(v, 101).unwrap_err();
        assert!(err.to_string().starts_with("Rollback attempt blocked"));
    }
}
