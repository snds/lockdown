//! Who signs manifest heads and item versions: the device (02-model §8).
//!
//! A head is signed by the device that made it, and other devices verify it
//! against that device's public key in the identity's `Device` record. On a
//! phone the private half sits in hardware and only ever answers "sign
//! this", so the vault cannot hold an Ed25519 key of its own; it holds a
//! [`Signer`]. The identity crate's [`DeviceKeystore`] is one; a bare
//! [`SigningKey`] is another, for tests and for a vault that has no
//! identity yet.

use std::rc::Rc;
use std::sync::Arc;

use ecosystem_identity::keystore::DeviceKeystore;
use lockdown_crypto::{SigningKey, VerifyingKey};

use crate::Result;

/// Signs on behalf of this device.
pub trait Signer {
    /// Ed25519 over `message`.
    fn sign(&self, message: &[u8]) -> Result<[u8; 64]>;
    /// The public half, which the device's identity record carries.
    fn verifying_key(&self) -> Result<VerifyingKey>;
}

impl Signer for SigningKey {
    fn sign(&self, message: &[u8]) -> Result<[u8; 64]> {
        Ok(SigningKey::sign(self, message))
    }

    fn verifying_key(&self) -> Result<VerifyingKey> {
        Ok(SigningKey::verifying_key(self))
    }
}

impl<K: DeviceKeystore + ?Sized> Signer for Rc<K> {
    fn sign(&self, message: &[u8]) -> Result<[u8; 64]> {
        Ok(DeviceKeystore::sign(self.as_ref(), message)?)
    }

    fn verifying_key(&self) -> Result<VerifyingKey> {
        Ok(VerifyingKey::from_bytes(&self.device_pub())?)
    }
}

impl<K: DeviceKeystore + ?Sized> Signer for Arc<K> {
    fn sign(&self, message: &[u8]) -> Result<[u8; 64]> {
        Ok(DeviceKeystore::sign(self.as_ref(), message)?)
    }

    fn verifying_key(&self) -> Result<VerifyingKey> {
        Ok(VerifyingKey::from_bytes(&self.device_pub())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keystore_signs_with_its_device_key() {
        let dir = tempfile::tempdir().unwrap();
        let keystore = Rc::new(
            ecosystem_identity::FileKeystore::open_or_create(&dir.path().join("device-key.cbor"))
                .unwrap(),
        );
        let signer: Box<dyn Signer> = Box::new(keystore.clone());
        let sig = signer.sign(b"head").unwrap();
        let key = signer.verifying_key().unwrap();
        assert_eq!(key.to_bytes(), keystore.device_pub());
        key.verify(b"head", &sig, "head").unwrap();
        assert!(key.verify(b"other", &sig, "head").is_err());
    }
}
