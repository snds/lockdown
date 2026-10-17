//! One error type for the crate. Messages are written for the person who
//! will read them in an app, not for a log file.

/// What can go wrong in a cryptographic step.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The authentication tag did not check out: a wrong key, or a changed
    /// ciphertext or associated data. Shown as "tampered", never as a
    /// corrupt file (04-security §5).
    #[error("this data didn't open: it was changed, or this isn't its key")]
    Tampered,
    /// Associated data was empty. Every ciphertext in Lockdown is bound to
    /// a place; an unbound ciphertext is a bug, not a default.
    #[error("a ciphertext must be bound to a place; empty associated data was refused")]
    EmptyAad,
    /// A signature did not check out.
    #[error("the signature on {0} didn't check out")]
    BadSignature(String),
    /// Password-stretching parameters below the floor were refused.
    #[error("the password-stretching settings ({0}) are weaker than the floor and were refused")]
    KdfBelowFloor(String),
    /// An account login did not go through. One message for every cause
    /// (wrong password, unknown login, a password file that is not the
    /// one this password made), so nothing is learned from the difference.
    #[error("the login didn't go through: check the login name and password")]
    LoginFailed,
    /// The operating system gave no random bytes.
    #[error("the device couldn't make a random number")]
    Random,
    /// Something was the wrong length or shape.
    #[error("{0}")]
    Invalid(String),
    /// The identity crate reported an error that is not one of the above.
    #[error(transparent)]
    Identity(ecosystem_identity::Error),
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

impl From<ecosystem_identity::Error> for Error {
    fn from(e: ecosystem_identity::Error) -> Self {
        use ecosystem_identity::Error as E;
        match e {
            E::KdfBelowFloor(p) => Error::KdfBelowFloor(p),
            E::WrongKey => Error::Tampered,
            E::Random => Error::Random,
            E::Unsupported(m) => Error::Unsupported(m),
            E::Invalid(m) => Error::Invalid(m),
            other => Error::Identity(other),
        }
    }
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, Error>;
