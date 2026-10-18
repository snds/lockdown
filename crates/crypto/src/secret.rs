//! Secret types that zero their memory when dropped and never print their
//! contents (03-architecture §8 "memory hygiene").

use std::fmt;

use ecosystem_identity::SymmetricKey;
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

use crate::locked::PageLock;

/// A 256-bit symmetric key. It implements the identity crate's
/// [`SymmetricKey`], so it can be handed to that crate's `wrap` and `unwrap`
/// unchanged. Lockdown's own key types (vault, item, version, attachment)
/// are newtypes over this. Its page is locked out of swap while it lives
/// ([`crate::locked`]).
pub struct Key32 {
    key: SecretBox<[u8; 32]>,
    _pages: Option<PageLock>,
}

impl Key32 {
    /// A fresh random key.
    pub fn generate() -> crate::Result<Self> {
        Ok(Self::from_bytes(crate::rand::random_32()?))
    }
}

impl SymmetricKey for Key32 {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        let boxed = SecretBox::new(Box::new(bytes));
        let lock = PageLock::of(boxed.expose_secret());
        Self {
            key: boxed,
            _pages: lock,
        }
    }

    fn expose(&self) -> &[u8; 32] {
        self.key.expose_secret()
    }
}

impl Clone for Key32 {
    fn clone(&self) -> Self {
        Self::from_bytes(*self.expose())
    }
}

impl fmt::Debug for Key32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key32(…)")
    }
}

/// Plaintext bytes that are wiped when dropped: a decrypted field, a
/// password, a seed phrase. Debug prints only the length. Its pages are
/// locked out of swap while it lives ([`crate::locked`]).
pub struct SecretBytes {
    bytes: Zeroizing<Vec<u8>>,
    _pages: Option<PageLock>,
}

impl SecretBytes {
    /// Takes ownership of `bytes`; they are wiped when this value drops.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self::from(Zeroizing::new(bytes))
    }

    /// The bytes. Use them at once; never copy them into a `String` that
    /// outlives this value.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(v: Vec<u8>) -> Self {
        Self::new(v)
    }
}

impl From<Zeroizing<Vec<u8>>> for SecretBytes {
    fn from(v: Zeroizing<Vec<u8>>) -> Self {
        let lock = PageLock::of(&v);
        Self {
            bytes: v,
            _pages: lock,
        }
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes({} bytes)", self.bytes.len())
    }
}

/// Declares a 256-bit key type over [`Key32`] with the identity crate's
/// [`SymmetricKey`] behaviour, so callers cannot mix up key kinds.
#[macro_export]
macro_rules! key32 {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone)]
        pub struct $name($crate::Key32);

        impl $crate::identity::SymmetricKey for $name {
            fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(<$crate::Key32 as $crate::identity::SymmetricKey>::from_bytes(bytes))
            }
            fn expose(&self) -> &[u8; 32] {
                <$crate::Key32 as $crate::identity::SymmetricKey>::expose(&self.0)
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(…)"))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    key32!(
        /// A test key kind.
        TestKey
    );

    #[test]
    fn keys_do_not_print_their_bytes() {
        let k = Key32::generate().unwrap();
        assert_eq!(format!("{k:?}"), "Key32(…)");
        let t = TestKey::from_bytes(*k.expose());
        assert_eq!(format!("{t:?}"), "TestKey(…)");
        assert_eq!(t.expose(), k.expose());
        let s = SecretBytes::new(b"hunter2".to_vec());
        assert_eq!(format!("{s:?}"), "SecretBytes(7 bytes)");
        assert!(!s.is_empty());
    }
}
