//! The app's settings, as one small file the desktop app and the CLI
//! share: `settings.cbor` beside `remembered.cbor` in the data directory
//! (D100). It holds **preferences only**: how dense the list is, how a
//! secret is revealed, how long a copy lives on the clipboard, when the
//! vault locks, and which server the person types by default. Nothing in
//! it is secret and nothing in it is trusted for security: the lock timers
//! are *applied* by the core from here, but the core enforces them; a file
//! edited to say "never lock" gets exactly what a person could set in the
//! app, no more.
//!
//! Unknown fields are kept out, missing fields take their defaults, and a
//! file from a newer version is refused with a plain sentence, so an older
//! CLI and a newer app can share the file without one wiping the other's
//! settings it doesn't know.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::lock::LockPolicy;

/// The file's format version.
const SETTINGS_V: u16 = 1;

/// The file name, beside `remembered.cbor`.
pub const SETTINGS_FILE: &str = "settings.cbor";

/// How much room a list row takes (D92).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// 40 px rows: the default.
    #[default]
    Comfortable,
    /// 32 px rows: the option.
    Compact,
}

/// How a hidden value is revealed (D97).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevealGesture {
    /// Press and hold (pointer down or Space held); release hides at once.
    #[default]
    Hold,
    /// Tap to show, tap to hide, with the 30-second auto-hide.
    Toggle,
}

/// Everything the settings file holds. Every field has a default, so a
/// missing file and a short file both read as "the defaults".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Format version.
    pub v: u16,
    /// List density.
    pub density: Density,
    /// Reveal gesture.
    pub reveal: RevealGesture,
    /// Seconds a copied secret stays on the clipboard (10–90, D85).
    pub clipboard_clear_secs: u64,
    /// Lock after this many idle seconds; `None` never (D84).
    pub idle_lock_secs: Option<u64>,
    /// Lock after the window has been out of view this long; `None` never.
    pub hidden_lock_secs: Option<u64>,
    /// The server the join, register and log-in forms suggest.
    pub server: Option<String>,
    /// Whether the security center checks stored passwords against known
    /// breaches (five hex characters of each password's SHA-1 leave the
    /// device; docs/plan/13-security-center.md).
    pub breach_check: bool,
    /// Where the breach ranges come from: an `https://` range API (the
    /// prefix is appended) or `file:///folder` of range files. `None` is
    /// the public Pwned Passwords API.
    pub breach_endpoint: Option<String>,
    /// Whether "Change password" asks the site whether it has a
    /// change-password page before offering it.
    pub change_password_check: bool,
    /// Automatic encrypted backups (preferences only: the key they are
    /// sealed under lives in `backup-key.cbor`, sealed under the vault's
    /// key, never here).
    pub backup: BackupSettings,
    /// Whether the browser extension offers Lockdown as the passkey
    /// provider in pages (docs/plan/19-passkeys.md); off, every passkey
    /// request goes to the browser's own.
    pub browser_passkeys: bool,
    /// Whether one-time codes are worked out from the account server's
    /// time when this computer's clock is off by up to five minutes
    /// (docs/plan/23-bridges.md §8).
    pub code_clock_correction: bool,
    /// Whether the account's server watches the person's email aliases for
    /// breaches (it then holds the list; off by default; changed only
    /// through `alias_monitoring_set`, docs/plan/23-bridges.md §9).
    pub alias_monitoring: bool,
    /// Whether the system-wide shortcut (Ctrl+Shift+Space, ⌘⇧Space on
    /// macOS) opens quick access from anywhere while Lockdown runs
    /// (docs/plan/26-desktop-finish.md §2).
    pub quick_access_hotkey: bool,
}

/// Where and how often automatic backups run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupSettings {
    /// Whether scheduled backups run.
    pub enabled: bool,
    /// The folder the person chose.
    pub folder: Option<String>,
    /// Hours between backups, 1 to 168 (a week).
    pub interval_hours: u32,
    /// How many backups to keep, 1 to 100; older ones Lockdown made are
    /// removed from the folder.
    pub keep_last: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            folder: None,
            interval_hours: 24,
            keep_last: 10,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let policy = LockPolicy::default();
        Self {
            v: SETTINGS_V,
            density: Density::default(),
            reveal: RevealGesture::default(),
            clipboard_clear_secs: 30,
            idle_lock_secs: policy.idle.map(|d| d.as_secs()),
            hidden_lock_secs: policy.hidden.map(|d| d.as_secs()),
            server: None,
            breach_check: true,
            breach_endpoint: None,
            change_password_check: true,
            backup: BackupSettings::default(),
            browser_passkeys: true,
            code_clock_correction: true,
            alias_monitoring: false,
            quick_access_hotkey: true,
        }
    }
}

impl Settings {
    /// Reads the file; a missing file is the defaults.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let settings: Self = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| format!("{} didn't decode; it was reset", path.display()))?;
                if settings.v > SETTINGS_V {
                    return Err(format!(
                        "{} was written by a newer Lockdown; update the app",
                        path.display()
                    ));
                }
                Ok(settings.clamped())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
        }
    }

    /// Writes the file, readable by the user only, through a temp file
    /// and a rename like every other file in the data directory.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut on_disk = self.clamped();
        on_disk.v = SETTINGS_V;
        let bytes = lockdown_crypto::cbor::to_canonical(&on_disk).map_err(|e| e.to_string())?;
        lockdown_store::write_private(path, &bytes)
            .map_err(|e| format!("couldn't write {}: {e}", path.display()))
    }

    /// The same settings with every number inside its allowed range: the
    /// clipboard timer 10–90 s, the idle lock 1 minute to 4 hours, the
    /// hidden lock 10 seconds to 4 hours, the server trimmed or dropped.
    pub fn clamped(&self) -> Self {
        let mut s = self.clone();
        s.clipboard_clear_secs = s.clipboard_clear_secs.clamp(10, 90);
        s.idle_lock_secs = s.idle_lock_secs.map(|n| n.clamp(60, 4 * 3600));
        s.hidden_lock_secs = s.hidden_lock_secs.map(|n| n.clamp(10, 4 * 3600));
        s.server = s
            .server
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        s.breach_endpoint = s
            .breach_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        s.backup.interval_hours = s.backup.interval_hours.clamp(1, 168);
        s.backup.keep_last = s.backup.keep_last.clamp(1, 100);
        s.backup.folder = s
            .backup
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        if s.backup.folder.is_none() {
            s.backup.enabled = false;
        }
        s
    }

    /// The lock timers these settings ask for. The sleep gap is not a
    /// setting (D84): the machine slept, the vault locks.
    pub fn lock_policy(&self) -> LockPolicy {
        LockPolicy {
            idle: self.idle_lock_secs.map(Duration::from_secs),
            hidden: self.hidden_lock_secs.map(Duration::from_secs),
            sleep_gap: LockPolicy::default().sleep_gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_defaults_and_a_saved_one_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lockdown").join(SETTINGS_FILE);
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded, Settings::default());
        assert_eq!(loaded.density, Density::Comfortable);
        assert_eq!(loaded.reveal, RevealGesture::Hold);
        assert_eq!(loaded.clipboard_clear_secs, 30);
        assert_eq!(loaded.idle_lock_secs, Some(600));
        assert_eq!(loaded.hidden_lock_secs, Some(300));

        let mut changed = loaded.clone();
        changed.density = Density::Compact;
        changed.reveal = RevealGesture::Toggle;
        changed.clipboard_clear_secs = 45;
        changed.idle_lock_secs = None;
        changed.server = Some("  https://vault.example  ".into());
        changed.save(&path).unwrap();
        let again = Settings::load(&path).unwrap();
        assert_eq!(again.density, Density::Compact);
        assert_eq!(again.reveal, RevealGesture::Toggle);
        assert_eq!(again.clipboard_clear_secs, 45);
        assert_eq!(again.idle_lock_secs, None);
        assert_eq!(again.server.as_deref(), Some("https://vault.example"));
        assert!(again.lock_policy().idle.is_none());
        assert_eq!(
            again.lock_policy().hidden,
            Some(Duration::from_secs(300)),
            "the hidden timer stayed at its default"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the file is the user's alone");
        }
    }

    #[test]
    fn numbers_out_of_range_are_pulled_back_on_save_and_on_load() {
        let s = Settings {
            clipboard_clear_secs: 5,
            idle_lock_secs: Some(1),
            hidden_lock_secs: Some(10 * 3600),
            server: Some("   ".into()),
            ..Settings::default()
        }
        .clamped();
        assert_eq!(s.clipboard_clear_secs, 10);
        assert_eq!(s.idle_lock_secs, Some(60));
        assert_eq!(s.hidden_lock_secs, Some(4 * 3600));
        assert_eq!(s.server, None);
        let s = Settings {
            clipboard_clear_secs: 900,
            ..Settings::default()
        }
        .clamped();
        assert_eq!(s.clipboard_clear_secs, 90);
    }

    #[test]
    fn a_short_file_reads_with_defaults_and_a_newer_one_is_refused() {
        // A file an older build wrote: only the version and the density.
        #[derive(Serialize)]
        struct Older {
            v: u16,
            density: Density,
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        let bytes = lockdown_crypto::cbor::to_canonical(&Older {
            v: 1,
            density: Density::Compact,
        })
        .unwrap();
        std::fs::write(&path, bytes).unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.density, Density::Compact);
        assert_eq!(s.reveal, RevealGesture::Hold);
        assert_eq!(s.clipboard_clear_secs, 30);

        let bytes = lockdown_crypto::cbor::to_canonical(&Older {
            v: 99,
            density: Density::Compact,
        })
        .unwrap();
        std::fs::write(&path, bytes).unwrap();
        let err = Settings::load(&path).unwrap_err();
        assert!(err.contains("newer Lockdown"), "{err}");
    }

    #[test]
    fn the_file_never_names_a_secret() {
        // Nothing in the struct is a secret: a serialised default has
        // exactly these keys and no other.
        let json = serde_json::to_value(Settings::default()).unwrap();
        let keys: std::collections::BTreeSet<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected: std::collections::BTreeSet<&str> = [
            "v",
            "density",
            "reveal",
            "clipboard_clear_secs",
            "idle_lock_secs",
            "hidden_lock_secs",
            "server",
            "breach_check",
            "breach_endpoint",
            "change_password_check",
            "backup",
            "browser_passkeys",
            "code_clock_correction",
            "alias_monitoring",
            "quick_access_hotkey",
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, expected);
        let backup: std::collections::BTreeSet<String> = json["backup"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            backup,
            ["enabled", "folder", "interval_hours", "keep_last"]
                .map(String::from)
                .into(),
            "the backup key is never in the settings file"
        );
    }

    #[test]
    fn the_security_checks_default_on_and_an_older_file_reads_them_as_on() {
        let s = Settings::default();
        assert!(s.breach_check && s.change_password_check);
        assert_eq!(s.breach_endpoint, None);
        // A file written before the security center existed.
        #[derive(serde::Serialize)]
        struct Older {
            v: u16,
            server: Option<String>,
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        let bytes = lockdown_crypto::cbor::to_canonical(&Older { v: 1, server: None }).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert!(loaded.breach_check && loaded.change_password_check);
        // An empty source is the public one.
        let blank = Settings {
            breach_endpoint: Some("   ".into()),
            ..Settings::default()
        }
        .clamped();
        assert_eq!(blank.breach_endpoint, None);
    }
}
