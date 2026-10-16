//! Secret types that zero their memory when dropped and never print their
//! contents (03-architecture §8 "memory hygiene").

use std::fmt;

use ecosystem_identity::SymmetricKey;
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

/// A 256-bit symmetric key. It implements the identity crate's
/// [`SymmetricKey`], so it can be handed to that crate's `wrap` and `unwrap`
/// unchanged. Lockdown's own key types (vault, item, version, attachment)
/// are newtypes over this.
pub struct Key32(SecretBox<[u8; 32]>);

impl Key32 {
    /// A fresh random key.
    pub fn generate() -> crate::Result<Self> {
        Ok(Self::from_bytes(crate::rand::random_32()?))
    }
}

impl SymmetricKey for Key32 {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
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
/// password, a seed phrase. Debug prints only the length.
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    /// Takes ownership of `bytes`; they are wiped when this value drops.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The bytes. Use them at once; never copy them into a `String` that
    /// outlives this value.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// How many bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(v: Vec<u8>) -> Self {
        Self::new(v)
    }
}

impl From<Zeroizing<Vec<u8>>> for SecretBytes {
    fn from(v: Zeroizing<Vec<u8>>) -> Self {
        Self(v)
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes({} bytes)", self.0.len())
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
