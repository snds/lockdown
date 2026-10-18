//! Automatic local backups (matrix `auto-backups`): the vault as a sealed
//! Lockdown file (`docs/format/backup-v1.md`) in a folder the person chose,
//! on a schedule, keeping the newest N. Never plaintext on disk: the file
//! is sealed in memory and written whole through a partial file and a
//! rename.
//!
//! The person sets a backup password once. Its Argon2id key is derived
//! then and kept in `backup-key.cbor` beside the other data files, sealed
//! under a key derived from the persona's App Data Key, so a scheduled run
//! needs no password while the vault is open and the file key is useless
//! without the vault. Any backup opens anywhere with the password alone,
//! no account, no device.

use std::path::{Path, PathBuf};

use ecosystem_identity::SymmetricKey;
use lockdown_crypto::{Key32, Sealed};
use lockdown_cxf::archive::{self, FileKey, Kdf};
use lockdown_cxf::{Limits, build::date_from_unix};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{Session, now_secs};
use crate::settings::BackupSettings;
use crate::types::{BackupFile, BackupStatus, ImportPreview};

const KEY_FILE: &str = "backup-key.cbor";
const STATE_FILE: &str = "backup-state.cbor";
const KEY_LABEL: &[u8] = b"lockdown/backup-key/v1";
const PREFIX: &str = "lockdown-backup-";
const SUFFIX: &str = ".lockdown";
/// How long a failed run waits before the next try.
const RETRY_SECS: i64 = 15 * 60;

#[derive(Serialize, Deserialize)]
struct KeyFile {
    v: u16,
    kdf: String,
    sealed: Sealed,
}

/// When backups last ran, beside the other data files.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupState {
    /// When the last backup was written.
    pub last_success_at: Option<i64>,
    /// When the last try was (success or not).
    pub last_attempt_at: Option<i64>,
    /// What went wrong last time.
    pub last_error: Option<String>,
}

/// `YYYYMMDD-HHMMSS` (UTC) for a file name.
fn stamp(secs: i64) -> String {
    let date = date_from_unix(secs).replace('-', "");
    let s = secs.rem_euclid(86_400);
    format!("{date}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Whether `name` is one of the files backups write (and so one they may
/// remove when pruning or read when restoring).
fn is_backup_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|r| r.strip_suffix(SUFFIX))
        .is_some_and(|mid| {
            mid.len() >= 15
                && mid
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b'-' || b == b'n')
        })
}

/// The backups in `folder`, newest first.
pub fn list(folder: &Path) -> Vec<BackupFile> {
    let mut out: Vec<BackupFile> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !is_backup_name(&name) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let made_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Some(BackupFile {
                name,
                bytes: meta.len(),
                made_at,
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

impl Session {
    fn backup_key_path(&self) -> PathBuf {
        self.paths.data.join(KEY_FILE)
    }

    fn backup_state_path(&self) -> PathBuf {
        self.paths.data.join(STATE_FILE)
    }

    fn sealing_key(&self) -> Result<Key32, String> {
        let k = lockdown_crypto::kdf::subkey(self.opened.keys.app_data_key.expose(), KEY_LABEL)
            .map_err(|e| e.to_string())?;
        Ok(Key32::from_bytes(*k))
    }

    fn key_aad(&self) -> Vec<u8> {
        let mut aad = KEY_LABEL.to_vec();
        aad.extend_from_slice(self.vault().context().vault_id.as_bytes());
        aad
    }

    /// Whether a backup password is set on this device for this vault.
    pub fn backup_key_set(&self) -> bool {
        self.backup_key().is_ok()
    }

    fn backup_key(&self) -> Result<(Kdf, FileKey), String> {
        let bytes = std::fs::read(self.backup_key_path())
            .map_err(|_| "no backup password is set on this device".to_string())?;
        let file: KeyFile = lockdown_crypto::cbor::from_slice(&bytes).map_err(|_| {
            "the backup key file didn't read; set the backup password again".to_string()
        })?;
        let kdf: Kdf = serde_json::from_str(&file.kdf).map_err(|_| {
            "the backup key file didn't read; set the backup password again".to_string()
        })?;
        let plain = lockdown_crypto::open(&self.sealing_key()?, &self.key_aad(), &file.sealed)
            .map_err(|_| {
                "the backup key belongs to another vault; set the backup password again".to_string()
            })?;
        let raw: [u8; 32] = plain
            .as_slice()
            .try_into()
            .map_err(|_| "the backup key file didn't read".to_string())?;
        Ok((kdf, FileKey::from_bytes(raw)))
    }

    /// Sets the backup password: derives its key once (Argon2id at this
    /// build's floor, a fresh salt) and keeps it sealed under the vault's
    /// key. Backups made before keep their own password.
    pub fn backup_set_password(&mut self, password: &[u8]) -> Result<(), String> {
        if password.len() < 8 {
            return Err("a backup password needs at least 8 characters".into());
        }
        let kdf = Kdf::fresh().map_err(|e| e.to_string())?;
        let key = kdf.derive(password).map_err(|e| e.to_string())?;
        let sealed = lockdown_crypto::seal(&self.sealing_key()?, &self.key_aad(), key.expose())
            .map_err(|e| e.to_string())?;
        let file = KeyFile {
            v: 1,
            kdf: serde_json::to_string(&kdf).map_err(|e| e.to_string())?,
            sealed,
        };
        let bytes = lockdown_crypto::cbor::to_canonical(&file).map_err(|e| e.to_string())?;
        lockdown_store::write_private(&self.backup_key_path(), &bytes)
            .map_err(|e| format!("couldn't keep the backup key: {e}"))
    }

    /// Forgets the backup password on this device (the files stay).
    pub fn backup_forget_key(&mut self) {
        let _ = std::fs::remove_file(self.backup_key_path());
    }

    /// When backups last ran.
    pub fn backup_state(&self) -> BackupState {
        std::fs::read(self.backup_state_path())
            .ok()
            .and_then(|b| lockdown_crypto::cbor::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save_backup_state(&self, state: &BackupState) {
        if let Ok(bytes) = lockdown_crypto::cbor::to_canonical(state) {
            let _ = lockdown_store::write_private(&self.backup_state_path(), &bytes);
        }
    }

    /// Whether a scheduled backup is due at `now`.
    pub fn backup_due(&self, cfg: &BackupSettings, now: i64) -> bool {
        if !cfg.enabled || cfg.folder.is_none() || !self.backup_key_set() {
            return false;
        }
        let state = self.backup_state();
        if let (Some(err_at), Some(_)) = (state.last_attempt_at, &state.last_error)
            && now - err_at < RETRY_SECS
        {
            return false;
        }
        match state.last_success_at {
            Some(t) => now - t >= i64::from(cfg.interval_hours) * 3600,
            None => true,
        }
    }

    /// Writes one backup now and prunes to the newest `keep_last`. Returns
    /// the file's name. The outcome is remembered for the status line.
    pub fn backup_run(&mut self, cfg: &BackupSettings) -> Result<String, String> {
        let now = now_secs();
        let result = self.backup_write(cfg, now);
        let mut state = self.backup_state();
        state.last_attempt_at = Some(now);
        match &result {
            Ok(_) => {
                state.last_success_at = Some(now);
                state.last_error = None;
            }
            Err(e) => state.last_error = Some(e.clone()),
        }
        self.save_backup_state(&state);
        result
    }

    fn backup_write(&self, cfg: &BackupSettings, now: i64) -> Result<String, String> {
        let folder = PathBuf::from(
            cfg.folder
                .as_deref()
                .ok_or("choose a folder for backups first")?,
        );
        if !folder.is_dir() {
            return Err(format!(
                "the backup folder {} isn't there (is the drive connected?)",
                folder.display()
            ));
        }
        let (kdf, key) = self.backup_key()?;
        let (doc, _) = self.backup_document(None)?;
        let plain = doc.to_bytes().map_err(|e| e.to_string())?;
        let sealed = Zeroizing::new(
            archive::seal(&plain, &key, &kdf, now.max(0) as u64).map_err(|e| e.to_string())?,
        );
        drop(plain);
        let mut name = format!("{PREFIX}{}{SUFFIX}", stamp(now));
        let mut n = 2;
        while folder.join(&name).exists() {
            name = format!("{PREFIX}{}-n{n}{SUFFIX}", stamp(now));
            n += 1;
        }
        super::transfer::write_user_file(&folder.join(&name), &sealed)?;
        for old in list(&folder)
            .into_iter()
            .skip(cfg.keep_last.max(1) as usize)
        {
            let _ = std::fs::remove_file(folder.join(&old.name));
        }
        Ok(name)
    }

    /// Automatic backups as they stand.
    pub fn backup_status(&self, cfg: &BackupSettings) -> BackupStatus {
        let state = self.backup_state();
        let key_set = self.backup_key_set();
        let next_due_at = (cfg.enabled && key_set && cfg.folder.is_some()).then(|| {
            state
                .last_success_at
                .map_or_else(now_secs, |t| t + i64::from(cfg.interval_hours) * 3600)
        });
        let crown_jewels_left_out = self
            .export_preview("lockdown", None)
            .map(|p| p.crown_jewels_left_out)
            .unwrap_or(0);
        BackupStatus {
            enabled: cfg.enabled,
            folder: cfg.folder.clone(),
            interval_hours: cfg.interval_hours,
            keep_last: cfg.keep_last,
            key_set,
            last_success_at: state.last_success_at,
            last_error: state.last_error,
            next_due_at,
            crown_jewels_left_out,
            backups: cfg
                .folder
                .as_deref()
                .map(|f| list(Path::new(f)))
                .unwrap_or_default(),
        }
    }

    /// Opens a backup from the folder for the import preview: with the key
    /// kept on this device when the backup was made under it, else with
    /// `password`.
    pub fn backup_restore_preview(
        &mut self,
        cfg: &BackupSettings,
        name: &str,
        password: Option<&[u8]>,
    ) -> Result<ImportPreview, String> {
        if !is_backup_name(name) {
            return Err(format!("'{name}' isn't one of Lockdown's backup files"));
        }
        let folder = cfg
            .folder
            .as_deref()
            .ok_or("choose a folder for backups first")?;
        let path = Path::new(folder).join(name);
        let limits = Limits::backup();
        let meta = std::fs::metadata(&path).map_err(|e| format!("couldn't open {name}: {e}"))?;
        if meta.len() as usize > limits.max_bytes {
            return Err(format!("{name} is larger than a backup can be"));
        }
        let bytes =
            Zeroizing::new(std::fs::read(&path).map_err(|e| format!("couldn't read {name}: {e}"))?);
        if password.is_none()
            && let Ok((kdf, key)) = self.backup_key()
            && archive::kdf_of(&bytes).is_ok_and(|k| k == kdf)
        {
            let plain = archive::open_with(&bytes, &key, &limits).map_err(|e| e.to_string())?;
            return self.import_preview(&plain, "lockdown", name, None, None);
        }
        self.import_preview(&bytes, "lockdown", name, password, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stamped_and_only_ours_are_touched() {
        assert_eq!(stamp(1_760_745_600), "20251018-000000");
        assert_eq!(stamp(1_760_745_600 + 3661), "20251018-010101");
        let name = format!("{PREFIX}{}{SUFFIX}", stamp(1_760_745_600));
        assert!(is_backup_name(&name));
        assert!(is_backup_name(
            "lockdown-backup-20251018-000000-n2.lockdown"
        ));
        for other in [
            "notes.txt",
            "lockdown-backup-../../x.lockdown",
            "lockdown-backup-2025.lockdown",
            "lockdown-backup-20251018-000000.lockdown.partial",
        ] {
            assert!(!is_backup_name(other), "{other}");
        }
    }
}
