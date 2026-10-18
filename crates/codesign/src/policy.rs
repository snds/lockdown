//! What one end of the channel requires of the other's code signature,
//! decided from its own (docs/plan/29-release-polish.md §2). Pure: the
//! platform modules read signatures from the OS and hand them here, so the
//! rules are tested on every platform.
//!
//! | This build is | The peer must be |
//! |---|---|
//! | signed with an Apple Team ID | signed under an Apple anchor by the same team, with the expected identifier |
//! | signed with any other certificate (Windows, or a macOS certificate without a team) | signed with the same leaf certificate, the OS accepting the signature (and on macOS the expected identifier) |
//! | unsigned, or ad hoc (macOS) | checked by path and SHA-256 only, as every build already is ([`Policy::ProcessOnly`]) |
//! | signed, but the OS rejects its own signature | nothing: every peer is refused ([`Policy::Refuse`]) |
//!
//! A signed build never falls back to the path check alone, so a signed
//! host talks only to a signed app of the same developer, and the reverse.

use std::fmt;

use hex::Hex;

/// How a piece of code is signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureKind {
    /// No signature at all.
    Unsigned,
    /// An ad hoc signature (macOS): a hash of the code with no signer. The
    /// linker signs every Apple silicon build this way.
    AdHoc,
    /// Signed by a certificate.
    Signed,
}

/// A code signature as the OS reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSignature {
    pub kind: SignatureKind,
    /// Whether the OS accepted the signature: on macOS the code is valid
    /// (`SecCodeCheckValidity`); on Windows `WinVerifyTrust` trusted the
    /// chain.
    pub valid: bool,
    /// The signing identifier (macOS); `None` on Windows, where Authenticode
    /// has none.
    pub identifier: Option<String>,
    /// The Apple Team ID, for a certificate Apple issued.
    pub team: Option<String>,
    /// The signer (leaf) certificate's SHA-256.
    pub leaf_sha256: Option<[u8; 32]>,
    /// The signer certificate's SHA-1, the hash the macOS requirement
    /// language names a certificate by.
    pub leaf_sha1: Option<[u8; 20]>,
    /// The signer's name, for messages.
    pub signer: Option<String>,
}

impl CodeSignature {
    /// An unsigned piece of code.
    pub fn unsigned() -> Self {
        Self {
            kind: SignatureKind::Unsigned,
            valid: false,
            identifier: None,
            team: None,
            leaf_sha256: None,
            leaf_sha1: None,
            signer: None,
        }
    }

    /// The fingerprints of a DER-encoded signer certificate.
    pub fn with_leaf(mut self, der: &[u8]) -> Self {
        use sha1::Digest as _;
        self.leaf_sha256 = Some(sha2::Sha256::digest(der).into());
        self.leaf_sha1 = Some(sha1::Sha1::digest(der).into());
        self
    }
}

/// What a signed build requires of its peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expected {
    /// The identifier the peer must be signed with (macOS).
    pub identifier: Option<String>,
    /// The Apple Team ID the peer must be signed by.
    pub team: Option<String>,
    /// Otherwise, the exact signer certificate (SHA-256, and SHA-1 for the
    /// macOS requirement).
    pub leaf_sha256: Option<[u8; 32]>,
    pub leaf_sha1: Option<[u8; 20]>,
    /// Who that is, for messages.
    pub signer: String,
}

/// What this end checks of the other end's code signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Policy {
    /// The peer must be signed by the same developer ([`Expected`]), on top
    /// of the path and hash check.
    Signature(Expected),
    /// This build carries no signer, so there is nothing to compare a
    /// peer's signature with: the peer is checked by path and SHA-256 only.
    ProcessOnly { reason: String },
    /// Every peer is refused, with the reason.
    Refuse { reason: String },
}

/// Characters a Team ID or an identifier may hold in a requirement string.
fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

impl Policy {
    /// The policy for a build signed as `own`, whose peer must carry
    /// `peer_identifier` (macOS; `None` where signatures name no
    /// identifier).
    pub fn from_own(own: &CodeSignature, peer_identifier: Option<&str>) -> Self {
        match own.kind {
            SignatureKind::Unsigned => Self::ProcessOnly {
                reason: "this is a development build without a code signature, so the other program is checked by its path and SHA-256 only".into(),
            },
            SignatureKind::AdHoc => Self::ProcessOnly {
                reason: "this is a development build with an ad hoc signature, which names no developer, so the other program is checked by its path and SHA-256 only".into(),
            },
            SignatureKind::Signed if !own.valid => Self::Refuse {
                reason: "this copy of Lockdown is signed, but the system doesn't accept its signature, so it trusts no other program".into(),
            },
            SignatureKind::Signed => {
                if let Some(id) = peer_identifier
                    && !plain(id)
                {
                    return Self::Refuse {
                        reason: format!("{id:?} can't be checked as a signing identifier"),
                    };
                }
                let signer = own
                    .signer
                    .clone()
                    .or_else(|| own.team.clone())
                    .unwrap_or_else(|| "this build's signer".into());
                match (&own.team, own.leaf_sha256, own.leaf_sha1) {
                    (Some(team), _, _) if plain(team) => Self::Signature(Expected {
                        identifier: peer_identifier.map(str::to_string),
                        team: Some(team.clone()),
                        leaf_sha256: None,
                        leaf_sha1: None,
                        signer,
                    }),
                    (Some(team), _, _) => Self::Refuse {
                        reason: format!("the team {team:?} in this build's signature can't be checked"),
                    },
                    (None, Some(sha256), sha1) => Self::Signature(Expected {
                        identifier: peer_identifier.map(str::to_string),
                        team: None,
                        leaf_sha256: Some(sha256),
                        leaf_sha1: sha1,
                        signer,
                    }),
                    (None, None, _) => Self::Refuse {
                        reason: "this build is signed, but its signer's certificate couldn't be read".into(),
                    },
                }
            }
        }
    }

    /// One word for the settings screen: `signature`, `path_and_hash` or
    /// `refused`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Signature(_) => "signature",
            Self::ProcessOnly { .. } => "path_and_hash",
            Self::Refuse { .. } => "refused",
        }
    }

    /// A sentence for the settings screen.
    pub fn describe(&self) -> String {
        match self {
            Self::Signature(e) => format!(
                "The other program must be signed by {} and match its installed path and SHA-256.",
                e.signer
            ),
            Self::ProcessOnly { reason } | Self::Refuse { reason } => {
                let mut s = reason.clone();
                if let Some(first) = s.get(..1) {
                    let upper = first.to_uppercase();
                    s.replace_range(..1, &upper);
                }
                s.push('.');
                s
            }
        }
    }
}

impl Expected {
    /// The code requirement (Apple's requirement language) the peer must
    /// satisfy: its identifier, and an Apple anchor with this team, or this
    /// exact signer certificate.
    pub fn requirement(&self) -> Result<String, String> {
        let mut parts = Vec::new();
        if let Some(id) = &self.identifier {
            if !plain(id) {
                return Err(format!("{id:?} can't be used in a code requirement"));
            }
            parts.push(format!("identifier \"{id}\""));
        }
        match (&self.team, &self.leaf_sha1) {
            (Some(team), _) => {
                if !plain(team) {
                    return Err(format!("{team:?} can't be used in a code requirement"));
                }
                parts.push("anchor apple generic".into());
                parts.push(format!("certificate leaf[subject.OU] = \"{team}\""));
            }
            (None, Some(sha1)) => {
                parts.push(format!("certificate leaf = H\"{}\"", Hex(sha1)));
            }
            (None, None) => {
                return Err("there is no signer to require".into());
            }
        }
        Ok(parts.join(" and "))
    }

    /// Whether `peer`, as the OS reported it, is what this end requires.
    /// The macOS check also runs [`Self::requirement`] through the Security
    /// framework; this comparison is the same rule over what it reported.
    pub fn check(&self, peer: &CodeSignature) -> Result<(), String> {
        match peer.kind {
            SignatureKind::Unsigned => {
                return Err(format!(
                    "the other program isn't signed; it must be signed by {}",
                    self.signer
                ));
            }
            SignatureKind::AdHoc => {
                return Err(format!(
                    "the other program has only an ad hoc signature; it must be signed by {}",
                    self.signer
                ));
            }
            SignatureKind::Signed => {}
        }
        if !peer.valid {
            return Err("the system doesn't accept the other program's signature".into());
        }
        if let Some(team) = &self.team {
            if peer.team.as_deref() != Some(team.as_str()) {
                return Err(format!(
                    "the other program is signed by another developer ({}), not {}",
                    peer.team.as_deref().unwrap_or("no team"),
                    self.signer
                ));
            }
        } else if let Some(leaf) = &self.leaf_sha256 {
            if peer.leaf_sha256.as_ref() != Some(leaf) {
                return Err(format!(
                    "the other program is signed by {}, not {}",
                    peer.signer.as_deref().unwrap_or("another certificate"),
                    self.signer
                ));
            }
        } else {
            return Err("there is no signer to compare with".into());
        }
        if let Some(id) = &self.identifier
            && peer.identifier.as_deref() != Some(id.as_str())
        {
            return Err(format!(
                "the other program is signed as {}, not {id}",
                peer.identifier.as_deref().unwrap_or("nothing")
            ));
        }
        Ok(())
    }
}

/// Lowercase hex for messages and requirement strings, without an
/// allocation in the caller.
mod hex {
    use super::fmt;

    pub struct Hex<'a>(pub &'a [u8]);

    impl fmt::Display for Hex<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for b in self.0 {
                write!(f, "{b:02x}")?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(team: Option<&str>, der: &[u8], id: &str) -> CodeSignature {
        CodeSignature {
            kind: SignatureKind::Signed,
            valid: true,
            identifier: Some(id.into()),
            team: team.map(str::to_string),
            leaf_sha256: None,
            leaf_sha1: None,
            signer: Some(format!("signer of {}", der.len())),
        }
        .with_leaf(der)
    }

    #[test]
    fn an_unsigned_or_ad_hoc_build_checks_by_path_and_hash_and_says_so() {
        let p = Policy::from_own(&CodeSignature::unsigned(), Some("lockdown-native-host"));
        assert_eq!(p.kind(), "path_and_hash");
        assert!(p.describe().starts_with("This is a development build"));
        assert!(p.describe().contains("path and SHA-256"));
        let adhoc = CodeSignature {
            kind: SignatureKind::AdHoc,
            valid: true,
            ..CodeSignature::unsigned()
        };
        assert!(matches!(
            Policy::from_own(&adhoc, None),
            Policy::ProcessOnly { reason } if reason.contains("ad hoc")
        ));
    }

    #[test]
    fn a_build_whose_own_signature_is_rejected_trusts_nobody() {
        let mut own = signed(None, b"cert", "design.snds.lockdown");
        own.valid = false;
        assert_eq!(Policy::from_own(&own, None).kind(), "refused");
        let mut no_leaf = signed(None, b"cert", "x");
        no_leaf.leaf_sha256 = None;
        assert_eq!(Policy::from_own(&no_leaf, None).kind(), "refused");
        let quote = signed(Some("TEAM\" or true"), b"cert", "x");
        assert_eq!(Policy::from_own(&quote, None).kind(), "refused");
        assert_eq!(
            Policy::from_own(&signed(None, b"c", "x"), Some("a\"b")).kind(),
            "refused"
        );
    }

    #[test]
    fn a_team_signed_build_requires_the_same_team_and_the_peer_identifier() {
        let own = signed(Some("ABCDE12345"), b"leaf-a", "design.snds.lockdown");
        let Policy::Signature(e) = Policy::from_own(&own, Some("lockdown-native-host")) else {
            panic!("a team-signed build checks signatures");
        };
        assert_eq!(
            e.requirement().unwrap(),
            "identifier \"lockdown-native-host\" and anchor apple generic and certificate leaf[subject.OU] = \"ABCDE12345\""
        );
        // The same team with another certificate (a renewal) is the same developer.
        assert!(
            e.check(&signed(
                Some("ABCDE12345"),
                b"leaf-b",
                "lockdown-native-host"
            ))
            .is_ok()
        );
        let other_team = e
            .check(&signed(
                Some("ZZZZZ99999"),
                b"leaf-a",
                "lockdown-native-host",
            ))
            .unwrap_err();
        assert!(
            other_team.contains("another developer (ZZZZZ99999)"),
            "{other_team}"
        );
        let other_id = e
            .check(&signed(Some("ABCDE12345"), b"leaf-a", "evil"))
            .unwrap_err();
        assert!(other_id.contains("signed as evil"), "{other_id}");
    }

    #[test]
    fn a_certificate_signed_build_requires_that_exact_certificate() {
        let own = signed(None, b"throwaway-a", "design.snds.lockdown");
        let Policy::Signature(e) = Policy::from_own(&own, None) else {
            panic!("a signed build checks signatures");
        };
        let sha1 = own.leaf_sha1.unwrap();
        assert_eq!(
            e.requirement().unwrap(),
            format!("certificate leaf = H\"{}\"", Hex(&sha1))
        );
        assert!(e.check(&signed(None, b"throwaway-a", "anything")).is_ok());
        assert!(
            e.check(&signed(None, b"throwaway-b", "anything"))
                .unwrap_err()
                .contains("not signer of 11")
        );
        // A peer the OS rejected, or one with no signer, is refused.
        let mut tampered = signed(None, b"throwaway-a", "x");
        tampered.valid = false;
        assert!(e.check(&tampered).unwrap_err().contains("doesn't accept"));
        assert!(
            e.check(&CodeSignature::unsigned())
                .unwrap_err()
                .contains("isn't signed")
        );
        let adhoc = CodeSignature {
            kind: SignatureKind::AdHoc,
            valid: true,
            ..CodeSignature::unsigned()
        };
        assert!(e.check(&adhoc).unwrap_err().contains("ad hoc"));
    }

    #[test]
    fn the_settings_line_names_the_signer() {
        let own = signed(Some("ABCDE12345"), b"leaf", "design.snds.lockdown");
        let p = Policy::from_own(&own, Some("lockdown-native-host"));
        assert_eq!(p.kind(), "signature");
        assert!(p.describe().contains("signer of 4"), "{}", p.describe());
    }
}
