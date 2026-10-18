//! Lockdown's own backup format, version 1 (`docs/format/backup-v1.md`).
//!
//! Two layers:
//!
//! 1. **The document**: one JSON object, `{"format": "lockdown-backup",
//!    "v": 1, ...}`, holding every item with every field (kind, label,
//!    section, value), its earlier versions, its attachments inline, and
//!    the list of what was left out and why. A plain export is this
//!    document as a file.
//! 2. **The sealed file**: the document encrypted under a password.
//!    `LDBACKUP`, a version byte, a two-byte header length and a JSON
//!    header naming the key derivation (Argon2id: memory, passes, lanes,
//!    salt) and the cipher (XChaCha20-Poly1305 in 1 MiB chunks with a
//!    19-byte nonce prefix); then the chunks. Chunk `i`'s nonce is the
//!    prefix, `i` as four big-endian bytes, and a last-chunk flag byte; its
//!    associated data is every byte before the first chunk, so the header
//!    can't be changed and chunks can't be reordered, dropped or added.
//!
//! The file key is HKDF-SHA-512 of the Argon2id output with the label
//! `lockdown/backup/v1/file-key`. Readers cap the Argon2id cost a file may
//! ask for, so a hostile file can't stall the app.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use lockdown_crypto::kdf::Argon2Params;
use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::convert::{ImportedAttachment, ImportedItem, Kept};
use crate::import::Parsed;
use crate::model::{b64url, from_b64url};
use crate::{CxfError, Limits, Result};

/// The `format` value of the document.
pub const FORMAT_NAME: &str = "lockdown-backup";
/// The document version this build writes and the highest it reads.
pub const DOC_V: u16 = 1;
/// The first bytes of a sealed file.
pub const MAGIC: &[u8; 8] = b"LDBACKUP";
/// The sealed-file version this build writes and reads.
pub const SEALED_V: u8 = 1;
/// Plaintext bytes per chunk.
pub const CHUNK: usize = 1 << 20;
/// The HKDF label of the file key.
pub const KEY_INFO: &[u8] = b"lockdown/backup/v1/file-key";

const NONCE_PREFIX: usize = 19;
const TAG: usize = 16;
const MAX_HEADER: usize = 4096;
/// The most Argon2id memory (KiB), passes and lanes a file may ask for.
const MAX_M_KIB: u32 = 1024 * 1024;
const MAX_T: u32 = 16;
const MAX_P: u32 = 16;

// --- the document -------------------------------------------------------

/// The whole backup.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    /// Always [`FORMAT_NAME`].
    pub format: String,
    /// [`DOC_V`].
    pub v: u16,
    /// When it was made, seconds since the epoch.
    pub exported_at: u64,
    /// The app that made it, in words.
    pub exporter: String,
    /// The items.
    pub items: Vec<DocItem>,
    /// What a person's choice or the format kept out, and why.
    #[serde(default)]
    pub left_out: Vec<LeftOut>,
    /// The vaults the items came from, with their folders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vaults: Vec<DocVault>,
}

/// A vault the items came from.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocVault {
    /// The vault's id (a UUID).
    pub id: String,
    /// The name the person gave it, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its folders.
    #[serde(default)]
    pub folders: Vec<DocFolder>,
}

/// A folder.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocFolder {
    /// Its id (a UUID): what an item's `org.folder` field names.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The folder it sits in, or none at the top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// One item.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocItem {
    /// The item's id (a UUID).
    pub id: String,
    /// The kind, as `ItemKind::as_str` writes it.
    pub kind: String,
    /// `standard` or `crown_jewel`.
    pub class: String,
    /// `live`, `archived` or `trashed`.
    pub state: String,
    /// When the first version was saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    /// When the newest version was saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    /// The newest version's fields, `title` first.
    pub fields: Vec<DocField>,
    /// Earlier versions, oldest first.
    #[serde(default)]
    pub history: Vec<DocVersion>,
    /// The newest version's files.
    #[serde(default)]
    pub attachments: Vec<DocAttachment>,
    /// The vault it came from (one of the document's `vaults`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
}

/// One field.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocField {
    /// The field id (`title`, `pw`, `url.0`, `custom.3`).
    pub id: String,
    /// The field kind (`text`, `hidden`, `url`, `totp`, `passkey`, …).
    pub kind: FieldKind,
    /// The label.
    pub label: String,
    /// The section heading, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The value.
    pub value: DocValue,
}

/// A value: exactly one of its members is set.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocValue {
    /// Text (also hidden text, dates, codes as `otpauth://` links).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Yes or no.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    /// A web address…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// …and its match rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_rule: Option<String>,
    /// Bytes, base64url without padding (passkey material).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    /// A link to another item, by id.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub item_ref: Option<String>,
}

impl Drop for DocValue {
    fn drop(&mut self) {
        for s in [&mut self.text, &mut self.url, &mut self.bytes]
            .into_iter()
            .flatten()
        {
            s.zeroize();
        }
    }
}

/// An earlier version.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocVersion {
    /// Its number in the item's history.
    pub n: u64,
    /// When it was saved.
    pub edited_at: i64,
    /// Its fields.
    pub fields: Vec<DocField>,
}

/// A file, inline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocAttachment {
    /// Its name.
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// Its size in bytes.
    pub size: u64,
    /// BLAKE3 of the bytes, hex: a reader checks it.
    pub blake3: String,
    /// The bytes, base64url without padding.
    pub data: String,
}

impl Drop for DocAttachment {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

/// Something kept out of the file, and why.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeftOut {
    /// The item's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its kind.
    pub kind: String,
    /// `crown-jewel` (not opted in, D121), or another reason in words.
    pub reason: String,
}

impl DocValue {
    /// From a vault value.
    pub fn from_value(v: &FieldValue) -> Self {
        let mut out = Self::default();
        match v {
            FieldValue::Text(t) => out.text = Some(t.clone()),
            FieldValue::Boolean(b) => out.flag = Some(*b),
            FieldValue::Url { value, match_rule } => {
                out.url = Some(value.clone());
                out.match_rule = Some(match_rule.clone());
            }
            FieldValue::Bytes(b) => out.bytes = Some(b64url(b)),
            FieldValue::Ref(id) => out.item_ref = Some(id.to_string()),
        }
        out
    }

    /// To a vault value; refused unless exactly one member is set.
    pub fn to_value(&self) -> std::result::Result<FieldValue, String> {
        let set = [
            self.text.is_some(),
            self.flag.is_some(),
            self.url.is_some(),
            self.bytes.is_some(),
            self.item_ref.is_some(),
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if set != 1 {
            return Err("a value must have exactly one of text, flag, url, bytes or ref".into());
        }
        if let Some(t) = &self.text {
            return Ok(FieldValue::Text(t.clone()));
        }
        if let Some(b) = self.flag {
            return Ok(FieldValue::Boolean(b));
        }
        if let Some(u) = &self.url {
            return Ok(FieldValue::Url {
                value: u.clone(),
                match_rule: self
                    .match_rule
                    .clone()
                    .unwrap_or_else(|| "exact_host".into()),
            });
        }
        if let Some(b) = &self.bytes {
            return from_b64url(b)
                .map(FieldValue::Bytes)
                .map_err(|e| e.to_string());
        }
        let r = self.item_ref.as_deref().unwrap_or_default();
        Id::parse(r)
            .map(FieldValue::Ref)
            .map_err(|_| "a link to another item isn't an id".to_string())
    }
}

impl DocField {
    /// From a vault field.
    pub fn from_plain(f: &PlainField) -> Self {
        Self {
            id: f.field_id.clone(),
            kind: f.kind,
            label: f.label.clone(),
            section: f.section.clone(),
            value: DocValue::from_value(&f.value),
        }
    }

    /// To a vault field.
    pub fn to_plain(&self) -> std::result::Result<PlainField, String> {
        let mut f = PlainField::new(
            self.id.clone(),
            self.kind,
            self.label.clone(),
            self.value
                .to_value()
                .map_err(|e| format!("'{}': {e}", self.id))?,
        );
        f.section = self.section.clone();
        Ok(f)
    }
}

impl DocAttachment {
    /// From a file's facts and bytes.
    pub fn new(name: &str, mime: &str, bytes: &[u8]) -> Self {
        Self {
            name: name.into(),
            mime: mime.into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(bytes).to_hex().to_string(),
            data: b64url(bytes),
        }
    }
}

/// The name a class is written as.
pub fn class_name(c: ItemClass) -> &'static str {
    match c {
        ItemClass::Standard => "standard",
        ItemClass::CrownJewel => "crown_jewel",
    }
}

/// The name a state is written as (a tombstone is never written).
pub fn state_name(s: ItemState) -> &'static str {
    match s {
        ItemState::Live => "live",
        ItemState::Archived => "archived",
        ItemState::Trashed => "trashed",
        ItemState::Tombstone => "purged",
    }
}

impl Document {
    /// An empty document stamped now.
    pub fn new(exported_at: u64, exporter: &str) -> Self {
        Self {
            format: FORMAT_NAME.into(),
            v: DOC_V,
            exported_at,
            exporter: exporter.into(),
            items: Vec::new(),
            left_out: Vec::new(),
            vaults: Vec::new(),
        }
    }

    /// The document as JSON bytes (wiped when dropped).
    pub fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>> {
        serde_json::to_vec_pretty(self)
            .map(Zeroizing::new)
            .map_err(|e| CxfError::Parse {
                format: "Lockdown backup",
                why: e.to_string(),
            })
    }

    /// Parses and checks a document.
    pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Self> {
        limits.check_size(bytes)?;
        let doc: Self =
            serde_json::from_slice(bytes).map_err(crate::import::json_err("Lockdown backup"))?;
        if doc.format != FORMAT_NAME {
            return Err(CxfError::Parse {
                format: "Lockdown backup",
                why: format!("the format says '{}', not '{FORMAT_NAME}'", doc.format),
            });
        }
        if doc.v > DOC_V {
            return Err(CxfError::Unsupported(format!(
                "this backup is version {} of the format; this build reads up to {DOC_V}; update Lockdown",
                doc.v
            )));
        }
        if doc.items.len() > limits.max_items {
            return Err(CxfError::TooMany {
                what: "items",
                found: doc.items.len(),
                limit: limits.max_items,
            });
        }
        Ok(doc)
    }

    /// The items, ready to land, with each one's id, class and state kept.
    pub fn to_items(&self, limits: &Limits) -> Parsed {
        let mut parsed = Parsed::default();
        for it in &self.items {
            match one(it, limits) {
                Ok(item) => parsed.items.push(item),
                Err(why) => parsed.problems.push(format!(
                    "an item ({}) didn't read and was left out: {why}",
                    it.kind
                )),
            }
        }
        for v in &self.vaults {
            for f in &v.folders {
                match Id::parse(&f.id) {
                    Ok(id) => parsed.folders.push(crate::import::KeptFolder {
                        id,
                        name: f.name.clone(),
                        parent: f.parent.as_deref().and_then(|p| Id::parse(p).ok()),
                    }),
                    Err(_) => parsed.problems.push(format!(
                        "the folder '{}' has no valid id and was left out",
                        f.name
                    )),
                }
            }
        }
        if !self.left_out.is_empty() {
            let names: Vec<&str> = self.left_out.iter().map(|l| l.title.as_str()).collect();
            parsed.problems.push(format!(
                "{} items were left out when this file was made: {}",
                self.left_out.len(),
                names.join(", ")
            ));
        }
        parsed
    }
}

fn one(it: &DocItem, limits: &Limits) -> std::result::Result<ImportedItem, String> {
    let kind = ItemKind::ALL
        .into_iter()
        // The person's own templates come back too (D232); the app's
        // other hidden records never travel in a backup.
        .find(|k| k.as_str() == it.kind && (!k.hidden() || *k == ItemKind::Template))
        .ok_or_else(|| format!("'{}' isn't a kind this build knows", it.kind))?;
    let class = match it.class.as_str() {
        "standard" => ItemClass::Standard,
        "crown_jewel" => ItemClass::CrownJewel,
        other => return Err(format!("'{other}' isn't a class")),
    };
    let state = match it.state.as_str() {
        "live" => ItemState::Live,
        "archived" => ItemState::Archived,
        "trashed" => ItemState::Trashed,
        other => return Err(format!("'{other}' isn't a state an item can come back in")),
    };
    let item_id = Id::parse(&it.id).map_err(|_| "its id isn't a UUID".to_string())?;
    let fields: Vec<PlainField> = it
        .fields
        .iter()
        .map(DocField::to_plain)
        .collect::<std::result::Result<_, _>>()?;
    let title = fields
        .iter()
        .find(|f| f.field_id == "title")
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            _ => None,
        })
        .ok_or("it has no title")?;
    let mut history = Vec::with_capacity(it.history.len());
    for v in &it.history {
        history.push(
            v.fields
                .iter()
                .map(DocField::to_plain)
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    let mut attachments = Vec::with_capacity(it.attachments.len());
    let mut report = Vec::new();
    for a in &it.attachments {
        let bytes = Zeroizing::new(from_b64url(&a.data).map_err(|e| e.to_string())?);
        if bytes.len() > limits.max_attachment_bytes {
            report.push(format!(
                "the file '{}' is over the size limit and was left out",
                a.name
            ));
            continue;
        }
        if blake3::hash(&bytes).to_hex().as_str() != a.blake3 || bytes.len() as u64 != a.size {
            return Err(format!("the file '{}' doesn't match its checksum", a.name));
        }
        attachments.push(ImportedAttachment {
            name: a.name.clone(),
            mime: a.mime.clone(),
            bytes,
        });
    }
    let mut item = ImportedItem::new(it.id.clone(), kind, title);
    item.fields = fields;
    item.history = history;
    item.attachments = attachments;
    item.report = report;
    item.archived = state == ItemState::Archived;
    item.created_at = it.created_at.map(|t| t.max(0) as u64);
    item.modified_at = it.modified_at.map(|t| t.max(0) as u64);
    item.kept = Some(Kept {
        item_id,
        class,
        state,
    });
    Ok(item)
}

// --- the sealed file ------------------------------------------------------

/// The key-derivation part of a sealed file's header.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Kdf {
    /// Always `argon2id`.
    pub alg: String,
    /// Memory in KiB.
    pub m_kib: u32,
    /// Passes.
    pub t: u32,
    /// Lanes.
    pub p: u32,
    /// The salt, base64url, 16 bytes or more.
    pub salt: String,
}

impl Kdf {
    /// Fresh settings: this build's Argon2id floor and a random salt.
    pub fn fresh() -> Result<Self> {
        let p = lockdown_crypto::kdf::floor();
        let salt =
            lockdown_crypto::rand::random_16().map_err(|e| CxfError::Unsupported(e.to_string()))?;
        Ok(Self {
            alg: "argon2id".into(),
            m_kib: p.m_cost_kib,
            t: p.t_cost,
            p: p.p_cost,
            salt: b64url(&salt),
        })
    }

    fn check(&self) -> Result<Vec<u8>> {
        if self.alg != "argon2id" {
            return Err(CxfError::Unsupported(format!(
                "the key derivation '{}'",
                self.alg
            )));
        }
        // Readers take anything from the smallest platform floor (the
        // browser's) up to the caps, so a file from a phone opens here.
        let floor = lockdown_crypto::kdf::floor();
        let min = Argon2Params {
            m_cost_kib: floor.m_cost_kib.min(32 * 1024),
            t_cost: floor.t_cost.min(3),
            p_cost: 1,
        };
        let asked = Argon2Params {
            m_cost_kib: self.m_kib,
            t_cost: self.t,
            p_cost: self.p,
        };
        if !asked.meets(&min) {
            return Err(CxfError::Unsupported(format!(
                "the file's key derivation ({asked}) is weaker than Lockdown accepts"
            )));
        }
        if self.m_kib > MAX_M_KIB || self.t > MAX_T || self.p > MAX_P {
            return Err(CxfError::Unsupported(format!(
                "the file asks for a key derivation heavier than Lockdown allows ({asked})"
            )));
        }
        let salt = from_b64url(&self.salt)?;
        if salt.len() < 16 || salt.len() > 64 {
            return Err(CxfError::Parse {
                format: "Lockdown backup",
                why: "the salt must be 16 to 64 bytes".into(),
            });
        }
        Ok(salt)
    }

    /// The file key for `password` under these settings.
    pub fn derive(&self, password: &[u8]) -> Result<FileKey> {
        let salt = self.check()?;
        let params = argon2::Params::new(self.m_kib, self.t, self.p, Some(32))
            .map_err(|e| CxfError::Unsupported(format!("Argon2id settings: {e}")))?;
        let mut out = Zeroizing::new([0u8; 32]);
        argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
            .hash_password_into(password, &salt, out.as_mut())
            .map_err(|e| CxfError::Unsupported(format!("Argon2id: {e}")))?;
        let key = lockdown_crypto::kdf::subkey(out.as_ref(), KEY_INFO)
            .map_err(|e| CxfError::Unsupported(e.to_string()))?;
        Ok(FileKey(key))
    }
}

/// The key a sealed file is encrypted under, wiped when dropped. Kept
/// (sealed under the vault's key) by automatic backups, so a scheduled
/// backup never needs the password again.
pub struct FileKey(Zeroizing<[u8; 32]>);

impl FileKey {
    /// From raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The raw bytes, to seal for keeping.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for FileKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileKey(..)")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Aead_ {
    alg: String,
    chunk: usize,
    nonce_prefix: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Header {
    v: u8,
    kdf: Kdf,
    aead: Aead_,
    payload: String,
    created_at: u64,
}

/// Whether `bytes` start like a sealed Lockdown file.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn nonce(prefix: &[u8], i: u32, last: bool) -> XNonce {
    let mut n = [0u8; 24];
    n[..NONCE_PREFIX].copy_from_slice(prefix);
    n[NONCE_PREFIX..NONCE_PREFIX + 4].copy_from_slice(&i.to_be_bytes());
    n[23] = u8::from(last);
    XNonce::from(n)
}

/// Seals `plain` (a document's bytes) under `key`, whose settings are
/// `kdf`, into a sealed file.
pub fn seal(plain: &[u8], key: &FileKey, kdf: &Kdf, created_at: u64) -> Result<Vec<u8>> {
    let mut prefix = [0u8; NONCE_PREFIX];
    lockdown_crypto::rand::random_bytes(&mut prefix)
        .map_err(|e| CxfError::Unsupported(e.to_string()))?;
    let header = Header {
        v: SEALED_V,
        kdf: kdf.clone(),
        aead: Aead_ {
            alg: "xchacha20poly1305".into(),
            chunk: CHUNK,
            nonce_prefix: b64url(&prefix),
        },
        payload: "lockdown-backup+json".into(),
        created_at,
    };
    let header_json = serde_json::to_vec(&header).map_err(|e| CxfError::Parse {
        format: "Lockdown backup",
        why: e.to_string(),
    })?;
    let mut out = Vec::with_capacity(plain.len() + plain.len() / CHUNK * TAG + 256);
    out.extend_from_slice(MAGIC);
    out.push(SEALED_V);
    out.extend_from_slice(&(header_json.len() as u16).to_be_bytes());
    out.extend_from_slice(&header_json);
    let aad = out.clone();
    let cipher = XChaCha20Poly1305::new(&chacha20poly1305::Key::from(*key.expose()));
    let chunks: Vec<&[u8]> = if plain.is_empty() {
        vec![&[][..]]
    } else {
        plain.chunks(CHUNK).collect()
    };
    let last = chunks.len() - 1;
    for (i, c) in chunks.into_iter().enumerate() {
        let i32_ = u32::try_from(i).map_err(|_| CxfError::TooLarge {
            found: plain.len(),
            limit: CHUNK.saturating_mul(u32::MAX as usize),
        })?;
        let ct = cipher
            .encrypt(
                &nonce(&prefix, i32_, i == last),
                Payload { msg: c, aad: &aad },
            )
            .map_err(|_| CxfError::Unsupported("couldn't seal the file".into()))?;
        out.extend_from_slice(&ct);
    }
    Ok(out)
}

/// The settings a sealed file's header names, without opening it.
pub fn kdf_of(bytes: &[u8]) -> Result<Kdf> {
    Ok(read_header(bytes)?.0.kdf)
}

fn read_header(bytes: &[u8]) -> Result<(Header, usize)> {
    let bad = |why: &str| CxfError::Parse {
        format: "Lockdown backup",
        why: why.into(),
    };
    if !is_sealed(bytes) || bytes.len() < 11 {
        return Err(bad("this isn't a sealed Lockdown file"));
    }
    if bytes[8] != SEALED_V {
        return Err(CxfError::Unsupported(format!(
            "sealed-file version {}; this build reads version {SEALED_V}",
            bytes[8]
        )));
    }
    let len = u16::from_be_bytes([bytes[9], bytes[10]]) as usize;
    if len > MAX_HEADER {
        return Err(bad("the header is too long"));
    }
    let end = 11 + len;
    let raw = bytes
        .get(11..end)
        .ok_or_else(|| bad("the header is cut short"))?;
    let header: Header =
        serde_json::from_slice(raw).map_err(|e| bad(&format!("the header: {e}")))?;
    if header.v != SEALED_V || header.aead.alg != "xchacha20poly1305" {
        return Err(CxfError::Unsupported(format!(
            "the cipher '{}'",
            header.aead.alg
        )));
    }
    if header.aead.chunk != CHUNK {
        return Err(bad("the chunk size isn't 1 MiB"));
    }
    Ok((header, end))
}

/// Opens a sealed file with `password`.
pub fn open(bytes: &[u8], password: &[u8], limits: &Limits) -> Result<Zeroizing<Vec<u8>>> {
    limits.check_size(bytes)?;
    let (header, _) = read_header(bytes)?;
    let key = header.kdf.derive(password)?;
    open_with(bytes, &key, limits)
}

/// Opens a sealed file with a key already derived.
pub fn open_with(bytes: &[u8], key: &FileKey, limits: &Limits) -> Result<Zeroizing<Vec<u8>>> {
    limits.check_size(bytes)?;
    let (header, start) = read_header(bytes)?;
    let prefix = from_b64url(&header.aead.nonce_prefix)?;
    if prefix.len() != NONCE_PREFIX {
        return Err(CxfError::Parse {
            format: "Lockdown backup",
            why: "the nonce prefix isn't 19 bytes".into(),
        });
    }
    let aad = &bytes[..start];
    let body = &bytes[start..];
    let cipher = XChaCha20Poly1305::new(&chacha20poly1305::Key::from(*key.expose()));
    let frame = CHUNK + TAG;
    let mut out = Zeroizing::new(Vec::with_capacity(body.len()));
    let mut at = 0usize;
    let mut i: u32 = 0;
    loop {
        let rest = body.len() - at;
        if rest < TAG {
            return Err(CxfError::Tampered(
                "the file ends before its last part".into(),
            ));
        }
        let last = rest <= frame;
        let take = if last { rest } else { frame };
        let ct = &body[at..at + take];
        let plain = cipher
            .decrypt(&nonce(&prefix, i, last), Payload { msg: ct, aad })
            .map_err(|_| {
                if i == 0 {
                    CxfError::WrongPassword
                } else {
                    CxfError::Tampered(format!("part {} didn't check", i + 1))
                }
            })?;
        out.extend_from_slice(&plain);
        drop(Zeroizing::new(plain));
        at += take;
        if last {
            break;
        }
        i = i
            .checked_add(1)
            .ok_or_else(|| CxfError::Tampered("too many parts".into()))?;
    }
    Ok(out)
}

/// Reads a Lockdown file, sealed or plain, into items.
pub fn read(bytes: &[u8], password: Option<&[u8]>, limits: &Limits) -> Result<Parsed> {
    let limits = Limits {
        max_bytes: limits.max_bytes.max(Limits::backup().max_bytes),
        ..*limits
    };
    if crate::age_file::is_age(bytes) {
        let password = password.ok_or(CxfError::PasswordNeeded)?;
        let plain = crate::age_file::open_with(bytes, password, &limits)?;
        return Ok(Document::parse(&plain, &limits)?.to_items(&limits));
    }
    let doc = if is_sealed(bytes) {
        let password = password.ok_or(CxfError::PasswordNeeded)?;
        let plain = open(bytes, password, &limits)?;
        Document::parse(&plain, &limits)?
    } else {
        Document::parse(bytes, &limits)?
    };
    Ok(doc.to_items(&limits))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let mut d = Document::new(1_760_000_000, "Lockdown test");
        d.items.push(DocItem {
            id: Id::new().to_string(),
            kind: "login".into(),
            class: "standard".into(),
            state: "live".into(),
            created_at: Some(1),
            modified_at: Some(2),
            fields: vec![
                DocField::from_plain(&PlainField::text("title", "title", "Forum")),
                DocField::from_plain(&PlainField::hidden("pw", "Password", "s3cret")),
                DocField::from_plain(&PlainField::url(
                    "url.0",
                    "https://forum.example",
                    "base_domain",
                )),
            ],
            history: vec![DocVersion {
                n: 1,
                edited_at: 1,
                fields: vec![DocField::from_plain(&PlainField::text(
                    "title", "title", "Forum",
                ))],
            }],
            attachments: vec![DocAttachment::new("a.txt", "text/plain", b"hello")],
            vault: None,
        });
        d
    }

    #[test]
    fn a_sealed_file_opens_with_its_password_and_nothing_else() {
        let d = doc();
        let plain = d.to_bytes().unwrap();
        let kdf = Kdf::fresh().unwrap();
        let key = kdf.derive(b"correct horse").unwrap();
        let file = seal(&plain, &key, &kdf, 5).unwrap();
        assert!(is_sealed(&file));
        assert!(!String::from_utf8_lossy(&file).contains("s3cret"));
        let back = open(&file, b"correct horse", &Limits::default()).unwrap();
        assert_eq!(back.as_slice(), plain.as_slice());
        assert!(matches!(
            open(&file, b"wrong", &Limits::default()),
            Err(CxfError::WrongPassword)
        ));
        // The header is bound: a changed cost is caught as a wrong key.
        let mut changed = file.clone();
        let at = 11 + 5;
        changed[at] ^= 1;
        assert!(open_with(&changed, &key, &Limits::default()).is_err());
        // Cut short, or one byte flipped in the body.
        let cut = &file[..file.len() - 3];
        assert!(open_with(cut, &key, &Limits::default()).is_err());
        let mut flipped = file.clone();
        let n = flipped.len() - 1;
        flipped[n] ^= 0x80;
        assert!(open_with(&flipped, &key, &Limits::default()).is_err());
        // The items come back with their ids, classes and history.
        let parsed = read(&file, Some(b"correct horse"), &Limits::default()).unwrap();
        let item = &parsed.items[0];
        assert_eq!(item.kept.unwrap().item_id.to_string(), d.items[0].id);
        assert_eq!(item.history.len(), 1);
        assert_eq!(item.attachments[0].bytes.as_slice(), b"hello");
        assert!(matches!(
            read(&file, None, &Limits::default()),
            Err(CxfError::PasswordNeeded)
        ));
    }

    #[test]
    fn many_chunks_round_trip_and_a_dropped_chunk_is_caught() {
        let plain = vec![7u8; CHUNK * 2 + 10];
        let kdf = Kdf::fresh().unwrap();
        let key = kdf.derive(b"pw").unwrap();
        let file = seal(&plain, &key, &kdf, 1).unwrap();
        let back = open_with(&file, &key, &Limits::backup()).unwrap();
        assert_eq!(back.len(), plain.len());
        let start = file.len() - (10 + TAG) - (CHUNK + TAG) * 2;
        // Drop the middle chunk.
        let mut dropped = file[..start + CHUNK + TAG].to_vec();
        dropped.extend_from_slice(&file[start + 2 * (CHUNK + TAG)..]);
        assert!(matches!(
            open_with(&dropped, &key, &Limits::backup()),
            Err(CxfError::Tampered(_))
        ));
        // Drop the last: the new last is not flagged last.
        let truncated = &file[..start + 2 * (CHUNK + TAG)];
        assert!(open_with(truncated, &key, &Limits::backup()).is_err());
        // An exact multiple of the chunk size, and an empty document.
        for n in [CHUNK, 0] {
            let p = vec![1u8; n];
            let f = seal(&p, &key, &kdf, 1).unwrap();
            assert_eq!(open_with(&f, &key, &Limits::backup()).unwrap().len(), n);
        }
    }

    #[test]
    fn hostile_headers_are_refused_before_any_work() {
        let kdf = Kdf {
            m_kib: 64 * 1024 * 1024,
            ..Kdf::fresh().unwrap()
        };
        assert!(kdf.derive(b"x").is_err(), "64 GiB of memory");
        let weak = Kdf {
            m_kib: 8,
            t: 1,
            p: 1,
            ..Kdf::fresh().unwrap()
        };
        assert!(weak.derive(b"x").is_err());
        for bytes in [
            &b"LDBACKUP"[..],
            b"LDBACKUP\x01\xff\xff{}",
            b"LDBACKUP\x02\x00\x02{}",
            b"LDBACKUP\x01\x00\x02{}",
        ] {
            assert!(open(bytes, b"x", &Limits::default()).is_err());
        }
        // A plain document of another format or version.
        let mut d = doc();
        d.v = 99;
        let bytes = d.to_bytes().unwrap();
        assert!(matches!(
            Document::parse(&bytes, &Limits::default()),
            Err(CxfError::Unsupported(_))
        ));
        // A value with two members, an attachment with a wrong checksum.
        let mut d = doc();
        d.items[0].fields[1].value.flag = Some(true);
        d.items[0].attachments[0].blake3 = "00".into();
        let parsed = d.to_items(&Limits::default());
        assert!(parsed.items.is_empty());
        assert_eq!(parsed.problems.len(), 1, "{:?}", parsed.problems);
    }
}
