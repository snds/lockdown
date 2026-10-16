//! Random bytes from the operating system's generator, and nothing else.
//! Lockdown never seeds from the clock or a user-space generator (brief
//! §4.9 "honest generators").

use crate::{Error, Result};

/// Fills `buf` from the operating system's random source.
pub fn random_bytes(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|_| Error::Random)
}

/// Thirty-two fresh random bytes: a key.
pub fn random_32() -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    random_bytes(&mut out)?;
    Ok(out)
}

/// Twenty-four fresh random bytes: an XChaCha20-Poly1305 nonce. Nonces are
/// random, never counters, because devices do not coordinate (04-security §5).
pub fn random_nonce() -> Result<[u8; 24]> {
    let mut out = [0u8; 24];
    random_bytes(&mut out)?;
    Ok(out)
}

/// Sixteen fresh random bytes: a Secret Key (04-security §3).
pub fn random_16() -> Result<[u8; 16]> {
    let mut out = [0u8; 16];
    random_bytes(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_draws_differ() {
        assert_ne!(random_32().unwrap(), random_32().unwrap());
        assert_ne!(random_nonce().unwrap(), random_nonce().unwrap());
        assert_ne!(random_16().unwrap(), random_16().unwrap());
    }
}
