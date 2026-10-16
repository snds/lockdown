//! The item envelope (02-model §4, §7, §12).
//!
//! Every item has one kind and a schema version, its own Item Key, and a
//! history of versions each under its own Version Key. The envelope on the
//! wire carries the latest version; the local model keeps the whole
//! history so a version can be restored or shredded.

use lockdown_crypto::aead::Sealed;
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::hash;
use lockdown_crypto::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::field::SealedField;
use crate::keys::WrappedKey;
use crate::{Error, Hash32, Id, Result, Timestamp};

/// The item envelope format version.
pub const ITEM_V: u16 = 1;

/// The item types (02-model §4). Each is schema-versioned, so an old client
/// refuses an item it cannot read rather than guessing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    /// The hub type: every way into one account.
    Login,
    /// A standalone passkey (embedded in a Login when it has one).
    Passkey,
    /// A one-time code set-up.
    Code,
    /// A Wi-Fi network.
    WiFi,
    /// An SSH key. Never autofilled.
    SshKey,
    /// An API key or token.
    ApiCredential,
    /// A document with attachments and expiry.
    Document,
    /// Person, address, business, vehicle, passport, licence: used for form fill.
    Identity,
    /// A payment card. Second deliberate action before any fill.
    Card,
    /// A bank account. Never autofilled.
    BankAccount,
    /// A Markdown note; the one type that allows CRDT merging in the body.
    Note,
    /// A crypto wallet; the seed is crown-jewel by default.
    CryptoWallet,
    /// Plain template: a server.
    Server,
    /// Plain template: a database.
    Database,
    /// Plain template: a software licence.
    SoftwareLicence,
    /// Plain template: a membership.
    Membership,
    /// A user-defined template.
    Custom,
    /// Hidden: a consent record for a tolerated bridge, so it lives in the chain.
    Consent,
}

impl ItemKind {
    /// Every kind, for template lists and tests.
    pub const ALL: [ItemKind; 18] = [
        Self::Login,
        Self::Passkey,
        Self::Code,
        Self::WiFi,
        Self::SshKey,
        Self::ApiCredential,
        Self::Document,
        Self::Identity,
        Self::Card,
        Self::BankAccount,
        Self::Note,
        Self::CryptoWallet,
        Self::Server,
        Self::Database,
        Self::SoftwareLicence,
        Self::Membership,
        Self::Custom,
        Self::Consent,
    ];

    /// Whether the extension and OS providers may ever offer this kind to
    /// fill (02-model §4 "never autofilled").
    pub fn autofill_allowed(self) -> bool {
        !matches!(self, Self::SshKey | Self::BankAccount | Self::Consent)
    }

    /// Whether the person sees this kind in lists.
    pub fn hidden(self) -> bool {
        matches!(self, Self::Consent)
    }

    /// The word used in exports and messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Passkey => "passkey",
            Self::Code => "code",
            Self::WiFi => "wi-fi",
            Self::SshKey => "ssh-key",
            Self::ApiCredential => "api-credential",
            Self::Document => "document",
            Self::Identity => "identity",
            Self::Card => "card",
            Self::BankAccount => "bank-account",
            Self::Note => "note",
            Self::CryptoWallet => "crypto-wallet",
            Self::Server => "server",
            Self::Database => "database",
            Self::SoftwareLicence => "software-licence",
            Self::Membership => "membership",
            Self::Custom => "custom",
            Self::Consent => "consent",
        }
    }
}

/// Which key tree branch an item's key hangs from (02-model §3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    /// Item Key wrapped by the Vault Key.
    #[default]
    Standard,
    /// Item Key wrapped by the persona's Crown-Jewel Key.
    CrownJewel,
}

/// Where an item is in its life (02-model §7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// Normal.
    #[default]
    Live,
    /// Out of autofill, agents and default search; kept in exports.
    Archived,
    /// Soft-deleted, in its grace period.
    Trashed,
    /// Purged: keys destroyed, only the last version hash remains.
    Tombstone,
}

/// One file attached to an item (02-model §6). The file's chunks live in
/// blob storage, named by the hash of the ciphertext.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AttachmentRef {
    /// The attachment.
    pub att_id: Id,
    /// Its key, wrapped by the Item Key. `None` once shredded.
    pub att_key: Option<WrappedKey>,
    /// How many 1 MiB chunks.
    pub chunks: u32,
    /// BLAKE3 of the whole ciphertext: the blob name.
    pub blob: Hash32,
    /// Sealed name, type and size.
    pub meta: Sealed,
}

/// One saved version of an item.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ItemVersion {
    /// Rises by one on every save.
    pub n: u64,
    /// This version's key, wrapped by the Item Key. `None` once shredded.
    pub version_key: Option<WrappedKey>,
    /// Hash of the previous version; zero for the first.
    pub parent_hash: Hash32,
    /// The device that saved it.
    pub edited_by: Id,
    /// When.
    pub edited_at: Timestamp,
    /// The sealed fields.
    pub fields: Vec<SealedField>,
    /// The attachments.
    pub attachments: Vec<AttachmentRef>,
    /// Ed25519 by the editing device over everything above.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

impl ItemVersion {
    /// The version with every key envelope removed. The signature and the
    /// hash are taken over this, so shredding a key (02-model §7) never
    /// rewrites history or breaks a signature. The envelopes need no cover
    /// from the signature: each is sealed with associated data naming its
    /// place, and a swapped envelope yields a key that opens nothing.
    fn without_keys(&self) -> ItemVersion {
        let mut copy = self.clone();
        copy.version_key = None;
        for a in &mut copy.attachments {
            a.att_key = None;
        }
        copy
    }

    /// The bytes the signature covers: canonical CBOR with `sig` empty and
    /// the key envelopes removed.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.without_keys();
        copy.sig.clear();
        Ok(to_canonical(&copy)?)
    }

    /// Signs the version with the editing device's key.
    pub fn sign(&mut self, key: &SigningKey) -> Result<()> {
        self.sig = key.sign(&self.signable()?).to_vec();
        Ok(())
    }

    /// Checks the device signature.
    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        Ok(key.verify(
            &self.signable()?,
            &self.sig,
            &format!("item version {}", self.n),
        )?)
    }

    /// BLAKE3 of the signed version without its key envelopes: what the
    /// manifest records and the next version names as `parent_hash`. Stable
    /// across shredding, so a tombstone keeps the last version hash.
    pub fn hash(&self) -> Result<Hash32> {
        Ok(hash(&to_canonical(&self.without_keys())?))
    }

    /// Whether this version's key was destroyed.
    pub fn is_shredded(&self) -> bool {
        self.version_key.is_none()
    }
}

/// The item envelope.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Item {
    /// Envelope format version.
    pub v: u16,
    /// The item.
    pub item_id: Id,
    /// The vault it is in.
    pub vault_id: Id,
    /// Server-assigned revision (03-architecture §6). Zero until first synced.
    pub rev: u64,
    /// The type.
    pub kind: ItemKind,
    /// The type's schema version.
    pub schema_v: u16,
    /// Standard or crown-jewel.
    pub class: ItemClass,
    /// Never syncs; the manifest holds a tombstone saying so.
    pub device_only: bool,
    /// The Item Key, wrapped by the Vault Key or the Crown-Jewel Key. `None`
    /// once purged.
    pub item_key: Option<WrappedKey>,
    /// Every saved version, oldest first.
    pub versions: Vec<ItemVersion>,
    /// Where it is in its life.
    pub state: ItemState,
    /// When it was trashed, if it was.
    pub trashed_at: Option<Timestamp>,
}

impl Item {
    /// The newest version.
    pub fn latest(&self) -> Result<&ItemVersion> {
        self.versions
            .last()
            .ok_or_else(|| Error::NotFound(format!("a version of item {}", self.item_id)))
    }

    /// Version `n`.
    pub fn version(&self, n: u64) -> Result<&ItemVersion> {
        self.versions
            .iter()
            .find(|v| v.n == n)
            .ok_or_else(|| Error::NotFound(format!("version {n} of item {}", self.item_id)))
    }

    /// Whether every key is gone.
    pub fn is_shredded(&self) -> bool {
        self.item_key.is_none() && self.versions.iter().all(ItemVersion::is_shredded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_the_never_autofill_list_holds() {
        for k in ItemKind::ALL {
            let json = serde_json_like(k);
            assert_eq!(json, format!("\"{}\"", k.as_str()), "{k:?}");
        }
        assert!(!ItemKind::SshKey.autofill_allowed());
        assert!(!ItemKind::BankAccount.autofill_allowed());
        assert!(ItemKind::Login.autofill_allowed());
        assert!(ItemKind::Consent.hidden());
    }

    /// The serde name as a JSON string, through CBOR text (no serde_json here).
    fn serde_json_like(k: ItemKind) -> String {
        let bytes = to_canonical(&k).unwrap();
        let v: ciborium_text::Text = lockdown_crypto::cbor::from_slice(&bytes).unwrap();
        format!("\"{}\"", v.0)
    }

    mod ciborium_text {
        #[derive(serde::Deserialize)]
        #[serde(transparent)]
        pub struct Text(pub String);
    }
}
