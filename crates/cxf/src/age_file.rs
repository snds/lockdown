//! Lockdown exports locked to public keys (docs/plan/23-bridges.md §11;
//! 02-model §11 "age-recipient keys"): the Lockdown document sealed in the
//! [age v1 format](https://age-encryption.org/v1) to one or more X25519
//! recipients (`age1…`), so the file opens only with a matching private
//! key (`AGE-SECRET-KEY-1…`), in Lockdown or in any age tool.
//!
//! Written here over the primitives the tree already links (X25519,
//! HKDF-SHA-256, HMAC-SHA-256, ChaCha20-Poly1305), because the `age` crate
//! doesn't fit the tree: 0.12 needs a pre-release `kem` that conflicts
//! with the one ML-KEM 0.3 uses here, and 0.11 brings an unmaintained
//! proc-macro crate that `cargo deny` refuses (RUSTSEC-2026-0173). The
//! tests read files written by the reference implementation (rage,
//! through `pyrage`) and check theirs open ours.
//!
//! What the format is, in short: a text header naming the format, one
//! `X25519` stanza per recipient (an ephemeral public key, and the
//! 16-byte file key sealed under HKDF of the shared secret), and an
//! HMAC over the header; then a 16-byte nonce and the payload in 64 KiB
//! ChaCha20-Poly1305 chunks whose nonces count up and mark the last one.
//! Only X25519 stanzas are written or read; a file sealed with a
//! passphrase (scrypt) or an SSH key is refused in words.
//!
//! The file is hostile on the way in: the header is capped in size,
//! lines and stanzas, every base64 must be canonical, the MAC is checked
//! before the payload is touched, a shared secret of zeros is refused,
//! and each chunk is authenticated before it is kept.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::{CxfError, Limits, Result};

/// The first line of every binary age file.
pub const AGE_MAGIC: &[u8] = b"age-encryption.org/v1\n";
/// The first line of an armored one.
pub const AGE_ARMOR: &[u8] = b"-----BEGIN AGE ENCRYPTED FILE-----";
const ARMOR_END: &str = "-----END AGE ENCRYPTED FILE-----";
/// The most recipients one export is locked to.
pub const MAX_RECIPIENTS: usize = 20;
/// The most stanzas a header Lockdown reads may carry.
const MAX_STANZAS: usize = 64;
/// The largest header Lockdown reads.
const MAX_HEADER: usize = 64 * 1024;
/// Plaintext per payload chunk.
const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const X25519_INFO: &[u8] = b"age-encryption.org/v1/X25519";
const RECIPIENT_HRP: &str = "age";
const IDENTITY_HRP: &str = "age-secret-key-";

fn age_err(why: impl Into<String>) -> CxfError {
    CxfError::Parse {
        format: "age",
        why: why.into(),
    }
}

/// Whether `bytes` is an age file.
pub fn is_age(bytes: &[u8]) -> bool {
    bytes.starts_with(AGE_MAGIC) || bytes.starts_with(AGE_ARMOR)
}

// --- Bech32 (BIP 173), which age keys are written in ------------------------------

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(*v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut v: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    v.push(0);
    v.extend(hrp.bytes().map(|b| b & 31));
    v
}

/// Regroups bits; `pad` adds a last partial group, else it must be zeros.
fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let (mut acc, mut bits) = (0u32, 0u32);
    let max = (1u32 << to) - 1;
    let mut out = Vec::new();
    for v in data {
        let v = u32::from(*v);
        if v >> from != 0 {
            return None;
        }
        acc = (acc << from) | v;
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & max) as u8);
        }
    }
    if pad {
        if bits > 0 {
            out.push(((acc << (to - bits)) & max) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & max) != 0 {
        return None;
    }
    Some(out)
}

/// Bech32 of `data` under `hrp` (lowercase).
fn bech32_encode(hrp: &str, data: &[u8]) -> String {
    let five = convert_bits(data, 8, 5, true).unwrap_or_default();
    let mut values = hrp_expand(hrp);
    values.extend(&five);
    values.extend([0u8; 6]);
    let pm = polymod(&values) ^ 1;
    let mut out = String::with_capacity(hrp.len() + 1 + five.len() + 6);
    out.push_str(hrp);
    out.push('1');
    for v in five {
        out.push(char::from(CHARSET[usize::from(v)]));
    }
    for i in 0..6 {
        out.push(char::from(CHARSET[((pm >> (5 * (5 - i))) & 31) as usize]));
    }
    out
}

/// Reads Bech32 text: its human part (lowercased) and its bytes. One case
/// throughout, a valid checksum, no stray characters.
fn bech32_decode(text: &str) -> Option<(String, Vec<u8>)> {
    if text.len() > 200 || !text.is_ascii() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if text != lower && text != text.to_ascii_uppercase() {
        return None;
    }
    let pos = lower.rfind('1')?;
    let (hrp, rest) = (&lower[..pos], &lower[pos + 1..]);
    if hrp.is_empty() || rest.len() < 6 {
        return None;
    }
    let values: Vec<u8> = rest
        .bytes()
        .map(|b| CHARSET.iter().position(|c| *c == b).map(|p| p as u8))
        .collect::<Option<_>>()?;
    let mut check = hrp_expand(hrp);
    check.extend(&values);
    if polymod(&check) != 1 {
        return None;
    }
    let data = convert_bits(&values[..values.len() - 6], 5, 8, false)?;
    Some((hrp.to_string(), data))
}

// --- keys ------------------------------------------------------------------------

/// A recipient: an X25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Recipient(pub [u8; 32]);

impl Recipient {
    /// Its `age1…` form.
    pub fn encode(&self) -> String {
        bech32_encode(RECIPIENT_HRP, &self.0)
    }
}

/// A private key, wiped when dropped.
pub struct Identity(Zeroizing<[u8; 32]>);

impl Identity {
    /// A new random private key.
    pub fn generate() -> std::result::Result<Self, String> {
        let k = lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?;
        Ok(Self(Zeroizing::new(k)))
    }

    /// Its public half.
    pub fn recipient(&self) -> Recipient {
        Recipient(PublicKey::from(&StaticSecret::from(*self.0)).to_bytes())
    }

    /// Its `AGE-SECRET-KEY-1…` form, wiped when dropped.
    pub fn encode(&self) -> Zeroizing<String> {
        Zeroizing::new(bech32_encode(IDENTITY_HRP, self.0.as_ref()).to_ascii_uppercase())
    }

    /// Reads `AGE-SECRET-KEY-1…`.
    pub fn parse(text: &str) -> Option<Self> {
        let (hrp, data) = bech32_decode(text.trim())?;
        if hrp != IDENTITY_HRP {
            return None;
        }
        let key: [u8; 32] = data.try_into().ok()?;
        Some(Self(Zeroizing::new(key)))
    }
}

/// Reads one recipient (`age1…`), with its words when it isn't one.
pub fn recipient(text: &str) -> std::result::Result<Recipient, String> {
    let t = text.trim();
    let shown: String = t.chars().take(12).collect();
    let bad = || format!("'{shown}…' isn't an age public key (they start with age1)");
    let (hrp, data) = bech32_decode(t).ok_or_else(bad)?;
    if hrp != RECIPIENT_HRP {
        return Err(bad());
    }
    Ok(Recipient(data.try_into().map_err(|_| bad())?))
}

// --- the format -----------------------------------------------------------------

fn b64(data: &[u8]) -> String {
    data_encoding::BASE64_NOPAD.encode(data)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    data_encoding::BASE64_NOPAD.decode(text.as_bytes()).ok()
}

fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut out = Zeroizing::new([0u8; 32]);
    let h = Hkdf::<Sha256>::new(Some(salt), ikm);
    // 32 bytes is always a valid HKDF-SHA-256 length.
    let _ = h.expand(info, out.as_mut());
    out
}

fn header_mac(file_key: &[u8; 16], header: &[u8]) -> Result<[u8; 32]> {
    let key = hkdf32(&[], file_key, b"header");
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key.as_ref())
        .map_err(|_| age_err("the header key didn't take"))?;
    mac.update(header);
    Ok(mac.finalize().into_bytes().into())
}

fn chunk_nonce(counter: u64, last: bool) -> Nonce {
    let mut n = [0u8; 12];
    n[3..11].copy_from_slice(&counter.to_be_bytes());
    n[11] = u8::from(last);
    Nonce::from(n)
}

/// The X25519 wrap key for one stanza.
fn wrap_key(shared: &[u8; 32], ephemeral: &[u8; 32], recipient: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(ephemeral);
    salt[32..].copy_from_slice(recipient);
    hkdf32(&salt, shared, X25519_INFO)
}

/// Seals `plain` to every recipient in `recipients` (one to
/// [`MAX_RECIPIENTS`], duplicates folded).
pub fn seal_to(plain: &[u8], recipients: &[String]) -> std::result::Result<Vec<u8>, String> {
    let mut keys: Vec<Recipient> = Vec::new();
    for r in recipients
        .iter()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
    {
        let key = recipient(r)?;
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    if keys.is_empty() {
        return Err("name at least one age public key to lock the file to".into());
    }
    if keys.len() > MAX_RECIPIENTS {
        return Err(format!(
            "a file can be locked to at most {MAX_RECIPIENTS} keys"
        ));
    }
    let random = |n: usize| -> std::result::Result<Zeroizing<Vec<u8>>, String> {
        let mut b = Zeroizing::new(vec![0u8; n]);
        lockdown_crypto::rand::random_bytes(&mut b).map_err(|e| e.to_string())?;
        Ok(b)
    };
    let mut file_key = Zeroizing::new([0u8; 16]);
    file_key.copy_from_slice(&random(16)?);
    let mut header = String::from("age-encryption.org/v1\n");
    for r in &keys {
        let mut e = Zeroizing::new([0u8; 32]);
        e.copy_from_slice(&random(32)?);
        let secret = StaticSecret::from(*e);
        let ephemeral = PublicKey::from(&secret).to_bytes();
        let shared = secret.diffie_hellman(&PublicKey::from(r.0));
        if !shared.was_contributory() {
            return Err("that public key can't be used (it is a low-order point)".into());
        }
        let key = wrap_key(shared.as_bytes(), &ephemeral, &r.0);
        let body = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*key))
            .encrypt(&Nonce::from([0u8; 12]), file_key.as_ref())
            .map_err(|_| "couldn't seal the file key".to_string())?;
        header.push_str(&format!("-> X25519 {}\n{}\n", b64(&ephemeral), b64(&body)));
    }
    header.push_str("---");
    let mac = header_mac(&file_key, header.as_bytes()).map_err(|e| e.to_string())?;
    header.push_str(&format!(" {}\n", b64(&mac)));
    let nonce = random(16)?;
    let payload_key = hkdf32(&nonce, file_key.as_ref(), b"payload");
    let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*payload_key));
    let mut out =
        Vec::with_capacity(header.len() + 16 + plain.len() + (plain.len() / CHUNK + 1) * TAG);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&nonce);
    let chunks: Vec<&[u8]> = if plain.is_empty() {
        vec![&[][..]]
    } else {
        plain.chunks(CHUNK).collect()
    };
    let last = chunks.len() - 1;
    for (i, c) in chunks.into_iter().enumerate() {
        let ct = cipher
            .encrypt(&chunk_nonce(i as u64, i == last), c)
            .map_err(|_| "couldn't seal the file".to_string())?;
        out.extend_from_slice(&ct);
    }
    Ok(out)
}

/// One stanza as read.
struct Stanza {
    kind: String,
    args: Vec<String>,
    body: Vec<u8>,
}

/// Parses the header: the stanzas, the bytes the MAC covers, the MAC, and
/// where the payload starts.
fn parse_header(bytes: &[u8]) -> Result<(Vec<Stanza>, usize, Vec<u8>, usize)> {
    let limit = bytes.len().min(MAX_HEADER);
    let mut pos = AGE_MAGIC.len();
    let mut stanzas = Vec::new();
    let line_at = |pos: usize| -> Result<(&str, usize)> {
        let end = bytes[pos..limit]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| pos + i)
            .ok_or_else(|| age_err("the header has no end"))?;
        let line =
            std::str::from_utf8(&bytes[pos..end]).map_err(|_| age_err("the header isn't text"))?;
        Ok((line, end + 1))
    };
    loop {
        let (line, next) = line_at(pos)?;
        if let Some(rest) = line.strip_prefix("---") {
            let mac_text = rest
                .strip_prefix(' ')
                .ok_or_else(|| age_err("the header's last line is malformed"))?;
            let mac = unb64(mac_text)
                .filter(|m| m.len() == 32)
                .ok_or_else(|| age_err("the header's MAC isn't readable"))?;
            return Ok((stanzas, pos + 3, mac, next));
        }
        let rest = line
            .strip_prefix("-> ")
            .ok_or_else(|| age_err("the header has a line that isn't a stanza"))?;
        let mut parts = rest.split(' ');
        let kind = parts.next().unwrap_or("").to_string();
        let args: Vec<String> = parts.map(str::to_string).collect();
        if kind.is_empty() || args.iter().any(String::is_empty) || stanzas.len() >= MAX_STANZAS {
            return Err(age_err("the header's stanzas are malformed"));
        }
        // The body: base64 lines of 64 characters, ending with a shorter one.
        let mut body_text = String::new();
        pos = next;
        loop {
            let (l, n) = line_at(pos)?;
            pos = n;
            if l.len() > 64 {
                return Err(age_err("a stanza's body has an over-long line"));
            }
            body_text.push_str(l);
            if l.len() < 64 {
                break;
            }
        }
        let body =
            unb64(&body_text).ok_or_else(|| age_err("a stanza's body isn't canonical base64"))?;
        stanzas.push(Stanza { kind, args, body });
    }
}

/// Undoes the armor: the PEM-like base64 lines between the begin and end
/// lines, padded, at most 64 columns.
fn dearmor(bytes: &[u8], cap: usize) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).map_err(|_| age_err("the armored file isn't text"))?;
    let mut lines = text.trim_end().lines().map(|l| l.trim_end_matches('\r'));
    if lines.next().map(str::as_bytes) != Some(AGE_ARMOR) {
        return Err(age_err("the armor doesn't begin right"));
    }
    let mut b64text = String::new();
    let mut ended = false;
    for l in lines {
        if l == ARMOR_END {
            ended = true;
            break;
        }
        if l.len() > 64 || b64text.len() > cap.saturating_mul(2) {
            return Err(age_err("the armor has an over-long line"));
        }
        b64text.push_str(l);
    }
    if !ended {
        return Err(age_err("the armor doesn't end"));
    }
    data_encoding::BASE64
        .decode(b64text.as_bytes())
        .map_err(|_| age_err("the armor isn't base64"))
}

/// Opens an age file with an X25519 private key (`AGE-SECRET-KEY-1…`,
/// pasted where a password goes), reading at most the backup cap.
pub fn open_with(bytes: &[u8], identity: &[u8], limits: &Limits) -> Result<Zeroizing<Vec<u8>>> {
    let cap = limits.max_bytes.max(Limits::backup().max_bytes);
    let text = Zeroizing::new(
        std::str::from_utf8(identity)
            .map_err(|_| CxfError::WrongPassword)?
            .to_string(),
    );
    let identity = text
        .lines()
        .map(str::trim)
        .find(|l| l.to_ascii_uppercase().starts_with("AGE-SECRET-KEY-1"))
        .ok_or_else(|| {
            age_err(
                "this file is locked to a public key; paste the matching private key (AGE-SECRET-KEY-1…) as its password",
            )
        })
        .and_then(|l| Identity::parse(l).ok_or(CxfError::WrongPassword))?;
    let dearmored;
    let bytes = if bytes.starts_with(AGE_ARMOR) {
        dearmored = dearmor(bytes, cap)?;
        dearmored.as_slice()
    } else {
        bytes
    };
    if !bytes.starts_with(AGE_MAGIC) {
        return Err(age_err("it isn't an age v1 file"));
    }
    let (stanzas, mac_end, mac, payload_at) = parse_header(bytes)?;
    if stanzas.iter().any(|s| s.kind == "scrypt") {
        return Err(age_err(
            "this age file is locked with a passphrase, not a key; Lockdown opens age files locked to a public key",
        ));
    }
    let secret = StaticSecret::from(*identity.0);
    let ours = PublicKey::from(&secret).to_bytes();
    let mut file_key: Option<Zeroizing<[u8; 16]>> = None;
    for s in stanzas.iter().filter(|s| s.kind == "X25519") {
        let Some(ephemeral) = s
            .args
            .first()
            .filter(|_| s.args.len() == 1)
            .and_then(|a| unb64(a))
        else {
            return Err(age_err("an X25519 stanza is malformed"));
        };
        let ephemeral: [u8; 32] = ephemeral
            .try_into()
            .map_err(|_| age_err("an X25519 stanza is malformed"))?;
        if s.body.len() != 32 {
            return Err(age_err("an X25519 stanza is malformed"));
        }
        let shared = secret.diffie_hellman(&PublicKey::from(ephemeral));
        if !shared.was_contributory() {
            continue;
        }
        let key = wrap_key(shared.as_bytes(), &ephemeral, &ours);
        if let Ok(fk) = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*key))
            .decrypt(&Nonce::from([0u8; 12]), s.body.as_slice())
        {
            let mut k = Zeroizing::new([0u8; 16]);
            k.copy_from_slice(&fk);
            file_key = Some(k);
            break;
        }
    }
    let file_key = file_key.ok_or(CxfError::WrongPassword)?;
    let expected = header_mac(&file_key, &bytes[..mac_end])?;
    if expected.iter().zip(&mac).fold(0u8, |a, (x, y)| a | (x ^ y)) != 0 {
        return Err(CxfError::Tampered(
            "the age file's header was changed".into(),
        ));
    }
    let rest = &bytes[payload_at..];
    if rest.len() < 16 + TAG {
        return Err(CxfError::Tampered("the age file is cut short".into()));
    }
    let (nonce, mut body) = rest.split_at(16);
    let payload_key = hkdf32(nonce, file_key.as_ref(), b"payload");
    let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*payload_key));
    let mut out = Zeroizing::new(Vec::new());
    let mut counter = 0u64;
    loop {
        let take = body.len().min(CHUNK + TAG);
        let (chunk, after) = body.split_at(take);
        let last = after.is_empty();
        let plain = cipher
            .decrypt(&chunk_nonce(counter, last), chunk)
            .map_err(|_| CxfError::Tampered("the age file failed its check part-way".into()))?;
        if last && plain.is_empty() && counter > 0 {
            return Err(CxfError::Tampered(
                "the age file ends with an empty part".into(),
            ));
        }
        if out.len() + plain.len() > cap {
            return Err(CxfError::TooLarge {
                found: out.len() + plain.len(),
                limit: cap,
            });
        }
        out.extend_from_slice(&plain);
        if last {
            return Ok(out);
        }
        counter += 1;
        body = after;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bech32_round_trips_and_refuses_a_changed_character() {
        let id = Identity::generate().unwrap();
        let r = id.recipient();
        let text = r.encode();
        assert!(text.starts_with("age1") && text.len() == 62, "{text}");
        assert_eq!(recipient(&text).unwrap(), r);
        let secret = id.encode();
        assert!(secret.starts_with("AGE-SECRET-KEY-1"));
        assert_eq!(Identity::parse(&secret).unwrap().recipient(), r);
        let mut bad: Vec<char> = text.chars().collect();
        bad[10] = if bad[10] == 'q' { 'p' } else { 'q' };
        assert!(recipient(&bad.into_iter().collect::<String>()).is_err());
        assert!(
            recipient(&text.to_ascii_uppercase()).is_ok(),
            "one case throughout is fine"
        );
        let mixed = format!("Age1{}", &text[4..]);
        assert!(recipient(&mixed).is_err(), "mixed case isn't");
        assert!(
            recipient(&secret).is_err(),
            "a private key isn't a recipient"
        );
    }

    #[test]
    fn a_file_locked_to_two_keys_opens_with_either_and_nothing_else() {
        let (a, b, other) = (
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
        );
        for plain in [
            &b""[..],
            b"{\"format\":\"lockdown-backup\"}",
            &vec![7u8; CHUNK],
            &vec![9u8; CHUNK * 2 + 5],
        ] {
            let sealed = seal_to(
                plain,
                &[
                    a.recipient().encode(),
                    format!("  {}  ", b.recipient().encode()),
                    a.recipient().encode(),
                ],
            )
            .unwrap();
            assert!(is_age(&sealed));
            assert_eq!(
                sealed.windows(9).filter(|w| w == b"-> X25519").count(),
                2,
                "duplicates fold"
            );
            for id in [&a, &b] {
                let opened =
                    open_with(&sealed, id.encode().as_bytes(), &Limits::default()).unwrap();
                assert_eq!(opened.as_slice(), plain);
            }
            assert!(matches!(
                open_with(&sealed, other.encode().as_bytes(), &Limits::default()),
                Err(CxfError::WrongPassword)
            ));
        }
        let sealed = seal_to(b"hello", &[a.recipient().encode()]).unwrap();
        assert!(matches!(
            open_with(&sealed, b"a password", &Limits::default()),
            Err(CxfError::Parse { .. })
        ));
        // Every flipped byte is refused, never a panic.
        for at in (0..sealed.len()).step_by(3) {
            let mut f = sealed.clone();
            f[at] ^= 0x04;
            assert!(
                open_with(&f, a.encode().as_bytes(), &Limits::default()).is_err(),
                "flip at {at}"
            );
        }
        for cut in 0..sealed.len() {
            let _ = open_with(&sealed[..cut], a.encode().as_bytes(), &Limits::default());
        }
    }

    #[test]
    fn only_real_public_keys_are_taken() {
        assert!(seal_to(b"x", &[]).is_err());
        assert!(seal_to(b"x", &["   ".into()]).is_err());
        let e = seal_to(b"x", &["ssh-ed25519 AAAA".into()]).unwrap_err();
        assert!(e.contains("isn't an age public key"), "{e}");
        let many: Vec<String> = (0..=MAX_RECIPIENTS)
            .map(|_| Identity::generate().unwrap().recipient().encode())
            .collect();
        assert!(seal_to(b"x", &many).is_err());
        // The all-zero point is low order.
        assert!(seal_to(b"x", &[Recipient([0; 32]).encode()]).is_err());
    }
}
