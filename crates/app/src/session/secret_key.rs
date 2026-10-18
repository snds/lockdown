//! The Secret Key kept on this device (`secret-key`;
//! docs/plan/22-security-depth.md §3), so an enrolled device can hand it to
//! a new one it approves (01-identity §0: "setting up a new device normally
//! means scanning a QR on an old one; the kit stays in the drawer").
//!
//! The file `secret-key.cbor` beside the device key holds the 16 bytes
//! sealed with XChaCha20-Poly1305 under this device's keystore unlock key,
//! bound to the profile and the device. That key already opens this
//! device's wrap of the root key, which reaches strictly more than the
//! Secret Key does, so keeping it here adds no new way in; it is never
//! synced, never logged and never sent anywhere but sealed to a device the
//! person approved. A device learns it only after checking it: the account
//! slot must open with it and the account password's export key.
//!
//! When the account was made without a Secret Key (the documented opt-out,
//! 04-security §3), the file says so instead, and an approval tells the new
//! device the same.

use std::path::{Path, PathBuf};

use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::{ExportKey, Identity};
use lockdown_crypto::aead::{self, Sealed};
use lockdown_crypto::cbor::to_canonical;
use lockdown_store::write_private;
use lockdown_vault::Id;
use lockdown_vault::device_link::SecretKeyState;
use serde::{Deserialize, Serialize};

use super::account_envelope;

const FILE: &str = "secret-key.cbor";
const FILE_V: u16 = 1;
const LABEL: &str = "lockdown/secret-key-at-rest/v1";

#[derive(Serialize, Deserialize)]
struct Kept {
    v: u16,
    profile_id: Id,
    device_id: Id,
    /// The account has no Secret Key.
    #[serde(default)]
    opted_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sealed: Option<Sealed>,
}

fn path(data: &Path) -> PathBuf {
    data.join(FILE)
}

fn aad(profile_id: Id, device_id: Id) -> Result<Vec<u8>, String> {
    #[derive(Serialize)]
    struct Place<'a> {
        label: &'a str,
        profile_id: Id,
        device_id: Id,
    }
    to_canonical(&Place {
        label: LABEL,
        profile_id,
        device_id,
    })
    .map_err(|e| e.to_string())
}

fn write(data: &Path, kept: &Kept) -> Result<(), String> {
    let bytes = to_canonical(kept).map_err(|e| e.to_string())?;
    write_private(&path(data), &bytes).map_err(|e| format!("couldn't keep the Secret Key: {e}"))
}

/// Keeps `secret_key` for `profile_id`, sealed under this device's keystore.
pub(crate) fn keep(
    data: &Path,
    keystore: &dyn DeviceKeystore,
    profile_id: Id,
    secret_key: &[u8; 16],
) -> Result<(), String> {
    let unlock = keystore.unlock().map_err(|e| e.to_string())?;
    let sealed = aead::seal(&unlock, &aad(profile_id, keystore.device_id())?, secret_key)
        .map_err(|e| e.to_string())?;
    write(
        data,
        &Kept {
            v: FILE_V,
            profile_id,
            device_id: keystore.device_id(),
            opted_out: false,
            sealed: Some(sealed),
        },
    )
}

/// Records that the account has no Secret Key (the opt-out).
pub(crate) fn keep_opted_out(
    data: &Path,
    keystore: &dyn DeviceKeystore,
    profile_id: Id,
) -> Result<(), String> {
    write(
        data,
        &Kept {
            v: FILE_V,
            profile_id,
            device_id: keystore.device_id(),
            opted_out: true,
            sealed: None,
        },
    )
}

/// Forgets it (the kit was replaced, or the device leaves).
pub(crate) fn forget(data: &Path) {
    let p = path(data);
    if p.exists() {
        let _ = std::fs::write(&p, [0u8; 64]);
        let _ = std::fs::remove_file(&p);
    }
}

/// What this device can hand on: the Secret Key, the opt-out, or nothing.
pub(crate) fn state(data: &Path, keystore: &dyn DeviceKeystore, profile_id: Id) -> SecretKeyState {
    let Ok(bytes) = std::fs::read(path(data)) else {
        return SecretKeyState::NotHeld;
    };
    let Ok(kept) = lockdown_crypto::cbor::from_slice::<Kept>(&bytes) else {
        return SecretKeyState::NotHeld;
    };
    if kept.v > FILE_V || kept.profile_id != profile_id || kept.device_id != keystore.device_id() {
        return SecretKeyState::NotHeld;
    }
    if kept.opted_out {
        return SecretKeyState::OptedOut;
    }
    let (Some(sealed), Ok(unlock), Ok(place)) = (
        kept.sealed.as_ref(),
        keystore.unlock(),
        aad(profile_id, keystore.device_id()),
    ) else {
        return SecretKeyState::NotHeld;
    };
    match aead::open(&unlock, &place, sealed) {
        Ok(plain) => match <[u8; 16]>::try_from(plain.as_slice()) {
            Ok(sk) => SecretKeyState::Held(sk),
            Err(_) => SecretKeyState::NotHeld,
        },
        Err(_) => SecretKeyState::NotHeld,
    }
}

/// The word a status shows: `kept`, `opted-out` or `not-kept`.
pub(crate) fn state_word(state: &SecretKeyState) -> &'static str {
    match state {
        SecretKeyState::Held(_) => "kept",
        SecretKeyState::OptedOut => "opted-out",
        SecretKeyState::NotHeld => "not-kept",
    }
}

/// Whether `secret_key` (or none, for the opt-out) opens the identity's
/// account slot with `export_key`: the check before a device keeps it.
pub(crate) fn opens_account_slot(
    identity: &Identity,
    export_key: &ExportKey,
    secret_key: Option<&[u8; 16]>,
) -> bool {
    let profile_id = identity.profile().profile_id;
    let Some(envelope) = account_envelope(identity) else {
        return false;
    };
    let Ok(auk) =
        ecosystem_identity::kdf::account_unlock_key(export_key.expose(), secret_key, &profile_id)
    else {
        return false;
    };
    let opened: Result<ecosystem_identity::keys::Irk, _> = ecosystem_identity::keys::unwrap_key(
        envelope,
        &auk,
        &ecosystem_identity::keys::aad(
            profile_id,
            None,
            envelope.key_id,
            ecosystem_identity::records::Purpose::Irk,
        ),
    );
    opened.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    #[test]
    fn the_secret_key_is_kept_sealed_to_this_device_and_profile() {
        let dir = tempfile::tempdir().unwrap();
        let ks = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let profile = Id::new();
        assert_eq!(state(dir.path(), &ks, profile), SecretKeyState::NotHeld);
        keep(dir.path(), &ks, profile, &[3; 16]).unwrap();
        assert_eq!(
            state(dir.path(), &ks, profile),
            SecretKeyState::Held([3; 16])
        );
        // The file says nothing readable, and opens for no other profile or device.
        let raw = std::fs::read(dir.path().join(FILE)).unwrap();
        assert!(!raw.windows(16).any(|w| w == [3; 16]));
        assert_eq!(state(dir.path(), &ks, Id::new()), SecretKeyState::NotHeld);
        let other = FileKeystore::open_or_create(&dir.path().join("o.cbor")).unwrap();
        assert_eq!(state(dir.path(), &other, profile), SecretKeyState::NotHeld);
        keep_opted_out(dir.path(), &ks, profile).unwrap();
        assert_eq!(state(dir.path(), &ks, profile), SecretKeyState::OptedOut);
        forget(dir.path());
        assert_eq!(state(dir.path(), &ks, profile), SecretKeyState::NotHeld);
    }
}
