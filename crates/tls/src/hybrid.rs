//! X25519MLKEM768: the hybrid TLS 1.3 key exchange of
//! draft-ietf-tls-ecdhe-mlkem (code point `0x11EC`), as a rustls
//! [`SupportedKxGroup`] over rustls's own `ring` X25519 and RustCrypto's
//! ML-KEM-768 (FIPS 203).
//!
//! The wire layout is the draft's, post-quantum part first:
//!
//! | | ML-KEM-768 part | X25519 part |
//! |---|---|---|
//! | client key share | encapsulation key, 1184 bytes | public key, 32 bytes |
//! | server key share | ciphertext, 1088 bytes | public key, 32 bytes |
//! | shared secret | ML-KEM shared key, 32 bytes | X25519 output, 32 bytes |
//!
//! The shared secret is the plain concatenation (64 bytes) that TLS 1.3's
//! key schedule then extracts from, so the session is safe while either
//! half holds. The server checks the client's encapsulation key as FIPS 203
//! §7.2 asks (the modulus check inside `EncapsulationKey::new`) and refuses
//! a share of the wrong length with `illegal_parameter`. The client also
//! offers its X25519 half on its own ([`ActiveKeyExchange::hybrid_component`]),
//! so a server without the hybrid group can answer with plain X25519 and no
//! extra round trip; TLS 1.3 signs the whole transcript, so a network
//! attacker cannot push two hybrid-capable ends down to the classical group.
//!
//! Randomness comes from the operating system through `ring`, the same
//! source the rest of the handshake uses. Every secret is zeroed after use.

use ml_kem::array::Array;
use ml_kem::{Decapsulate, KeyExport, MlKem768};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::crypto::{ActiveKeyExchange, CompletedKeyExchange, SharedSecret, SupportedKxGroup};
use rustls::ffdhe_groups::FfdheGroup;
use rustls::{Error, NamedGroup, PeerMisbehaved, ProtocolVersion};
use zeroize::{Zeroize, Zeroizing};

/// The ML-KEM-768 encapsulation key length (the client's half).
pub const MLKEM768_ENCAPSULATION_KEY_LEN: usize = 1184;
/// The ML-KEM-768 ciphertext length (the server's half).
pub const MLKEM768_CIPHERTEXT_LEN: usize = 1088;
/// The X25519 public key length.
pub const X25519_LEN: usize = 32;

/// The hybrid group, ready for a [`rustls::crypto::CryptoProvider`]'s
/// `kx_groups`.
pub static X25519MLKEM768: &dyn SupportedKxGroup = &X25519MlKem768;

const INVALID_KEY_SHARE: Error = Error::PeerMisbehaved(PeerMisbehaved::InvalidKeyShare);

fn classical() -> &'static dyn SupportedKxGroup {
    rustls::crypto::ring::kx_group::X25519
}

fn fill(buf: &mut [u8]) -> Result<(), Error> {
    SystemRandom::new()
        .fill(buf)
        .map_err(|_| Error::FailedToGetRandomBytes)
}

/// The group itself; [`X25519MLKEM768`] is its one instance.
#[derive(Debug)]
pub struct X25519MlKem768;

impl SupportedKxGroup for X25519MlKem768 {
    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        let classical = classical().start()?;
        let mut seed = Zeroizing::new([0u8; 64]);
        fill(seed.as_mut())?;
        let decapsulation = ml_kem::DecapsulationKey::<MlKem768>::from_seed(Array::from(*seed));
        let encapsulation = decapsulation.encapsulation_key().to_bytes();
        let mut pub_key = Vec::with_capacity(MLKEM768_ENCAPSULATION_KEY_LEN + X25519_LEN);
        pub_key.extend_from_slice(encapsulation.as_slice());
        pub_key.extend_from_slice(classical.pub_key());
        Ok(Box::new(ActiveHybrid {
            decapsulation,
            classical,
            pub_key,
        }))
    }

    fn start_and_complete(&self, client_share: &[u8]) -> Result<CompletedKeyExchange, Error> {
        if client_share.len() != MLKEM768_ENCAPSULATION_KEY_LEN + X25519_LEN {
            return Err(INVALID_KEY_SHARE);
        }
        let (ek_bytes, x25519_share) = client_share.split_at(MLKEM768_ENCAPSULATION_KEY_LEN);
        let ek_array = Array::try_from(ek_bytes).map_err(|_| INVALID_KEY_SHARE)?;
        // FIPS 203 §7.2: the modulus check runs here; a bad key is refused.
        let encapsulation =
            ml_kem::EncapsulationKey::<MlKem768>::new(&ek_array).map_err(|_| INVALID_KEY_SHARE)?;
        let mut m = Zeroizing::new([0u8; 32]);
        fill(m.as_mut())?;
        let (ciphertext, mut pq_secret) = encapsulation.encapsulate_deterministic(&Array::from(*m));
        let classical = classical().start_and_complete(x25519_share)?;
        let mut pub_key = Vec::with_capacity(MLKEM768_CIPHERTEXT_LEN + X25519_LEN);
        pub_key.extend_from_slice(ciphertext.as_slice());
        pub_key.extend_from_slice(&classical.pub_key);
        let secret = combine(pq_secret.as_slice(), classical.secret.secret_bytes());
        pq_secret.as_mut_slice().zeroize();
        Ok(CompletedKeyExchange {
            group: NamedGroup::X25519MLKEM768,
            pub_key,
            secret,
        })
    }

    fn ffdhe_group(&self) -> Option<FfdheGroup<'static>> {
        None
    }

    fn name(&self) -> NamedGroup {
        NamedGroup::X25519MLKEM768
    }

    fn usable_for_version(&self, version: ProtocolVersion) -> bool {
        version == ProtocolVersion::TLSv1_3
    }
}

/// `ML-KEM shared key ‖ X25519 output`, the draft's order for this group.
fn combine(post_quantum: &[u8], classical: &[u8]) -> SharedSecret {
    let mut both = Zeroizing::new(Vec::with_capacity(post_quantum.len() + classical.len()));
    both.extend_from_slice(post_quantum);
    both.extend_from_slice(classical);
    // `SharedSecret` zeroes its own copy on drop.
    SharedSecret::from(both.as_slice())
}

struct ActiveHybrid {
    decapsulation: ml_kem::DecapsulationKey<MlKem768>,
    classical: Box<dyn ActiveKeyExchange>,
    pub_key: Vec<u8>,
}

impl ActiveKeyExchange for ActiveHybrid {
    fn complete(self: Box<Self>, server_share: &[u8]) -> Result<SharedSecret, Error> {
        if server_share.len() != MLKEM768_CIPHERTEXT_LEN + X25519_LEN {
            return Err(INVALID_KEY_SHARE);
        }
        let (ct_bytes, x25519_share) = server_share.split_at(MLKEM768_CIPHERTEXT_LEN);
        let ciphertext: ml_kem::Ciphertext<MlKem768> =
            Array::try_from(ct_bytes).map_err(|_| INVALID_KEY_SHARE)?;
        // ML-KEM decapsulation never fails: a forged ciphertext gives an
        // unrelated key (implicit rejection), and the handshake's Finished
        // check then fails.
        let mut pq_secret = self.decapsulation.decapsulate(&ciphertext);
        let classical = self.classical.complete(x25519_share)?;
        let secret = combine(pq_secret.as_slice(), classical.secret_bytes());
        pq_secret.as_mut_slice().zeroize();
        Ok(secret)
    }

    fn hybrid_component(&self) -> Option<(NamedGroup, &[u8])> {
        Some((self.classical.group(), self.classical.pub_key()))
    }

    fn complete_hybrid_component(
        self: Box<Self>,
        peer_pub_key: &[u8],
    ) -> Result<SharedSecret, Error> {
        self.classical.complete(peer_pub_key)
    }

    fn pub_key(&self) -> &[u8] {
        &self.pub_key
    }

    fn ffdhe_group(&self) -> Option<FfdheGroup<'static>> {
        None
    }

    fn group(&self) -> NamedGroup {
        NamedGroup::X25519MLKEM768
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_agree_and_the_shares_have_the_drafts_layout() {
        let client = X25519MLKEM768.start().unwrap();
        assert_eq!(client.group(), NamedGroup::X25519MLKEM768);
        assert_eq!(
            client.pub_key().len(),
            MLKEM768_ENCAPSULATION_KEY_LEN + X25519_LEN
        );
        // The X25519 half is offered on its own too, and it is the tail.
        let (group, x25519) = client.hybrid_component().unwrap();
        assert_eq!(group, NamedGroup::X25519);
        assert_eq!(x25519, &client.pub_key()[MLKEM768_ENCAPSULATION_KEY_LEN..]);

        let server = X25519MLKEM768
            .start_and_complete(client.pub_key())
            .unwrap();
        assert_eq!(server.group, NamedGroup::X25519MLKEM768);
        assert_eq!(server.pub_key.len(), MLKEM768_CIPHERTEXT_LEN + X25519_LEN);
        let server_secret = server.secret.secret_bytes().to_vec();
        assert_eq!(server_secret.len(), 64);

        let client_secret = client.complete(&server.pub_key).unwrap();
        assert_eq!(client_secret.secret_bytes(), server_secret.as_slice());

        // The X25519 half alone is a plain X25519 exchange: the classical
        // second half of the hybrid secret matches it.
        let client = X25519MLKEM768.start().unwrap();
        let x_share = client.pub_key()[MLKEM768_ENCAPSULATION_KEY_LEN..].to_vec();
        let plain = classical().start_and_complete(&x_share).unwrap();
        let ours = client.complete_hybrid_component(&plain.pub_key).unwrap();
        assert_eq!(ours.secret_bytes(), plain.secret.secret_bytes());
    }

    #[test]
    fn bent_shares_are_refused_and_a_forged_ciphertext_gives_another_key() {
        let client = X25519MLKEM768.start().unwrap();
        let share = client.pub_key().to_vec();
        // Wrong lengths, both ways.
        assert!(X25519MLKEM768.start_and_complete(&share[1..]).is_err());
        assert!(X25519MLKEM768.start_and_complete(&[0u8; 32]).is_err());
        // An encapsulation key whose coefficients are out of range (all
        // ones encode 4095 > q - 1) fails FIPS 203's modulus check.
        let mut bad = share.clone();
        for b in &mut bad[..MLKEM768_ENCAPSULATION_KEY_LEN - 32] {
            *b = 0xff;
        }
        assert!(X25519MLKEM768.start_and_complete(&bad).is_err());

        let server = X25519MLKEM768.start_and_complete(&share).unwrap();
        let expected = server.secret.secret_bytes().to_vec();
        let mut forged = server.pub_key.clone();
        forged[7] ^= 1;
        let got = client.complete(&forged).unwrap();
        assert_ne!(got.secret_bytes(), expected.as_slice());
        // And a short server share is refused outright.
        let client = X25519MLKEM768.start().unwrap();
        assert!(client.complete(&server.pub_key[..100]).is_err());
    }
}
