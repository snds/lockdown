//! Session tokens: what a client holds after an OPAQUE login and sends as
//! `Authorization: Bearer …` on every call.
//!
//! Format (`lk1`): `lk1.<claims>.<signature>`, both parts base64url with
//! no padding. The claims are canonical CBOR of [`TokenClaims`]; the
//! signature is Ed25519 by the server's token key over those bytes. The
//! server checks a token without a database lookup, so a stolen database
//! cannot mint one (the key lives in the key file) and a token lists only
//! which account and device it stands for, never a password or a key.
//! Tokens are short-lived (the server's `session_ttl`); there is no
//! refresh yet, a client logs in again.

use data_encoding::BASE64URL_NOPAD;
use lockdown_crypto::{SigningKey, VerifyingKey, cbor};
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};

/// The token format tag.
pub const TAG: &str = "lk1";

/// What a token stands for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Claims format version.
    pub v: u16,
    /// The account.
    pub account_id: Id,
    /// The device that logged in.
    pub device_id: Id,
    /// Issued at (seconds since the Unix epoch).
    pub iat: i64,
    /// Stops working at.
    pub exp: i64,
    /// Random, so two logins in one second give two tokens.
    #[serde(with = "serde_bytes")]
    pub nonce: [u8; 16],
}

/// Why a token was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// Not a token at all, or not one of ours.
    #[error("the session token didn't check out")]
    Invalid,
    /// A real token past its time.
    #[error("the session has run out; log in again")]
    Expired,
}

/// Issues and checks tokens with the server's token key.
pub struct TokenSigner {
    key: SigningKey,
    verifying: VerifyingKey,
}

impl std::fmt::Debug for TokenSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSigner(…)")
    }
}

impl TokenSigner {
    /// A signer over the server's token key.
    pub fn new(key: SigningKey) -> Self {
        let verifying = key.verifying_key();
        Self { key, verifying }
    }

    /// Mints a token for `account_id` on `device_id`, valid for `ttl_secs`
    /// from `now`. Returns the token and its expiry.
    pub fn issue(
        &self,
        account_id: Id,
        device_id: Id,
        now: i64,
        ttl_secs: i64,
    ) -> lockdown_crypto::Result<(String, i64)> {
        let claims = TokenClaims {
            v: 1,
            account_id,
            device_id,
            iat: now,
            exp: now.saturating_add(ttl_secs),
            nonce: lockdown_crypto::rand::random_16()?,
        };
        let body = cbor::to_canonical(&claims)?;
        let sig = self.key.sign(&body);
        Ok((
            format!(
                "{TAG}.{}.{}",
                BASE64URL_NOPAD.encode(&body),
                BASE64URL_NOPAD.encode(&sig)
            ),
            claims.exp,
        ))
    }

    /// Checks a token at `now`.
    pub fn verify(&self, token: &str, now: i64) -> Result<TokenClaims, TokenError> {
        let mut parts = token.split('.');
        let (Some(tag), Some(body), Some(sig), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(TokenError::Invalid);
        };
        if tag != TAG {
            return Err(TokenError::Invalid);
        }
        let body = BASE64URL_NOPAD
            .decode(body.as_bytes())
            .map_err(|_| TokenError::Invalid)?;
        let sig = BASE64URL_NOPAD
            .decode(sig.as_bytes())
            .map_err(|_| TokenError::Invalid)?;
        self.verifying
            .verify(&body, &sig, "session token")
            .map_err(|_| TokenError::Invalid)?;
        let claims: TokenClaims = cbor::from_slice(&body).map_err(|_| TokenError::Invalid)?;
        if claims.v != 1 {
            return Err(TokenError::Invalid);
        }
        if claims.exp <= now {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_verify_expire_and_refuse_tampering() {
        let signer = TokenSigner::new(SigningKey::generate().unwrap());
        let (account, device) = (Id::new(), Id::new());
        let (token, exp) = signer.issue(account, device, 1_000, 60).unwrap();
        assert_eq!(exp, 1_060);
        assert!(token.starts_with("lk1."));
        let claims = signer.verify(&token, 1_030).unwrap();
        assert_eq!(claims.account_id, account);
        assert_eq!(claims.device_id, device);
        assert_eq!(signer.verify(&token, 1_060), Err(TokenError::Expired));
        assert_eq!(signer.verify(&token, 2_000), Err(TokenError::Expired));

        // Another server's key, a changed claim, a changed signature, junk.
        let other = TokenSigner::new(SigningKey::generate().unwrap());
        assert_eq!(other.verify(&token, 1_030), Err(TokenError::Invalid));
        let mut parts: Vec<String> = token.split('.').map(String::from).collect();
        let mut body = BASE64URL_NOPAD.decode(parts[1].as_bytes()).unwrap();
        body[10] ^= 1;
        parts[1] = BASE64URL_NOPAD.encode(&body);
        assert_eq!(
            signer.verify(&parts.join("."), 1_030),
            Err(TokenError::Invalid)
        );
        assert_eq!(signer.verify("lk1.abc", 1_030), Err(TokenError::Invalid));
        assert_eq!(signer.verify("", 1_030), Err(TokenError::Invalid));
        // Two tokens in the same second differ.
        let (again, _) = signer.issue(account, device, 1_000, 60).unwrap();
        assert_ne!(again, token);
    }
}
