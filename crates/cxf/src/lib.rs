//! `lockdown-cxf`: import and export (03-architecture §2 `cxf` row;
//! 02-model §11; brief §2.9 point 8 "hostile imports").
//!
//! - [`model`] — the FIDO Credential Exchange Format (CXF) v1.0 JSON shape:
//!   <https://fidoalliance.org/specs/cx/cxf-v1.0-ps-20250814.html>
//!   (Proposed Standard, 14 August 2025; the 2026-03 errata are tracked in
//!   02-model §13 question 6).
//! - [`convert`] — CXF ↔ Lockdown items for logins (`basic-auth`), passkeys,
//!   notes, credit cards, identities (`person-name` + `address`) and codes
//!   (`totp`). Anything that does not map is reported, never dropped silently.
//! - [`csv`] — a small, never-panicking RFC 4180 reader and an
//!   injection-safe writer, with a login importer over the common header
//!   names. The reader is one pure function over bytes, the fuzz target.
//! - [`import`] — the other managers' files (1Password 1PUX, Bitwarden JSON
//!   plain and password-protected, Proton Pass, KeePass KDBX, Dashlane,
//!   Keeper, LastPass, Enpass), each read into the same items through
//!   [`build::Builder`], with what it couldn't carry in words.
//! - [`archive`] — Lockdown's own backup format (`docs/format/backup-v1.md`):
//!   the JSON document and the password-sealed container around it.
//! - [`kdbx_out`] — a KeePass KDBX 4 file of the vault, the exit path.
//!
//! Every parser is size-capped by [`Limits`] and treats the file as hostile.

pub mod archive;
pub mod build;
pub mod convert;
pub mod csv;
pub mod import;
pub mod kdbx_out;
pub mod model;
mod unzip;

pub use convert::{ImportedAttachment, ImportedItem, Kept, from_items, to_items};
pub use model::{Account, Credential, EditableField, Header, Item};

/// What can go wrong on import or export.
#[derive(Debug, thiserror::Error)]
pub enum CxfError {
    /// The file is bigger than the cap.
    #[error("the file is {found} bytes; the limit is {limit}")]
    TooLarge {
        /// Bytes found.
        found: usize,
        /// The cap.
        limit: usize,
    },
    /// Too many of something.
    #[error("too many {what}: {found}, the limit is {limit}")]
    TooMany {
        /// What there were too many of.
        what: &'static str,
        /// How many.
        found: usize,
        /// The cap.
        limit: usize,
    },
    /// The document did not parse.
    #[error("the file didn't read as {format}: {why}")]
    Parse {
        /// The format name.
        format: &'static str,
        /// What was wrong.
        why: String,
    },
    /// A value was not valid base64url.
    #[error("'{0}' in the file isn't valid base64url")]
    BadBase64(String),
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
    /// The file is locked with a password and none was given.
    #[error("this file is protected with a password; type it to open the file")]
    PasswordNeeded,
    /// The password didn't open the file (or the file was changed).
    #[error("that password didn't open the file, or the file was changed after it was made")]
    WrongPassword,
    /// A sealed file failed its check part-way: changed or cut short.
    #[error("the file was changed or cut short after it was made: {0}")]
    Tampered(String),
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, CxfError>;

/// Size caps every importer applies before and after parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Whole-file cap in bytes.
    pub max_bytes: usize,
    /// Items (or CSV rows) per file.
    pub max_items: usize,
    /// Bytes per single field value or CSV cell.
    pub max_field_bytes: usize,
    /// Credentials per CXF item.
    pub max_credentials_per_item: usize,
    /// Bytes per attached file.
    pub max_attachment_bytes: usize,
    /// Entries in a ZIP file.
    pub max_entries: usize,
    /// Bytes of everything a ZIP file unpacks to, together.
    pub max_unpacked_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_items: 100_000,
            max_field_bytes: 64 * 1024,
            max_credentials_per_item: 64,
            max_attachment_bytes: 32 * 1024 * 1024,
            max_entries: 20_000,
            max_unpacked_bytes: 256 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// The caps for a Lockdown backup, which carries attachments inline:
    /// a bigger file, the same caps on everything inside it.
    pub fn backup() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
            max_unpacked_bytes: 512 * 1024 * 1024,
            ..Self::default()
        }
    }
}

impl Limits {
    /// Refuses `bytes` if over the whole-file cap.
    pub fn check_size(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > self.max_bytes {
            return Err(CxfError::TooLarge {
                found: bytes.len(),
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}
