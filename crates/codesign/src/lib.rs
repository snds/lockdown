//! Code-signature checks on the browser channel (docs/plan/29-release-polish.md
//! §2; 03-architecture §7, 04-security §13, D4, D33).
//!
//! Every build already checks the program on the other end of the channel
//! by user, executable path and SHA-256 (the identity crate's
//! `ProcessIdentityVerifier` on Unix, `lockdown-pipe` on Windows). A
//! **signed** build also checks its peer's code signature, both ways:
//!
//! ```text
//! lockdown-native-host ── socket (macOS) / pipe (Windows) ── desktop app
//!   own signature → Policy                                    own signature → Policy
//!   checks the app: path, SHA-256,                            checks the host: path, SHA-256,
//!   and signed by the same developer                          and signed by the same developer
//! ```
//!
//! - [`policy`]: what this end requires, decided once from its own
//!   signature (pure, tested everywhere).
//! - `macos`: the Security framework, the peer found by its audit token.
//! - `windows`: `WinVerifyTrust` and the signer's certificate.
//! - [`socket`]: the macOS socket that carries the audit token.
//! - [`SignatureVerifier`] and [`Both`]: the identity crate's
//!   [`PeerVerifier`] over one channel, and the path check and the signature
//!   check together.
//!
//! Linux has no OS code signatures, so there the path and hash check is the
//! whole check and [`own_policy`] says so.

#![deny(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;
use std::sync::Arc;

use ecosystem_identity::handshake::{PeerIdentity, PeerVerifier, TrustedPeer};
use ecosystem_identity::{Error, Result};

pub mod policy;
#[cfg(unix)]
pub mod socket;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

pub use policy::{CodeSignature, Expected, Policy, SignatureKind};
#[cfg(unix)]
pub use socket::{AuditToken, SocketChannel, SocketTransport};

/// The desktop app's signing identifier on macOS: the bundle identifier
/// Tauri signs `Lockdown.app` with (`tauri.conf.json`).
pub const APP_IDENTIFIER: &str = "design.snds.lockdown";

/// The browser host's signing identifier on macOS: `codesign` names a bare
/// executable by its file name, which is how Tauri signs the host inside
/// `Contents/MacOS`.
pub const HOST_IDENTIFIER: &str = "lockdown-native-host";

/// This build's policy toward a peer that must be signed as
/// `peer_identifier` (macOS; ignored on Windows). Read once at start.
pub fn own_policy(peer_identifier: &str) -> Policy {
    #[cfg(target_os = "macos")]
    {
        match macos::own() {
            Ok(own) => Policy::from_own(&own, Some(peer_identifier)),
            Err(e) => Policy::Refuse {
                reason: format!("this program's own signature couldn't be read ({e})"),
            },
        }
    }
    #[cfg(windows)]
    {
        let _ = peer_identifier;
        match windows::own() {
            Ok(own) => Policy::from_own(&own, None),
            Err(e) => Policy::Refuse {
                reason: format!("this program's own signature couldn't be read ({e})"),
            },
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = peer_identifier;
        Policy::ProcessOnly {
            reason: "Linux has no code signatures, so the other program is checked by its path and SHA-256 only".into(),
        }
    }
}

/// What a channel knows of the code on its other end, for the signature
/// check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerCode {
    /// The audit token the macOS socket reported.
    #[cfg(unix)]
    AuditToken(AuditToken),
    /// The image path the Windows pipe reported (the process is held open
    /// by the pipe channel).
    Image(PathBuf),
    /// Nothing: refused whenever a signature is required.
    Unknown,
}

/// The signature half of a peer check, for one channel: made from the
/// [`Policy`] and what that channel learned about its peer, so it must be
/// used for that channel only. Under [`Policy::ProcessOnly`] it passes and
/// leaves the decision to the path and hash check beside it ([`Both`]).
#[derive(Clone, Debug)]
pub struct SignatureVerifier {
    policy: Arc<Policy>,
    peer: PeerCode,
}

impl SignatureVerifier {
    /// The check of `peer` under `policy`.
    pub fn new(policy: Arc<Policy>, peer: PeerCode) -> Self {
        Self { policy, peer }
    }

    /// The check, with its reason when it refuses.
    pub fn check(&self) -> std::result::Result<(), String> {
        match self.policy.as_ref() {
            Policy::ProcessOnly { .. } => Ok(()),
            Policy::Refuse { reason } => Err(reason.clone()),
            Policy::Signature(expected) => verify(&self.peer, expected),
        }
    }
}

/// Checks `peer` against `expected` on this platform.
fn verify(peer: &PeerCode, expected: &Expected) -> std::result::Result<(), String> {
    match peer {
        #[cfg(target_os = "macos")]
        PeerCode::AuditToken(token) => macos::verify(token, expected),
        #[cfg(windows)]
        PeerCode::Image(path) => windows::verify(path, expected),
        PeerCode::Unknown => Err("the channel didn't say which program is on the other end".into()),
        #[allow(unreachable_patterns)]
        _ => {
            let _ = expected;
            Err("this platform can't check that program's code signature".into())
        }
    }
}

impl PeerVerifier for SignatureVerifier {
    fn verify(&self, _identity: &PeerIdentity) -> Result<TrustedPeer> {
        self.check().map_err(Error::PeerUntrusted)?;
        Ok(TrustedPeer {
            app: "signature".into(),
        })
    }
}

/// Two checks that must both pass, the first one's answer kept: the path
/// and hash check, then the signature check.
#[derive(Clone, Debug)]
pub struct Both<A, B>(pub A, pub B);

impl<A: PeerVerifier, B: PeerVerifier> PeerVerifier for Both<A, B> {
    fn verify(&self, identity: &PeerIdentity) -> Result<TrustedPeer> {
        let first = self.0.verify(identity)?;
        self.1.verify(identity)?;
        Ok(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::handshake::{AllowAll, Scripted};

    #[test]
    fn this_build_on_ci_linux_or_unsigned_checks_by_path_and_hash() {
        let p = own_policy(HOST_IDENTIFIER);
        // An unsigned test binary (Linux, Windows, Intel macOS) or a
        // linker-signed one (Apple silicon) has nothing to compare with.
        // CI's signed-peer jobs run their own signed binaries
        // (tests/signed.rs).
        if std::env::var_os("LOCKDOWN_CODESIGN_TEST").is_none() {
            assert_eq!(p.kind(), "path_and_hash", "{}", p.describe());
        }
    }

    #[test]
    fn both_checks_must_pass_and_process_only_leaves_it_to_the_path_check() {
        let process_only = Arc::new(Policy::ProcessOnly {
            reason: "unsigned".into(),
        });
        let refuse = Arc::new(Policy::Refuse {
            reason: "no".into(),
        });
        let sig = |p: &Arc<Policy>| SignatureVerifier::new(Arc::clone(p), PeerCode::Unknown);
        let ok = Both(AllowAll, sig(&process_only));
        assert_eq!(ok.verify(&PeerIdentity::Unknown).unwrap().app, "allow-all");
        assert!(
            Both(Scripted::refusing("wrong hash", 1), sig(&process_only))
                .verify(&PeerIdentity::Unknown)
                .is_err()
        );
        match Both(AllowAll, sig(&refuse)).verify(&PeerIdentity::Unknown) {
            Err(Error::PeerUntrusted(m)) => assert_eq!(m, "no"),
            other => panic!("{other:?}"),
        }
        let required = Arc::new(Policy::Signature(Expected {
            identifier: None,
            team: None,
            leaf_sha256: Some([1; 32]),
            leaf_sha1: Some([1; 20]),
            signer: "A".into(),
        }));
        assert!(sig(&required).check().unwrap_err().contains("didn't say"));
    }
}
