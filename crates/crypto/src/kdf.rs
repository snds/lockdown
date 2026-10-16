//! Password stretching and key derivation, taken from the identity crate so
//! the floors cannot drift (04-security §4; 01-identity §3, §10a).
//!
//! The constants here are **re-exports**, not copies. The desktop floor is
//! 64 MiB, 3 passes, 4 lanes; mobile 32 MiB, 3 passes, 2 lanes; WASM 32 MiB,
//! 3 passes, 1 lane. Parameters only ever rise (D26, D32). If Lockdown ever
//! needs a different floor, the change goes to `ecosystem-identity`, with the
//! spec, not here.

pub use ecosystem_identity::kdf::{
    AUK_INFO, Argon2Params, FLOOR_DESKTOP, FLOOR_MOBILE, FLOOR_WASM, account_unlock_key, calibrate,
    calibrate_at_least, floor, hkdf_sha512, platform_floor,
};

use zeroize::Zeroizing;

use crate::Result;

/// Argon2id over `secret` with `salt`, refusing parameters below this
/// build's floor. Thirty-two bytes out.
pub fn stretch(secret: &[u8], salt: &[u8], params: &Argon2Params) -> Result<Zeroizing<[u8; 32]>> {
    Ok(ecosystem_identity::kdf::stretch(secret, salt, params)?)
}

/// A 256-bit sub-key of `ikm` for `info` through HKDF-SHA-512, with no salt.
pub fn subkey(ikm: &[u8], info: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    Ok(ecosystem_identity::kdf::subkey(ikm, info)?)
}

/// HKDF-SHA-512 with a salt, filling `out`. For the labelled derivations
/// Lockdown owns (for example the per-vault search-index key later).
pub fn derive(ikm: &[u8], salt: Option<&[u8]>, info: &[u8], out: &mut [u8]) -> Result<()> {
    Ok(hkdf_sha512(ikm, salt, info, out)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn floors_are_the_identity_crates() {
        assert_eq!(FLOOR_DESKTOP.m_cost_kib, 64 * 1024);
        assert_eq!((FLOOR_DESKTOP.t_cost, FLOOR_DESKTOP.p_cost), (3, 4));
        assert_eq!(FLOOR_MOBILE.m_cost_kib, 32 * 1024);
        assert_eq!((FLOOR_MOBILE.t_cost, FLOOR_MOBILE.p_cost), (3, 2));
        assert_eq!(FLOOR_WASM.m_cost_kib, 32 * 1024);
        assert_eq!((FLOOR_WASM.t_cost, FLOOR_WASM.p_cost), (3, 1));
        // OWASP's server number (19 MiB, 2 passes) is below every floor.
        let owasp = Argon2Params {
            m_cost_kib: 19 * 1024,
            t_cost: 2,
            p_cost: 1,
        };
        assert!(!owasp.meets(&FLOOR_DESKTOP));
        assert!(!owasp.meets(&FLOOR_MOBILE));
        assert!(!owasp.meets(&FLOOR_WASM));
    }

    #[test]
    fn below_floor_is_refused_and_at_floor_is_deterministic() {
        let weak = Argon2Params {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        assert!(matches!(
            stretch(b"pw", b"salt-salt", &weak),
            Err(Error::KdfBelowFloor(_))
        ));
        // `floor()` is the test floor under `insecure-test-kdf`, so this is fast.
        let params = floor();
        let a = stretch(b"pw", b"salt-salt", &params).unwrap();
        let b = stretch(b"pw", b"salt-salt", &params).unwrap();
        assert_eq!(a.as_ref(), b.as_ref());
        assert_ne!(
            a.as_ref(),
            stretch(b"pw", b"other-salt", &params).unwrap().as_ref()
        );
    }

    #[test]
    fn hkdf_is_labelled() {
        let a = subkey(b"ikm", b"lockdown/a/v1").unwrap();
        let b = subkey(b"ikm", b"lockdown/b/v1").unwrap();
        assert_ne!(a.as_ref(), b.as_ref());
        let mut out = [0u8; 64];
        derive(b"ikm", Some(b"salt"), b"lockdown/a/v1", &mut out).unwrap();
        assert_ne!(&out[..32], a.as_ref());
    }
}
