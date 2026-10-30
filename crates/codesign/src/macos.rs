//! Code signatures on macOS, through the Security framework
//! (03-architecture §7: "audit token of the connecting process → `SecCode`
//! → verify the signing identity and team").
//!
//! - [`own`]: this process's signature (`SecCodeCopySelf`), from which the
//!   [`Policy`](crate::Policy) is made.
//! - [`verify`]: the peer, by the audit token its socket reported: a
//!   `SecCode` for that exact process (`SecCodeCopyGuestWithAttributes`
//!   with `kSecGuestAttributeAudit`; a token whose process has exec'd or
//!   gone is refused), checked against the requirement the policy wrote
//!   (`SecCodeCheckValidity`, which checks the running code and the file it
//!   came from), and then its signing information compared by the same
//!   rule ([`Expected::check`]).

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::data::{CFDataGetBytePtr, CFDataGetLength, CFDataRef};
use core_foundation_sys::dictionary::{
    CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation_sys::number::{
    CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, kCFNumberSInt64Type,
};
use core_foundation_sys::string::{CFStringGetTypeID, CFStringRef};

use crate::policy::{CodeSignature, Expected, SignatureKind};
use crate::socket::AuditToken;

type OSStatus = i32;
type SecCSFlags = u32;

const K_SEC_CS_DEFAULT_FLAGS: SecCSFlags = 0;
/// `kSecCSSigningInformation`: include the certificates and team.
const K_SEC_CS_SIGNING_INFORMATION: SecCSFlags = 1 << 1;
/// `kSecCSRequirementInformation`: include the entitlements.
const K_SEC_CS_REQUIREMENT_INFORMATION: SecCSFlags = 1 << 2;
/// `kSecCodeSignatureAdhoc` in `kSecCodeInfoFlags`.
const K_SEC_CODE_SIGNATURE_ADHOC: i64 = 0x0002;

const ERR_SEC_CS_UNSIGNED: OSStatus = -67062;
const ERR_SEC_CS_REQ_FAILED: OSStatus = -67050;
const ERR_SEC_CS_NO_SUCH_CODE: OSStatus = -67065;
const ERR_SEC_CS_GUEST_INVALID: OSStatus = -67063;
const ERR_SEC_CS_SIGNATURE_FAILED: OSStatus = -67061;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecGuestAttributeAudit: CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;
    static kSecCodeInfoCertificates: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
    static kSecCodeInfoEntitlementsDict: CFStringRef;

    fn SecCodeCopySelf(flags: SecCSFlags, code: *mut CFTypeRef) -> OSStatus;
    fn SecCodeCopyGuestWithAttributes(
        host: CFTypeRef,
        attributes: CFDictionaryRef,
        flags: SecCSFlags,
        guest: *mut CFTypeRef,
    ) -> OSStatus;
    fn SecCodeCheckValidity(code: CFTypeRef, flags: SecCSFlags, requirement: CFTypeRef)
    -> OSStatus;
    fn SecRequirementCreateWithString(
        text: CFStringRef,
        flags: SecCSFlags,
        requirement: *mut CFTypeRef,
    ) -> OSStatus;
    fn SecCodeCopySigningInformation(
        code: CFTypeRef,
        flags: SecCSFlags,
        information: *mut CFDictionaryRef,
    ) -> OSStatus;
    fn SecCertificateCopyData(certificate: CFTypeRef) -> CFDataRef;
}

/// A Core Foundation object this module owns (released on drop).
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` came from a Copy/Create call, so this module
            // holds one reference to it, released exactly once here.
            unsafe { CFRelease(self.0) }
        }
    }
}

fn status(what: &str, s: OSStatus) -> String {
    let why = match s {
        ERR_SEC_CS_UNSIGNED => "it isn't signed",
        ERR_SEC_CS_REQ_FAILED => {
            "it isn't the expected program signed by the same developer as this program"
        }
        ERR_SEC_CS_NO_SUCH_CODE => "that process is gone or has changed since it connected",
        ERR_SEC_CS_GUEST_INVALID => "the running code no longer matches its signature",
        ERR_SEC_CS_SIGNATURE_FAILED => "its signature doesn't verify",
        _ => "the Security framework refused it",
    };
    format!("{what}: {why} (OSStatus {s})")
}

/// Reads one value of the signing-information dictionary.
///
/// # Safety
/// `info` must be a live CFDictionary and `key` a live CFString.
unsafe fn value(info: CFDictionaryRef, key: CFStringRef) -> CFTypeRef {
    // SAFETY: per the caller's contract; the result is borrowed from `info`.
    unsafe { CFDictionaryGetValue(info, key.cast::<c_void>()) }
}

/// A borrowed CFString value as Rust text.
///
/// # Safety
/// `v` must be null or a live CF object.
unsafe fn text(v: CFTypeRef) -> Option<String> {
    // SAFETY: a non-null live CF object has a type id; the wrap takes its
    // own reference (get rule), so the dictionary's stays untouched.
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
            return None;
        }
        Some(CFString::wrap_under_get_rule(v as CFStringRef).to_string())
    }
}

/// The signature of `code` (a `SecCodeRef`), given whether it checked out.
///
/// # Safety
/// `code` must be a live `SecCodeRef`.
unsafe fn signature_of(code: CFTypeRef, valid: bool) -> Result<CodeSignature, String> {
    let mut info: CFDictionaryRef = std::ptr::null();
    // SAFETY: `code` is live per the contract; `info` is a valid out-pointer.
    let s =
        unsafe { SecCodeCopySigningInformation(code, K_SEC_CS_SIGNING_INFORMATION, &raw mut info) };
    if s != 0 {
        return Err(status("couldn't read the signing information", s));
    }
    let _info = Owned(info.cast());
    // SAFETY: `info` is a live dictionary owned by `_info`; the keys are
    // the framework's constant strings.
    unsafe {
        let identifier = text(value(info, kSecCodeInfoIdentifier));
        let Some(identifier) = identifier else {
            return Ok(CodeSignature::unsigned());
        };
        let mut flags: i64 = 0;
        let f = value(info, kSecCodeInfoFlags);
        if !f.is_null() && CFGetTypeID(f) == CFNumberGetTypeID() {
            CFNumberGetValue(
                f as CFNumberRef,
                kCFNumberSInt64Type,
                (&raw mut flags).cast(),
            );
        }
        let team = text(value(info, kSecCodeInfoTeamIdentifier));
        let certs = value(info, kSecCodeInfoCertificates);
        let leaf = if !certs.is_null()
            && CFGetTypeID(certs) == CFArrayGetTypeID()
            && CFArrayGetCount(certs.cast()) > 0
        {
            let cert = CFArrayGetValueAtIndex(certs.cast(), 0);
            let data = SecCertificateCopyData(cert);
            if data.is_null() {
                None
            } else {
                let owned = Owned(data.cast());
                let der = std::slice::from_raw_parts(
                    CFDataGetBytePtr(data),
                    usize::try_from(CFDataGetLength(data)).unwrap_or(0),
                )
                .to_vec();
                drop(owned);
                Some(der)
            }
        } else {
            None
        };
        if flags & K_SEC_CODE_SIGNATURE_ADHOC != 0 || leaf.is_none() {
            return Ok(CodeSignature {
                kind: SignatureKind::AdHoc,
                valid,
                identifier: Some(identifier),
                ..CodeSignature::unsigned()
            });
        }
        let signature = CodeSignature {
            kind: SignatureKind::Signed,
            valid,
            identifier: Some(identifier),
            team,
            leaf_sha256: None,
            leaf_sha1: None,
            signer: None,
        };
        let der = leaf.unwrap_or_default();
        let mut signature = signature.with_leaf(&der);
        signature.signer = Some(match &signature.team {
            Some(team) => format!("the developer with Team ID {team}"),
            None => "the certificate this build is signed with".into(),
        });
        Ok(signature)
    }
}

/// This process's own code signature.
pub fn own() -> Result<CodeSignature, String> {
    let mut code: CFTypeRef = std::ptr::null();
    // SAFETY: `code` is a valid out-pointer.
    let s = unsafe { SecCodeCopySelf(K_SEC_CS_DEFAULT_FLAGS, &raw mut code) };
    if s != 0 {
        return Err(status("couldn't read this program's code", s));
    }
    let code = Owned(code);
    // SAFETY: `code.0` is the live SecCodeRef just copied; no requirement.
    let validity =
        unsafe { SecCodeCheckValidity(code.0, K_SEC_CS_DEFAULT_FLAGS, std::ptr::null()) };
    if validity == ERR_SEC_CS_UNSIGNED {
        return Ok(CodeSignature::unsigned());
    }
    // SAFETY: as above.
    unsafe { signature_of(code.0, validity == 0) }
}

/// Checks the process behind `token` against `expected`.
pub fn verify(token: &AuditToken, expected: &Expected) -> Result<(), String> {
    let requirement_text = expected.requirement()?;
    let bytes = token.to_bytes();
    // SAFETY: the framework's constant key, alive for the program's life.
    let key = unsafe { CFString::wrap_under_get_rule(kSecGuestAttributeAudit) };
    let data = CFData::from_buffer(&bytes);
    let attributes = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), data.as_CFType())]);
    let mut guest: CFTypeRef = std::ptr::null();
    // SAFETY: a null host means "the system"; `attributes` is a live
    // dictionary; `guest` is a valid out-pointer.
    let s = unsafe {
        SecCodeCopyGuestWithAttributes(
            std::ptr::null(),
            attributes.as_concrete_TypeRef(),
            K_SEC_CS_DEFAULT_FLAGS,
            &raw mut guest,
        )
    };
    if s != 0 {
        return Err(status(
            "the program on the other end couldn't be checked",
            s,
        ));
    }
    let guest = Owned(guest);
    let text = CFString::new(&requirement_text);
    let mut requirement: CFTypeRef = std::ptr::null();
    // SAFETY: `text` is a live CFString; `requirement` a valid out-pointer.
    let s = unsafe {
        SecRequirementCreateWithString(
            text.as_concrete_TypeRef(),
            K_SEC_CS_DEFAULT_FLAGS,
            &raw mut requirement,
        )
    };
    if s != 0 {
        return Err(status("the code requirement couldn't be made", s));
    }
    let requirement = Owned(requirement);
    // SAFETY: both are live objects owned above.
    let s = unsafe { SecCodeCheckValidity(guest.0, K_SEC_CS_DEFAULT_FLAGS, requirement.0) };
    if s != 0 {
        return Err(status("the program on the other end was refused", s));
    }
    // SAFETY: `guest.0` is the live SecCodeRef.
    let peer = unsafe { signature_of(guest.0, true)? };
    expected.check(&peer)
}

/// The entitlement that names the app groups a program may use.
pub const APP_GROUPS_ENTITLEMENT: &str = "com.apple.security.application-groups";

/// The app groups this process's own signature names
/// ([`APP_GROUPS_ENTITLEMENT`]), in the order they were signed; empty when
/// the program is unsigned or names none. This reads what the signature
/// says, not whether macOS honours it: the caller decides that from the
/// kind of signature ([`own`]), since an ad-hoc one can't carry a usable
/// group (docs/plan/41-mac-paths.md).
pub fn own_app_groups() -> Result<Vec<String>, String> {
    let mut code: CFTypeRef = std::ptr::null();
    // SAFETY: `code` is a valid out-pointer.
    let s = unsafe { SecCodeCopySelf(K_SEC_CS_DEFAULT_FLAGS, &raw mut code) };
    if s != 0 {
        return Err(status("couldn't read this program's code", s));
    }
    let code = Owned(code);
    let mut info: CFDictionaryRef = std::ptr::null();
    // SAFETY: `code.0` is the live SecCodeRef just copied; `info` is a
    // valid out-pointer.
    let s = unsafe {
        SecCodeCopySigningInformation(code.0, K_SEC_CS_REQUIREMENT_INFORMATION, &raw mut info)
    };
    if s != 0 {
        return Err(status("couldn't read this program's entitlements", s));
    }
    let _info = Owned(info.cast());
    let key = CFString::new(APP_GROUPS_ENTITLEMENT);
    // SAFETY: `info` is a live dictionary owned by `_info`; every value
    // read from it is borrowed and its type checked before use.
    unsafe {
        let entitlements = value(info, kSecCodeInfoEntitlementsDict);
        if entitlements.is_null() || CFGetTypeID(entitlements) != CFDictionaryGetTypeID() {
            return Ok(Vec::new());
        }
        let groups = CFDictionaryGetValue(
            entitlements as CFDictionaryRef,
            key.as_concrete_TypeRef().cast::<c_void>(),
        );
        if groups.is_null() || CFGetTypeID(groups) != CFArrayGetTypeID() {
            return Ok(Vec::new());
        }
        let count = CFArrayGetCount(groups.cast());
        Ok((0..count)
            .filter_map(|i| text(CFArrayGetValueAtIndex(groups.cast(), i)))
            .collect())
    }
}
