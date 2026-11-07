//! Where the desktop keeps the session token between runs
//! (`lockdown_app::secrets`, D86, D87), so that an update never brings up a
//! keychain dialog (docs/plan/60-auto-update.md §3).
//!
//! On a Mac, Lockdown writes the token through the `keyring` crate into the
//! login keychain as a generic password (the file-based keychain). macOS files
//! the app that created the item in the item's access list by the app's code
//! requirement. An ad-hoc signed build (D450) has no signer, so its
//! requirement is its own code hash, and the hash changes with every build. The
//! next build is then a stranger to the item, and reading it makes macOS ask
//! "Lockdown wants to use your confidential information stored in
//! 'design.snds.lockdown' in your keychain". That is a manual step after an
//! update, which Sean's rule forbids.
//!
//! So, on a Mac:
//!
//! - A build signed by an Apple-issued certificate (a Developer ID, D493)
//!   keeps the token in the keychain. Its requirement names the team and
//!   the identifier, which every build shares.
//! - Any other build (ad hoc, unsigned, or a signature the OS didn't accept)
//!   keeps the token in `sync.cbor`, readable only by this user. That is
//!   where the CLI keeps it on every platform, beside the device key's own
//!   file (D87). The keychain is only read, for a token an older build left
//!   there.
//! - Every keychain read is made with the user interface switched off. An
//!   item the OS would ask about reads as absent, so the app falls back to
//!   "sign in again" with the master password, and no dialog ever appears.
//!
//! Windows Credential Manager and the Secret Service on Linux have no
//! per-program access list, so an update reads the token there as before.

use lockdown_codesign::{CodeSignature, SignatureKind};

/// Whether the token is kept in the OS keychain or in `sync.cbor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The keychain holds the token (Windows, Linux, a Developer ID Mac build).
    Keep,
    /// The token stays in `sync.cbor`; the keychain is only read, with no
    /// dialog, for a token an older build left.
    ReadOnly,
}

/// The mode for a Mac build with this signature (`None`: it couldn't be
/// read). Only a valid signature from an Apple-issued certificate, which
/// names a team, gives every build the same identity.
pub fn mac_mode(own: Option<&CodeSignature>) -> Mode {
    match own {
        Some(s) if s.kind == SignatureKind::Signed && s.valid && s.team.is_some() => Mode::Keep,
        _ => Mode::ReadOnly,
    }
}

/// This build's mode.
pub fn mode() -> Mode {
    #[cfg(target_os = "macos")]
    {
        mac_mode(lockdown_codesign::macos::own().ok().as_ref())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Mode::Keep
    }
}

#[cfg(feature = "shell")]
pub use store::Keychain;

#[cfg(feature = "shell")]
mod store {
    use lockdown_app::secrets::SecretStore;

    use super::Mode;

    /// The keychain service every entry is filed under.
    const SERVICE: &str = "design.snds.lockdown";

    /// The session token in the OS keychain, or only read from it.
    pub struct Keychain {
        pub mode: Mode,
    }

    impl Keychain {
        /// The store for this build.
        pub fn for_this_build() -> Self {
            Self {
                mode: super::mode(),
            }
        }
    }

    /// Reads `key` with the keychain's dialogs switched off (macOS). A
    /// refusal reads as no entry: the person signs in again with the
    /// master password, and is never asked about the keychain.
    #[cfg(target_os = "macos")]
    fn read(entry: &keyring::Entry) -> Result<Option<String>, String> {
        use security_framework::os::macos::keychain::SecKeychain;
        let _quiet = SecKeychain::disable_user_interaction()
            .map_err(|e| format!("the keychain isn't available: {e}"))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn read(entry: &keyring::Entry) -> Result<Option<String>, String> {
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("couldn't read from the keychain: {e}")),
        }
    }

    impl SecretStore for Keychain {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            let entry = keyring::Entry::new(SERVICE, key)
                .map_err(|e| format!("the keychain isn't available: {e}"))?;
            read(&entry)
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            if self.mode == Mode::ReadOnly {
                return Ok(());
            }
            let entry = keyring::Entry::new(SERVICE, key)
                .map_err(|e| format!("the keychain isn't available: {e}"))?;
            entry
                .set_password(value)
                .map_err(|e| format!("couldn't save to the keychain: {e}"))
        }

        fn delete(&self, key: &str) {
            // An item an older ad-hoc build wrote can't be deleted without
            // the same dialog; the developer reset finds it (D482).
            if self.mode == Mode::ReadOnly {
                return;
            }
            if let Ok(e) = keyring::Entry::new(SERVICE, key) {
                let _ = e.delete_credential();
            }
        }

        fn keeps_values(&self) -> bool {
            self.mode == Mode::Keep
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(team: Option<&str>, valid: bool) -> CodeSignature {
        CodeSignature {
            kind: SignatureKind::Signed,
            valid,
            identifier: Some("design.snds.lockdown".into()),
            team: team.map(str::to_string),
            ..CodeSignature::unsigned()
        }
    }

    #[test]
    fn only_a_developer_id_build_keeps_the_token_in_the_mac_keychain() {
        assert_eq!(
            mac_mode(Some(&signed(Some("ABCDE12345"), true))),
            Mode::Keep
        );
        assert_eq!(
            mac_mode(Some(&signed(Some("ABCDE12345"), false))),
            Mode::ReadOnly
        );
        // A self-signed certificate names no team.
        assert_eq!(mac_mode(Some(&signed(None, true))), Mode::ReadOnly);
        let adhoc = CodeSignature {
            kind: SignatureKind::AdHoc,
            valid: true,
            ..CodeSignature::unsigned()
        };
        assert_eq!(mac_mode(Some(&adhoc)), Mode::ReadOnly);
        assert_eq!(mac_mode(Some(&CodeSignature::unsigned())), Mode::ReadOnly);
        assert_eq!(mac_mode(None), Mode::ReadOnly);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn windows_and_linux_keep_the_token_in_the_keychain() {
        assert_eq!(mode(), Mode::Keep);
    }
}
