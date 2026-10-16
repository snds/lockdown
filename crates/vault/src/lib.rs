//! `lockdown-vault`: vaults, items, fields, versions, the crown-jewel class
//! and the signed manifest chain (03-architecture §2 `vault` row;
//! `docs/plan/02-model.md` throughout; 04-security §5).
//!
//! A vault is one random 256-bit Vault Key plus the items it locks. Every
//! item has its own Item Key; every saved version its own Version Key;
//! every field is sealed on its own with XChaCha20-Poly1305 and associated
//! data naming `{profile_id, persona_id, vault_id, item_id, version_n,
//! field_id, kind, schema_v}`. A server that moves a ciphertext between
//! fields or items gets a tag failure, not a swapped value.
//!
//! ```text
//! Persona Key ─wraps─▶ Lockdown App Data Key ─wraps─▶ Vault Key ─wraps─▶ Item Key ─wraps─▶ Version Key ─encrypts─▶ fields
//!                                                                           └─wraps─▶ Attachment Key (one per file)
//! Persona Key ─wraps─▶ Crown-Jewel Key ─wraps─▶ Item Key of every crown-jewel item
//! ```
//!
//! The keys above the Vault Key belong to the identity crate; this crate
//! starts at the Vault Key. Deleting a version destroys its key, so the
//! ciphertext is dead wherever it was copied ("crypto-shredding"). A
//! signed, hash-chained [`ManifestHead`] per vault is the table of contents
//! that stops a hostile server rolling anything back.
//!
//! - [`keys`] — the four key kinds and the wrap that binds a key to its place
//! - [`field`] — plain and sealed fields and the field associated data
//! - [`item`] — the item envelope, kinds, classes, states, versions
//! - [`crown_jewel`] — the crown-jewel rules and default classification
//! - [`manifest`] — the signed, hash-chained manifest head and chain check
//! - [`vault`] — [`Vault`]: create, update, open, purge, commit heads

pub mod crown_jewel;
pub mod error;
pub mod field;
pub mod item;
pub mod keys;
pub mod manifest;
pub mod test_support;
pub mod vault;

/// Identifiers and timestamps are the identity crate's, so an `Id` here is
/// the same `Id` in a profile or persona record.
pub use ecosystem_identity::records::{Id, Timestamp};
pub use lockdown_crypto::Hash32;

pub use crown_jewel::{
    CROWN_JEWEL_RULES, Classify, CrownJewelDefault, LoginRole, crown_jewel_default,
};
pub use error::{Error, Result};
pub use field::{FieldAad, FieldKind, FieldValue, PlainField, SCHEMA_V, SealedField};
pub use item::{AttachmentRef, Item, ItemClass, ItemKind, ItemState, ItemVersion};
pub use keys::{
    AttachmentKey, ItemKey, KeyPlace, KeyPurpose, VaultKey, VersionKey, WrappedBy, WrappedKey,
    WrapperKind, unwrap_key, wrap_key,
};
pub use manifest::{Checkpoint, ManifestEntry, ManifestHead, Member, verify_chain};
pub use vault::{Vault, VaultContext};
