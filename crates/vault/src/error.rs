//! One error type for the vault. Messages are for the person reading them
//! in an app: a failed tag is "tampered", never "corrupt file"
//! (04-security §5).

use crate::Id;

/// What can go wrong in a vault.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A cryptographic step failed (a tag check, a bad key length).
    #[error(transparent)]
    Crypto(#[from] lockdown_crypto::Error),
    /// A key envelope's place does not match where it was found. It was not
    /// opened.
    #[error("this key belongs to {0}; it was not opened")]
    KeyMoved(String),
    /// A sealed field's associated data names a different item, version or
    /// field than the one it was found in. Shown as tampering.
    #[error(
        "field '{field_id}' didn't open where it was found; the vault may have been tampered with"
    )]
    FieldTampered {
        /// The field.
        field_id: String,
    },
    /// A signature or hash in the manifest chain did not check out.
    #[error("the vault's history didn't check out at manifest {seq}: {why}")]
    ChainBroken {
        /// The head where the chain failed.
        seq: u64,
        /// What was wrong.
        why: String,
    },
    /// An item in the store does not match what the newest signed manifest
    /// head says about it. Shown as tampering: the manifest is the table of
    /// contents, and a blob that disagrees with it was changed after the
    /// head was signed.
    #[error(
        "item {item_id} doesn't match the vault's manifest ({why}); the store may have been tampered with"
    )]
    ManifestMismatch {
        /// The item.
        item_id: Id,
        /// What disagreed.
        why: String,
    },
    /// The local store failed or refused.
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    /// The server offered an older manifest than this device accepted.
    #[error(
        "Rollback attempt blocked: the server offered manifest {found} but this device has already accepted {remembered}"
    )]
    Rollback {
        /// What was offered.
        found: u64,
        /// What this device remembers.
        remembered: u64,
    },
    /// The item or version is not there.
    #[error("{0} wasn't found")]
    NotFound(String),
    /// The version's key was destroyed; its ciphertext is unreadable forever.
    #[error("version {version_n} of item {item_id} was shredded and can't be read")]
    Shredded {
        /// The item.
        item_id: Id,
        /// The version.
        version_n: u64,
    },
    /// An item was written by a newer schema than this build reads.
    #[error(
        "this item uses schema {found}, newer than this app understands ({supported}); update the app"
    )]
    SchemaTooNew {
        /// What the item says.
        found: u16,
        /// What this build reads.
        supported: u16,
    },
    /// A head or version is signed by a device this vault was not told
    /// about. Its key comes from a signed identity record, never from the
    /// server; until that record is known, nothing it signed is trusted.
    #[error(
        "signed by device {0}, which this vault doesn't know; enrol or learn that device first"
    )]
    UnknownDevice(Id),
    /// A head signed by a device after it was revoked. Heads it signed up
    /// to `after` still count; this one does not (D73).
    #[error(
        "manifest {seq} was signed by device {device} after that device was revoked (its last accepted head was {after}); refused"
    )]
    RevokedDevice {
        /// The revoked device.
        device: Id,
        /// The head it tried to sign.
        seq: u64,
        /// The newest head it may have signed.
        after: u64,
    },
    /// A crown-jewel item was asked for but the vault holds no Crown-Jewel Key.
    #[error("this vault has no crown-jewel key; unlock the persona first")]
    NoCrownJewelKey,
    /// Anything else the caller did wrong.
    #[error("{0}")]
    Invalid(String),
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

impl From<ecosystem_identity::Error> for Error {
    fn from(e: ecosystem_identity::Error) -> Self {
        Error::Crypto(e.into())
    }
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, Error>;
