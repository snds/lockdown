//! The platform's own check as the `Approver` lockdown-app calls (D99,
//! D101): Touch ID on macOS, Windows Hello on Windows, through
//! `lockdown-platform-auth`. On Linux, and on a Mac or PC with nothing
//! enrolled, [`PlatformApprover::detect`] is `None` and nothing is
//! installed, so the app routes approvals to the account password or to
//! its own window (`lockdown_app::approval::route`). No Tauri needed, so
//! the Linux path is tested in the plain `cargo test`.

use lockdown_app::{ApprovalKind, Approver};
use lockdown_platform_auth::Method;

/// Touch ID or Windows Hello, whichever this machine has.
pub struct PlatformApprover {
    method: Method,
}

impl PlatformApprover {
    /// The platform check available right now, or `None`.
    pub fn detect() -> Option<Self> {
        lockdown_platform_auth::available().map(|method| Self { method })
    }
}

impl Approver for PlatformApprover {
    fn confirm(&self, kind: ApprovalKind, reason: &str) -> Result<bool, String> {
        let verb = match kind {
            ApprovalKind::Use => "Use",
            ApprovalKind::Reveal => "Reveal",
            ApprovalKind::Approve => "Approve",
        };
        lockdown_platform_auth::verify(&format!("{verb}: {reason}"))
    }

    fn method(&self) -> &'static str {
        self.method.name()
    }
}

#[cfg(all(test, not(any(target_os = "macos", windows))))]
mod tests {
    use super::*;

    #[test]
    fn linux_installs_nothing() {
        assert!(PlatformApprover::detect().is_none());
    }
}
