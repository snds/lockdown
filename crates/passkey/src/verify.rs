//! The relying party's half of WebAuthn (L3 §7.1, §7.2): what Lockdown's
//! own server checks when a person adds a security key or a passkey to
//! their **account** and later signs in with it as a second factor
//! (`account-security-keys`; docs/plan/22-security-depth.md §4). The
//! authenticator half in the rest of this crate makes the same bytes, so
//! the two are tested against each other.
//!
//! Registration checks `clientDataJSON` (type `webauthn.create`, the
//! challenge the server issued, the server's own origin, not a
//! cross-origin frame), then the attestation object's authenticator data:
//! the RP ID hash, user presence, attested credential data, and a COSE key
//! of a kind this verifier can check (ES256 or Ed25519). The attestation
//! statement itself is not judged (any make of key is welcome; `none`,
//! `packed` and the rest are read past), so a key's make is never a claim
//! the server relies on.
//!
//! An assertion checks the same client data (type `webauthn.get`), the RP
//! ID hash, user presence, the signature over `authData ‖
//! SHA-256(clientDataJSON)` under the stored key, and the signature
//! counter: a key that counts must count up, or the sign-in is refused as a
//! possible clone (§6.1.1). A key that never counts (zero both times, as
//! synced passkeys do) passes.

use ciborium::Value;
use ring::signature::{ECDSA_P256_SHA256_ASN1, ED25519, UnparsedPublicKey};

use crate::encode::from_b64url;
use crate::{Alg, COSE_EDDSA, COSE_ES256, sha256};

/// What the ceremony must have been made for.
#[derive(Clone, Copy, Debug)]
pub struct Expected<'a> {
    /// The server's host name.
    pub rp_id: &'a str,
    /// The server's origin, as the browser writes it (`https://host[:port]`).
    pub origin: &'a str,
    /// The challenge the server issued.
    pub challenge: &'a [u8],
}

/// A key the person just added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registered {
    /// The credential id.
    pub credential_id: Vec<u8>,
    /// The COSE public key, as the authenticator wrote it.
    pub cose_key: Vec<u8>,
    /// Its algorithm.
    pub alg: Alg,
    /// The counter it started at.
    pub sign_count: u32,
    /// The make of key it claims (unverified).
    pub aaguid: [u8; 16],
    /// Whether the person was verified (a PIN or biometric on the key).
    pub user_verified: bool,
}

/// An assertion that checked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asserted {
    /// The counter to keep.
    pub sign_count: u32,
    /// Whether the person was verified on the key.
    pub user_verified: bool,
}

fn fail<T>(why: impl Into<String>) -> Result<T, String> {
    Err(why.into())
}

/// Checks `clientDataJSON` for `ceremony` against what was expected.
fn check_client_data(json: &[u8], ceremony: &str, expected: &Expected<'_>) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(|_| "the key's client data isn't JSON".to_string())?;
    let field = |name: &str| value.get(name).and_then(|v| v.as_str());
    if field("type") != Some(ceremony) {
        return fail("the key answered a different kind of request");
    }
    let challenge = field("challenge")
        .and_then(from_b64url)
        .ok_or_else(|| "the key's answer carries no challenge".to_string())?;
    if challenge != expected.challenge {
        return fail("the key answered another request (the challenge differs)");
    }
    if field("origin") != Some(expected.origin) {
        return fail(format!(
            "the key was used on {} rather than {}",
            field("origin").unwrap_or("an unnamed page"),
            expected.origin
        ));
    }
    if value.get("crossOrigin").and_then(|v| v.as_bool()) == Some(true) {
        return fail("the key was used inside a frame from another site");
    }
    Ok(())
}

/// The fixed head of authenticator data: RP ID hash, flags, counter.
struct AuthData<'a> {
    flags: u8,
    sign_count: u32,
    rest: &'a [u8],
}

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const AT: u8 = 0x40;

fn read_auth_data<'a>(bytes: &'a [u8], rp_id: &str) -> Result<AuthData<'a>, String> {
    if bytes.len() < 37 {
        return fail("the key's authenticator data is too short");
    }
    if bytes[..32] != sha256(rp_id.as_bytes()) {
        return fail(format!(
            "the key was registered for another site than {rp_id}"
        ));
    }
    let flags = bytes[32];
    if flags & UP == 0 {
        return fail("the key didn't register a touch (user presence)");
    }
    let sign_count = u32::from_be_bytes([bytes[33], bytes[34], bytes[35], bytes[36]]);
    Ok(AuthData {
        flags,
        sign_count,
        rest: &bytes[37..],
    })
}

fn map_get<'a>(map: &'a [(Value, Value)], key: &Value) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn int(i: i64) -> Value {
    Value::Integer(i.into())
}

/// The algorithm and raw public key of a COSE key this verifier can check.
fn read_cose_key(value: &Value) -> Result<(Alg, Vec<u8>), String> {
    let Value::Map(map) = value else {
        return fail("the key's public key isn't a COSE key");
    };
    let get = |k: i64| map_get(map, &int(k));
    let as_int = |v: Option<&Value>| match v {
        Some(Value::Integer(i)) => i64::try_from(*i).ok(),
        _ => None,
    };
    let as_bytes = |v: Option<&Value>| match v {
        Some(Value::Bytes(b)) => Some(b.clone()),
        _ => None,
    };
    match (as_int(get(1)), as_int(get(3)), as_int(get(-1))) {
        (Some(2), Some(COSE_ES256), Some(1)) => {
            let x = as_bytes(get(-2)).filter(|b| b.len() == 32);
            let y = as_bytes(get(-3)).filter(|b| b.len() == 32);
            match (x, y) {
                (Some(x), Some(y)) => Ok((Alg::Es256, [&[0x04][..], &x, &y].concat())),
                _ => fail("the key's P-256 point is malformed"),
            }
        }
        (Some(1), Some(COSE_EDDSA), Some(6)) => match as_bytes(get(-2)).filter(|b| b.len() == 32) {
            Some(x) => Ok((Alg::Ed25519, x)),
            None => fail("the key's Ed25519 point is malformed"),
        },
        _ => fail("the key uses a kind of public key Lockdown can't check (ES256 and Ed25519 are)"),
    }
}

/// Checks a registration and returns the key to keep.
pub fn verify_registration(
    client_data_json: &[u8],
    attestation_object: &[u8],
    expected: &Expected<'_>,
) -> Result<Registered, String> {
    check_client_data(client_data_json, "webauthn.create", expected)?;
    let object: Value = ciborium::de::from_reader(attestation_object)
        .map_err(|_| "the key's attestation isn't CBOR".to_string())?;
    let Value::Map(object) = object else {
        return fail("the key's attestation is malformed");
    };
    let Some(Value::Bytes(auth)) = map_get(&object, &Value::Text("authData".into())) else {
        return fail("the key's attestation has no authenticator data");
    };
    let data = read_auth_data(auth, expected.rp_id)?;
    if data.flags & AT == 0 {
        return fail("the key sent no credential to register");
    }
    let rest = data.rest;
    if rest.len() < 18 {
        return fail("the key's credential data is too short");
    }
    let aaguid: [u8; 16] = rest[..16].try_into().expect("sixteen bytes");
    let id_len = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
    if id_len == 0 || id_len > 1023 || rest.len() < 18 + id_len {
        return fail("the key's credential id is malformed");
    }
    let credential_id = rest[18..18 + id_len].to_vec();
    let key_bytes = &rest[18 + id_len..];
    let mut reader = key_bytes;
    let key: Value = ciborium::de::from_reader(&mut reader)
        .map_err(|_| "the key's public key isn't CBOR".to_string())?;
    let used = key_bytes.len() - reader.len();
    let (alg, _) = read_cose_key(&key)?;
    Ok(Registered {
        credential_id,
        cose_key: key_bytes[..used].to_vec(),
        alg,
        sign_count: data.sign_count,
        aaguid,
        user_verified: data.flags & UV != 0,
    })
}

/// Checks an assertion under the stored key (`cose_key`) with the counter
/// last kept (`stored_count`).
pub fn verify_assertion(
    client_data_json: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
    cose_key: &[u8],
    stored_count: u32,
    expected: &Expected<'_>,
) -> Result<Asserted, String> {
    check_client_data(client_data_json, "webauthn.get", expected)?;
    let data = read_auth_data(authenticator_data, expected.rp_id)?;
    let key: Value = ciborium::de::from_reader(cose_key)
        .map_err(|_| "the stored key is unreadable".to_string())?;
    let (alg, raw) = read_cose_key(&key)?;
    let mut message = authenticator_data.to_vec();
    message.extend_from_slice(&sha256(client_data_json));
    let verified = match alg {
        Alg::Es256 => {
            UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &raw).verify(&message, signature)
        }
        Alg::Ed25519 => UnparsedPublicKey::new(&ED25519, &raw).verify(&message, signature),
    };
    if verified.is_err() {
        return fail("the key's signature didn't check out");
    }
    if (data.sign_count != 0 || stored_count != 0) && data.sign_count <= stored_count {
        return fail(
            "the key's counter went backwards, which a copied key does; this sign-in was refused",
        );
    }
    Ok(Asserted {
        sign_count: data.sign_count,
        user_verified: data.flags & UV != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ceremony, ClientData, Flags, MakeCredential, get_assertion, make_credential};

    const RP: &str = "vault.example";
    const ORIGIN: &str = "https://vault.example";

    fn made(challenge: &[u8]) -> crate::Made {
        make_credential(&MakeCredential {
            rp_id: RP,
            client_data: &ClientData {
                ceremony: Ceremony::Create,
                challenge: challenge.to_vec(),
                origin: ORIGIN.into(),
                top_origin: None,
            },
            flags: Flags {
                up: true,
                uv: true,
                be: false,
                bs: false,
            },
            algs: &[COSE_ES256],
        })
        .unwrap()
    }

    fn expected(challenge: &[u8]) -> Expected<'_> {
        Expected {
            rp_id: RP,
            origin: ORIGIN,
            challenge,
        }
    }

    #[test]
    fn a_key_registers_and_signs_in_and_every_bent_part_is_refused() {
        let made = made(b"register-challenge");
        let reg = verify_registration(
            made.client_data_json.as_bytes(),
            &made.attestation_object,
            &expected(b"register-challenge"),
        )
        .unwrap();
        assert_eq!(reg.credential_id, made.credential_id);
        assert_eq!(reg.alg, Alg::Es256);
        assert!(reg.user_verified);
        // Another challenge, origin or site: refused.
        assert!(
            verify_registration(
                made.client_data_json.as_bytes(),
                &made.attestation_object,
                &expected(b"other"),
            )
            .is_err()
        );
        let elsewhere = Expected {
            origin: "https://evil.example",
            ..expected(b"register-challenge")
        };
        assert!(
            verify_registration(
                made.client_data_json.as_bytes(),
                &made.attestation_object,
                &elsewhere
            )
            .unwrap_err()
            .contains("evil.example")
        );
        let other_rp = Expected {
            rp_id: "other.example",
            ..expected(b"register-challenge")
        };
        assert!(
            verify_registration(
                made.client_data_json.as_bytes(),
                &made.attestation_object,
                &other_rp
            )
            .is_err()
        );

        let client = ClientData {
            ceremony: Ceremony::Get,
            challenge: b"sign-in".to_vec(),
            origin: ORIGIN.into(),
            top_origin: None,
        };
        let flags = Flags {
            up: true,
            uv: false,
            be: false,
            bs: false,
        };
        let a = get_assertion(&made.pkcs8, RP, &client, flags).unwrap();
        let ok = verify_assertion(
            a.client_data_json.as_bytes(),
            &a.authenticator_data,
            &a.signature,
            &reg.cose_key,
            0,
            &expected(b"sign-in"),
        )
        .unwrap();
        assert_eq!(ok.sign_count, 0);
        // A bent signature, a create answer passed as a get, a framed page.
        let mut bent = a.signature.clone();
        let last = bent.len() - 1;
        bent[last] ^= 1;
        assert!(
            verify_assertion(
                a.client_data_json.as_bytes(),
                &a.authenticator_data,
                &bent,
                &reg.cose_key,
                0,
                &expected(b"sign-in"),
            )
            .is_err()
        );
        let framed = ClientData {
            top_origin: Some("https://evil.example".into()),
            ..client.clone()
        };
        let b = get_assertion(&made.pkcs8, RP, &framed, flags).unwrap();
        assert!(
            verify_assertion(
                b.client_data_json.as_bytes(),
                &b.authenticator_data,
                &b.signature,
                &reg.cose_key,
                0,
                &expected(b"sign-in"),
            )
            .unwrap_err()
            .contains("frame")
        );
        // No touch: refused.
        let untouched = get_assertion(&made.pkcs8, RP, &client, Flags::default()).unwrap();
        assert!(
            verify_assertion(
                untouched.client_data_json.as_bytes(),
                &untouched.authenticator_data,
                &untouched.signature,
                &reg.cose_key,
                0,
                &expected(b"sign-in"),
            )
            .unwrap_err()
            .contains("touch")
        );
        // A counting key that goes backwards is a clone.
        assert!(
            verify_assertion(
                a.client_data_json.as_bytes(),
                &a.authenticator_data,
                &a.signature,
                &reg.cose_key,
                5,
                &expected(b"sign-in"),
            )
            .unwrap_err()
            .contains("backwards")
        );
    }
}
