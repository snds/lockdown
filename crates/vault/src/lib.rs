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
//! - [`attachment`] — files in 1 MiB chunks under per-file keys, named by ciphertext hash
//! - [`field`] — plain and sealed fields and the field associated data
//! - [`item`] — the item envelope, kinds, classes, states, versions
//! - [`crown_jewel`] — the crown-jewel rules and default classification
//! - [`template`] — every kind's fields as data: control, required, crown jewel, fill rule, generator
//! - [`exposure`] — per-item exposure tiers: app only, ask each time, fill freely
//! - [`device_link`] — approving a new device from an enrolled one, through the server's relay
//! - [`handover`] — the record changes a device approval needs, and Lockdown's profile fields
//! - [`pq`] — every new wrap to another key is post-quantum hybrid
//! - [`compose`] — a shell's add or edit checked against the template, with an explicit keep for secrets
//! - [`manifest`] — the signed, hash-chained manifest head and chain check
//! - [`organization`] — folders, tags and favourites; the vault's name and folder tree in a hidden item
//! - [`signer`] — who signs heads and versions: the device's key
//! - [`store`] — the local store contract a vault loads from and saves to
//! - [`vault`] — [`Vault`]: create, update, open, purge, commit heads, load, save
//! - [`identity`] — the adoption path: open or create the ecosystem identity,
//!   the persona's Lockdown keys, and a vault that opens from the identity alone
//! - [`identity_set`] — the identity's signed records under one hash-chained
//!   manifest, as they travel between machines, with device revocations
//! - [`transparency`] — the key-transparency log's leaves, signed tree heads,
//!   the citation a vault head carries, and the exported checkpoint

pub mod attachment;
pub mod compose;
pub mod contact;
pub mod crown_jewel;
pub mod device_link;
pub mod error;
pub mod exposure;
pub mod field;
pub mod handover;
pub mod identity;
pub mod identity_set;
pub mod item;
pub mod keys;
pub mod manifest;
pub mod organization;
pub mod phrase_check;
pub mod pq;
pub mod send;
pub mod sharing;
pub mod signer;
pub mod store;
pub mod template;
pub mod test_support;
pub mod transparency;
pub mod vault;

/// Identifiers and timestamps are the identity crate's, so an `Id` here is
/// the same `Id` in a profile or persona record.
pub use ecosystem_identity::records::{Id, Timestamp};
pub use lockdown_crypto::Hash32;

pub use attachment::{
    AttachmentMeta, AttachmentPlace, CHUNK_SIZE, SealedAttachment, open_attachment, padded_size,
    seal_attachment,
};
pub use compose::{Change, FieldChange, FieldInput, compose_edit, compose_new, is_reserved};
pub use crown_jewel::{
    CROWN_JEWEL_RULES, Classify, CrownJewelDefault, LoginRole, crown_jewel_default,
};
pub use error::{Error, Result};
pub use field::{FieldAad, FieldKind, FieldValue, PlainField, SCHEMA_V, SealedField};
pub use identity_set::{IdentityManifest, RecordRef, RecordSet, Revocation, VaultBoundary};
pub use item::{AttachmentRef, Item, ItemClass, ItemHeader, ItemKind, ItemState, ItemVersion};
pub use keys::{
    AttachmentKey, ItemKey, KeyPlace, KeyPurpose, VaultKey, VersionKey, WrappedBy, WrappedKey,
    WrapperKind, unwrap_key, wrap_key,
};
pub use manifest::{
    Checkpoint, ManifestEntry, ManifestHead, Member, check_revocations, verify_chain,
};
pub use organization::{Folder, Organization, VaultInfo, is_org_field};
pub use signer::Signer;
pub use store::{Store, StoreError, StoreResult, VAULT_RECORD_V, VaultRecord};
pub use template::{
    COMMON, CUSTOM_KINDS, Control, FieldTemplate, Fill, SIGN_IN_METHODS, TEMPLATES, Template,
    common_field, template_for,
};
pub use transparency::{LogEntry, LogLeaf, SignedTreeHead, SthRef, TreeHead};
pub use vault::{
    AttachmentInfo, EXPORT_OPT_IN, ImportSpec, LEGACY_OPT_IN, NewAttachment, Unlock, Vault,
    VaultContext, export_opted_in, is_reserved_field, legacy_opted_in, member_record,
};
