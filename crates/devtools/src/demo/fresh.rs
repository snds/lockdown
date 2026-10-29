//! Keys made when the demo opens, so no pack ever carries a private key:
//! a passkey's P-256 key, credential id and user handle (the passkey
//! crate's own generator), and an SSH key (Ed25519, written in OpenSSH's
//! own private-key format, unencrypted, with its public line and SHA-256
//! fingerprint as `ssh-keygen -l` prints it).

use data_encoding::{BASE64, BASE64_NOPAD};
use lockdown_vault::{FieldKind, FieldValue, PlainField};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).map_err(|e| format!("the system's random source failed: {e}"))?;
    Ok(out)
}

fn set(fields: &mut Vec<PlainField>, field: PlainField) {
    match fields.iter_mut().find(|f| f.field_id == field.field_id) {
        Some(f) => *f = field,
        None => fields.push(field),
    }
}

/// Gives every `passkey.<n>` the item names a new key, credential id and
/// user handle. Returns how many it made.
pub fn passkeys(fields: &mut Vec<PlainField>) -> Result<usize, String> {
    let mut made = 0;
    for n in 0.. {
        if !fields
            .iter()
            .any(|f| f.field_id == format!("passkey.{n}.rp_id"))
        {
            break;
        }
        let (_, pkcs8) = lockdown_passkey::CredentialKey::generate().map_err(|e| e.to_string())?;
        let credential_id = random::<16>()?.to_vec();
        let user_handle = random::<16>()?.to_vec();
        for (id, label, bytes) in [
            ("credential_id", "Credential", credential_id),
            ("user_handle", "User handle", user_handle),
            ("key", "Private key", pkcs8.to_vec()),
        ] {
            let mut f = PlainField::new(
                format!("passkey.{n}.{id}"),
                FieldKind::Passkey,
                label,
                FieldValue::Bytes(bytes),
            );
            f.section = Some("Passkey".into());
            set(fields, f);
        }
        made += 1;
    }
    Ok(made)
}

/// An SSH string: a length, then the bytes.
fn ssh_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// A new Ed25519 key: the OpenSSH private key file, the public line and
/// the fingerprint.
pub struct SshKey {
    /// `-----BEGIN OPENSSH PRIVATE KEY-----` … , unencrypted.
    pub private: Zeroizing<String>,
    /// `ssh-ed25519 AAAA… <comment>`.
    pub public: String,
    /// `SHA256:…`.
    pub fingerprint: String,
}

/// Makes an Ed25519 key with `comment`.
pub fn ssh_key(comment: &str) -> Result<SshKey, String> {
    let seed = Zeroizing::new(random::<32>()?);
    let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
    let public = signing.verifying_key().to_bytes();
    let mut blob = Vec::new();
    ssh_string(&mut blob, b"ssh-ed25519");
    ssh_string(&mut blob, &public);

    let check = u32::from_be_bytes(random::<4>()?);
    let mut private = Zeroizing::new(Vec::new());
    private.extend_from_slice(&check.to_be_bytes());
    private.extend_from_slice(&check.to_be_bytes());
    ssh_string(&mut private, b"ssh-ed25519");
    ssh_string(&mut private, &public);
    let mut pair = Zeroizing::new(Vec::with_capacity(64));
    pair.extend_from_slice(seed.as_ref());
    pair.extend_from_slice(&public);
    ssh_string(&mut private, &pair);
    ssh_string(&mut private, comment.as_bytes());
    let mut pad = 1u8;
    while private.len() % 8 != 0 {
        private.push(pad);
        pad += 1;
    }

    let mut file = Zeroizing::new(Vec::new());
    file.extend_from_slice(b"openssh-key-v1\0");
    ssh_string(&mut file, b"none");
    ssh_string(&mut file, b"none");
    ssh_string(&mut file, b"");
    file.extend_from_slice(&1u32.to_be_bytes());
    ssh_string(&mut file, &blob);
    ssh_string(&mut file, &private);

    let body = Zeroizing::new(BASE64.encode(&file));
    let mut armored = Zeroizing::new(String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n"));
    for line in body.as_bytes().chunks(70) {
        armored.push_str(std::str::from_utf8(line).map_err(|e| e.to_string())?);
        armored.push('\n');
    }
    armored.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    Ok(SshKey {
        private: armored,
        public: format!("ssh-ed25519 {} {comment}", BASE64.encode(&blob)),
        fingerprint: format!("SHA256:{}", BASE64_NOPAD.encode(&Sha256::digest(&blob))),
    })
}

/// Fills an SSH key item's key fields with a new key; the comment is the
/// item's title.
pub fn ssh_fields(fields: &mut Vec<PlainField>, title: &str) -> Result<(), String> {
    let comment: String = title
        .chars()
        .map(|c| if c.is_ascii_graphic() { c } else { '-' })
        .collect();
    let key = ssh_key(&format!("{comment}@demo.test"))?;
    let mut private = PlainField::hidden("ssh.private_key", "Private key", key.private.as_str());
    private.section = Some("Key".into());
    set(fields, private);
    for (id, label, value) in [
        ("ssh.public_key", "Public key", key.public.as_str()),
        ("ssh.fingerprint", "Fingerprint", key.fingerprint.as_str()),
    ] {
        let mut f = PlainField::text(id, label, value);
        f.section = Some("Public".into());
        set(fields, f);
    }
    if !fields.iter().any(|f| f.field_id == "ssh.key_type") {
        let mut f = PlainField::text("ssh.key_type", "Key type", "Ed25519");
        f.section = Some("Key".into());
        set(fields, f);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_key_is_openssh_shaped_and_fresh_each_time() {
        let a = ssh_key("sam@demo.test").unwrap();
        let b = ssh_key("sam@demo.test").unwrap();
        assert!(
            a.private
                .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n")
        );
        assert!(a.private.ends_with("-----END OPENSSH PRIVATE KEY-----\n"));
        assert!(a.private.lines().all(|l| l.len() <= 70));
        assert!(
            a.public
                .starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI")
        );
        assert!(a.public.ends_with(" sam@demo.test"));
        assert!(a.fingerprint.starts_with("SHA256:") && a.fingerprint.len() == 50);
        assert_ne!(a.public, b.public);
        // The body decodes, starts with the magic, and carries the public key.
        let body: String = a
            .private
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        let raw = BASE64.decode(body.as_bytes()).unwrap();
        assert!(raw.starts_with(b"openssh-key-v1\0"));
        let blob = BASE64
            .decode(a.public.split(' ').nth(1).unwrap().as_bytes())
            .unwrap();
        assert!(raw.windows(blob.len()).any(|w| w == blob.as_slice()));
    }

    #[test]
    fn every_named_passkey_gets_its_own_key() {
        let mut fields = vec![
            PlainField::text("passkey.0.rp_id", "Website", "shop.example.com"),
            PlainField::text("passkey.1.rp_id", "Website", "shop.example.com"),
        ];
        assert_eq!(passkeys(&mut fields).unwrap(), 2);
        let key = |n: u32| {
            fields
                .iter()
                .find(|f| f.field_id == format!("passkey.{n}.key"))
                .map(|f| f.value.clone())
                .unwrap()
        };
        assert_ne!(key(0), key(1));
        if let FieldValue::Bytes(k) = &key(0) {
            lockdown_passkey::CredentialKey::from_pkcs8(k).unwrap();
        } else {
            panic!("not bytes");
        }
    }
}
