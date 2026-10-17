//! The identity's record set over the wire (D71): pulled before any vault
//! head, so a device knows every other device (and every revocation)
//! before it judges a signature; pushed whenever this device's records
//! differ from what the server holds.
//!
//! The rules are the vault chain's: the server's current `seq` below what
//! this device accepted is a rollback; a page that does not chain to the
//! accepted one is a fork; and `SyncedIdentity::accept` checks every
//! signature under the profile's own key and that revocations only grew.

use lockdown_store::SyncedIdentity;
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};

use crate::SyncError;
use crate::engine::session_or;
use crate::transport::Transport;
use crate::wire::{HeadAccepted, HeadRefused, IdentityPush, RecordWire};

/// Pulls every identity manifest after the one `synced` holds and accepts
/// the newest with its records. `isk_pub` is the profile's key when this
/// device holds the identity; `None` on an empty device, which then
/// anchors on the kit or the password when it opens the root key. Returns
/// whether anything new was accepted.
pub fn pull_identity(
    transport: &dyn Transport,
    token: &str,
    synced: &mut SyncedIdentity,
    isk_pub: Option<&[u8; 32]>,
) -> Result<bool, SyncError> {
    let since = synced.seq();
    let remote = transport.identity_since(token, since).map_err(session_or)?;
    if remote.current_seq < since {
        return Err(SyncError::Rollback {
            offered: remote.current_seq,
            remembered: since,
        });
    }
    if remote.manifests.is_empty() {
        if since > 0 && remote.current_hash != synced.hash() {
            return Err(SyncError::Fork { seq: since });
        }
        return Ok(false);
    }
    let mut manifests = Vec::with_capacity(remote.manifests.len());
    for bytes in &remote.manifests {
        manifests.push(IdentityManifest::from_bytes(bytes)?);
    }
    let newest = manifests.pop().expect("not empty");
    let set = RecordSet {
        manifest: newest,
        records: remote
            .records
            .into_iter()
            .map(|r| (r.path, r.body))
            .collect(),
    };
    synced.accept(&manifests, set, isk_pub)?;
    Ok(true)
}

/// Pushes `set` (signed, continuing what `synced` holds) with the records
/// the server may lack, and on acceptance makes it what this device holds.
/// A chain refusal (`fork`, `gap`, `older_seq`, `boundary_moved`) comes
/// back as `Ok(Err(_))` for the caller to pull and try again.
pub fn push_identity(
    transport: &dyn Transport,
    token: &str,
    set: &RecordSet,
    synced: &mut SyncedIdentity,
    isk_pub: Option<&[u8; 32]>,
) -> Result<Result<HeadAccepted, HeadRefused>, SyncError> {
    let held = synced.record_set();
    let records = set
        .records
        .iter()
        .filter(|(path, body)| {
            held.as_ref()
                .and_then(|h| h.records.get(*path))
                .is_none_or(|mine| mine != *body)
        })
        .map(|(path, body)| RecordWire {
            path: path.clone(),
            body: body.clone(),
        })
        .collect();
    let push = IdentityPush {
        manifest: set.manifest.to_bytes()?,
        records,
    };
    match transport.put_identity(token, &push).map_err(session_or)? {
        Ok(accepted) => {
            synced.accept(&[], set.clone(), isk_pub)?;
            Ok(Ok(accepted))
        }
        Err(refused) => Ok(Err(refused)),
    }
}
