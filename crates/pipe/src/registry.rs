//! The one registry value the browsers need on Windows (Windows only): a
//! native-messaging host is found through
//! `HKCU\Software\<browser>\NativeMessagingHosts\design.snds.lockdown`,
//! whose default value is the manifest file's path
//! (`lockdown_native_messaging::manifest::Target::registry_key`). Only the
//! current user's hive is written, so no elevation is ever needed.

#![allow(unsafe_code)]

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The key under the current user's hive: `subkey` may be written with or
/// without a leading `HKEY_CURRENT_USER\` (or `HKCU\`), as
/// `Target::registry_key` writes it.
fn under_hkcu(subkey: &str) -> Vec<u16> {
    let bare = subkey
        .strip_prefix(r"HKEY_CURRENT_USER\")
        .or_else(|| subkey.strip_prefix(r"HKCU\"))
        .unwrap_or(subkey);
    wide(bare)
}

fn check(call: &str, code: u32) -> std::io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        let e = std::io::Error::from_raw_os_error(code as i32);
        Err(std::io::Error::new(e.kind(), format!("{call}: {e}")))
    }
}

/// Sets the default value of `HKCU\<subkey>` to `value`, creating the key.
pub fn set_default(subkey: &str, value: &str) -> std::io::Result<()> {
    let key_name = under_hkcu(subkey);
    let mut key: HKEY = null_mut();
    // SAFETY: the name is NUL-terminated; `key` receives a key handle that
    // is closed below.
    check("RegCreateKeyExW", unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        )
    })?;
    let data = wide(value);
    // SAFETY: `key` is open with KEY_SET_VALUE; the data is a
    // NUL-terminated UTF-16 string of the length given, in bytes.
    let set = unsafe {
        RegSetValueExW(
            key,
            null(),
            0,
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    // SAFETY: closes the handle opened above.
    unsafe { RegCloseKey(key) };
    check("RegSetValueExW", set)
}

/// The default value of `HKCU\<subkey>`, when the key and a string value
/// exist.
pub fn default_value(subkey: &str) -> Option<String> {
    let key_name = under_hkcu(subkey);
    let mut buf = vec![0u16; 4096];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: the name is NUL-terminated; `buf` holds `bytes` bytes.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            null(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let units = (bytes as usize / 2).min(buf.len());
    let end = buf[..units].iter().position(|u| *u == 0).unwrap_or(units);
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Removes `HKCU\<subkey>` and everything under it; a missing key is fine.
pub fn remove(subkey: &str) -> std::io::Result<()> {
    let key_name = under_hkcu(subkey);
    // SAFETY: the name is NUL-terminated.
    let code = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key_name.as_ptr()) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check("RegDeleteTreeW", code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_value_is_set_read_and_removed_under_the_users_hive() {
        let key = format!(
            r"Software\snds-lockdown-test-{}\NativeMessagingHosts\design.snds.lockdown",
            std::process::id()
        );
        let path = r"C:\Users\me\AppData\Local\snds\lockdown\native-messaging\chrome\design.snds.lockdown.json";
        set_default(&format!(r"HKEY_CURRENT_USER\{key}"), path).unwrap();
        assert_eq!(default_value(&key).as_deref(), Some(path));
        let top = format!(r"Software\snds-lockdown-test-{}", std::process::id());
        remove(&top).unwrap();
        assert_eq!(default_value(&key), None);
        remove(&top).unwrap();
    }
}
