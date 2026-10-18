//! Approving a new device from one already enrolled (`device-approval`,
//! `secret-key`; 01-identity §8 row 2 "existing device shows a QR …; both
//! show a 6-digit short authentication string; existing device wraps the
//! IRK to it"; 04-security §8 "Another device"; docs/plan/22-security-depth.md §2).
//!
//! The two devices meet through the server, which relays opaque frames and
//! can read none of what matters: the protocol is the identity crate's
//! local handshake (wire v1, D31) unchanged, carried as relay frames, plus
//! one Lockdown frame after the grant.
//!
//! ```text
//! existing device (Offer)                   relay        new device (Join)
//! Hello{profile, device, kem_pub, commit} ───────────▶   scans the QR or types the code
//!                                          ◀───────────  Request{device record, nonce, proof of possession}
//! Reveal{nonce}                            ───────────▶   both show the six digits
//! the person compares and approves (the `Approve` gate)
//! Granted{root key wrapped to the new device, head}  ──▶
//! Welcome{records, Secret Key sealed to it, ISK-signed}─▶ restores the identity, then signs in
//! ```
//!
//! What each side checks:
//!
//! - **Commit, then reveal.** The existing device commits to its nonce
//!   before it sees the new device's, so neither side (and no relay) can
//!   steer the six digits; a relay that runs two handshakes shows two
//!   different codes, which the person sees.
//! - **The QR pins the existing device.** It carries a fingerprint of the
//!   existing device's receiving key; the new device refuses a Hello whose
//!   key doesn't match. A typed code has no fingerprint, so the six digits
//!   carry the whole check.
//! - **Proof of possession.** The new device signs the transcript with the
//!   key its record names, and its receiving key must be hybrid
//!   (`pq-key-wrapping`) before the root key is wrapped to it.
//! - **The root key proves the identity.** The new device accepts the
//!   records only when the root key it was handed derives the profile's
//!   signing key, every record verifies under it, the approving device is
//!   one of the identity's own, and its own record came back as proposed.
//! - **The Secret Key** (when the existing device holds it) rides in the
//!   Welcome, sealed to the new device's receiving key with X-Wing and
//!   signed with the Identity Signing Key; the new device checks it once
//!   more against the account slot when it signs in.

use ecosystem_identity::ed25519_dalek::Signer as _;
use ecosystem_identity::handshake::wire::{
    Grant, HANDSHAKE_V, Hello, Message, Refusal, RefusalReason, Request, Reveal, commitment,
    proof_of_possession_message, short_authentication_string,
};
use ecosystem_identity::kem::KemPublicKey;
use ecosystem_identity::keys::{Irk, SymmetricKey};
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::{
    Device, EnrolledBy, KeyEnvelope, KeyRef, KeyRefKind, Purpose, RECORD_V,
    Timestamp as IdTimestamp,
};
use ecosystem_identity::{Backup, Head, Identity, Record};
use lockdown_crypto::Key32;
use lockdown_crypto::aead::{self, Sealed};
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::{derive_key, hash};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::{Error, Id, Result};

/// The version of Lockdown's frames around the handshake.
pub const LINK_V: u16 = 1;
/// Characters in a link code.
pub const CODE_LEN: usize = 10;
/// How long a link stays open on the server.
pub const LINK_TTL_SECS: i64 = 10 * 60;
/// The QR payload's prefix.
pub const QR_PREFIX: &str = "lockdown:link:v1?";

/// Crockford base32: no I, L, O or U, so a typed code is hard to misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The derive-key context of a code's hash, which is all the server holds.
const CODE_CONTEXT: &str = "lockdown 2026-10-18 device link code";
/// The derive-key context of the existing device's fingerprint in the QR.
const FINGERPRINT_CONTEXT: &str = "lockdown 2026-10-18 device link fingerprint";
/// Domain separation for the Welcome's signature.
const WELCOME_SIG_PREFIX: &[u8] = b"lockdown-link-welcome-v1\0";
/// The HKDF label of the Secret Key's seal.
const SECRET_KEY_INFO: &[u8] = b"lockdown/link/secret-key/v1";

// --- the code and the QR --------------------------------------------------------

/// The ten characters a person types on the new device, or that the QR
/// carries. 50 random bits; the server only ever sees their hash, and a
/// link takes one join within ten minutes.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkCode(String);

impl std::fmt::Debug for LinkCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkCode(…)")
    }
}

impl LinkCode {
    /// A fresh random code.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; CODE_LEN];
        lockdown_crypto::rand::random_bytes(&mut bytes)?;
        Ok(Self(
            bytes
                .iter()
                .map(|b| ALPHABET[usize::from(b & 31)] as char)
                .collect(),
        ))
    }

    /// The code from what a person typed: case, spaces and dashes ignored,
    /// `O` read as zero and `I`/`L` as one, as Crockford's base32 does.
    pub fn parse(text: &str) -> Result<Self> {
        let mut out = String::with_capacity(CODE_LEN);
        for c in text.chars() {
            let c = c.to_ascii_uppercase();
            let c = match c {
                ' ' | '-' | '\u{a0}' => continue,
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            };
            if !ALPHABET.contains(&(c as u8)) || !c.is_ascii() {
                return Err(Error::Invalid(format!(
                    "'{c}' can't be in a device code; check it against the other screen"
                )));
            }
            out.push(c);
        }
        if out.len() != CODE_LEN {
            return Err(Error::Invalid(format!(
                "a device code has {CODE_LEN} characters; this has {}",
                out.len()
            )));
        }
        Ok(Self(out))
    }

    /// As shown: two groups of five.
    pub fn display(&self) -> String {
        format!("{}-{}", &self.0[..5], &self.0[5..])
    }

    /// The hash the server files the link under.
    pub fn token_hash(&self) -> [u8; 32] {
        derive_key(CODE_CONTEXT, self.0.as_bytes())
    }

    /// The bare ten characters.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The short fingerprint of a receiving key the QR carries.
pub fn key_fingerprint(key: &KemPublicKey) -> [u8; 16] {
    let full = derive_key(FINGERPRINT_CONTEXT, &key.bytes);
    full[..16].try_into().expect("sixteen of thirty-two")
}

/// What the new device needs to find the existing one: the server, the
/// code, and (from a QR) the existing device's fingerprint.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LinkInvite {
    /// The server both devices use, when the QR named it.
    pub server: Option<String>,
    /// The code.
    pub code: LinkCode,
    /// The existing device's receiving-key fingerprint, from a QR.
    pub fingerprint: Option<[u8; 16]>,
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| Error::Invalid("the QR code's address is malformed".into()))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| Error::Invalid("the QR code's address is malformed".into()))
}

impl LinkInvite {
    /// The QR payload: `lockdown:link:v1?s=<server>&c=<code>&f=<fingerprint>`.
    pub fn to_qr(&self) -> String {
        let mut parts = Vec::new();
        if let Some(s) = &self.server {
            parts.push(format!("s={}", percent_encode(s)));
        }
        parts.push(format!("c={}", self.code.as_str()));
        if let Some(f) = &self.fingerprint {
            parts.push(format!("f={}", data_encoding::HEXLOWER.encode(f)));
        }
        format!("{QR_PREFIX}{}", parts.join("&"))
    }

    /// A scanned QR payload or a typed code. Anything else is refused.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let Some(query) = text.strip_prefix(QR_PREFIX) else {
            if text.starts_with("lockdown:") {
                return Err(Error::Invalid(
                    "that QR code is from Lockdown but isn't a device code (it may be a contact's fingerprint)".into(),
                ));
            }
            return Ok(Self {
                server: None,
                code: LinkCode::parse(text)?,
                fingerprint: None,
            });
        };
        let (mut server, mut code, mut fingerprint) = (None, None, None);
        for pair in query.split('&') {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| Error::Invalid("the QR code is malformed".into()))?;
            match k {
                "s" => {
                    let s = percent_decode(v)?;
                    if !(s.starts_with("https://") || s.starts_with("http://")) {
                        return Err(Error::Invalid(
                            "the QR code names a server that isn't a web address".into(),
                        ));
                    }
                    server = Some(s);
                }
                "c" => code = Some(LinkCode::parse(v)?),
                "f" => {
                    let bytes = data_encoding::HEXLOWER_PERMISSIVE
                        .decode(v.as_bytes())
                        .ok()
                        .and_then(|b| <[u8; 16]>::try_from(b.as_slice()).ok())
                        .ok_or_else(|| {
                            Error::Invalid("the QR code's fingerprint is malformed".into())
                        })?;
                    fingerprint = Some(bytes);
                }
                // Fields a later version adds are read past (D62's rule).
                _ => {}
            }
        }
        Ok(Self {
            server,
            code: code.ok_or_else(|| Error::Invalid("the QR code has no device code".into()))?,
            fingerprint,
        })
    }
}

// --- frames ----------------------------------------------------------------------

/// What the Welcome says about the Secret Key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SecretKeyTransfer {
    /// Sealed to the new device's receiving key.
    Sealed {
        /// The X-Wing encapsulation.
        enc: ByteBuf,
        /// The Secret Key under the key the encapsulation carries.
        sealed: Sealed,
    },
    /// The account was made without one (the documented opt-out).
    OptedOut,
    /// The existing device doesn't keep it (made before it was kept, or
    /// never typed there); the new device asks for it from the kit when it
    /// needs it.
    NotHeld,
}

/// The Lockdown frame after the grant: the identity's records, and the
/// Secret Key, signed with the Identity Signing Key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Welcome {
    /// Format version.
    pub v: u16,
    /// The identity's records as the existing device holds them, the new
    /// device's own record among them ([`Backup`] bytes).
    pub backup: ByteBuf,
    /// The Secret Key, or why not.
    pub secret_key: SecretKeyTransfer,
    /// Ed25519 by the Identity Signing Key over the prefix and the Welcome
    /// with `sig` empty.
    pub sig: ByteBuf,
}

impl Welcome {
    fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig = ByteBuf::new();
        let mut out = WELCOME_SIG_PREFIX.to_vec();
        out.extend(to_canonical(&copy)?);
        Ok(out)
    }
}

/// One frame on the relay.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LinkFrame {
    /// A message of the identity crate's handshake, as its bytes.
    Handshake {
        /// Frame format version.
        v: u16,
        /// `Message::to_bytes`.
        message: ByteBuf,
    },
    /// Lockdown's frame after the grant.
    Welcome(Welcome),
}

impl LinkFrame {
    fn handshake(message: &Message) -> Result<Vec<u8>> {
        Ok(to_canonical(&LinkFrame::Handshake {
            v: LINK_V,
            message: ByteBuf::from(message.to_bytes()?),
        })?)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let frame: LinkFrame = lockdown_crypto::cbor::from_slice(bytes)
            .map_err(|_| Error::Invalid("the other device sent something unreadable".into()))?;
        match &frame {
            LinkFrame::Handshake { v, .. } if *v > LINK_V => Err(Error::SchemaTooNew {
                found: *v,
                supported: LINK_V,
            }),
            LinkFrame::Welcome(w) if w.v > LINK_V => Err(Error::SchemaTooNew {
                found: w.v,
                supported: LINK_V,
            }),
            _ => Ok(frame),
        }
    }

    fn message(bytes: &[u8]) -> Result<Message> {
        match Self::decode(bytes)? {
            LinkFrame::Handshake { message, .. } => Ok(Message::from_bytes(&message)?),
            LinkFrame::Welcome(_) => Err(Error::Invalid(
                "the other device sent its welcome before its answer".into(),
            )),
        }
    }
}

fn refusal(reason: RefusalReason, detail: &str) -> Result<Vec<u8>> {
    LinkFrame::handshake(&Message::Refused(Refusal {
        v: HANDSHAKE_V,
        reason,
        detail: detail.to_string(),
    }))
}

/// The six digits as a person reads them: `123 456`.
pub fn sas_words(sas: u32) -> String {
    format!("{:03} {:03}", sas / 1000, sas % 1000)
}

fn secret_key_aad(profile_id: Id, device_id: Id) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct Place {
        v: u16,
        profile_id: Id,
        device_id: Id,
        purpose: &'static str,
    }
    Ok(to_canonical(&Place {
        v: LINK_V,
        profile_id,
        device_id,
        purpose: "secret-key",
    })?)
}

fn secret_key_seal_key(shared: &[u8; 32], enc: &[u8]) -> Result<Key32> {
    let mut ikm = Vec::with_capacity(32 + enc.len());
    ikm.extend_from_slice(shared);
    ikm.extend_from_slice(enc);
    let key = lockdown_crypto::kdf::subkey(&ikm, SECRET_KEY_INFO)?;
    Ok(Key32::from_bytes(*key))
}

// --- the existing device ---------------------------------------------------------

/// The Secret Key as the existing device can hand it on.
#[derive(Clone, PartialEq, Eq)]
pub enum SecretKeyState {
    /// It keeps the Secret Key (sealed on the device).
    Held([u8; 16]),
    /// The account has none.
    OptedOut,
    /// It doesn't keep it.
    NotHeld,
}

impl std::fmt::Debug for SecretKeyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Held(_) => "Held(…)",
            Self::OptedOut => "OptedOut",
            Self::NotHeld => "NotHeld",
        })
    }
}

/// A request the person is now asked about.
#[derive(Clone, PartialEq, Debug)]
pub struct Asked {
    /// The six digits both screens show.
    pub sas: u32,
    /// What the new device calls itself.
    pub device_name: String,
    /// Its platform, as the identity names it.
    pub platform: String,
    /// The frame to send now (the reveal).
    pub reveal: Vec<u8>,
}

/// The existing device's side of one link.
pub struct Offer {
    code: LinkCode,
    profile_id: Id,
    own: Id,
    own_kem: KemPublicKey,
    nonce: [u8; 32],
    request: Option<(Request, u32)>,
}

impl std::fmt::Debug for Offer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Offer")
            .field("asked", &self.request.is_some())
            .finish_non_exhaustive()
    }
}

/// What a grant leaves the existing device with.
#[derive(Debug)]
pub struct Granted {
    /// The frames to send, in order: the crate's `Granted`, then the Welcome.
    pub frames: Vec<Vec<u8>>,
    /// The new device's record as signed.
    pub device: Device,
    /// The identity head after it was added.
    pub head: Head,
}

impl Offer {
    /// A new link for `identity`: a fresh code, the commitment to this
    /// device's nonce, and the Hello frame to post first.
    pub fn new(identity: &Identity) -> Result<(Self, Vec<u8>)> {
        let own = identity
            .devices()
            .find(|d| d.device_id == identity.device_id())
            .cloned()
            .ok_or_else(|| Error::NotFound("this device's own record".into()))?;
        let nonce = lockdown_crypto::rand::random_32()?;
        let offer = Self {
            code: LinkCode::generate()?,
            profile_id: identity.profile().profile_id,
            own: own.device_id,
            own_kem: own.kem_pub.clone(),
            nonce,
            request: None,
        };
        let hello = LinkFrame::handshake(&Message::Hello(Hello {
            v: HANDSHAKE_V,
            profile_id: offer.profile_id,
            device_id: own.device_id,
            kem_pub: own.kem_pub,
            commit: commitment(&nonce),
        }))?;
        Ok((offer, hello))
    }

    /// The code to show.
    pub fn code(&self) -> &LinkCode {
        &self.code
    }

    /// What the QR carries for `server`.
    pub fn invite(&self, server: Option<&str>) -> LinkInvite {
        LinkInvite {
            server: server.map(str::to_string),
            code: self.code.clone(),
            fingerprint: Some(key_fingerprint(&self.own_kem)),
        }
    }

    /// Whether a request is waiting for the person.
    pub fn asked(&self) -> Option<u32> {
        self.request.as_ref().map(|(_, sas)| *sas)
    }

    /// Takes the new device's request: checks it as the crate's responder
    /// does and answers with the reveal; `Err` carries the refusal frame to
    /// send and why. A second request on one link is refused.
    pub fn take_request(
        &mut self,
        frame: &[u8],
        identity: &Identity,
    ) -> std::result::Result<Asked, (String, Option<Vec<u8>>)> {
        let fail = |reason: RefusalReason, why: String| {
            let frame = refusal(reason, &why).ok();
            Err((why, frame))
        };
        if self.request.is_some() {
            return fail(
                RefusalReason::Replayed,
                "this link already has a device asking".into(),
            );
        }
        let request = match LinkFrame::message(frame) {
            Ok(Message::Request(r)) => r,
            Ok(Message::Refused(r)) => {
                return Err((format!("the new device stopped: {}", r.detail), None));
            }
            Ok(other) => {
                return fail(
                    RefusalReason::BadRequest,
                    format!("expected a request, got {}", other.name()),
                );
            }
            Err(e) => return fail(RefusalReason::BadRequest, e.to_string()),
        };
        if request.v != HANDSHAKE_V {
            return fail(
                RefusalReason::BadRequest,
                format!("handshake version {} isn't one this app speaks", request.v),
            );
        }
        let device = &request.device;
        if let Err(e) = crate::handover::check_proposed_device(device, self.own) {
            return fail(RefusalReason::BadRequest, e.to_string());
        }
        if identity.devices().any(|d| d.device_id == device.device_id) {
            return fail(
                RefusalReason::AlreadyEnrolled,
                "this device is already enrolled".into(),
            );
        }
        let pop_ok = (|| -> Option<()> {
            let key =
                ecosystem_identity::ed25519_dalek::VerifyingKey::from_bytes(&device.device_pub)
                    .ok()?;
            let sig =
                ecosystem_identity::ed25519_dalek::Signature::from_slice(&request.pop).ok()?;
            let message = proof_of_possession_message(
                self.profile_id,
                &commitment(&self.nonce),
                &request.nonce,
                device,
            )
            .ok()?;
            ecosystem_identity::ed25519_dalek::Verifier::verify(&key, &message, &sig).ok()
        })();
        if pop_ok.is_none() {
            return fail(
                RefusalReason::BadRequest,
                "the request isn't signed by the device key it names".into(),
            );
        }
        let sas = short_authentication_string(
            &request.nonce,
            &self.nonce,
            &device.kem_pub,
            &self.own_kem,
        );
        let reveal = match LinkFrame::handshake(&Message::Reveal(Reveal {
            v: HANDSHAKE_V,
            nonce: self.nonce,
        })) {
            Ok(f) => f,
            Err(e) => return Err((e.to_string(), None)),
        };
        let asked = Asked {
            sas,
            device_name: device.name.clone(),
            platform: format!("{:?}", device.platform).to_lowercase(),
            reveal,
        };
        self.request = Some((request, sas));
        Ok(asked)
    }

    /// The person approved: the new device's record is signed and stored,
    /// the root key wrapped to its receiving key, and the Welcome signed.
    /// The caller has run the `Approve` gate first.
    pub fn grant(
        self,
        identity: &mut Identity,
        keystore: &dyn DeviceKeystore,
        secret_key: &SecretKeyState,
    ) -> Result<Granted> {
        let (request, _) = self
            .request
            .ok_or_else(|| Error::Invalid("no device has asked on this link yet".into()))?;
        let profile_id = self.profile_id;
        let (irk, irk_id) = crate::handover::root_key(identity, keystore)?;
        let isk = ecosystem_identity::signing_key(&irk)?;
        // Wrap first, so a receiving key that can't be used writes nothing.
        let mut envelope = ecosystem_identity::keys::wrap_to_peer(
            irk.expose(),
            &request.device.kem_pub,
            KeyRef::new(KeyRefKind::Peer, request.device.device_id),
            &ecosystem_identity::keys::aad(profile_id, None, irk_id, Purpose::Irk),
        )?;
        envelope.sign(&isk)?;
        drop(irk);
        let (device, head) =
            crate::handover::add_linked_device(identity, keystore, &request.device)?;
        let secret = match secret_key {
            SecretKeyState::Held(sk) => {
                let (enc, shared) = lockdown_crypto::kem::encapsulate(&device.kem_pub)?;
                let key = secret_key_seal_key(shared.expose(), &enc)?;
                let sealed = aead::seal(&key, &secret_key_aad(profile_id, device.device_id)?, sk)?;
                SecretKeyTransfer::Sealed {
                    enc: ByteBuf::from(enc),
                    sealed,
                }
            }
            SecretKeyState::OptedOut => SecretKeyTransfer::OptedOut,
            SecretKeyState::NotHeld => SecretKeyTransfer::NotHeld,
        };
        let mut welcome = Welcome {
            v: LINK_V,
            backup: ByteBuf::from(identity.export_backup().to_bytes()?),
            secret_key: secret,
            sig: ByteBuf::new(),
        };
        welcome.sig = ByteBuf::from(isk.sign(&welcome.signable()?).to_bytes().to_vec());
        let granted = LinkFrame::handshake(&Message::Granted(Grant {
            v: HANDSHAKE_V,
            envelope,
            head,
        }))?;
        let welcome = to_canonical(&LinkFrame::Welcome(welcome))?;
        Ok(Granted {
            frames: vec![granted, welcome],
            device,
            head,
        })
    }

    /// The person declined: the frame to send. Nothing was written.
    pub fn decline(&self) -> Result<Vec<u8>> {
        refusal(
            RefusalReason::Declined,
            "the person on the other device declined",
        )
    }
}

// --- the new device --------------------------------------------------------------

/// What the new device has once both frames arrived and checked out.
pub struct HandedOver {
    /// The root key.
    pub irk: Irk,
    /// Its key id.
    pub irk_id: Id,
    /// The identity's records, every one verified under the root key's
    /// signing key.
    pub backup: Backup,
    /// The Secret Key, when it came.
    pub secret_key: Option<[u8; 16]>,
    /// What the Welcome said about it.
    pub secret_key_state: &'static str,
}

impl std::fmt::Debug for HandedOver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandedOver")
            .field("profile", &self.backup.profile.profile_id)
            .field("secret_key", &self.secret_key_state)
            .finish_non_exhaustive()
    }
}

/// Where the new device stands after a frame.
#[derive(Debug)]
pub enum JoinStep {
    /// Show these digits and wait for the other device.
    Compare(u32),
    /// The grant came; the Welcome is next.
    Granted,
    /// Everything arrived and checked out.
    Done(Box<HandedOver>),
}

/// The new device's side of one link.
pub struct Join {
    hello: Hello,
    nonce: [u8; 32],
    device: Device,
    sas: Option<u32>,
    envelope: Option<KeyEnvelope>,
}

impl std::fmt::Debug for Join {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Join")
            .field("sas_shown", &self.sas.is_some())
            .field("granted", &self.envelope.is_some())
            .finish_non_exhaustive()
    }
}

impl Join {
    /// Reads the existing device's Hello (checked against the QR's
    /// fingerprint when there is one), proposes this device under
    /// `device_name`, and returns the request frame to send.
    pub fn start(
        hello_frame: &[u8],
        keystore: &dyn DeviceKeystore,
        device_name: &str,
        fingerprint: Option<[u8; 16]>,
    ) -> Result<(Self, Vec<u8>)> {
        let hello = match LinkFrame::message(hello_frame)? {
            Message::Hello(h) => h,
            Message::Refused(r) => return Err(Error::Invalid(r.detail)),
            other => {
                return Err(Error::Invalid(format!(
                    "expected the other device's hello, got {}",
                    other.name()
                )));
            }
        };
        if hello.v != HANDSHAKE_V {
            return Err(Error::Invalid(format!(
                "the other device speaks handshake version {}, which this app doesn't",
                hello.v
            )));
        }
        if let Some(want) = fingerprint
            && key_fingerprint(&hello.kem_pub) != want
        {
            return Err(Error::Transparency(
                "the device answering isn't the one whose QR code you scanned; nothing was sent"
                    .into(),
            ));
        }
        let nonce = lockdown_crypto::rand::random_32()?;
        let device = Device {
            v: RECORD_V,
            device_id: keystore.device_id(),
            name: device_name.to_string(),
            platform: keystore.platform(),
            attestation: keystore.attestation(),
            device_pub: keystore.device_pub(),
            kem_pub: keystore.kem_pub(),
            enrolled_at: IdTimestamp::now(),
            enrolled_by: EnrolledBy::Device {
                id: hello.device_id,
            },
            revoked_at: None,
            extra: Default::default(),
            sig: Vec::new(),
        };
        let message =
            proof_of_possession_message(hello.profile_id, &hello.commit, &nonce, &device)?;
        let pop = keystore.sign(&message)?.to_vec();
        let frame = LinkFrame::handshake(&Message::Request(Request {
            v: HANDSHAKE_V,
            device: device.clone(),
            nonce,
            pop,
        }))?;
        Ok((
            Self {
                hello,
                nonce,
                device,
                sas: None,
                envelope: None,
            },
            frame,
        ))
    }

    /// The profile the other device says it belongs to.
    pub fn profile_id(&self) -> Id {
        self.hello.profile_id
    }

    /// The digits to show, once the reveal came.
    pub fn sas(&self) -> Option<u32> {
        self.sas
    }

    /// Takes the next frame from the other device.
    pub fn on_frame(&mut self, frame: &[u8], keystore: &dyn DeviceKeystore) -> Result<JoinStep> {
        match LinkFrame::decode(frame)? {
            LinkFrame::Handshake { message, .. } => match Message::from_bytes(&message)? {
                Message::Reveal(r) if self.sas.is_none() => {
                    if commitment(&r.nonce) != self.hello.commit {
                        return Err(Error::Invalid(
                            "the other device revealed a nonce it hadn't committed to".into(),
                        ));
                    }
                    let sas = short_authentication_string(
                        &self.nonce,
                        &r.nonce,
                        &self.device.kem_pub,
                        &self.hello.kem_pub,
                    );
                    self.sas = Some(sas);
                    Ok(JoinStep::Compare(sas))
                }
                Message::Granted(g) if self.sas.is_some() && self.envelope.is_none() => {
                    if g.envelope.purpose != Purpose::Irk
                        || g.envelope.wrapped_by
                            != KeyRef::new(KeyRefKind::Peer, self.device.device_id)
                    {
                        return Err(Error::Invalid(
                            "the other device sent a key that isn't the root key for this device"
                                .into(),
                        ));
                    }
                    self.envelope = Some(g.envelope);
                    Ok(JoinStep::Granted)
                }
                Message::Refused(r) => Err(match r.reason {
                    RefusalReason::Declined => {
                        Error::Invalid("the other device declined; nothing was changed".into())
                    }
                    _ => Error::Invalid(format!("the other device refused: {}", r.detail)),
                }),
                other => Err(Error::Invalid(format!(
                    "the other device sent a {} out of turn",
                    other.name()
                ))),
            },
            LinkFrame::Welcome(w) => {
                let envelope = self.envelope.take().ok_or_else(|| {
                    Error::Invalid("the other device sent its welcome before its grant".into())
                })?;
                Ok(JoinStep::Done(Box::new(
                    self.finish(envelope, w, keystore)?,
                )))
            }
        }
    }

    fn finish(
        &self,
        envelope: KeyEnvelope,
        welcome: Welcome,
        keystore: &dyn DeviceKeystore,
    ) -> Result<HandedOver> {
        let backup = Backup::from_bytes(&welcome.backup)?;
        let profile = &backup.profile;
        if profile.profile_id != self.hello.profile_id {
            return Err(Error::Invalid(
                "the records that came belong to another identity than the one that answered"
                    .into(),
            ));
        }
        let isk_pub = ecosystem_identity::ed25519_dalek::VerifyingKey::from_bytes(&profile.isk_pub)
            .map_err(|_| Error::Invalid("the profile's signing key is malformed".into()))?;
        envelope.verify(&isk_pub)?;
        let irk: Irk = ecosystem_identity::keys::unwrap_key_from_peer(
            &envelope,
            &keystore.kem_secret()?,
            &ecosystem_identity::keys::aad(profile.profile_id, None, envelope.key_id, Purpose::Irk),
        )?;
        if ecosystem_identity::signing_key(&irk)?.verifying_key() != isk_pub {
            return Err(Error::Invalid(
                "the root key handed over doesn't derive the profile's signing key".into(),
            ));
        }
        let sig = ecosystem_identity::ed25519_dalek::Signature::from_slice(&welcome.sig)
            .map_err(|_| Error::Invalid("the welcome's signature is malformed".into()))?;
        ecosystem_identity::ed25519_dalek::Verifier::verify(&isk_pub, &welcome.signable()?, &sig)
            .map_err(|_| {
            Error::Invalid("the welcome isn't signed by the identity's signing key".into())
        })?;
        let approver = backup
            .devices
            .iter()
            .find(|d| d.device_id == self.hello.device_id)
            .ok_or_else(|| {
                Error::Invalid(
                    "the device that approved isn't one of this identity's devices".into(),
                )
            })?;
        if approver.revoked_at.is_some() || approver.kem_pub != self.hello.kem_pub {
            return Err(Error::Invalid(
                "the device that approved isn't a current device of this identity".into(),
            ));
        }
        let mine = backup
            .devices
            .iter()
            .find(|d| d.device_id == self.device.device_id)
            .ok_or_else(|| Error::Invalid("the other device didn't record this one".into()))?;
        if mine.device_pub != self.device.device_pub
            || mine.kem_pub != self.device.kem_pub
            || mine.enrolled_by != self.device.enrolled_by
        {
            return Err(Error::Invalid(
                "the record the other device made for this one isn't the one it asked for".into(),
            ));
        }
        let (secret_key, secret_key_state) = match &welcome.secret_key {
            SecretKeyTransfer::Sealed { enc, sealed } => {
                let shared = lockdown_crypto::kem::decapsulate(&keystore.kem_secret()?, enc)?;
                let key = secret_key_seal_key(shared.expose(), enc)?;
                let plain = aead::open(
                    &key,
                    &secret_key_aad(profile.profile_id, self.device.device_id)?,
                    sealed,
                )
                .map_err(|_| Error::Invalid("the Secret Key that came didn't open".into()))?;
                let sk: [u8; 16] = plain
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Invalid("the Secret Key that came is malformed".into()))?;
                (Some(sk), "sent")
            }
            SecretKeyTransfer::OptedOut => (None, "opted-out"),
            SecretKeyTransfer::NotHeld => (None, "not-held"),
        };
        Ok(HandedOver {
            irk,
            irk_id: envelope.key_id,
            backup,
            secret_key,
            secret_key_state,
        })
    }
}

/// A frame's hash, for a log line that names a frame without its bytes.
pub fn frame_id(frame: &[u8]) -> String {
    hash(frame).to_hex()[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    #[test]
    fn codes_read_forgivingly_and_qr_payloads_round_trip() {
        let code = LinkCode::generate().unwrap();
        assert_eq!(code.as_str().len(), CODE_LEN);
        let typed = code.display().to_lowercase().replace('0', "o");
        assert_eq!(LinkCode::parse(&typed).unwrap(), code);
        assert!(LinkCode::parse("ABCDE-FGHJ").is_err(), "nine characters");
        assert!(LinkCode::parse("ABCDE-FGHJU").is_err(), "U isn't Crockford");
        assert_ne!(
            code.token_hash(),
            LinkCode::generate().unwrap().token_hash()
        );

        let invite = LinkInvite {
            server: Some("https://vault.example:8443/".into()),
            code: code.clone(),
            fingerprint: Some([9; 16]),
        };
        let qr = invite.to_qr();
        assert!(qr.starts_with(QR_PREFIX));
        assert_eq!(LinkInvite::parse(&qr).unwrap(), invite);
        assert_eq!(LinkInvite::parse(code.as_str()).unwrap().code, code);
        assert!(
            LinkInvite::parse("lockdown:contact:v1:abc:def")
                .unwrap_err()
                .to_string()
                .contains("contact")
        );
        assert!(LinkInvite::parse("lockdown:link:v1?s=javascript%3Ax&c=0123456789").is_err());
        assert!(LinkInvite::parse("lockdown:link:v1?s=https%3A%2F%2Fa").is_err());
        assert_eq!(format!("{code:?}"), "LinkCode(…)");
    }

    struct Pair {
        _dir: tempfile::TempDir,
        old_ks: FileKeystore,
        old: Identity,
        new_ks: FileKeystore,
    }

    fn pair() -> Pair {
        let dir = tempfile::tempdir().unwrap();
        let old_ks = FileKeystore::open_or_create(&dir.path().join("old.cbor")).unwrap();
        let old = crate::identity::open_or_create_identity(
            &dir.path().join("old-root"),
            "Lockdown on old",
            &old_ks,
            None,
        )
        .unwrap();
        let new_ks = FileKeystore::open_or_create(&dir.path().join("new.cbor")).unwrap();
        Pair {
            _dir: dir,
            old_ks,
            old,
            new_ks,
        }
    }

    #[test]
    fn a_new_device_gets_the_root_key_and_the_secret_key_after_a_compare() {
        let Pair {
            _dir,
            old_ks,
            mut old,
            new_ks,
        } = pair();
        let (mut offer, hello) = Offer::new(&old).unwrap();
        let fp = offer.invite(None).fingerprint;
        let (mut join, request) = Join::start(&hello, &new_ks, "Lockdown on new", fp).unwrap();
        let asked = offer.take_request(&request, &old).unwrap();
        assert_eq!(asked.device_name, "Lockdown on new");
        let JoinStep::Compare(shown) = join.on_frame(&asked.reveal, &new_ks).unwrap() else {
            panic!("the reveal gives the digits");
        };
        assert_eq!(shown, asked.sas, "both screens show the same six digits");
        assert_eq!(offer.asked(), Some(shown));

        let granted = offer
            .grant(&mut old, &old_ks, &SecretKeyState::Held([5; 16]))
            .unwrap();
        assert_eq!(granted.device.device_id, new_ks.device_id());
        assert!(old.devices().any(|d| d.device_id == new_ks.device_id()));
        assert!(matches!(
            join.on_frame(&granted.frames[0], &new_ks).unwrap(),
            JoinStep::Granted
        ));
        let JoinStep::Done(handed) = join.on_frame(&granted.frames[1], &new_ks).unwrap() else {
            panic!("the welcome finishes it");
        };
        assert_eq!(handed.secret_key, Some([5; 16]));
        assert_eq!(handed.backup.profile.profile_id, old.profile().profile_id);
        let opened = crate::handover::restore_from_root_key(
            &_dir.path().join("new-root"),
            crate::identity::APP,
            &new_ks,
            handed.irk,
            handed.irk_id,
            &handed.backup,
        )
        .unwrap();
        assert_eq!(opened.device_id(), new_ks.device_id());
    }

    #[test]
    fn a_relay_in_the_middle_shows_other_digits_and_a_wrong_qr_stops_the_join() {
        let Pair {
            _dir,
            old_ks,
            old,
            new_ks,
        } = pair();
        let (mut offer, hello) = Offer::new(&old).unwrap();
        // The relay answers the new device with a Hello of its own.
        let relay_ks = FileKeystore::open_or_create(&_dir.path().join("relay.cbor")).unwrap();
        let relay_identity = crate::identity::open_or_create_identity(
            &_dir.path().join("relay-root"),
            "relay",
            &relay_ks,
            None,
        )
        .unwrap();
        let (mut fake_offer, fake_hello) = Offer::new(&relay_identity).unwrap();
        // With the QR's fingerprint, the new device refuses the substitute.
        let fp = offer.invite(None).fingerprint;
        let err = Join::start(&fake_hello, &new_ks, "new", fp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("isn't the one whose QR code"), "{err}");
        // With a typed code it can't tell, but the digits differ: the relay
        // runs one handshake with each side and can't make them agree.
        let (mut join, request) = Join::start(&fake_hello, &new_ks, "new", None).unwrap();
        let asked_fake = fake_offer.take_request(&request, &relay_identity).unwrap();
        let JoinStep::Compare(new_shows) = join.on_frame(&asked_fake.reveal, &new_ks).unwrap()
        else {
            panic!()
        };
        let (_relay_join, relay_request) = Join::start(&hello, &relay_ks, "new", None).unwrap();
        let asked_real = offer.take_request(&relay_request, &old).unwrap();
        assert_ne!(new_shows, asked_real.sas);
        drop(old_ks);
    }

    #[test]
    fn bad_requests_are_refused_and_a_decline_writes_nothing() {
        let Pair {
            _dir,
            old_ks,
            mut old,
            new_ks,
        } = pair();
        let (mut offer, hello) = Offer::new(&old).unwrap();
        let (_join, request) = Join::start(&hello, &new_ks, "new", None).unwrap();
        // A request whose proof was made for another link is refused.
        let (mut other_offer, _) = Offer::new(&old).unwrap();
        let (why, frame) = other_offer.take_request(&request, &old).unwrap_err();
        assert!(why.contains("isn't signed by the device key"), "{why}");
        assert!(frame.is_some());
        // The real one is taken once; a second is refused.
        offer.take_request(&request, &old).unwrap();
        assert!(offer.take_request(&request, &old).is_err());
        // Declining sends a refusal and writes nothing.
        let before = old.devices().count();
        let mut join = _join;
        let decline = offer.decline().unwrap();
        let reveal_first = join.on_frame(&decline, &new_ks).unwrap_err().to_string();
        assert!(reveal_first.contains("declined"), "{reveal_first}");
        assert_eq!(old.devices().count(), before);
        // A grant with nothing asked is refused.
        let (fresh, _) = Offer::new(&old).unwrap();
        assert!(
            fresh
                .grant(&mut old, &old_ks, &SecretKeyState::NotHeld)
                .is_err()
        );
    }

    #[test]
    fn a_forged_welcome_or_secret_key_is_refused() {
        let Pair {
            _dir,
            old_ks,
            mut old,
            new_ks,
        } = pair();
        let (mut offer, hello) = Offer::new(&old).unwrap();
        let (mut join, request) = Join::start(&hello, &new_ks, "new", None).unwrap();
        let asked = offer.take_request(&request, &old).unwrap();
        join.on_frame(&asked.reveal, &new_ks).unwrap();
        let granted = offer
            .grant(&mut old, &old_ks, &SecretKeyState::OptedOut)
            .unwrap();
        join.on_frame(&granted.frames[0], &new_ks).unwrap();
        // The relay swaps in a Secret Key of its own, sealed to the new
        // device's public key (anyone can do that): the signature fails.
        let LinkFrame::Welcome(mut w) = LinkFrame::decode(&granted.frames[1]).unwrap() else {
            panic!()
        };
        let (enc, shared) = lockdown_crypto::kem::encapsulate(&new_ks.kem_pub()).unwrap();
        let key = secret_key_seal_key(shared.expose(), &enc).unwrap();
        let sealed = aead::seal(
            &key,
            &secret_key_aad(old.profile().profile_id, new_ks.device_id()).unwrap(),
            &[1; 16],
        )
        .unwrap();
        w.secret_key = SecretKeyTransfer::Sealed {
            enc: ByteBuf::from(enc),
            sealed,
        };
        let forged = to_canonical(&LinkFrame::Welcome(w)).unwrap();
        let err = join.on_frame(&forged, &new_ks).unwrap_err().to_string();
        assert!(err.contains("isn't signed"), "{err}");
    }
}
