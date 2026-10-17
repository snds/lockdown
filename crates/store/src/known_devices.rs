//! Devices this device has learned about from the identity's signed
//! records, kept in a file of its own (`known-devices.cbor`).
//!
//! Another device enrolled in the same identity (through the kit or the
//! account slot on a second machine) signs heads and versions with a key
//! this machine's identity store has never seen: the identity crate does
//! not sync its records between machines yet. The server relays each
//! account's identity backup, and the `Device` records in it are signed
//! by the Identity Signing Key, whose public half this machine holds in
//! its own profile. So a record is trusted when *that* signature checks
//! out, never because the server said so, and once learned it is kept
//! here so the vault still opens offline.
//!
//! Devices are only ever added. Refusing a revoked device's new heads is
//! the sync step's job (07-workspace §5 q.6).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecosystem_identity::records::Device;
use ecosystem_identity::{Record, ed25519_dalek};
use lockdown_crypto::VerifyingKey;
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};

use crate::remembered::write_private;
use crate::{StoreError, StoreResult};

const KNOWN_V: u16 = 1;

#[derive(Default, Serialize, Deserialize)]
struct KnownFile {
    v: u16,
    #[serde(default)]
    devices: BTreeMap<Id, Device>,
}

/// The learned devices, backed by one file.
pub struct KnownDevices {
    path: PathBuf,
    file: KnownFile,
}

impl std::fmt::Debug for KnownDevices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnownDevices")
            .field("path", &self.path)
            .field("devices", &self.file.devices.len())
            .finish()
    }
}

impl KnownDevices {
    /// Opens the file at `path`, or starts empty if there is none.
    pub fn open(path: &Path) -> StoreResult<Self> {
        let file = match std::fs::read(path) {
            Ok(bytes) => {
                let file: KnownFile = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| StoreError::Tampered("the known-devices file".into()))?;
                if file.v > KNOWN_V {
                    return Err(StoreError::SchemaTooNew {
                        found: u32::from(file.v),
                        supported: u32::from(KNOWN_V),
                    });
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => KnownFile {
                v: KNOWN_V,
                ..Default::default()
            },
            Err(e) => return Err(StoreError::Backend(e.to_string())),
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Learns every record in `devices` whose signature checks out under
    /// `isk_pub` (the profile's Identity Signing Key). Returns how many
    /// were new. A record that fails its signature is skipped and counted
    /// in the error, never kept: the server may have changed it.
    pub fn learn(&mut self, devices: &[Device], isk_pub: &[u8; 32]) -> StoreResult<usize> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(isk_pub)
            .map_err(|_| StoreError::Backend("the profile's signing key isn't usable".into()))?;
        let mut added = 0;
        let mut bad = 0;
        for device in devices {
            if device.verify(&key).is_err() {
                bad += 1;
                continue;
            }
            if self
                .file
                .devices
                .insert(device.device_id, device.clone())
                .is_none()
            {
                added += 1;
            }
        }
        if added > 0 {
            let bytes = lockdown_crypto::cbor::to_canonical(&self.file)
                .map_err(|e| StoreError::Backend(e.to_string()))?;
            write_private(&self.path, &bytes).map_err(|e| StoreError::Backend(e.to_string()))?;
        }
        if bad > 0 {
            return Err(StoreError::Tampered(format!(
                "{bad} device record(s) in the identity backup (their signatures didn't check out; the rest were kept)"
            )));
        }
        Ok(added)
    }

    /// The learned records.
    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.file.devices.values()
    }

    /// The learned devices' public keys, by id.
    pub fn keys(&self) -> BTreeMap<Id, VerifyingKey> {
        self.file
            .devices
            .values()
            .filter_map(|d| {
                VerifyingKey::from_bytes(&d.device_pub)
                    .ok()
                    .map(|k| (d.device_id, k))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::{FileKeystore, Identity};

    #[test]
    fn only_records_the_profile_signed_are_learned_and_they_persist() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let identity =
            Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
        let isk_pub = identity.profile().isk_pub;
        let devices: Vec<Device> = identity.devices().cloned().collect();
        assert_eq!(devices.len(), 1);

        let path = dir.path().join("known-devices.cbor");
        let mut known = KnownDevices::open(&path).unwrap();
        assert_eq!(known.learn(&devices, &isk_pub).unwrap(), 1);
        assert_eq!(known.learn(&devices, &isk_pub).unwrap(), 0);
        assert_eq!(known.keys().len(), 1);
        assert_eq!(
            known.keys()[&devices[0].device_id].to_bytes(),
            devices[0].device_pub
        );

        // A record with a swapped key fails its signature and is not kept.
        let mut forged = devices[0].clone();
        forged.device_id = Id::new();
        forged.device_pub = [7; 32];
        let err = known.learn(&[forged.clone()], &isk_pub).unwrap_err();
        assert!(matches!(err, StoreError::Tampered(_)), "{err}");
        assert_eq!(known.keys().len(), 1);
        // And under another profile's key, nothing is learned either.
        let other = FileKeystore::open_or_create(&dir.path().join("k2.cbor")).unwrap();
        let other_identity =
            Identity::create(&dir.path().join("root2"), "lockdown", "phone", &other).unwrap();
        assert!(
            known
                .learn(&devices, &other_identity.profile().isk_pub)
                .is_err()
        );

        let again = KnownDevices::open(&path).unwrap();
        assert_eq!(again.devices().count(), 1);
    }
}
