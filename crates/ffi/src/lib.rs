//! `lockdown-ffi`: the facade the Swift and Kotlin shells call
//! (03-architecture §3 and §8).
//!
//! **Status: skeleton with the secret-handle discipline real.** The UniFFI
//! scaffolding (the `.udl` or proc-macro surface and the generated Swift
//! and Kotlin) is added when the first shell is built. What is here is the
//! rule every binding must keep: no secret crosses the boundary as a
//! `String`. A shell holds a [`SecretHandle`], asks for a [`RedactedView`],
//! or takes a one-shot [`reveal`](SecretRegistry::reveal) that removes the
//! secret from the registry and wipes it when the returned buffer drops.

use std::collections::BTreeMap;

use zeroize::Zeroizing;

/// An opaque reference to a secret held in Rust memory. The shell sees only
/// the number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SecretHandle(pub u64);

/// What a shell may show without a reveal: the length and a masked form.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RedactedView {
    /// How many bytes the secret has.
    pub len: usize,
    /// A mask of the same length in characters, never the value.
    pub masked: String,
}

/// Why a handle call failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FfiError {
    /// The handle is not (or no longer) in the registry.
    #[error("that secret handle is no longer valid")]
    InvalidHandle,
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

/// The registry behind the handles. One per unlocked session; locking
/// drops it, which wipes every secret.
#[derive(Default, Debug)]
pub struct SecretRegistry {
    next: u64,
    secrets: BTreeMap<SecretHandle, Zeroizing<Vec<u8>>>,
}

impl SecretRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `secret` and returns its handle.
    pub fn insert(&mut self, secret: Vec<u8>) -> SecretHandle {
        self.next += 1;
        let h = SecretHandle(self.next);
        self.secrets.insert(h, Zeroizing::new(secret));
        h
    }

    /// A view with no value in it.
    pub fn redacted(&self, handle: SecretHandle) -> Result<RedactedView, FfiError> {
        let s = self.secrets.get(&handle).ok_or(FfiError::InvalidHandle)?;
        Ok(RedactedView {
            len: s.len(),
            masked: "•".repeat(s.len().min(12)),
        })
    }

    /// Takes the secret out, once. The handle is dead afterwards and the
    /// returned buffer wipes itself when dropped.
    pub fn reveal(&mut self, handle: SecretHandle) -> Result<Zeroizing<Vec<u8>>, FfiError> {
        self.secrets.remove(&handle).ok_or(FfiError::InvalidHandle)
    }

    /// Wipes every secret: lock.
    pub fn lock(&mut self) {
        self.secrets.clear();
    }

    /// How many secrets are held.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Whether none are held.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reveal_is_one_shot_and_views_never_leak() {
        let mut reg = SecretRegistry::new();
        let h = reg.insert(b"hunter2".to_vec());
        let view = reg.redacted(h).unwrap();
        assert_eq!(view.len, 7);
        assert!(!view.masked.contains("hunter"));
        let value = reg.reveal(h).unwrap();
        assert_eq!(value.as_slice(), b"hunter2");
        assert_eq!(reg.reveal(h), Err(FfiError::InvalidHandle));
        assert_eq!(reg.redacted(h), Err(FfiError::InvalidHandle));
        let h2 = reg.insert(b"x".to_vec());
        reg.lock();
        assert!(reg.is_empty());
        assert_eq!(reg.redacted(h2), Err(FfiError::InvalidHandle));
    }
}
