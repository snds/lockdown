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
//!
//! Every parser is size-capped by [`Limits`] and treats the file as hostile.
//! The Lockdown archive (`.lockdown`) and the other-manager importers
//! (1PUX, Bitwarden JSON, KDBX, …) are not built yet.

pub mod convert;
pub mod csv;
pub mod model;

pub use convert::{ImportedItem, from_items, to_items};
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
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_items: 100_000,
            max_field_bytes: 64 * 1024,
            max_credentials_per_item: 64,
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
