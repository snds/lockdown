//! Authenticode signatures on Windows (03-architecture §7:
//! "`GetNamedPipeClientProcessId` → open the process → verify the
//! Authenticode signature"). `lockdown-pipe` already found the peer's
//! process, holds it open and reads its image path; this module checks the
//! file at that path:
//!
//! 1. Opens it for reading with writes and deletes shut out
//!    (`FILE_SHARE_READ` only), so it cannot change while it is checked.
//! 2. `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) on that handle:
//!    the signature must verify and its chain must reach a root this
//!    machine trusts. Revocation is not fetched (no network on a
//!    connection; `WTD_REVOKE_NONE`, cache only).
//! 3. Reads the signer's certificate from the verified signature and
//!    compares its SHA-256 with this program's own signer
//!    ([`Expected::check`]).

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, TRUST_E_NOSIGNATURE,
    TRUST_E_PROVIDER_UNKNOWN, TRUST_E_SUBJECT_FORM_UNKNOWN,
};
use windows_sys::Win32::Security::Cryptography::{
    CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
};
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
    WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, OPEN_EXISTING,
};

use crate::policy::{CodeSignature, Expected, SignatureKind};

/// A handle closed on drop.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// The Authenticode signature of the file at `path`.
pub fn signature_of(path: &Path) -> Result<CodeSignature, String> {
    let name = wide(path);
    // SAFETY: `name` is NUL-terminated and outlives the call; no security
    // attributes or template.
    let file = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE || file.is_null() {
        return Err(format!(
            "{} couldn't be opened to check its signature: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let file = Handle(file);
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: name.as_ptr(),
        hFile: file.0,
        pgKnownSubject: std::ptr::null_mut(),
    };
    // SAFETY: an all-zero WINTRUST_DATA is the documented starting point;
    // every field used is set below.
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = &raw mut info;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `info` are valid for the call and the state data
    // is closed below; no window.
    let result = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &raw mut action,
            (&raw mut data).cast(),
        )
    };
    let outcome = (|| {
        if result == TRUST_E_NOSIGNATURE
            || result == TRUST_E_SUBJECT_FORM_UNKNOWN
            || result == TRUST_E_PROVIDER_UNKNOWN
        {
            return Ok(CodeSignature::unsigned());
        }
        // SAFETY: the state data is the one WinVerifyTrust just returned;
        // the provider data, signer and certificate it points at stay valid
        // until the state is closed below, and are only read here.
        let der = unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if prov.is_null() {
                None
            } else {
                let signer = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
                if signer.is_null()
                    || (*signer).csCertChain == 0
                    || (*signer).pasCertChain.is_null()
                {
                    None
                } else {
                    let cert = (*(*signer).pasCertChain).pCert;
                    if cert.is_null() {
                        None
                    } else {
                        let der = std::slice::from_raw_parts(
                            (*cert).pbCertEncoded,
                            (*cert).cbCertEncoded as usize,
                        )
                        .to_vec();
                        let mut name = vec![0u16; 256];
                        let n = CertGetNameStringW(
                            cert,
                            CERT_NAME_SIMPLE_DISPLAY_TYPE,
                            0,
                            std::ptr::null(),
                            name.as_mut_ptr(),
                            name.len() as u32,
                        );
                        let subject =
                            (n > 1).then(|| String::from_utf16_lossy(&name[..n as usize - 1]));
                        Some((der, subject))
                    }
                }
            }
        };
        let Some((der, subject)) = der else {
            return Err(format!(
                "{} is signed, but its signer couldn't be read (WinVerifyTrust {result:#010x})",
                path.display()
            ));
        };
        let mut signature = CodeSignature {
            kind: SignatureKind::Signed,
            valid: result == 0,
            identifier: None,
            team: None,
            leaf_sha256: None,
            leaf_sha1: None,
            signer: subject.map(|s| format!("\"{s}\"")),
        }
        .with_leaf(&der);
        if signature.signer.is_none() {
            signature.signer = Some("this build's certificate".into());
        }
        Ok(signature)
    })();
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the state the verify call opened, with the same data.
    unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &raw mut action,
            (&raw mut data).cast(),
        )
    };
    drop(file);
    outcome
}

/// This program's own signature.
pub fn own() -> Result<CodeSignature, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("couldn't find this program's file: {e}"))?;
    signature_of(&exe)
}

/// Checks the program whose image is `path` against `expected`.
pub fn verify(path: &Path, expected: &Expected) -> Result<(), String> {
    let peer = signature_of(path)?;
    expected.check(&peer)
}
