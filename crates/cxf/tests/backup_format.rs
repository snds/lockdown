//! The published backup format (`docs/format/backup-v1.md`) against its
//! test vectors: a decoder written here from the spec alone (no Lockdown
//! code) opens the sealed fixture, finds the plain fixture's document
//! inside, and opens what the crate writes again; the crate reads both
//! fixtures back to the same items.
//!
//! The vectors are sealed at the test Argon2id cost (256 KiB, 1 pass, 1
//! lane) so the suite stays fast; a release build refuses that cost, as
//! the spec says a reader must.

use std::path::PathBuf;

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use lockdown_cxf::Limits;
use lockdown_cxf::archive::{
    self, DocAttachment, DocField, DocItem, DocVersion, Document, LeftOut,
};
use lockdown_vault::{FieldKind, FieldValue, Id, PlainField};
use serde_json::Value;

const PASSWORD: &[u8] = b"correct horse battery staple";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn b64url(s: &str) -> Vec<u8> {
    data_encoding::BASE64URL_NOPAD
        .decode(s.trim_end_matches('=').as_bytes())
        .unwrap()
}

/// The spec's §3 decoder, step by step.
fn spec_open(file: &[u8], password: &[u8]) -> Vec<u8> {
    // 3.1: magic, version, header length, header.
    assert_eq!(&file[..8], b"LDBACKUP");
    assert_eq!(file[8], 1);
    let len = u16::from_be_bytes([file[9], file[10]]) as usize;
    let header: Value = serde_json::from_slice(&file[11..11 + len]).unwrap();
    assert_eq!(header["v"], 1);
    assert_eq!(header["payload"], "lockdown-backup+json");
    // 3.2: the key.
    let kdf = &header["kdf"];
    assert_eq!(kdf["alg"], "argon2id");
    let params = argon2::Params::new(
        kdf["m_kib"].as_u64().unwrap() as u32,
        kdf["t"].as_u64().unwrap() as u32,
        kdf["p"].as_u64().unwrap() as u32,
        Some(32),
    )
    .unwrap();
    let mut stretched = [0u8; 32];
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(
            password,
            &b64url(kdf["salt"].as_str().unwrap()),
            &mut stretched,
        )
        .unwrap();
    let mut key = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha512>::new(None, &stretched)
        .expand(b"lockdown/backup/v1/file-key", &mut key)
        .unwrap();
    // 3.3: the chunks.
    let aead = &header["aead"];
    assert_eq!(aead["alg"], "xchacha20poly1305");
    let chunk = aead["chunk"].as_u64().unwrap() as usize;
    assert_eq!(chunk, 1 << 20);
    let prefix = b64url(aead["nonce_prefix"].as_str().unwrap());
    assert_eq!(prefix.len(), 19);
    let aad = &file[..11 + len];
    let body = &file[11 + len..];
    let cipher = XChaCha20Poly1305::new(&chacha20poly1305::Key::from(key));
    let frames: Vec<&[u8]> = body.chunks(chunk + 16).collect();
    let mut out = Vec::new();
    for (i, frame) in frames.iter().enumerate() {
        let mut n = [0u8; 24];
        n[..19].copy_from_slice(&prefix);
        n[19..23].copy_from_slice(&(i as u32).to_be_bytes());
        n[23] = u8::from(i == frames.len() - 1);
        out.extend(
            cipher
                .decrypt(&XNonce::from(n), Payload { msg: frame, aad })
                .expect("each chunk opens in its place"),
        );
    }
    out
}

#[test]
fn the_sealed_vector_opens_by_the_spec_alone_and_holds_the_plain_vector() {
    let plain = fixture("backup-v1.json");
    let opened = spec_open(&fixture("backup-v1.lockdown"), PASSWORD);
    let a: Value = serde_json::from_slice(&opened).unwrap();
    let b: Value = serde_json::from_slice(&plain).unwrap();
    assert_eq!(
        a, b,
        "the sealed vector carries the plain vector's document"
    );

    // §2: the document's shape, read as plain JSON.
    assert_eq!(b["format"], "lockdown-backup");
    assert_eq!(b["v"], 1);
    let items = b["items"].as_array().unwrap();
    let login = &items[0];
    assert_eq!(login["kind"], "login");
    assert_eq!(login["fields"][0]["id"], "title");
    let pw = login["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "pw")
        .unwrap();
    assert_eq!(pw["kind"], "hidden");
    assert_eq!(pw["value"]["text"], "correct horse battery staple");
    let att = &login["attachments"][0];
    let data = b64url(att["data"].as_str().unwrap());
    assert_eq!(att["size"].as_u64().unwrap() as usize, data.len());
    assert_eq!(att["blake3"], blake3::hash(&data).to_hex().as_str());
    assert_eq!(login["history"].as_array().unwrap().len(), 1);
    assert_eq!(b["left_out"][0]["reason"], "crown-jewel");
}

#[test]
fn what_the_crate_seals_opens_by_the_spec_and_both_vectors_read_back_the_same() {
    let plain = fixture("backup-v1.json");
    let doc = Document::parse(&plain, &Limits::default()).unwrap();
    let kdf = archive::Kdf::fresh().unwrap();
    let key = kdf.derive(PASSWORD).unwrap();
    let sealed = archive::seal(&doc.to_bytes().unwrap(), &key, &kdf, 1_760_000_000).unwrap();
    let again: Value = serde_json::from_slice(&spec_open(&sealed, PASSWORD)).unwrap();
    assert_eq!(again, serde_json::from_slice::<Value>(&plain).unwrap());

    let from_sealed = archive::read(
        &fixture("backup-v1.lockdown"),
        Some(PASSWORD),
        &Limits::default(),
    )
    .unwrap();
    let from_plain = archive::read(&plain, None, &Limits::default()).unwrap();
    assert_eq!(from_sealed.items, from_plain.items);
    assert_eq!(from_sealed.items.len(), 4);
    let wallet = from_plain
        .items
        .iter()
        .find(|i| i.kind == lockdown_vault::ItemKind::CryptoWallet)
        .unwrap();
    assert_eq!(
        wallet.kept.unwrap().class,
        lockdown_vault::ItemClass::CrownJewel
    );
    assert!(lockdown_vault::export_opted_in(&wallet.fields));
    assert!(from_plain.problems[0].contains("1 items were left out"));
}

fn f(field: PlainField) -> DocField {
    DocField::from_plain(&field)
}

/// Writes the two test vectors. `cargo test -p lockdown-cxf --test
/// backup_format -- --ignored` remakes them; the ids are fixed so the
/// files only change when the format does.
#[test]
#[ignore]
fn write_backup_vectors() {
    let id = |n: u8| Id::parse(&format!("00000000-0000-4000-8000-0000000000{n:02}")).unwrap();
    let mut doc = Document::new(1_760_000_000, "Lockdown 0.1.0");
    let login_now = vec![
        f(PlainField::text("title", "title", "Example Forum")),
        f(PlainField::text("user", "Username", "ada@example.com")),
        f(PlainField::hidden(
            "pw",
            "Password",
            "correct horse battery staple",
        )),
        f(PlainField::new(
            "code.0",
            FieldKind::Totp,
            "One-time code",
            FieldValue::Text(
                "otpauth://totp/Forum:ada?secret=JBSWY3DPEHPK3PXP&issuer=Forum".into(),
            ),
        )),
        f(PlainField::url(
            "url.0",
            "https://forum.example.com",
            "base_domain",
        )),
        f(PlainField::new(
            "passkey.0.key",
            FieldKind::Passkey,
            "private key",
            FieldValue::Bytes(vec![0x30, 0x81, 0x87, 0x02, 0x01, 0x00]),
        )),
        f(PlainField::text("notes", "Notes", "Signed up in 2024.")),
    ];
    let mut login_then = login_now.clone();
    login_then[2] = f(PlainField::hidden("pw", "Password", "an older password"));
    doc.items.push(DocItem {
        id: id(1).to_string(),
        kind: "login".into(),
        class: "standard".into(),
        state: "live".into(),
        created_at: Some(1_700_000_000),
        modified_at: Some(1_750_000_000),
        fields: login_now,
        history: vec![DocVersion {
            n: 1,
            edited_at: 1_700_000_000,
            fields: login_then,
        }],
        attachments: vec![DocAttachment::new(
            "receipt.txt",
            "text/plain",
            b"receipt: one kettle",
        )],
    });
    doc.items.push(DocItem {
        id: id(2).to_string(),
        kind: "crypto-wallet".into(),
        class: "crown_jewel".into(),
        state: "live".into(),
        created_at: Some(1_700_000_000),
        modified_at: Some(1_700_000_100),
        fields: vec![
            f(PlainField::text("title", "title", "Cold wallet")),
            f(PlainField::hidden(
                "seed",
                "Recovery phrase",
                "abandon ability able about above absent absorb abstract absurd abuse access accident",
            )),
            f(PlainField::new(
                lockdown_vault::EXPORT_OPT_IN,
                FieldKind::Boolean,
                "Included in exports and backups",
                FieldValue::Boolean(true),
            )),
        ],
        history: vec![],
        attachments: vec![],
    });
    doc.items.push(DocItem {
        id: id(3).to_string(),
        kind: "wi-fi".into(),
        class: "standard".into(),
        state: "archived".into(),
        created_at: None,
        modified_at: None,
        fields: vec![
            f(PlainField::text("title", "title", "Old flat")),
            f(PlainField::text("wifi.ssid", "Network name", "flat-net")),
            f(PlainField::new(
                "wifi.hidden",
                FieldKind::Boolean,
                "Hidden network",
                FieldValue::Boolean(false),
            )),
        ],
        history: vec![],
        attachments: vec![],
    });
    doc.items.push(DocItem {
        id: id(4).to_string(),
        kind: "note".into(),
        class: "standard".into(),
        state: "trashed".into(),
        created_at: None,
        modified_at: None,
        fields: vec![
            f(PlainField::text("title", "title", "Draft")),
            f(PlainField::text(
                "body",
                "Note",
                "In the trash, still kept.",
            )),
        ],
        history: vec![],
        attachments: vec![],
    });
    doc.left_out.push(LeftOut {
        id: id(5).to_string(),
        title: "Exchange recovery codes".into(),
        kind: "document".into(),
        reason: "crown-jewel".into(),
    });
    let plain = doc.to_bytes().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::write(dir.join("backup-v1.json"), plain.as_slice()).unwrap();
    let kdf = archive::Kdf::fresh().unwrap();
    let key = kdf.derive(PASSWORD).unwrap();
    let sealed = archive::seal(&plain, &key, &kdf, 1_760_000_000).unwrap();
    std::fs::write(dir.join("backup-v1.lockdown"), sealed).unwrap();
}
