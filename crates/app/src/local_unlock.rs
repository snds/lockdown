//! The device's own keys, and the master password that locks them at rest
//! (docs/plan/39-first-run.md §1.3, D461).
//!
//! A device holds three secrets the identity never stores: the unlock key
//! that opens the device's wrap of the identity root key, the Ed25519 seed
//! that identifies it, and the receiving key's seed. The identity crate's
//! file keystore keeps them in plain bytes (`device-key.cbor`), so anyone
//! who copies the data directory could open the vault. Here they are
//! sealed with XChaCha20-Poly1305 in `device-key.sealed.cbor`:
//!
//! ```text
//! master password ──Argon2id(salt, params ≥ the desktop floor, ~1 s)──► KEK
//! recovery kit    ──the kit's own stretching, then HKDF──────────────► kit key
//! device-key.sealed.cbor:
//!   plain   {v, device_id, device_pub, kem_alg, created_at, profile_id?}
//!   password slot {salt, params, sealed secrets}   AAD = {label, v, device_id, "password"}
//!   kit slot      {params, sealed secrets}         AAD = {label, v, device_id, "kit"}
//! ```
//!
//! The password slot opens the vault on every unlock; it never syncs and
//! never talks to a server, so unlock works offline and after a move to a
//! real server. The kit slot is how "Forgot your password?" gets back in
//! on this computer: the kit opens the same secrets, and a new master
//! password seals them again. The public halves stay plain, so a locked
//! app can still say which device it is.
//!
//! [`DeviceKeys`] holds the secrets in memory while the vault is open and
//! implements the identity crate's [`DeviceKeystore`]; every secret is
//! wiped when it drops.

use std::path::{Path, PathBuf};

use ecosystem_identity::ed25519_dalek::{Signer as _, SigningKey};
use ecosystem_identity::kem::{self, DEFAULT_KEM, KemAlg, KemPublicKey, KemSecretKey};
use ecosystem_identity::keys::{DeviceUnlockKey, SymmetricKey};
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::{Platform, Timestamp};
use ecosystem_identity::{Argon2Params, RecoveryKit};
use lockdown_crypto::Key32;
use lockdown_crypto::aead::{self, Sealed};
use lockdown_crypto::cbor::{from_slice, to_canonical};
use lockdown_store::write_private;
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// The identity crate's plain key file (kept until a master password is set).
pub const PLAIN_FILE: &str = "device-key.cbor";
/// The keys sealed under the master password and the recovery kit.
pub const SEALED_FILE: &str = "device-key.sealed.cbor";
const PLAIN_V: u16 = 1;
const SEALED_V: u16 = 1;
const LABEL: &str = "lockdown/local-unlock/v1";
const KIT_INFO: &[u8] = b"lockdown/local-unlock/kit/v1";
/// What a stretching aims for on this computer.
const CALIBRATE_TO: std::time::Duration = std::time::Duration::from_secs(1);

/// The words for a wrong master password, everywhere.
pub const WRONG_PASSWORD: &str = "That isn't the master password for this vault.";
/// The words for a vault that needs its master password to open.
pub const PASSWORD_NEEDED: &str = "This vault is locked with a master password.";

/// Which key file is here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyFile {
    /// Neither: a device that holds nothing yet.
    None,
    /// The plain file alone: no master password yet (an older install, or
    /// a device that just joined an account).
    Plain,
    /// The sealed file: the master password opens it.
    Sealed,
}

/// Which key file `data` holds. A sealed file wins over a plain one left
/// by a switch that stopped half way (§1.6).
pub fn key_file(data: &Path) -> KeyFile {
    if data.join(SEALED_FILE).exists() {
        KeyFile::Sealed
    } else if data.join(PLAIN_FILE).exists() {
        KeyFile::Plain
    } else {
        KeyFile::None
    }
}

/// The three secrets, wiped on drop.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct Secrets {
    #[serde(with = "serde_bytes")]
    unlock_key: [u8; 32],
    #[serde(with = "serde_bytes")]
    signing_seed: [u8; 32],
    #[serde(with = "serde_bytes")]
    kem_seed: [u8; 32],
}

/// The identity crate's `device-key.cbor`, field for field, so a file
/// either side wrote reads on the other.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct PlainFile {
    v: u16,
    #[zeroize(skip)]
    device_id: Id,
    #[serde(with = "serde_bytes")]
    unlock_key: [u8; 32],
    #[serde(with = "serde_bytes")]
    signing_seed: [u8; 32],
    #[zeroize(skip)]
    kem_alg: KemAlg,
    #[serde(with = "serde_bytes")]
    kem_seed: [u8; 32],
    #[zeroize(skip)]
    created_at: Timestamp,
}

#[derive(Clone, Serialize, Deserialize)]
struct PasswordSlot {
    #[serde(with = "serde_bytes")]
    salt: Vec<u8>,
    params: Argon2Params,
    sealed: Sealed,
}

#[derive(Clone, Serialize, Deserialize)]
struct KitSlot {
    params: Argon2Params,
    sealed: Sealed,
}

#[derive(Clone, Serialize, Deserialize)]
struct SealedFile {
    v: u16,
    device_id: Id,
    #[serde(with = "serde_bytes")]
    device_pub: Vec<u8>,
    kem_alg: KemAlg,
    created_at: Timestamp,
    /// The profile whose kit opens the kit slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile_id: Option<Id>,
    password: PasswordSlot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kit: Option<KitSlot>,
}

/// The device's keys, open in memory.
pub struct DeviceKeys {
    device_id: Id,
    kem_alg: KemAlg,
    created_at: Timestamp,
    secrets: Secrets,
    platform: Platform,
}

impl std::fmt::Debug for DeviceKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeviceKeys(device {})", self.device_id)
    }
}

fn aad(device_id: Id, slot: &str) -> Result<Vec<u8>, String> {
    #[derive(Serialize)]
    struct Place<'a> {
        label: &'a str,
        v: u16,
        device_id: Id,
        slot: &'a str,
    }
    to_canonical(&Place {
        label: LABEL,
        v: SEALED_V,
        device_id,
        slot,
    })
    .map_err(|e| e.to_string())
}

fn password_key(password: &str, salt: &[u8], params: &Argon2Params) -> Result<Key32, String> {
    let kek = lockdown_crypto::kdf::stretch(password.as_bytes(), salt, params)
        .map_err(|e| e.to_string())?;
    Ok(Key32::from_bytes(*kek))
}

fn kit_key(kit: &RecoveryKit, params: &Argon2Params) -> Result<Key32, String> {
    let key = kit.kit_key(params).map_err(|e| e.to_string())?;
    let sub = lockdown_crypto::kdf::subkey(key.expose(), KIT_INFO).map_err(|e| e.to_string())?;
    Ok(Key32::from_bytes(*sub))
}

fn read_sealed(data: &Path) -> Result<SealedFile, String> {
    let path = data.join(SEALED_FILE);
    let bytes =
        std::fs::read(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
    let file: SealedFile = from_slice(&bytes).map_err(|_| {
        format!(
            "{} didn't decode; it was changed or is damaged",
            path.display()
        )
    })?;
    if file.v > SEALED_V {
        return Err(format!(
            "{} was written by a newer Lockdown; update the app",
            path.display()
        ));
    }
    if !file.password.params.meets(&lockdown_crypto::kdf::floor()) {
        return Err(format!(
            "{} asks for less password stretching than Lockdown allows, so it isn't trusted",
            path.display()
        ));
    }
    Ok(file)
}

impl DeviceKeys {
    /// Fresh keys, kept nowhere yet.
    pub fn generate() -> Result<Self, String> {
        let r = || lockdown_crypto::rand::random_32().map_err(|e| e.to_string());
        Ok(Self {
            device_id: Id::new(),
            kem_alg: DEFAULT_KEM,
            created_at: Timestamp::now(),
            secrets: Secrets {
                unlock_key: r()?,
                signing_seed: r()?,
                kem_seed: r()?,
            },
            platform: Platform::current(),
        })
    }

    fn from_plain(file: &PlainFile) -> Result<Self, String> {
        if file.v != PLAIN_V {
            return Err(format!(
                "the device key file is version {}, which this app doesn't read",
                file.v
            ));
        }
        Ok(Self {
            device_id: file.device_id,
            kem_alg: file.kem_alg,
            created_at: file.created_at,
            secrets: Secrets {
                unlock_key: file.unlock_key,
                signing_seed: file.signing_seed,
                kem_seed: file.kem_seed,
            },
            platform: Platform::current(),
        })
    }

    /// Opens the plain key file in `data`: an install with no master
    /// password yet. Refused once the keys are sealed.
    pub fn open_plain(data: &Path) -> Result<Self, String> {
        if key_file(data) == KeyFile::Sealed {
            return Err(PASSWORD_NEEDED.into());
        }
        let path = data.join(PLAIN_FILE);
        let bytes = Zeroizing::new(
            std::fs::read(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?,
        );
        let file: PlainFile = from_slice(&bytes).map_err(|e| e.to_string())?;
        Self::from_plain(&file)
    }

    /// Opens the plain key file in `data`, or makes one with fresh keys:
    /// what a device that joins an account starts with (its master password
    /// is set right after, §1.9 screen J). Refused once the keys are sealed.
    pub fn open_or_create_plain(data: &Path) -> Result<Self, String> {
        if key_file(data) == KeyFile::None {
            let keys = Self::generate()?;
            keys.write_plain(data)?;
            return Ok(keys);
        }
        Self::open_plain(data)
    }

    fn write_plain(&self, data: &Path) -> Result<(), String> {
        crate::paths::make_dir(data)?;
        let file = PlainFile {
            v: PLAIN_V,
            device_id: self.device_id,
            unlock_key: self.secrets.unlock_key,
            signing_seed: self.secrets.signing_seed,
            kem_alg: self.kem_alg,
            kem_seed: self.secrets.kem_seed,
            created_at: self.created_at,
        };
        let bytes = Zeroizing::new(to_canonical(&file).map_err(|e| e.to_string())?);
        write_private(&data.join(PLAIN_FILE), &bytes).map_err(|e| e.to_string())
    }

    /// Opens the sealed keys with the master password.
    pub fn open_with_password(data: &Path, password: &str) -> Result<Self, String> {
        let file = read_sealed(data)?;
        let key = password_key(password, &file.password.salt, &file.password.params)?;
        let opened = aead::open(
            &key,
            &aad(file.device_id, "password")?,
            &file.password.sealed,
        )
        .map_err(|_| WRONG_PASSWORD.to_string())?;
        Self::from_sealed(&file, &opened)
    }

    /// Opens the sealed keys with the recovery kit (a forgotten password).
    pub fn open_with_kit(data: &Path, kit: &RecoveryKit) -> Result<Self, String> {
        let file = read_sealed(data)?;
        let wrong = "That kit doesn't open this vault. Check each character against the page.";
        if file.profile_id != Some(kit.profile_id()) {
            return Err(wrong.into());
        }
        let slot = file.kit.as_ref().ok_or(
            "This vault was locked before it had a recovery kit on this computer, so the kit can't open it here.",
        )?;
        let key = kit_key(kit, &slot.params)?;
        let opened = aead::open(&key, &aad(file.device_id, "kit")?, &slot.sealed)
            .map_err(|_| wrong.to_string())?;
        Self::from_sealed(&file, &opened)
    }

    fn from_sealed(file: &SealedFile, opened: &[u8]) -> Result<Self, String> {
        let secrets: Secrets = from_slice(opened).map_err(|e| e.to_string())?;
        let keys = Self {
            device_id: file.device_id,
            kem_alg: file.kem_alg,
            created_at: file.created_at,
            secrets,
            platform: Platform::current(),
        };
        if keys.device_pub().as_slice() != file.device_pub.as_slice() {
            return Err("the sealed device key is damaged: its keys don't match".into());
        }
        Ok(keys)
    }

    /// Whether `password` is the master password, without keeping anything.
    pub fn check_password(data: &Path, password: &str) -> Result<bool, String> {
        match Self::open_with_password(data, password) {
            Ok(_) => Ok(true),
            Err(e) if e == WRONG_PASSWORD => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn sealed_secrets(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        Ok(Zeroizing::new(
            to_canonical(&self.secrets).map_err(|e| e.to_string())?,
        ))
    }

    fn seal_password(&self, password: &str) -> Result<PasswordSlot, String> {
        let floor = lockdown_crypto::kdf::floor();
        let params = lockdown_crypto::kdf::calibrate_at_least(&floor, CALIBRATE_TO)
            .map_err(|e| e.to_string())?;
        let salt = lockdown_crypto::rand::random_16()
            .map_err(|e| e.to_string())?
            .to_vec();
        let key = password_key(password, &salt, &params)?;
        let sealed = aead::seal(
            &key,
            &aad(self.device_id, "password")?,
            &self.sealed_secrets()?,
        )
        .map_err(|e| e.to_string())?;
        Ok(PasswordSlot {
            salt,
            params,
            sealed,
        })
    }

    fn seal_kit(&self, kit: &RecoveryKit) -> Result<KitSlot, String> {
        let params = lockdown_crypto::kdf::floor();
        let key = kit_key(kit, &params)?;
        let sealed = aead::seal(&key, &aad(self.device_id, "kit")?, &self.sealed_secrets()?)
            .map_err(|e| e.to_string())?;
        Ok(KitSlot { params, sealed })
    }

    /// Seals these keys under `password` (and `kit`, when given; otherwise
    /// the kit slot already on disk for this device is kept), then reopens
    /// the file to check it yields the same device, and only then removes
    /// the plain file (§1.6: a crash between the two leaves both, and the
    /// sealed one wins).
    pub fn seal(
        &self,
        data: &Path,
        password: &str,
        kit: Option<&RecoveryKit>,
    ) -> Result<(), String> {
        let previous = if key_file(data) == KeyFile::Sealed {
            read_sealed(data)
                .ok()
                .filter(|f| f.device_id == self.device_id)
        } else {
            None
        };
        let (profile_id, kit_slot) = match kit {
            Some(kit) => (Some(kit.profile_id()), Some(self.seal_kit(kit)?)),
            None => match previous {
                Some(p) => (p.profile_id, p.kit),
                None => (None, None),
            },
        };
        let file = SealedFile {
            v: SEALED_V,
            device_id: self.device_id,
            device_pub: self.device_pub().to_vec(),
            kem_alg: self.kem_alg,
            created_at: self.created_at,
            profile_id,
            password: self.seal_password(password)?,
            kit: kit_slot,
        };
        self.write_sealed(data, &file)?;
        let reopened = Self::open_with_password(data, password)?;
        if reopened.device_id != self.device_id || reopened.device_pub() != self.device_pub() {
            return Err(
                "the sealed device key didn't read back the same; nothing was changed".into(),
            );
        }
        remove_plain(data);
        Ok(())
    }

    /// Writes the kit slot again for a new kit, keeping the password slot.
    pub fn reseal_kit(&self, data: &Path, kit: &RecoveryKit) -> Result<(), String> {
        let mut file = read_sealed(data)?;
        if file.device_id != self.device_id {
            return Err("the sealed device key belongs to another device".into());
        }
        file.profile_id = Some(kit.profile_id());
        file.kit = Some(self.seal_kit(kit)?);
        self.write_sealed(data, &file)
    }

    fn write_sealed(&self, data: &Path, file: &SealedFile) -> Result<(), String> {
        crate::paths::make_dir(data)?;
        let bytes = to_canonical(file).map_err(|e| e.to_string())?;
        write_private(&data.join(SEALED_FILE), &bytes).map_err(|e| e.to_string())
    }

    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.secrets.signing_seed)
    }
}

/// The profile whose kit opens the sealed keys in `data`, when they have a
/// kit slot.
pub fn profile_of(data: &Path) -> Result<Option<Id>, String> {
    let file = read_sealed(data)?;
    Ok(file.kit.as_ref().and(file.profile_id))
}

/// Overwrites and removes the plain key file, if one is here.
pub fn remove_plain(data: &Path) {
    let path: PathBuf = data.join(PLAIN_FILE);
    if let Ok(meta) = std::fs::metadata(&path) {
        let len = usize::try_from(meta.len()).unwrap_or(0).max(64);
        let _ = std::fs::write(&path, vec![0u8; len]);
        let _ = std::fs::remove_file(&path);
    }
}

impl DeviceKeystore for DeviceKeys {
    fn device_id(&self) -> Id {
        self.device_id
    }

    fn platform(&self) -> Platform {
        self.platform
    }

    fn device_pub(&self) -> [u8; 32] {
        self.signing_key().verifying_key().to_bytes()
    }

    fn sign(&self, message: &[u8]) -> ecosystem_identity::Result<[u8; 64]> {
        Ok(self.signing_key().sign(message).to_bytes())
    }

    fn kem_pub(&self) -> KemPublicKey {
        kem::kem(self.kem_alg)
            .keypair(self.secrets.kem_seed)
            .map(|(_, public)| public)
            .unwrap_or(KemPublicKey {
                alg: self.kem_alg,
                bytes: Vec::new(),
            })
    }

    fn kem_secret(&self) -> ecosystem_identity::Result<KemSecretKey> {
        Ok(KemSecretKey::from_seed(self.kem_alg, self.secrets.kem_seed))
    }

    fn attestation(&self) -> Option<Vec<u8>> {
        None
    }

    fn unlock(&self) -> ecosystem_identity::Result<DeviceUnlockKey> {
        Ok(DeviceUnlockKey::from_bytes(self.secrets.unlock_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    #[test]
    fn a_plain_file_reads_the_same_on_either_side() {
        let dir = tempfile::tempdir().unwrap();
        let theirs = FileKeystore::open_or_create(&dir.path().join(PLAIN_FILE)).unwrap();
        let ours = DeviceKeys::open_plain(dir.path()).unwrap();
        assert_eq!(theirs.device_id(), ours.device_id());
        assert_eq!(theirs.device_pub(), ours.device_pub());
        assert_eq!(theirs.kem_pub().bytes, ours.kem_pub().bytes);
        assert_eq!(
            theirs.unlock().unwrap().expose(),
            ours.unlock().unwrap().expose()
        );

        let dir2 = tempfile::tempdir().unwrap();
        let made = DeviceKeys::open_or_create_plain(dir2.path()).unwrap();
        let read = FileKeystore::open_or_create(&dir2.path().join(PLAIN_FILE)).unwrap();
        assert_eq!(made.device_id(), read.device_id());
        assert_eq!(made.device_pub(), read.device_pub());
    }

    #[test]
    fn the_master_password_opens_the_sealed_keys_and_nothing_else_does() {
        let dir = tempfile::tempdir().unwrap();
        let keys = DeviceKeys::open_or_create_plain(dir.path()).unwrap();
        keys.seal(dir.path(), "correct horse battery staple", None)
            .unwrap();
        assert_eq!(key_file(dir.path()), KeyFile::Sealed);
        assert!(!dir.path().join(PLAIN_FILE).exists());
        assert_eq!(
            DeviceKeys::open_plain(dir.path()).unwrap_err(),
            PASSWORD_NEEDED
        );
        let opened =
            DeviceKeys::open_with_password(dir.path(), "correct horse battery staple").unwrap();
        assert_eq!(opened.device_id(), keys.device_id());
        assert_eq!(
            opened.unlock().unwrap().expose(),
            keys.unlock().unwrap().expose()
        );
        assert_eq!(
            DeviceKeys::open_with_password(dir.path(), "Correct horse battery staple").unwrap_err(),
            WRONG_PASSWORD
        );
        assert!(!DeviceKeys::check_password(dir.path(), "nope").unwrap());
        // The secrets are nowhere on disk in the clear.
        let on_disk = std::fs::read(dir.path().join(SEALED_FILE)).unwrap();
        let unlock = *keys.unlock().unwrap().expose();
        assert!(!on_disk.windows(32).any(|w| w == unlock));
    }

    #[test]
    fn the_kit_opens_the_keys_when_the_password_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let profile = Id::new();
        let kit = RecoveryKit::generate(profile).unwrap();
        let keys = DeviceKeys::generate().unwrap();
        keys.seal(dir.path(), "first phrase of words", Some(&kit))
            .unwrap();
        let back =
            RecoveryKit::from_text(profile, &kit.secret_key_text(), &kit.recovery_code_text())
                .unwrap();
        let opened = DeviceKeys::open_with_kit(dir.path(), &back).unwrap();
        assert_eq!(opened.device_id(), keys.device_id());
        // A new password; the kit slot stays.
        opened
            .seal(dir.path(), "second phrase of words", None)
            .unwrap();
        assert_eq!(
            DeviceKeys::open_with_password(dir.path(), "first phrase of words").unwrap_err(),
            WRONG_PASSWORD
        );
        DeviceKeys::open_with_password(dir.path(), "second phrase of words").unwrap();
        DeviceKeys::open_with_kit(dir.path(), &back).unwrap();
        // Another kit doesn't open it, and a new kit replaces the old one.
        let other = RecoveryKit::generate(profile).unwrap();
        assert!(DeviceKeys::open_with_kit(dir.path(), &other).is_err());
        opened.reseal_kit(dir.path(), &other).unwrap();
        DeviceKeys::open_with_kit(dir.path(), &other).unwrap();
        assert!(DeviceKeys::open_with_kit(dir.path(), &back).is_err());
        DeviceKeys::open_with_password(dir.path(), "second phrase of words").unwrap();
    }

    #[test]
    fn a_changed_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let keys = DeviceKeys::generate().unwrap();
        keys.seal(dir.path(), "a long phrase of words", None)
            .unwrap();
        let mut file = read_sealed(dir.path()).unwrap();
        file.password.params = Argon2Params {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        keys.write_sealed(dir.path(), &file).unwrap();
        assert!(
            DeviceKeys::open_with_password(dir.path(), "a long phrase of words")
                .unwrap_err()
                .contains("less password stretching")
        );
        // Another device's id in the plain part fails the tag.
        let keys = DeviceKeys::generate().unwrap();
        keys.seal(dir.path(), "a long phrase of words", None)
            .unwrap();
        let mut file = read_sealed(dir.path()).unwrap();
        file.device_id = Id::new();
        keys.write_sealed(dir.path(), &file).unwrap();
        assert_eq!(
            DeviceKeys::open_with_password(dir.path(), "a long phrase of words").unwrap_err(),
            WRONG_PASSWORD
        );
    }

    #[test]
    fn a_crash_before_the_plain_file_goes_leaves_the_sealed_one_in_charge() {
        let dir = tempfile::tempdir().unwrap();
        let keys = DeviceKeys::open_or_create_plain(dir.path()).unwrap();
        let plain = std::fs::read(dir.path().join(PLAIN_FILE)).unwrap();
        keys.seal(dir.path(), "a long phrase of words", None)
            .unwrap();
        // As if the app stopped between writing the sealed file and
        // removing the plain one.
        std::fs::write(dir.path().join(PLAIN_FILE), &plain).unwrap();
        assert_eq!(key_file(dir.path()), KeyFile::Sealed);
        assert_eq!(
            DeviceKeys::open_plain(dir.path()).unwrap_err(),
            PASSWORD_NEEDED
        );
        let opened = DeviceKeys::open_with_password(dir.path(), "a long phrase of words").unwrap();
        opened
            .seal(dir.path(), "a long phrase of words", None)
            .unwrap();
        assert!(!dir.path().join(PLAIN_FILE).exists());
    }
}
