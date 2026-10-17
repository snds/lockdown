//! The signed matcher data file (09-extension-boundary §2).
//!
//! The curated equivalent-domain groups and the confusables table reach
//! the matcher as one file: a CBOR **envelope** holding the canonical CBOR
//! bytes of a **payload** and an Ed25519 signature over those bytes. The
//! matcher embeds the publisher keys it trusts; a file that is unsigned,
//! signed by a key it does not know, or changed by one byte is refused
//! before the payload is parsed.
//!
//! ```text
//! envelope (CBOR map) {
//!   "format":    "lockdown-matcher-data",
//!   "v":         1,                      // envelope version
//!   "key_id":    "dev-2026-10",          // which trusted key signed it
//!   "payload":   bytes,                  // canonical CBOR of the payload
//!   "signature": bytes (64)              // Ed25519 over PREFIX || payload
//! }
//! payload (canonical CBOR map) {
//!   "matcher_data": 1,                   // monotonic data version
//!   "published":    "2026-10-17",
//!   "groups":       [ {name, domains}, … ],
//!   "confusables":  { map, folds },
//!   … unknown fields pass through …
//! }
//! ```
//!
//! **Canonical** means RFC 8949 §4.2.1 core deterministic encoding: shortest
//! integer forms (ciborium's default) and map keys sorted by the bytewise
//! order of their encodings. The signature covers the bytes as stored, so
//! canonical form matters for reproducibility (re-signing the same source
//! gives the same file), not for security.
//!
//! **Downgrade.** `matcher_data` only ever goes up. [`DataState`] remembers
//! the newest version it has loaded in this process and refuses an older
//! one. The host persists that number across restarts (the extension in
//! `chrome.storage.local`, the desktop app beside `remembered.cbor`) and
//! hands it back through [`DataState::with_floor`]; this crate has no file
//! of its own to write to.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ciborium::Value;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::confusables::{Confusables, ConfusablesTable};
use crate::{DomainGroup, EquivalentDomains};

/// The envelope's `format` field.
pub const FORMAT: &str = "lockdown-matcher-data";
/// The envelope version this crate reads and writes.
pub const ENVELOPE_VERSION: u32 = 1;
/// Domain separation: what is signed is this prefix followed by the
/// payload bytes, so a signature over matcher data can never be replayed
/// as a signature over anything else.
pub const SIGNING_PREFIX: &[u8] = b"lockdown-matcher-data/v1\0";
/// The id of the development key checked in under `data/dev/`.
pub const DEV_KEY_ID: &str = "dev-2026-10";

/// The built-in signed file, as shipped in this build.
pub const BUILTIN_SIGNED: &[u8] = include_bytes!("../data/matcher-data.signed.cbor");
/// The development public key, lowercase hex.
pub const DEV_PUBLISHER_KEY_HEX: &str = include_str!("../data/dev/publisher.pub");

/// Why a data file was refused.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum DataError {
    /// Not a CBOR envelope of the expected shape.
    #[error("the matcher data file isn't a signed envelope")]
    NotAnEnvelope,
    /// An envelope from a newer or different format.
    #[error(
        "the matcher data envelope is format '{format}' v{v}; this build reads '{expected}' v{expected_v}"
    )]
    UnsupportedFormat {
        /// What the file said.
        format: String,
        /// What the file said.
        v: u32,
        /// What this build reads.
        expected: &'static str,
        /// What this build reads.
        expected_v: u32,
    },
    /// The envelope names a key this build does not trust.
    #[error("the matcher data file is signed by '{0}', a key this build doesn't trust")]
    UnknownKey(String),
    /// The signature does not verify over the payload.
    #[error("the matcher data file's signature doesn't match its contents")]
    BadSignature,
    /// The payload bytes verified but did not parse.
    #[error("the matcher data payload didn't read correctly: {0}")]
    BadPayload(String),
    /// The file is older than one already loaded.
    #[error(
        "the matcher data file is version {offered}, older than version {newest} already loaded"
    )]
    Downgrade {
        /// The version in the file.
        offered: u64,
        /// The newest version loaded so far.
        newest: u64,
    },
    /// A trusted-key entry was not a valid Ed25519 public key.
    #[error("'{0}' isn't a valid Ed25519 public key")]
    BadKey(String),
}

impl DataError {
    /// The stable wire code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            DataError::NotAnEnvelope
            | DataError::UnsupportedFormat { .. }
            | DataError::BadPayload(_)
            | DataError::BadKey(_) => "bad_data",
            DataError::UnknownKey(_) => "unknown_key",
            DataError::BadSignature => "bad_signature",
            DataError::Downgrade { .. } => "downgrade",
        }
    }
}

/// The outer, signed shape.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// [`FORMAT`].
    pub format: String,
    /// [`ENVELOPE_VERSION`].
    pub v: u32,
    /// Which trusted key signed it.
    pub key_id: String,
    /// Canonical CBOR of [`MatcherPayload`].
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    /// Ed25519 signature over [`SIGNING_PREFIX`] followed by `payload`.
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

/// What the publisher signs. Unknown fields are kept in `extra` and
/// written back out, so an older build carries a newer file's fields
/// through untouched.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MatcherPayload {
    /// The monotonic data version. Raise on every change; never reuse.
    pub matcher_data: u64,
    /// A date label for people; not compared.
    #[serde(default)]
    pub published: String,
    /// The curated equivalent-domain groups.
    #[serde(default)]
    pub groups: Vec<DomainGroup>,
    /// The confusables table.
    #[serde(default)]
    pub confusables: ConfusablesTable,
    /// Fields this build does not know.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The publisher keys a build trusts, by id.
#[derive(Clone, Debug, Default)]
pub struct TrustedKeys {
    keys: BTreeMap<String, VerifyingKey>,
}

impl TrustedKeys {
    /// Trusts nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// Adds a key under `id`.
    pub fn with(mut self, id: &str, public_key: &[u8; 32]) -> Result<Self, DataError> {
        let key = VerifyingKey::from_bytes(public_key).map_err(|_| DataError::BadKey(id.into()))?;
        self.keys.insert(id.to_string(), key);
        Ok(self)
    }

    /// Adds a key given as lowercase hex.
    pub fn with_hex(self, id: &str, hex: &str) -> Result<Self, DataError> {
        let bytes = data_encoding::HEXLOWER
            .decode(hex.trim().as_bytes())
            .map_err(|_| DataError::BadKey(id.into()))?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| DataError::BadKey(id.into()))?;
        self.with(id, &arr)
    }

    /// The development key only. **A release build must not use this**;
    /// see `data/dev/README.md`.
    pub fn dev() -> Self {
        Self::none()
            .with_hex(DEV_KEY_ID, DEV_PUBLISHER_KEY_HEX)
            .unwrap_or_default()
    }

    /// The keys this build trusts for its built-in file. Today that is the
    /// development key; the release pipeline replaces this body.
    pub fn builtin() -> Self {
        Self::dev()
    }

    /// The key under `id`, if trusted.
    pub fn get(&self, id: &str) -> Option<&VerifyingKey> {
        self.keys.get(id)
    }

    /// The trusted ids.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }
}

/// Recursively sorts map keys by the bytewise order of their encodings
/// (RFC 8949 §4.2.1).
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Map(entries) => {
            let mut keyed: Vec<(Vec<u8>, Value, Value)> = entries
                .into_iter()
                .map(|(k, v)| {
                    let k = canonicalize(k);
                    let mut enc = Vec::new();
                    // Encoding a Value cannot fail for the shapes serde produces.
                    let _ = ciborium::into_writer(&k, &mut enc);
                    (enc, k, canonicalize(v))
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Map(keyed.into_iter().map(|(_, k, v)| (k, v)).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize).collect()),
        Value::Tag(t, inner) => Value::Tag(t, Box::new(canonicalize(*inner))),
        other => other,
    }
}

/// Canonical CBOR bytes of any serialisable value.
pub fn to_canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, DataError> {
    let v = Value::serialized(value).map_err(|e| DataError::BadPayload(e.to_string()))?;
    let mut out = Vec::new();
    ciborium::into_writer(&canonicalize(v), &mut out)
        .map_err(|e| DataError::BadPayload(e.to_string()))?;
    Ok(out)
}

fn signed_message(payload: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(SIGNING_PREFIX.len() + payload.len());
    m.extend_from_slice(SIGNING_PREFIX);
    m.extend_from_slice(payload);
    m
}

/// Signs a payload into envelope bytes. Used by the `sign-data` example
/// and by tests; production signing runs the same function in the release
/// signing service with a key that is not in this repository.
pub fn sign(
    payload: &MatcherPayload,
    key: &SigningKey,
    key_id: &str,
) -> Result<Vec<u8>, DataError> {
    let payload_bytes = to_canonical_cbor(payload)?;
    let signature: Signature = key.sign(&signed_message(&payload_bytes));
    let env = Envelope {
        format: FORMAT.into(),
        v: ENVELOPE_VERSION,
        key_id: key_id.into(),
        payload: payload_bytes,
        signature: signature.to_bytes().to_vec(),
    };
    to_canonical_cbor(&env)
}

/// A verified file: who signed it and what it says.
#[derive(Clone, PartialEq, Debug)]
pub struct Verified {
    /// The trusted key that signed it.
    pub key_id: String,
    /// The payload.
    pub payload: MatcherPayload,
}

/// Decodes exactly one CBOR item that fills `bytes`; trailing bytes are an
/// error, so nothing can ride along after a signed item.
fn from_exact_cbor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value: T = ciborium::from_reader(&mut cursor).map_err(|e| e.to_string())?;
    if cursor.position() != bytes.len() as u64 {
        return Err(format!(
            "{} trailing byte(s) after the CBOR item",
            bytes.len() as u64 - cursor.position()
        ));
    }
    Ok(value)
}

/// Verifies envelope bytes against `keys`. Nothing in the payload is read
/// before the signature checks out.
pub fn verify(bytes: &[u8], keys: &TrustedKeys) -> Result<Verified, DataError> {
    let env: Envelope = from_exact_cbor(bytes).map_err(|_| DataError::NotAnEnvelope)?;
    if env.format != FORMAT || env.v != ENVELOPE_VERSION {
        return Err(DataError::UnsupportedFormat {
            format: env.format,
            v: env.v,
            expected: FORMAT,
            expected_v: ENVELOPE_VERSION,
        });
    }
    let key = keys
        .get(&env.key_id)
        .ok_or_else(|| DataError::UnknownKey(env.key_id.clone()))?;
    let signature = Signature::from_slice(&env.signature).map_err(|_| DataError::BadSignature)?;
    key.verify_strict(&signed_message(&env.payload), &signature)
        .map_err(|_| DataError::BadSignature)?;
    let payload: MatcherPayload = from_exact_cbor(&env.payload).map_err(DataError::BadPayload)?;
    Ok(Verified {
        key_id: env.key_id,
        payload,
    })
}

/// The data in force: the groups and the table, compiled for lookups.
#[derive(Clone, Debug)]
pub struct MatcherData {
    /// The payload's `matcher_data`.
    pub version: u64,
    /// The payload's `published` label.
    pub published: String,
    /// Which key signed it.
    pub key_id: String,
    /// The equivalent-domain groups.
    pub equivalent: EquivalentDomains,
    /// The confusables table.
    pub confusables: Confusables,
    /// Fields this build does not know, kept for the next writer.
    pub extra: BTreeMap<String, Value>,
}

impl MatcherData {
    /// Compiles a verified payload.
    pub fn from_verified(v: Verified) -> Result<Self, DataError> {
        let mut equivalent = EquivalentDomains::none();
        for g in v.payload.groups {
            equivalent.add_group(g);
        }
        let confusables = Confusables::from_table(&v.payload.confusables)
            .map_err(|e| DataError::BadPayload(e.to_string()))?;
        Ok(Self {
            version: v.payload.matcher_data,
            published: v.payload.published,
            key_id: v.key_id,
            equivalent,
            confusables,
            extra: v.payload.extra,
        })
    }

    /// Verifies and compiles `bytes` under `keys`.
    pub fn load(bytes: &[u8], keys: &TrustedKeys) -> Result<Self, DataError> {
        Self::from_verified(verify(bytes, keys)?)
    }

    /// The data compiled into this build, verified once per process
    /// against the built-in trusted keys.
    pub fn builtin() -> Result<Self, DataError> {
        static BUILTIN: OnceLock<Result<MatcherData, DataError>> = OnceLock::new();
        BUILTIN
            .get_or_init(|| MatcherData::load(BUILTIN_SIGNED, &TrustedKeys::builtin()))
            .clone()
    }

    /// Nothing: no groups, no confusables, version 0.
    pub fn empty() -> Self {
        Self {
            version: 0,
            published: String::new(),
            key_id: String::new(),
            equivalent: EquivalentDomains::none(),
            confusables: Confusables::none(),
            extra: BTreeMap::new(),
        }
    }
}

/// The data in force plus the downgrade memory for this process.
#[derive(Clone, Debug)]
pub struct DataState {
    data: MatcherData,
    newest_loaded: u64,
}

impl DataState {
    /// Starts from the built-in data. The floor is the built-in version.
    pub fn builtin() -> Result<Self, DataError> {
        let data = MatcherData::builtin()?;
        Ok(Self {
            newest_loaded: data.version,
            data,
        })
    }

    /// Starts from `data`.
    pub fn new(data: MatcherData) -> Self {
        Self {
            newest_loaded: data.version,
            data,
        }
    }

    /// Raises the floor to a version the host remembers from an earlier
    /// run. Never lowers it.
    pub fn with_floor(mut self, version: u64) -> Self {
        self.newest_loaded = self.newest_loaded.max(version);
        self
    }

    /// Raises the floor in place.
    pub fn raise_floor(&mut self, version: u64) {
        self.newest_loaded = self.newest_loaded.max(version);
    }

    /// Verifies `bytes`, refuses a version below the floor, and otherwise
    /// makes it the data in force. The same version loads again (a restart
    /// re-reading the file it already has).
    pub fn load(&mut self, bytes: &[u8], keys: &TrustedKeys) -> Result<&MatcherData, DataError> {
        let data = MatcherData::load(bytes, keys)?;
        if data.version < self.newest_loaded {
            return Err(DataError::Downgrade {
                offered: data.version,
                newest: self.newest_loaded,
            });
        }
        self.newest_loaded = data.version;
        self.data = data;
        Ok(&self.data)
    }

    /// The data in force.
    pub fn data(&self) -> &MatcherData {
        &self.data
    }

    /// The newest version loaded or remembered: what the host persists.
    pub fn newest_loaded(&self) -> u64 {
        self.newest_loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source file the signed bytes are made from.
    const SOURCE_JSON: &str = include_str!("../data/matcher-data.json");

    fn dev_signing_key() -> SigningKey {
        let seed = data_encoding::HEXLOWER
            .decode(include_str!("../data/dev/publisher.key").trim().as_bytes())
            .unwrap();
        SigningKey::from_bytes(&seed.try_into().unwrap())
    }

    fn other_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn source_payload() -> MatcherPayload {
        serde_json::from_str(SOURCE_JSON).unwrap()
    }

    #[test]
    fn the_builtin_file_verifies_and_matches_the_source() {
        let data = MatcherData::builtin().expect("built-in data verifies");
        assert_eq!(data.key_id, DEV_KEY_ID);
        assert!(data.version >= 1);
        assert!(data.equivalent.len() >= 16);
        assert!(data.confusables.len() >= 30);
        // Unknown fields pass through: the source carries a `note`.
        assert!(data.extra.contains_key("note"));
        // The committed bytes are exactly a fresh signing of the source:
        // edit the JSON, run `cargo run -p lockdown-matcher --example sign-data`.
        let fresh = sign(&source_payload(), &dev_signing_key(), DEV_KEY_ID).unwrap();
        assert!(
            fresh == BUILTIN_SIGNED,
            "data/matcher-data.signed.cbor is stale; regenerate it with the sign-data example"
        );
        // The dev public key file matches the seed.
        assert_eq!(
            data_encoding::HEXLOWER.encode(&dev_signing_key().verifying_key().to_bytes()),
            DEV_PUBLISHER_KEY_HEX.trim()
        );
    }

    #[test]
    fn every_tampered_byte_is_refused() {
        let keys = TrustedKeys::dev();
        let good = sign(&source_payload(), &dev_signing_key(), DEV_KEY_ID).unwrap();
        assert!(verify(&good, &keys).is_ok());
        // Flip one bit at a spread of positions across the whole file:
        // header, key id, payload, signature.
        let step = (good.len() / 97).max(1);
        for i in (0..good.len()).step_by(step) {
            let mut bad = good.clone();
            bad[i] ^= 0x01;
            let r = verify(&bad, &keys);
            assert!(r.is_err(), "byte {i} flipped and still verified");
        }
        // One byte appended or removed.
        let mut longer = good.clone();
        longer.push(0);
        assert!(verify(&longer, &keys).is_err());
        assert!(verify(&good[..good.len() - 1], &keys).is_err());
    }

    #[test]
    fn unsigned_wrong_key_and_unknown_key_are_refused() {
        let keys = TrustedKeys::dev();
        let payload = source_payload();
        // Plain payload bytes, no envelope.
        let bare = to_canonical_cbor(&payload).unwrap();
        assert_eq!(verify(&bare, &keys), Err(DataError::NotAnEnvelope));
        // An envelope with an empty signature.
        let mut env: Envelope =
            ciborium::from_reader(&sign(&payload, &dev_signing_key(), DEV_KEY_ID).unwrap()[..])
                .unwrap();
        env.signature.clear();
        let bytes = to_canonical_cbor(&env).unwrap();
        assert_eq!(verify(&bytes, &keys), Err(DataError::BadSignature));
        // Signed by another key under the trusted id.
        let forged = sign(&payload, &other_key(), DEV_KEY_ID).unwrap();
        assert_eq!(verify(&forged, &keys), Err(DataError::BadSignature));
        // Signed by another key under its own id, which this build does not trust.
        let stranger = sign(&payload, &other_key(), "prod-2027-01").unwrap();
        assert_eq!(
            verify(&stranger, &keys),
            Err(DataError::UnknownKey("prod-2027-01".into()))
        );
        // Trusting that key makes the same bytes load.
        let more = TrustedKeys::dev()
            .with("prod-2027-01", &other_key().verifying_key().to_bytes())
            .unwrap();
        assert!(verify(&stranger, &more).is_ok());
        // Nothing trusted: nothing loads.
        assert!(matches!(
            verify(&forged, &TrustedKeys::none()),
            Err(DataError::UnknownKey(_))
        ));
        // A future envelope version is refused by name.
        let mut future: Envelope =
            ciborium::from_reader(&sign(&payload, &dev_signing_key(), DEV_KEY_ID).unwrap()[..])
                .unwrap();
        future.v = 2;
        let bytes = to_canonical_cbor(&future).unwrap();
        assert!(matches!(
            verify(&bytes, &keys),
            Err(DataError::UnsupportedFormat { v: 2, .. })
        ));
    }

    #[test]
    fn a_version_downgrade_is_refused_and_the_floor_persists() {
        let keys = TrustedKeys::dev();
        let key = dev_signing_key();
        let mut v1 = source_payload();
        v1.matcher_data = 1;
        let mut v2 = v1.clone();
        v2.matcher_data = 2;
        v2.groups.push(DomainGroup {
            name: "test".into(),
            domains: vec!["example.com".into(), "example.net".into()],
        });
        let b1 = sign(&v1, &key, DEV_KEY_ID).unwrap();
        let b2 = sign(&v2, &key, DEV_KEY_ID).unwrap();

        let mut state = DataState::new(MatcherData::empty());
        assert_eq!(state.load(&b1, &keys).unwrap().version, 1);
        assert_eq!(state.load(&b2, &keys).unwrap().version, 2);
        assert!(
            state
                .data()
                .equivalent
                .equivalent("example.com", "example.net")
        );
        // Back to 1: refused, and 2 stays in force.
        assert_eq!(
            state.load(&b1, &keys).unwrap_err(),
            DataError::Downgrade {
                offered: 1,
                newest: 2
            }
        );
        assert_eq!(state.data().version, 2);
        // The same version again is fine (a restart re-reading its file).
        assert_eq!(state.load(&b2, &keys).unwrap().version, 2);
        assert_eq!(state.newest_loaded(), 2);

        // A fresh process with the host's remembered floor refuses 1 too,
        // even though it has not loaded 2 itself.
        let mut fresh = DataState::new(MatcherData::empty()).with_floor(state.newest_loaded());
        assert!(matches!(
            fresh.load(&b1, &keys),
            Err(DataError::Downgrade {
                offered: 1,
                newest: 2
            })
        ));
        // A tampered file never moves the floor.
        let mut bad = b2.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xff;
        assert!(fresh.load(&bad, &keys).is_err());
        assert_eq!(fresh.newest_loaded(), 2);
        // The built-in state's floor is the built-in version.
        let builtin = DataState::builtin().unwrap();
        assert_eq!(builtin.newest_loaded(), builtin.data().version);
    }

    #[test]
    fn unknown_fields_pass_through_a_round_trip() {
        let key = dev_signing_key();
        let mut payload = source_payload();
        payload.extra.insert(
            "future_field".into(),
            Value::Map(vec![(
                Value::Text("nested".into()),
                Value::Array(vec![Value::Integer(1.into()), Value::Text("two".into())]),
            )]),
        );
        let bytes = sign(&payload, &key, DEV_KEY_ID).unwrap();
        let back = verify(&bytes, &TrustedKeys::dev()).unwrap().payload;
        assert_eq!(back.extra["future_field"], payload.extra["future_field"]);
        assert_eq!(back, payload);
        // Re-signing what came back gives the same bytes: canonical.
        assert_eq!(sign(&back, &key, DEV_KEY_ID).unwrap(), bytes);
    }

    #[test]
    fn canonical_cbor_sorts_map_keys() {
        #[derive(Serialize)]
        struct Unsorted {
            zebra: u8,
            apple: u8,
            mango: u8,
        }
        let bytes = to_canonical_cbor(&Unsorted {
            zebra: 1,
            apple: 2,
            mango: 3,
        })
        .unwrap();
        let v: Value = ciborium::from_reader(&bytes[..]).unwrap();
        let keys: Vec<String> = v
            .into_map()
            .unwrap()
            .into_iter()
            .map(|(k, _)| k.into_text().unwrap())
            .collect();
        assert_eq!(keys, ["apple", "mango", "zebra"]);
    }
}
