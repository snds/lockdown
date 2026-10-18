//! `lockdown-crypto`: the cryptographic choices Lockdown builds on.
//!
//! Plan: `docs/plan/03-architecture.md` §2 (the `crypto` crate row) and
//! `docs/plan/04-security.md` §3–§5.
//!
//! This crate is thin on purpose. The ecosystem identity crate
//! (`ecosystem-identity`, D12 and D34) already made the audited choices:
//! XChaCha20-Poly1305 for every ciphertext, Argon2id with a signed floor,
//! HKDF-SHA-512, Ed25519 for signatures, and X-Wing (hybrid X25519 +
//! ML-KEM-768) for public-key wrapping. Lockdown must never drift from them,
//! so this crate **re-exports** those pieces and adds only what the vault
//! needs on top: a plain AEAD with associated data, BLAKE3, a CSPRNG, and
//! zeroizing secret types.
//!
//! - [`aead`] — seal and open with associated data; empty associated data is refused
//! - [`kdf`] — Argon2id with the identity crate's floors; HKDF-SHA-512
//! - [`sign`] — Ed25519 signing and verification
//! - [`kem`] — X-Wing encapsulate and decapsulate
//! - [`hash`] — BLAKE3 hashing, keyed hashing, and key derivation
//! - [`pq_sign`] — ML-DSA-65 beside Ed25519 (the identity's post-quantum signature)
//! - [`merkle`] — the RFC 6962-style tree and proofs behind the key-transparency log
//! - [`rand`] — random bytes from the operating system
//! - [`generator`] — the password generator: uniform draws from that source under a rule
//! - [`secret`] — key and plaintext types that zero their memory on drop
//! - [`locked`] — the pages under those types kept out of swap; core dumps off
//! - [`auth`] — account login: OPAQUE (RFC 9807) as a pure state machine, behind [`AccountAuth`]
//! - [`cbor`] — canonical CBOR, the encoding every piece of associated data uses

pub mod aead;
pub mod auth;
pub mod error;
pub mod generator;
pub mod hash;
pub mod kdf;
pub mod kem;
pub mod locked;
pub mod merkle;
pub mod pq_sign;
pub mod rand;
pub mod secret;
pub mod sign;

/// The identity crate itself, for callers that need a type this crate does
/// not re-export by name.
pub use ecosystem_identity as identity;
/// Canonical CBOR (RFC 8949 §4.2.1), the identity crate's encoder, so every
/// signature and every piece of associated data in Lockdown is byte-stable.
pub use ecosystem_identity::cbor;
pub use error::{Error, Result};

pub use aead::{Sealed, open, seal};
pub use auth::{AccountAuth, ExportKey, OpaqueAccountAuth};
pub use hash::Hash32;
pub use kdf::{Argon2Params, FLOOR_DESKTOP, FLOOR_MOBILE, FLOOR_WASM};
pub use secret::{Key32, SecretBytes};
pub use sign::{SigningKey, VerifyingKey};
