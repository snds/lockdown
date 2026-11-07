//! The session token in the OS keychain (`lockdown_app::secrets`, D86,
//! D87), kept so that an update never brings up a keychain dialog
//! (docs/plan/60-auto-update.md §3, D684 as Sean decided it on 2026-11-07).
//!
//! On a Mac the token is a generic password in the login keychain, written
//! through `keyring` (the file-based keychain API). macOS puts the app that
//! made the item on the item's access list, named by the app's designated
//! requirement. Another build that doesn't satisfy that requirement must
//! ask the person ("Lockdown wants to use your confidential information
//! stored in 'design.snds.lockdown' in your keychain") before it can read
//! the item or change it. That is a manual step after an update, which
//! Sean's rule forbids.
//!
//! Two things keep every build away from that dialog:
//!
//! - **Dialogs off.** Every keychain call on a Mac runs with
//!   `SecKeychainSetUserInteractionAllowed(false)`. A read macOS would have
//!   asked about comes back refused, and reads as no token: the person signs
//!   in again with the master password, and is never shown a dialog.
//! - **One item per signing identity.** The item's account is the key
//!   followed by `#` and a tag for this build's code identity:
//!   - `cert-<first 8 bytes of the signing certificate's SHA-256>` for a
//!     build signed by a certificate (a Developer ID, or the stable
//!     self-signed identity CI holds). Every build signed by the same
//!     certificate shares the tag, and satisfies the item's access list, so
//!     its token carries across updates;
//!   - `build-<first 8 bytes of the executable's SHA-256>` for an ad-hoc or
//!     unsigned build, whose identity is its own code.
//!
//!   A build never changes an item another identity wrote; changing one
//!   would ask. It adds its own instead, which asks nothing. An item from
//!   before this scheme (the bare key, no tag) is still read once, dialogs
//!   off, so a build with the same identity keeps its token.
//!
//! Windows Credential Manager and the Secret Service on Linux have no
//! per-program access list, so there the key is used as it is.

use lockdown_codesign::{CodeSignature, SignatureKind};

/// This build's code identity, as the keychain's access list sees it: the
/// signing certificate when there is one, else the code itself.
pub fn identity_tag(own: Option<&CodeSignature>, exe_sha256: Option<&[u8; 32]>) -> Option<String> {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    match own {
        Some(s) if s.kind == SignatureKind::Signed && s.leaf_sha256.is_some() => s
            .leaf_sha256
            .map(|leaf| format!("cert-{}", hex(&leaf[..8]))),
        _ => exe_sha256.map(|h| format!("build-{}", hex(&h[..8]))),
    }
}

/// The keychain account for `key` under an identity tag.
pub fn account(key: &str, tag: Option<&str>) -> String {
    match tag {
        Some(tag) => format!("{key}#{tag}"),
        None => key.to_string(),
    }
}

/// The SHA-256 of a build's executable: an ad-hoc build's identity.
pub fn code_digest(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes).into()
}

/// This build's tag (macOS); `None` elsewhere, where the bare key is used.
pub fn this_build_tag() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let own = lockdown_codesign::macos::own().ok();
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .map(|bytes| code_digest(&bytes));
        identity_tag(own.as_ref(), exe.as_ref())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(feature = "shell")]
pub use store::Keychain;

#[cfg(feature = "shell")]
mod store {
    use lockdown_app::secrets::SecretStore;

    use super::account;

    /// The keychain service every entry is filed under.
    pub const SERVICE: &str = "design.snds.lockdown";

    /// The session token in the OS keychain.
    pub struct Keychain {
        /// This build's identity tag (macOS); `None` uses the bare key.
        pub tag: Option<String>,
    }

    impl Keychain {
        /// The store for this build.
        pub fn for_this_build() -> Self {
            Self {
                tag: super::this_build_tag(),
            }
        }
    }

    /// Runs `f` with the keychain's dialogs switched off (macOS).
    #[cfg(target_os = "macos")]
    fn quietly<T>(f: impl FnOnce() -> T) -> Result<T, String> {
        use security_framework::os::macos::keychain::SecKeychain;
        let _quiet = SecKeychain::disable_user_interaction()
            .map_err(|e| format!("the keychain isn't available: {e}"))?;
        Ok(f())
    }

    #[cfg(not(target_os = "macos"))]
    fn quietly<T>(f: impl FnOnce() -> T) -> Result<T, String> {
        Ok(f())
    }

    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account)
            .map_err(|e| format!("the keychain isn't available: {e}"))
    }

    /// A read: the value, none, or (macOS) a refusal, which reads as none.
    fn read(account: &str) -> Result<Option<String>, String> {
        let entry = entry(account)?;
        match quietly(|| entry.get_password())? {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // macOS: an item this identity may not read without asking.
            Err(_) if cfg!(target_os = "macos") => Ok(None),
            Err(e) => Err(format!("couldn't read from the keychain: {e}")),
        }
    }

    impl SecretStore for Keychain {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            if let Some(value) = read(&account(key, self.tag.as_deref()))? {
                return Ok(Some(value));
            }
            if self.tag.is_none() {
                return Ok(None);
            }
            // An item from before identity tags: read once, dialogs off;
            // the next save writes this identity's own item.
            read(key)
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            let entry = entry(&account(key, self.tag.as_deref()))?;
            quietly(|| entry.set_password(value))?
                .map_err(|e| format!("couldn't save to the keychain: {e}"))
        }

        fn delete(&self, key: &str) {
            // This identity's item goes. Another identity's would need the
            // person's permission, which is never asked: with dialogs off
            // that delete is refused and the item stays (doc 60 §8).
            for account in [account(key, self.tag.as_deref()), key.to_string()] {
                if let Ok(e) = entry(&account) {
                    let _ = quietly(|| e.delete_credential());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(leaf: Option<[u8; 32]>) -> CodeSignature {
        CodeSignature {
            kind: SignatureKind::Signed,
            valid: true,
            identifier: Some("design.snds.lockdown".into()),
            leaf_sha256: leaf,
            ..CodeSignature::unsigned()
        }
    }

    #[test]
    fn a_signed_build_is_tagged_by_its_certificate_and_any_other_by_its_code() {
        let cert = [0xab; 32];
        let exe_a = [0x01; 32];
        let exe_b = [0x02; 32];
        // Two builds signed by one certificate share the tag, whatever
        // their code.
        assert_eq!(
            identity_tag(Some(&signed(Some(cert))), Some(&exe_a)),
            Some("cert-abababababababab".into())
        );
        assert_eq!(
            identity_tag(Some(&signed(Some(cert))), Some(&exe_b)),
            identity_tag(Some(&signed(Some(cert))), Some(&exe_a))
        );
        // Ad hoc and unsigned builds are their code.
        let adhoc = CodeSignature {
            kind: SignatureKind::AdHoc,
            valid: true,
            ..CodeSignature::unsigned()
        };
        assert_eq!(
            identity_tag(Some(&adhoc), Some(&exe_a)),
            Some("build-0101010101010101".into())
        );
        assert_ne!(
            identity_tag(Some(&adhoc), Some(&exe_a)),
            identity_tag(Some(&adhoc), Some(&exe_b))
        );
        assert_eq!(
            identity_tag(Some(&CodeSignature::unsigned()), Some(&exe_b)),
            Some("build-0202020202020202".into())
        );
        // A signature whose certificate couldn't be read falls back to the code.
        assert_eq!(
            identity_tag(Some(&signed(None)), Some(&exe_a)),
            Some("build-0101010101010101".into())
        );
        assert_eq!(identity_tag(None, None), None);
        // Builds that differ by one byte have different code identities.
        assert_ne!(code_digest(b"build a"), code_digest(b"build b"));
        assert_eq!(
            identity_tag(None, Some(&code_digest(b"build a"))).map(|t| t.len()),
            Some("build-".len() + 16)
        );
    }

    #[test]
    fn the_account_carries_the_tag_and_the_bare_key_without_one() {
        let key = lockdown_app::secrets::session_key("https://s.example", "sean");
        assert_eq!(account(&key, Some("cert-ab")), format!("{key}#cert-ab"));
        assert_eq!(account(&key, None), key);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn windows_and_linux_use_the_bare_key() {
        assert_eq!(this_build_tag(), None);
    }
}
