//! The platform's own "is it you?" check, for approvals (D99, D101):
//! Touch ID through `LocalAuthentication` on macOS, Windows Hello through
//! `UserConsentVerifier` on Windows, nothing on Linux (the app falls back
//! to the account password, then to confirming in its own window).
//!
//! This crate has no Lockdown dependencies and no cryptography of its own,
//! so it compile-checks for every target on its own
//! (`cargo check -p lockdown-platform-auth --target aarch64-apple-darwin`,
//! `--target x86_64-pc-windows-msvc`). The desktop shell wraps it as the
//! `Approver` lockdown-app calls. One rule here: a check that cannot run
//! is `Err` with the words, never a silent yes.
//!
//! **Verified where:** the Linux path (nothing available) runs in CI. The
//! macOS and Windows paths are compile-checked against the target, not run,
//! in the environment this was written in; the first run on real hardware
//! should confirm the prompt appears and that cancelling it is a no.

// The macOS path calls Objective-C through the objc2 bindings, which is
// `unsafe` by construction; everywhere else this crate is forbidden it.
#![cfg_attr(not(target_os = "macos"), forbid(unsafe_code))]
#![deny(unsafe_op_in_unsafe_fn)]

/// What the platform offers here, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// macOS, a Mac with Touch ID enrolled.
    TouchId,
    /// Windows, Windows Hello set up (face, fingerprint or PIN).
    WindowsHello,
}

impl Method {
    /// The word the UI shows.
    pub fn name(self) -> &'static str {
        match self {
            Self::TouchId => "Touch ID",
            Self::WindowsHello => "Windows Hello",
        }
    }
}

/// The platform check available on this machine right now, if any.
pub fn available() -> Option<Method> {
    platform::available()
}

/// Asks the person to confirm `reason` with the platform's check.
/// `Ok(true)` confirmed, `Ok(false)` cancelled or didn't match, `Err` the
/// check couldn't run (not set up, locked out, the OS refused).
pub fn verify(reason: &str) -> Result<bool, String> {
    platform::verify(reason)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAError, LAPolicy};

    use super::Method;

    pub fn available() -> Option<Method> {
        // SAFETY: LAContext::new has no preconditions; canEvaluatePolicy
        // takes a plain enum and answers through an NSError out-pointer the
        // bindings own.
        let can = unsafe {
            let ctx = LAContext::new();
            ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
                .is_ok()
        };
        can.then_some(Method::TouchId)
    }

    pub fn verify(reason: &str) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel::<Result<bool, String>>();
        let reply = RcBlock::new(move |ok: Bool, error: *mut NSError| {
            let outcome = if ok.as_bool() {
                Ok(true)
            } else if error.is_null() {
                Ok(false)
            } else {
                // SAFETY: a non-null error from the framework is a live
                // NSError for the duration of the reply block.
                let (code, message) = unsafe {
                    (
                        LAError((*error).code()),
                        (*error).localizedDescription().to_string(),
                    )
                };
                // Cancelling, "Use Password…", and a failed match are a
                // no; anything else means the check couldn't run.
                match code {
                    LAError::AuthenticationFailed
                    | LAError::UserCancel
                    | LAError::UserFallback
                    | LAError::SystemCancel
                    | LAError::AppCancel => Ok(false),
                    _ => Err(format!("Touch ID couldn't run: {message}")),
                }
            };
            let _ = tx.send(outcome);
        });
        // SAFETY: the reason is a live NSString and the block outlives the
        // call; the framework copies the block before returning.
        unsafe {
            let ctx = LAContext::new();
            ctx.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthenticationWithBiometrics,
                &NSString::from_str(reason),
                &reply,
            );
        }
        rx.recv()
            .map_err(|_| "the Touch ID prompt went away without answering".to_string())?
    }
}

#[cfg(windows)]
mod platform {
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::core::HSTRING;

    use super::Method;

    pub fn available() -> Option<Method> {
        let availability = UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .ok()?;
        (availability == UserConsentVerifierAvailability::Available).then_some(Method::WindowsHello)
    }

    pub fn verify(reason: &str) -> Result<bool, String> {
        let result = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(reason))
            .and_then(|op| op.join())
            .map_err(|e| format!("Windows Hello couldn't run: {e}"))?;
        match result {
            UserConsentVerificationResult::Verified => Ok(true),
            UserConsentVerificationResult::Canceled
            | UserConsentVerificationResult::RetriesExhausted => Ok(false),
            other => Err(format!(
                "Windows Hello isn't available right now ({other:?})"
            )),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::Method;

    pub fn available() -> Option<Method> {
        None
    }

    pub fn verify(_reason: &str) -> Result<bool, String> {
        Err("this platform has no built-in check; the password is used instead".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_the_words_people_know() {
        assert_eq!(Method::TouchId.name(), "Touch ID");
        assert_eq!(Method::WindowsHello.name(), "Windows Hello");
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn linux_has_nothing_and_says_so() {
        assert_eq!(available(), None);
        let err = verify("test").unwrap_err();
        assert!(err.contains("no built-in check"));
    }
}
