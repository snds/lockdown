//! Share links and request links (02-model §1; matrix rows `share-links`
//! and `request-link`; `docs/plan/18-sharing.md` §5–§6).
//!
//! **A share link** ("Send") carries an item, a file or some text to
//! someone who may have no Lockdown at all. Everything is sealed on the
//! sender's device under a key derived from a random 256-bit link secret
//! that travels only in the URL's fragment (`#…`), which browsers never
//! send to a server. With a password, the key also needs Argon2id over the
//! password, and the server holds only a hash of a proof derived from it,
//! so it can refuse to hand the ciphertext out without ever being able to
//! open it. The server keeps ciphertext, an expiry and a view count.
//!
//! **A request link** asks someone to send you a secret. The link names a
//! request id and, in its fragment, the BLAKE3 of the receiving key of the
//! persona that asked, so the person answering seals to that key and a
//! server that offers another key is caught. The answer is the identity
//! crate's peer wrap (`x-wing-xchacha20poly1305`, purpose `share`) of the
//! payload. One answer per request; the server cannot read it.
//!
//! Both are plain Rust over `lockdown-crypto` and the identity crate, so
//! the web vault can open a share link and answer a request in a browser
//! from the same code (03-architecture §3).

use ecosystem_identity::kem::{KemPublicKey, KemSecretKey};
use ecosystem_identity::keys::{aad as identity_aad, unwrap_from_peer, wrap_to_peer};
use ecosystem_identity::records::{KeyEnvelope, KeyRef, KeyRefKind, Purpose};
use ecosystem_identity::{Record, SymmetricKey};
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::hash;
use lockdown_crypto::kdf::{Argon2Params, FLOOR_WASM, derive, floor, stretch};
use lockdown_crypto::key32;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{Error, Hash32, Id, Result};

/// The payload format version.
pub const SEND_V: u16 = 1;

/// The largest payload a share link carries (a file and its name), 64 MiB.
pub const MAX_SEND_BYTES: usize = 64 << 20;

/// The largest answer to a request (text and fields only), 64 KiB.
pub const MAX_RESPONSE_BYTES: usize = 64 << 10;

const SEND_INFO: &[u8] = b"lockdown/send/v1";
const ACCESS_INFO: &[u8] = b"lockdown/send-access/v1";
const SEND_AAD: &[u8] = b"lockdown/send-payload/v1";

key32!(
    /// The link secret: 256 random bits that travel only in the URL fragment.
    LinkSecret
);
key32!(
    /// The key a share link's payload is sealed under.
    SendKey
);

/// What a share link carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendKind {
    /// Some text.
    Text,
    /// A file.
    File,
    /// A copy of an item's fields, as they were when the link was made.
    Item,
}

/// One labelled value of a shared item.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendField {
    /// What it is ("Username", "Password").
    pub label: String,
    /// The value.
    pub value: String,
    /// Whether the opener should mask it until asked.
    pub secret: bool,
}

impl std::fmt::Debug for SendField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendField")
            .field("label", &self.label)
            .field("secret", &self.secret)
            .finish_non_exhaustive()
    }
}

impl Drop for SendField {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

/// A file in a share link.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendFile {
    /// Its name.
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// Its bytes.
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for SendFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendFile")
            .field("name", &self.name)
            .field("size", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl Drop for SendFile {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Text that is wiped when it drops.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretText(pub String);

impl SecretText {
    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretText(…)")
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// What is sealed into a share link or an answer to a request.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendPayload {
    /// Format version.
    pub v: u16,
    /// What it is.
    pub kind: SendKind,
    /// A title for the opener's screen.
    pub title: String,
    /// The text, for a text link or an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<SecretText>,
    /// The file, for a file link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<SendFile>,
    /// The fields, for an item link or an answer with several values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SendField>,
}

impl SendPayload {
    /// A text payload.
    pub fn text(title: &str, text: &str) -> Self {
        Self {
            v: SEND_V,
            kind: SendKind::Text,
            title: title.to_string(),
            text: Some(SecretText(text.to_string())),
            file: None,
            fields: Vec::new(),
        }
    }

    fn encode(&self) -> Result<Zeroizing<Vec<u8>>> {
        Ok(Zeroizing::new(to_canonical(self)?))
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let payload: Self = lockdown_crypto::cbor::from_slice(bytes)?;
        if payload.v > SEND_V {
            return Err(Error::SchemaTooNew {
                found: payload.v,
                supported: SEND_V,
            });
        }
        Ok(payload)
    }
}

/// The password half of a share link, as the server keeps it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SendPassword {
    /// The Argon2id salt.
    #[serde(with = "serde_bytes")]
    pub salt: [u8; 16],
    /// The Argon2id cost; never below the browser floor.
    pub params: Argon2Params,
    /// BLAKE3 of the access proof. The server compares the proof a viewer
    /// presents against this and learns nothing that opens the payload.
    pub verifier: Hash32,
}

/// A share link, sealed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SealedSend {
    /// The link's id on the server.
    pub send_id: Id,
    /// The sealed payload.
    pub ciphertext: Vec<u8>,
    /// The password half, when there is one.
    pub password: Option<SendPassword>,
}

/// The least Argon2id cost a share link's password may use: the browser
/// floor, so the web vault can open it (04-security §4), or this build's
/// floor when that is lower (the test build).
pub fn send_password_floor() -> Argon2Params {
    let f = floor();
    if f.m_cost_kib < FLOOR_WASM.m_cost_kib {
        f
    } else {
        FLOOR_WASM
    }
}

fn password_key(
    password: &[u8],
    salt: &[u8; 16],
    params: &Argon2Params,
) -> Result<Zeroizing<[u8; 32]>> {
    if !params.meets(&send_password_floor()) {
        return Err(Error::Invalid(
            "the link's password settings are below the floor; it wasn't opened".into(),
        ));
    }
    Ok(stretch(password, salt, params)?)
}

fn send_key(send_id: Id, secret: &LinkSecret, password_key: Option<&[u8; 32]>) -> Result<SendKey> {
    let mut ikm = Zeroizing::new(secret.expose().to_vec());
    if let Some(pk) = password_key {
        ikm.extend_from_slice(pk);
    }
    let mut out = Zeroizing::new([0u8; 32]);
    derive(&ikm, Some(send_id.as_bytes()), SEND_INFO, out.as_mut())?;
    Ok(SendKey::from_bytes(*out))
}

fn access_proof(send_id: Id, password_key: &[u8; 32]) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    derive(
        password_key,
        Some(send_id.as_bytes()),
        ACCESS_INFO,
        &mut out,
    )?;
    Ok(out)
}

fn payload_aad(send_id: Id) -> Vec<u8> {
    let mut aad = SEND_AAD.to_vec();
    aad.extend_from_slice(send_id.as_bytes());
    aad
}

/// Seals a payload into a new share link. Returns the link secret (for the
/// URL fragment, never the server) and what the server stores.
pub fn seal_send(
    payload: &SendPayload,
    password: Option<&[u8]>,
) -> Result<(LinkSecret, SealedSend)> {
    let bytes = payload.encode()?;
    if bytes.len() > MAX_SEND_BYTES {
        return Err(Error::Invalid(format!(
            "a share link carries at most {} MB",
            MAX_SEND_BYTES >> 20
        )));
    }
    let send_id = Id::new();
    let secret = LinkSecret::from_bytes(lockdown_crypto::rand::random_32()?);
    let (pk, pw) = match password.filter(|p| !p.is_empty()) {
        Some(p) => {
            let salt = lockdown_crypto::rand::random_16()?;
            let params = send_password_floor();
            let pk = password_key(p, &salt, &params)?;
            let proof = access_proof(send_id, &pk)?;
            (
                Some(pk),
                Some(SendPassword {
                    salt,
                    params,
                    verifier: hash(&proof),
                }),
            )
        }
        None => (None, None),
    };
    let key = send_key(send_id, &secret, pk.as_deref())?;
    let sealed = seal(&key, &payload_aad(send_id), &bytes)?;
    Ok((
        secret,
        SealedSend {
            send_id,
            ciphertext: to_canonical(&sealed)?,
            password: pw,
        },
    ))
}

/// The proof a viewer presents to the server for a password-locked link,
/// and the password key to open it with afterwards.
pub fn send_access(
    send_id: Id,
    password: &[u8],
    meta: &SendPassword,
) -> Result<([u8; 32], Zeroizing<[u8; 32]>)> {
    let pk = password_key(password, &meta.salt, &meta.params)?;
    Ok((access_proof(send_id, &pk)?, pk))
}

/// Opens a share link's ciphertext with the link secret (and the password
/// key, for a locked link).
pub fn open_send(
    send_id: Id,
    secret: &LinkSecret,
    password_key: Option<&[u8; 32]>,
    ciphertext: &[u8],
) -> Result<SendPayload> {
    let sealed: Sealed = lockdown_crypto::cbor::from_slice(ciphertext)?;
    let key = send_key(send_id, secret, password_key)?;
    let plain = open(&key, &payload_aad(send_id), &sealed).map_err(|_| {
        Error::Invalid(
            "the link didn't open: it's incomplete, the password is wrong, or it was changed"
                .into(),
        )
    })?;
    SendPayload::decode(&plain)
}

fn b64url(bytes: &[u8]) -> String {
    data_encoding::BASE64URL_NOPAD.encode(bytes)
}

fn unb64url(text: &str) -> Option<Vec<u8>> {
    data_encoding::BASE64URL_NOPAD.decode(text.as_bytes()).ok()
}

/// The link a person shares: `<server>/s/<id>#<secret>`.
pub fn send_link(server: &str, send_id: Id, secret: &LinkSecret) -> String {
    format!(
        "{}/s/{send_id}#{}",
        server.trim_end_matches('/'),
        b64url(secret.expose())
    )
}

/// A share link taken apart: the server, the id and the secret.
pub fn parse_send_link(link: &str) -> Result<(String, Id, LinkSecret)> {
    let bad = || Error::Invalid("that isn't a whole Lockdown share link".into());
    let (base, fragment) = link.trim().split_once('#').ok_or_else(bad)?;
    let (server, id) = base.rsplit_once("/s/").ok_or_else(bad)?;
    let send_id = Id::parse(id.trim_end_matches('/')).map_err(|_| bad())?;
    let bytes: [u8; 32] = unb64url(fragment)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(bad)?;
    Ok((server.to_string(), send_id, LinkSecret::from_bytes(bytes)))
}

// --- request links ------------------------------------------------------------

/// What the fragment of a request link carries.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct RequestFragment {
    /// BLAKE3 of the asking persona's receiving key.
    k: Hash32,
    /// What is asked for, in the asker's words.
    l: String,
}

/// A request link taken apart.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RequestLink {
    /// The server.
    pub server: String,
    /// The request.
    pub request_id: Id,
    /// BLAKE3 of the receiving key the answer must be sealed to.
    pub key_hash: Hash32,
    /// What is asked for.
    pub label: String,
}

/// The hash a request link pins: BLAKE3 of the receiving key's bytes.
pub fn kem_hash(key: &KemPublicKey) -> Hash32 {
    hash(&key.bytes)
}

/// The link a person sends to whoever should answer:
/// `<server>/r/<id>#<key hash and label>`.
pub fn request_link(
    server: &str,
    request_id: Id,
    key: &KemPublicKey,
    label: &str,
) -> Result<String> {
    let fragment = to_canonical(&RequestFragment {
        k: kem_hash(key),
        l: label.to_string(),
    })?;
    Ok(format!(
        "{}/r/{request_id}#{}",
        server.trim_end_matches('/'),
        b64url(&fragment)
    ))
}

/// A request link taken apart.
pub fn parse_request_link(link: &str) -> Result<RequestLink> {
    let bad = || Error::Invalid("that isn't a whole Lockdown request link".into());
    let (base, fragment) = link.trim().split_once('#').ok_or_else(bad)?;
    let (server, id) = base.rsplit_once("/r/").ok_or_else(bad)?;
    let request_id = Id::parse(id.trim_end_matches('/')).map_err(|_| bad())?;
    let bytes = unb64url(fragment).ok_or_else(bad)?;
    let f: RequestFragment = lockdown_crypto::cbor::from_slice(&bytes).map_err(|_| bad())?;
    Ok(RequestLink {
        server: server.to_string(),
        request_id,
        key_hash: f.k,
        label: f.l,
    })
}

fn response_aad(
    request_id: Id,
    profile_id: Id,
    persona_id: Id,
) -> ecosystem_identity::records::Aad {
    identity_aad(profile_id, Some(persona_id), request_id, Purpose::Share)
}

/// Seals an answer to a request. `key` is the receiving key the server
/// served for the request; it must hash to what the link pinned.
pub fn seal_response(
    link: &RequestLink,
    key: &KemPublicKey,
    profile_id: Id,
    persona_id: Id,
    payload: &SendPayload,
) -> Result<Vec<u8>> {
    if kem_hash(key) != link.key_hash {
        return Err(Error::Transparency(
            "the server offered a different key from the one the request link names; nothing was sent".into(),
        ));
    }
    if payload.file.is_some() {
        return Err(Error::Invalid(
            "an answer to a request carries text, not files".into(),
        ));
    }
    let bytes = payload.encode()?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(Error::Invalid(format!(
            "an answer carries at most {} KB",
            MAX_RESPONSE_BYTES >> 10
        )));
    }
    let env = wrap_to_peer(
        &bytes,
        key,
        KeyRef::new(KeyRefKind::Peer, persona_id),
        &response_aad(link.request_id, profile_id, persona_id),
    )?;
    Ok(env.to_bytes()?)
}

/// Opens an answer with the asking persona's private receiving key.
pub fn open_response(
    bytes: &[u8],
    with: &KemSecretKey,
    request_id: Id,
    profile_id: Id,
    persona_id: Id,
) -> Result<SendPayload> {
    let env = KeyEnvelope::from_bytes(bytes)?;
    let plain = unwrap_from_peer(
        &env,
        with,
        &response_aad(request_id, profile_id, persona_id),
    )?;
    SendPayload::decode(&plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_link_opens_with_its_secret_and_password_only() {
        let payload = SendPayload::text("Wi-Fi", "hunter2 at the cabin");
        let (secret, sealed) = seal_send(&payload, None).unwrap();
        let link = send_link("https://lockdown.example/", sealed.send_id, &secret);
        assert!(link.starts_with("https://lockdown.example/s/"));
        let (server, id, back) = parse_send_link(&link).unwrap();
        assert_eq!(server, "https://lockdown.example");
        assert_eq!(id, sealed.send_id);
        assert_eq!(
            open_send(id, &back, None, &sealed.ciphertext).unwrap(),
            payload
        );
        // Another secret, or the ciphertext under another id: refused.
        let other = LinkSecret::from_bytes([7; 32]);
        assert!(open_send(id, &other, None, &sealed.ciphertext).is_err());
        assert!(open_send(Id::new(), &back, None, &sealed.ciphertext).is_err());
        assert!(parse_send_link("https://lockdown.example/s/abc").is_err());

        // With a password: the proof matches the verifier, and the payload
        // needs the password key as well as the secret.
        let (secret, sealed) = seal_send(&payload, Some(b"blue door")).unwrap();
        let meta = sealed.password.clone().unwrap();
        let (proof, pk) = send_access(sealed.send_id, b"blue door", &meta).unwrap();
        assert_eq!(hash(&proof), meta.verifier);
        assert_eq!(
            open_send(sealed.send_id, &secret, Some(&pk), &sealed.ciphertext).unwrap(),
            payload
        );
        assert!(open_send(sealed.send_id, &secret, None, &sealed.ciphertext).is_err());
        let (wrong, wrong_pk) = send_access(sealed.send_id, b"red door", &meta).unwrap();
        assert_ne!(hash(&wrong), meta.verifier);
        assert!(open_send(sealed.send_id, &secret, Some(&wrong_pk), &sealed.ciphertext).is_err());
        // Settings below the floor are refused before any work.
        let mut weak = meta.clone();
        weak.params.m_cost_kib = 8;
        assert!(send_access(sealed.send_id, b"blue door", &weak).is_err());
    }

    #[test]
    fn a_request_is_answered_to_the_key_its_link_pins() {
        let (sk, pk) = ecosystem_identity::kem::kem(ecosystem_identity::kem::DEFAULT_KEM)
            .keypair(lockdown_crypto::rand::random_32().unwrap())
            .unwrap();
        let (profile, persona, request) = (Id::new(), Id::new(), Id::new());
        let text = request_link(
            "https://lockdown.example",
            request,
            &pk,
            "the router password",
        )
        .unwrap();
        let link = parse_request_link(&text).unwrap();
        assert_eq!(link.request_id, request);
        assert_eq!(link.label, "the router password");
        let answer = SendPayload::text("Router", "correct horse");
        let sealed = seal_response(&link, &pk, profile, persona, &answer).unwrap();
        assert_eq!(
            open_response(&sealed, &sk, request, profile, persona).unwrap(),
            answer
        );
        // Opened as an answer to another request: refused.
        assert!(open_response(&sealed, &sk, Id::new(), profile, persona).is_err());
        // A server that offers its own key is caught before anything is sealed.
        let (_, server_pk) = ecosystem_identity::kem::kem(ecosystem_identity::kem::DEFAULT_KEM)
            .keypair(lockdown_crypto::rand::random_32().unwrap())
            .unwrap();
        assert!(matches!(
            seal_response(&link, &server_pk, profile, persona, &answer),
            Err(Error::Transparency(_))
        ));
    }
}
