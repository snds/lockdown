//! Who confirms a sensitive action, and how (D99, D101).
//!
//! Three kinds of confirmation exist. **Use** is "yes, fill or copy this
//! now" (a crown jewel, a card); **Approve** is "yes, let that happen"
//! (revoke a device, approve another device); **Reveal** is "show me the
//! value" of a crown jewel, or hand the whole vault out as a file. The
//! brief's rule (§3.1, crown-jewel row): a fresh biometric to use, the
//! full password to reveal. So:
//!
//! - `Use` and `Approve` go to the platform's own check (Touch ID, Windows
//!   Hello) when the shell installed one, else to the account password;
//! - `Reveal` goes to the account password whenever one exists, whatever
//!   else is installed; the platform check stands in only when the device
//!   has no account yet;
//! - with neither a password nor a platform check (a device with no
//!   account, on Linux), the only thing left is the person confirming in
//!   the app's own window. [`route`] says so (`Route::Window`), and the
//!   shell shows a plain confirm dialog; nothing pretends to be a check it
//!   isn't.
//!
//! The password is never prompted for from here: a shell asks in its own
//! window, sends it once, and this module checks it. A password check is
//! an OPAQUE sign-in to the account (the password never leaves the device
//! either way); a wrong one comes back as `Ok(false)`, a server that
//! can't be reached as `Err`.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// What is being confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Fill or copy a protected value without seeing it.
    Use,
    /// See a protected value, or export the vault.
    Reveal,
    /// Let something happen: revoke a device, approve a request.
    Approve,
}

/// Something that can ask the person and come back with yes or no.
pub trait Approver {
    /// `Ok(true)` confirmed, `Ok(false)` declined or failed to match,
    /// `Err` the check couldn't run at all (the words say why).
    fn confirm(&self, kind: ApprovalKind, reason: &str) -> Result<bool, String>;
    /// The word for it in the UI: `Touch ID`, `Windows Hello`, `password`.
    fn method(&self) -> &'static str;
}

/// The fallback: the account password, typed once into the app's own
/// window and checked by `check` (an OPAQUE sign-in, in the session).
pub struct PasswordApprover<F: Fn(&str) -> Result<bool, String>> {
    password: Zeroizing<String>,
    check: F,
}

impl<F: Fn(&str) -> Result<bool, String>> PasswordApprover<F> {
    /// Holds the typed password (wiped on drop) and how to check it.
    pub fn new(password: Zeroizing<String>, check: F) -> Self {
        Self { password, check }
    }
}

impl<F: Fn(&str) -> Result<bool, String>> Approver for PasswordApprover<F> {
    fn confirm(&self, _kind: ApprovalKind, _reason: &str) -> Result<bool, String> {
        if self.password.is_empty() {
            return Ok(false);
        }
        (self.check)(&self.password)
    }

    fn method(&self) -> &'static str {
        "password"
    }
}

/// Which approver a kind goes to, given what this device has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The platform's check (Touch ID, Windows Hello).
    Platform,
    /// The account password.
    Password,
    /// Neither exists: the person confirms in the app's own window.
    Window,
}

/// The rule in the module doc. Pure, so a shell can show the right prompt
/// before calling.
pub fn route(kind: ApprovalKind, platform_present: bool, password_exists: bool) -> Route {
    match kind {
        ApprovalKind::Reveal => {
            if password_exists {
                Route::Password
            } else if platform_present {
                Route::Platform
            } else {
                Route::Window
            }
        }
        ApprovalKind::Use | ApprovalKind::Approve => {
            if platform_present {
                Route::Platform
            } else if password_exists {
                Route::Password
            } else {
                Route::Window
            }
        }
    }
}

/// What a shell sends with a gated call: the password it asked for, or
/// that the person confirmed in the window when nothing else exists. The
/// password is wiped when the input drops.
#[derive(Clone, Debug, Default)]
pub struct GateInput {
    /// The account password, when the shell was told to ask for it.
    pub password: Option<String>,
    /// The person pressed Confirm in the app's own window (`Route::Window`
    /// only; ignored when a real check exists).
    pub confirmed: bool,
}

impl<'de> Deserialize<'de> for GateInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Raw {
            password: Option<String>,
            confirmed: bool,
        }
        let raw = Raw::deserialize(d)?;
        Ok(Self {
            password: raw.password,
            confirmed: raw.confirmed,
        })
    }
}

impl Drop for GateInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        if let Some(p) = self.password.as_mut() {
            p.zeroize();
        }
    }
}

impl Route {
    /// The name a shell switches on: `platform`, `password`, `window`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Password => "password",
            Self::Window => "window",
        }
    }
}

/// How a gated call was confirmed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Approval {
    /// `password`, `Touch ID`, `Windows Hello`, or `window`.
    pub method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_wants_the_password_and_use_wants_the_platform() {
        use ApprovalKind::*;
        // Both present.
        assert_eq!(route(Reveal, true, true), Route::Password);
        assert_eq!(route(Use, true, true), Route::Platform);
        assert_eq!(route(Approve, true, true), Route::Platform);
        // No platform check: everything falls back to the password.
        assert_eq!(route(Reveal, false, true), Route::Password);
        assert_eq!(route(Use, false, true), Route::Password);
        assert_eq!(route(Approve, false, true), Route::Password);
        // No account: the platform check stands in for the password.
        assert_eq!(route(Reveal, true, false), Route::Platform);
        assert_eq!(route(Use, true, false), Route::Platform);
        // Nothing at all: the window.
        assert_eq!(route(Reveal, false, false), Route::Window);
        assert_eq!(route(Use, false, false), Route::Window);
        assert_eq!(route(Approve, false, false), Route::Window);
    }

    #[test]
    fn the_password_approver_checks_once_and_an_empty_password_is_a_no() {
        let asked = std::cell::Cell::new(0);
        let check = |p: &str| {
            asked.set(asked.get() + 1);
            Ok(p == "correct horse")
        };
        let right = PasswordApprover::new(Zeroizing::new("correct horse".into()), check);
        assert_eq!(right.confirm(ApprovalKind::Reveal, "export"), Ok(true));
        assert_eq!(right.method(), "password");
        let wrong = PasswordApprover::new(Zeroizing::new("hunter2".into()), check);
        assert_eq!(wrong.confirm(ApprovalKind::Reveal, "export"), Ok(false));
        let empty = PasswordApprover::new(Zeroizing::new(String::new()), check);
        assert_eq!(empty.confirm(ApprovalKind::Reveal, "export"), Ok(false));
        assert_eq!(asked.get(), 2, "an empty password is never sent anywhere");

        let offline = PasswordApprover::new(Zeroizing::new("correct horse".into()), |_: &str| {
            Err("the server couldn't be reached".to_string())
        });
        assert!(offline.confirm(ApprovalKind::Use, "copy").is_err());
    }
}
