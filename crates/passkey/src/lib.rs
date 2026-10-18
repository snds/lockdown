//! Lockdown's WebAuthn authenticator (docs/plan/19-passkeys.md): the pure
//! half of making a passkey and signing in with one, with nothing about
//! where passkeys are stored or who approves. `lockdown-app` stores the
//! credential on a Login and runs the gates; this crate makes the bytes.
//!
//! - [`rp`]: the caller and its RP ID, checked against the browser's own
//!   record of the page (WebAuthn L3 §5.1.3 step 8, §5.11).
//! - [`key`]: the credential key (ES256 made here; ES256 and Ed25519 read
//!   from PKCS#8), the COSE and SPKI public keys, signing.
//! - [`client_data`]: `clientDataJSON` in the spec's own serialisation
//!   (§5.8.1.1), built here from the browser-reported origin, so a page
//!   cannot put another origin into it.
//! - [`verify`]: the relying party's checks, for security keys and
//!   passkeys as a second factor on the Lockdown account itself.
//! - [`make_credential`] / [`get_assertion`]: the authenticator data, the
//!   `none` attestation object and the assertion signature.
//!
//! The rules this authenticator keeps (D-rows in the doc): the sign count
//! is always zero (02-model §4: no signature counter, so a passkey moves
//! under CXF and syncs between devices without a counter to fork); every
//! credential is discoverable; the attestation is `none` with Lockdown's
//! AAGUID in the attested data; backup eligibility and state are both set,
//! because the passkey syncs with the vault.

#![forbid(unsafe_code)]

pub mod client_data;
pub mod encode;
pub mod key;
pub mod rp;
pub mod verify;

use ring::digest::{SHA256, digest};
use serde::Serialize;

pub use client_data::{Ceremony, ClientData};
pub use key::{Alg, COSE_EDDSA, COSE_ES256, CredentialKey};
pub use rp::{Caller, rp_id_for};

use encode::Cbor;

/// Lockdown's authenticator model id (AAGUID), the same for every build:
/// `07017dfd-2717-4c97-92c2-b3d6513f46ce`. It names the provider, never a
/// device or a person.
pub const LOCKDOWN_AAGUID: [u8; 16] = [
    0x07, 0x01, 0x7d, 0xfd, 0x27, 0x17, 0x4c, 0x97, 0x92, 0xc2, 0xb3, 0xd6, 0x51, 0x3f, 0x46, 0xce,
];

/// The length of the credential ids Lockdown makes.
pub const CREDENTIAL_ID_LEN: usize = 16;

/// The most bytes a user handle may have (WebAuthn §5.4.3).
pub const MAX_USER_HANDLE: usize = 64;

/// The DOMException a refusal becomes in the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RefusalName {
    /// The person declined, the check failed, or nothing may be said.
    #[serde(rename = "NotAllowedError")]
    NotAllowed,
    /// The authenticator already holds an excluded credential.
    #[serde(rename = "InvalidStateError")]
    InvalidState,
    /// The page may not use this RP ID.
    #[serde(rename = "SecurityError")]
    Security,
    /// No algorithm or key this authenticator supports.
    #[serde(rename = "NotSupportedError")]
    NotSupported,
    /// The request itself is malformed.
    #[serde(rename = "TypeError")]
    Type,
}

impl RefusalName {
    /// The DOMException name.
    pub fn dom_name(self) -> &'static str {
        match self {
            Self::NotAllowed => "NotAllowedError",
            Self::InvalidState => "InvalidStateError",
            Self::Security => "SecurityError",
            Self::NotSupported => "NotSupportedError",
            Self::Type => "TypeError",
        }
    }
}

/// Why a ceremony did not happen, as the page learns it, with words for
/// the person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Refusal {
    /// The DOMException name.
    pub name: RefusalName,
    /// Plain words.
    pub message: String,
}

impl Refusal {
    /// A refusal.
    pub fn new(name: RefusalName, message: impl Into<String>) -> Self {
        Self {
            name,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name.dom_name(), self.message)
    }
}

impl std::error::Error for Refusal {}

/// The authenticator data flags (§6.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Flags {
    /// User present: a deliberate action in Lockdown's own frame.
    pub up: bool,
    /// User verified: the platform check or the account password.
    pub uv: bool,
    /// Backup eligible: the credential may sync.
    pub be: bool,
    /// Backed up: it does sync.
    pub bs: bool,
}

impl Flags {
    const UP: u8 = 0x01;
    const UV: u8 = 0x04;
    const BE: u8 = 0x08;
    const BS: u8 = 0x10;
    const AT: u8 = 0x40;

    fn byte(self, attested: bool) -> u8 {
        let mut b = 0;
        if self.up {
            b |= Self::UP;
        }
        if self.uv {
            b |= Self::UV;
        }
        if self.be {
            b |= Self::BE;
        }
        if self.bs {
            b |= Self::BS;
        }
        if attested {
            b |= Self::AT;
        }
        b
    }

    /// Reads the flags back from an authenticator data's flag byte.
    pub fn from_byte(b: u8) -> Self {
        Self {
            up: b & Self::UP != 0,
            uv: b & Self::UV != 0,
            be: b & Self::BE != 0,
            bs: b & Self::BS != 0,
        }
    }
}

/// SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let d = digest(&SHA256, bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

/// The authenticator data: `rpIdHash ‖ flags ‖ signCount (0) ‖
/// [aaguid ‖ credentialIdLength ‖ credentialId ‖ COSE key]`.
pub fn authenticator_data(rp_id: &str, flags: Flags, attested: Option<(&[u8], &[u8])>) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(37 + attested.map_or(0, |(id, key)| 18 + id.len() + key.len()));
    out.extend_from_slice(&sha256(rp_id.as_bytes()));
    out.push(flags.byte(attested.is_some()));
    // The sign count: always zero (no counter; see the module doc).
    out.extend_from_slice(&0u32.to_be_bytes());
    if let Some((credential_id, cose_key)) = attested {
        out.extend_from_slice(&LOCKDOWN_AAGUID);
        out.extend_from_slice(&(credential_id.len() as u16).to_be_bytes());
        out.extend_from_slice(credential_id);
        out.extend_from_slice(cose_key);
    }
    out
}

/// The `none` attestation object: `{"fmt": "none", "attStmt": {},
/// "authData": …}` in CTAP2 canonical order.
pub fn attestation_object(auth_data: &[u8]) -> Vec<u8> {
    let mut c = Cbor::new();
    c.map(3)
        .text("fmt")
        .text("none")
        .text("attStmt")
        .map(0)
        .text("authData")
        .bytes(auth_data);
    c.finish()
}

/// Which algorithm to make for a request's `pubKeyCredParams` (in the RP's
/// order of preference): ES256 when the RP accepts it. An empty list means
/// the spec's default (ES256 and RS256).
pub fn choose_alg(params: &[i64]) -> Result<Alg, Refusal> {
    if params.is_empty() || params.contains(&COSE_ES256) {
        return Ok(Alg::Es256);
    }
    Err(Refusal::new(
        RefusalName::NotSupported,
        "this site asks for a kind of key Lockdown doesn't make (Lockdown makes ES256 passkeys)",
    ))
}

/// What making a credential needs, all already checked by the caller.
pub struct MakeCredential<'a> {
    pub rp_id: &'a str,
    pub client_data: &'a ClientData,
    pub flags: Flags,
    /// The `pubKeyCredParams` algorithms in order.
    pub algs: &'a [i64],
}

/// A credential just made. `pkcs8` is the private key for the vault (wiped
/// on drop); everything else is public and goes to the page.
pub struct Made {
    pub credential_id: Vec<u8>,
    pub pkcs8: zeroize::Zeroizing<Vec<u8>>,
    pub alg: Alg,
    pub client_data_json: String,
    pub authenticator_data: Vec<u8>,
    pub attestation_object: Vec<u8>,
    /// SubjectPublicKeyInfo DER, for `getPublicKey()`.
    pub public_key: Vec<u8>,
}

impl std::fmt::Debug for Made {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Made({} bytes of credential id)",
            self.credential_id.len()
        )
    }
}

/// Makes a new discoverable credential: a fresh ES256 key and a random
/// credential id, the attested authenticator data and the `none`
/// attestation object.
pub fn make_credential(req: &MakeCredential<'_>) -> Result<Made, Refusal> {
    choose_alg(req.algs)?;
    let (key, pkcs8) = CredentialKey::generate()?;
    let rng = ring::rand::SystemRandom::new();
    let mut credential_id = vec![0u8; CREDENTIAL_ID_LEN];
    ring::rand::SecureRandom::fill(&rng, &mut credential_id)
        .map_err(|_| Refusal::new(RefusalName::NotAllowed, "the system's random source failed"))?;
    let cose = key.cose_public_key();
    let auth = authenticator_data(req.rp_id, req.flags, Some((&credential_id, &cose)));
    Ok(Made {
        attestation_object: attestation_object(&auth),
        authenticator_data: auth,
        credential_id,
        pkcs8,
        alg: key.alg(),
        client_data_json: req.client_data.to_json(),
        public_key: key.spki(),
    })
}

/// An assertion: what `navigator.credentials.get` hands back.
#[derive(Clone, Debug)]
pub struct Assertion {
    pub client_data_json: String,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Signs in with a stored key (its PKCS#8 bytes, read from the vault and
/// wiped by the caller): the authenticator data without attested data, and
/// the signature over `authData ‖ SHA-256(clientDataJSON)`.
pub fn get_assertion(
    pkcs8: &[u8],
    rp_id: &str,
    client_data: &ClientData,
    flags: Flags,
) -> Result<Assertion, Refusal> {
    let key = CredentialKey::from_pkcs8(pkcs8)?;
    let client_data_json = client_data.to_json();
    let authenticator_data = authenticator_data(rp_id, flags, None);
    let mut message = authenticator_data.clone();
    message.extend_from_slice(&sha256(client_data_json.as_bytes()));
    let signature = key.sign(&message)?;
    Ok(Assertion {
        client_data_json,
        authenticator_data,
        signature,
    })
}

/// The public key of a stored credential as SPKI DER and its algorithm
/// (for the management view and tests); no secret leaves.
pub fn public_key_of(pkcs8: &[u8]) -> Result<(Alg, Vec<u8>), Refusal> {
    let key = CredentialKey::from_pkcs8(pkcs8)?;
    Ok((key.alg(), key.spki()))
}

/// An AAGUID as the usual dashed lowercase hex.
pub fn aaguid_text(a: &[u8; 16]) -> String {
    let h = data_encoding::HEXLOWER.encode(a);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// The passkey providers this build can name from an AAGUID, from the
/// community list of passkey provider AAGUIDs; anything else shows as its
/// hex.
pub const KNOWN_PROVIDERS: &[(&str, &str)] = &[
    ("07017dfd-2717-4c97-92c2-b3d6513f46ce", "Lockdown"),
    (
        "ea9b8d66-4d01-1d21-3ce4-b6b48cb575d4",
        "Google Password Manager",
    ),
    ("fbfc3007-154e-4ecc-8c0b-6e020557d7bd", "iCloud Keychain"),
    ("d548826e-79b4-db40-a3d8-11116f7e8349", "Bitwarden"),
    ("bada5566-a7aa-401f-bd96-45619a55120d", "1Password"),
    ("50726f74-6f6e-5061-7373-50726f746f6e", "Proton Pass"),
];

/// The provider an AAGUID names, if this build knows it.
pub fn provider_name(aaguid: &str) -> Option<&'static str> {
    let a = aaguid.to_ascii_lowercase();
    KNOWN_PROVIDERS
        .iter()
        .find(|(id, _)| *id == a)
        .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};

    fn client(ceremony: Ceremony) -> ClientData {
        ClientData {
            ceremony,
            challenge: vec![1, 2, 3, 4],
            origin: "https://example.com".into(),
            top_origin: None,
        }
    }

    /// The relying party's side, as a server library does it: parse the
    /// attestation object, find the COSE key, check the hash and flags.
    fn rp_reads(attestation: &[u8]) -> (Vec<u8>, Vec<u8>, Flags) {
        let v: ciborium::Value = ciborium::from_reader(attestation).unwrap();
        let map = v.as_map().unwrap();
        let keys: Vec<&str> = map.iter().map(|(k, _)| k.as_text().unwrap()).collect();
        assert_eq!(keys, ["fmt", "attStmt", "authData"]);
        assert_eq!(map[0].1.as_text(), Some("none"));
        assert_eq!(map[1].1.as_map().map(Vec::len), Some(0));
        let auth = map[2].1.as_bytes().unwrap().clone();
        assert_eq!(&auth[..32], &sha256(b"example.com"));
        let flags = Flags::from_byte(auth[32]);
        assert_eq!(auth[32] & 0x40, 0x40, "attested data present");
        assert_eq!(&auth[33..37], &[0, 0, 0, 0], "sign count zero");
        assert_eq!(&auth[37..53], &LOCKDOWN_AAGUID);
        let len = u16::from_be_bytes([auth[53], auth[54]]) as usize;
        let cred = auth[55..55 + len].to_vec();
        let cose: ciborium::Value = ciborium::from_reader(&auth[55 + len..]).unwrap();
        let m = cose.as_map().unwrap();
        let coord = |k: i64| {
            m.iter()
                .find(|(a, _)| a.as_integer() == Some(k.into()))
                .and_then(|(_, b)| b.as_bytes().cloned())
                .unwrap()
        };
        let mut point = vec![4u8];
        point.extend(coord(-2));
        point.extend(coord(-3));
        (cred, point, flags)
    }

    #[test]
    fn a_made_credential_reads_back_and_its_assertions_verify() {
        let cd = client(Ceremony::Create);
        let flags = Flags {
            up: true,
            uv: false,
            be: true,
            bs: true,
        };
        let made = make_credential(&MakeCredential {
            rp_id: "example.com",
            client_data: &cd,
            flags,
            algs: &[-8, -7, -257],
        })
        .unwrap();
        assert_eq!(
            made.alg,
            Alg::Es256,
            "ES256 even when the RP lists EdDSA first"
        );
        assert_eq!(made.credential_id.len(), CREDENTIAL_ID_LEN);
        let (cred, point, got) = rp_reads(&made.attestation_object);
        assert_eq!(cred, made.credential_id);
        assert_eq!(got, flags);
        assert_eq!(&made.public_key[26..], point.as_slice());
        assert!(
            made.client_data_json
                .starts_with(r#"{"type":"webauthn.create""#)
        );

        let get = client(Ceremony::Get);
        let a = get_assertion(
            &made.pkcs8,
            "example.com",
            &get,
            Flags { uv: true, ..flags },
        )
        .unwrap();
        assert_eq!(a.authenticator_data.len(), 37, "no attested data on a get");
        assert_eq!(a.authenticator_data[32], 0x01 | 0x04 | 0x08 | 0x10);
        assert_eq!(&a.authenticator_data[33..], &[0, 0, 0, 0]);
        let mut signed = a.authenticator_data.clone();
        signed.extend(sha256(a.client_data_json.as_bytes()));
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &point)
            .verify(&signed, &a.signature)
            .expect("the RP verifies the assertion with the attested key");
        // Signed for another RP ID, the hash differs and the RP refuses it.
        let other = get_assertion(&made.pkcs8, "evil.example", &get, flags).unwrap();
        assert_ne!(&other.authenticator_data[..32], &sha256(b"example.com"));
    }

    #[test]
    fn only_es256_is_made() {
        assert_eq!(choose_alg(&[]).unwrap(), Alg::Es256);
        assert_eq!(choose_alg(&[-257, -7]).unwrap(), Alg::Es256);
        assert_eq!(
            choose_alg(&[-8, -257]).unwrap_err().name,
            RefusalName::NotSupported
        );
    }

    #[test]
    fn providers_are_named_from_their_aaguid() {
        assert_eq!(
            aaguid_text(&LOCKDOWN_AAGUID),
            "07017dfd-2717-4c97-92c2-b3d6513f46ce"
        );
        assert_eq!(
            provider_name(&aaguid_text(&LOCKDOWN_AAGUID)),
            Some("Lockdown")
        );
        assert_eq!(
            provider_name("EA9B8D66-4D01-1D21-3CE4-B6B48CB575D4"),
            Some("Google Password Manager")
        );
        assert_eq!(provider_name("00000000-0000-0000-0000-000000000000"), None);
    }
}
