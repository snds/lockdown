//! The OS keystore, searched by service name (docs/plan/40-devtools.md §2).
//! The `keyring` crate the desktop writes through can't list, so each
//! platform's own search finds the entries, and `keyring` removes each one
//! by the same name it was written under:
//!
//! - **macOS:** a generic-password search in the login keychain by service
//!   (`SecItemCopyMatching`, every match, attributes only: no secret is
//!   read).
//! - **Windows:** Credential Manager's `CredEnumerateW` over
//!   `*.design.snds.lockdown` (`keyring` files an entry as
//!   `<account>.<service>`).
//! - **Linux and the BSDs:** the Secret Service, items whose `service`
//!   attribute is Lockdown's (locked collections are searched too; their
//!   attributes are readable without unlocking).

use crate::reset::Keystore;

/// The platform's keystore.
#[derive(Default)]
pub struct OsKeystore;

impl Keystore for OsKeystore {
    fn list(&self, service: &str) -> Result<Vec<String>, String> {
        let mut found = platform::list(service)?;
        found.sort();
        found.dedup();
        Ok(found)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| format!("the keystore isn't available: {e}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};

    pub fn list(service: &str) -> Result<Vec<String>, String> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        let results = match found {
            Ok(r) => r,
            // errSecItemNotFound: nothing filed under the service.
            Err(e) if e.code() == -25300 => return Ok(Vec::new()),
            Err(e) => return Err(format!("the keychain search failed: {e}")),
        };
        Ok(results
            .iter()
            .filter_map(|r| match r {
                SearchResult::Dict(_) => r.simplify_dict(),
                _ => None,
            })
            .filter_map(|d| d.get("acct").cloned())
            .collect())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A NUL-terminated wide string, read.
    ///
    /// # Safety
    /// `p` is null or points at a NUL-terminated UTF-16 string.
    unsafe fn read(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0;
        // SAFETY: the caller promises a terminator within the allocation.
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` units were just read from `p`.
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
    }

    pub fn list(service: &str) -> Result<Vec<String>, String> {
        let suffix = format!(".{service}");
        let filter = wide(&format!("*{suffix}"));
        let mut count: u32 = 0;
        let mut creds: *mut *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: the filter is NUL-terminated; on success the system
        // hands back `count` pointers, freed below with CredFree.
        let ok = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut creds) };
        if ok == 0 {
            // SAFETY: no arguments.
            let code = unsafe { GetLastError() };
            return if code == ERROR_NOT_FOUND {
                Ok(Vec::new())
            } else {
                Err(format!("Credential Manager's search failed (error {code})"))
            };
        }
        let mut out = Vec::new();
        for i in 0..count as usize {
            // SAFETY: `creds` holds `count` valid credential pointers.
            let cred = unsafe { &**creds.add(i) };
            if cred.Type != CRED_TYPE_GENERIC {
                continue;
            }
            // SAFETY: TargetName is a NUL-terminated string the system owns.
            let target = unsafe { read(cred.TargetName) };
            if let Some(account) = target.strip_suffix(&suffix) {
                out.push(account.to_string());
            }
        }
        // SAFETY: `creds` came from CredEnumerateW.
        unsafe { CredFree(creds.cast()) };
        Ok(out)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
mod platform {
    use std::collections::HashMap;

    use dbus_secret_service::{EncryptionType, SecretService};

    pub fn list(service: &str) -> Result<Vec<String>, String> {
        let ss = SecretService::connect(EncryptionType::Plain)
            .map_err(|e| format!("the Secret Service isn't available: {e}"))?;
        let found = ss
            .search_items(HashMap::from([("service", service)]))
            .map_err(|e| format!("the Secret Service search failed: {e}"))?;
        let mut out = Vec::new();
        for item in found.unlocked.iter().chain(found.locked.iter()) {
            let attrs = item
                .get_attributes()
                .map_err(|e| format!("couldn't read an entry's name: {e}"))?;
            if let Some(user) = attrs.get("username") {
                out.push(user.clone());
            }
        }
        Ok(out)
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
mod platform {
    pub fn list(_service: &str) -> Result<Vec<String>, String> {
        Err("this platform's keystore can't be searched".into())
    }
}
