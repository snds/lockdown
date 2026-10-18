//! Authenticator apps' and Steam's files (docs/plan/23-bridges.md §1-§2),
//! each read from a sample written by an independent library (Python's
//! `cryptography` and libsodium through PyNaCl; `fixtures/src/
//! make-authenticator-fixtures.py`), then every reader against broken
//! copies: cut short, bytes flipped, garbage. A reader may refuse; it may
//! never panic.

use std::path::PathBuf;

use lockdown_codes::{Algorithm, Code, Variant};
use lockdown_cxf::import::{self, Format, Parsed, otp, steam};
use lockdown_cxf::{CxfError, ImportedItem, Limits};
use lockdown_vault::{FieldValue, ItemKind};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// RFC 6238's SHA-1 key.
const KEY: &[u8] = b"12345678901234567890";

fn read(name: &str, format: Format, password: Option<&[u8]>) -> Parsed {
    let bytes = fixture(name);
    import::read(&bytes, format, password, &Limits::default())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn detected(name: &str) -> Option<Format> {
    import::detect(&fixture(name), name)
}

fn code_of(item: &ImportedItem) -> Code {
    let f = item
        .fields
        .iter()
        .find(|f| f.field_id == "code.0")
        .unwrap_or_else(|| panic!("{} has no code", item.title));
    let FieldValue::Text(uri) = &f.value else {
        panic!("not text")
    };
    lockdown_codes::parse_uri(uri).unwrap()
}

fn by_title<'a>(p: &'a Parsed, title: &str) -> &'a ImportedItem {
    p.items
        .iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| {
            panic!(
                "no '{title}' in {:?}",
                p.items.iter().map(|i| &i.title).collect::<Vec<_>>()
            )
        })
}

/// The code an RFC 6238 key gives at 59 s, six digits.
fn is_rfc_key(code: &Code) {
    assert_eq!(code.secret.expose(), KEY);
}

#[test]
fn google_authenticator_transfer_batches_read_in_order() {
    assert_eq!(
        detected("google-authenticator.txt"),
        Some(Format::GoogleAuthenticator)
    );
    let p = read(
        "google-authenticator.txt",
        Format::GoogleAuthenticator,
        None,
    );
    assert_eq!(p.items.len(), 3, "{:?}", p.problems);
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    let example = code_of(by_title(&p, "Example"));
    is_rfc_key(&example);
    assert_eq!(example.account.as_deref(), Some("sean@example.com"));
    assert_eq!(example.generate_at(59).unwrap(), "287082");
    let bank = code_of(by_title(&p, "Bank"));
    assert_eq!((bank.algorithm, bank.digits), (Algorithm::Sha256, 8));
    let counter = code_of(by_title(&p, "Counter"));
    assert_eq!((counter.variant, counter.counter), (Variant::Hotp, 5));
    assert!(
        p.items
            .iter()
            .all(|i| i.kind == ItemKind::Code && !i.crown_jewel)
    );
}

#[test]
fn aegis_plain_and_encrypted_read_the_same_and_the_wrong_password_is_said() {
    assert_eq!(detected("aegis-plain.json"), Some(Format::Aegis));
    assert_eq!(detected("aegis-encrypted.json"), Some(Format::Aegis));
    let plain = read("aegis-plain.json", Format::Aegis, None);
    let sealed = read(
        "aegis-encrypted.json",
        Format::Aegis,
        Some(b"fixture-aegis"),
    );
    for p in [&plain, &sealed] {
        assert_eq!(p.items.len(), 3);
        assert!(
            p.problems.iter().any(|x| x.contains("motp")),
            "{:?}",
            p.problems
        );
        let example = by_title(p, "Example");
        is_rfc_key(&code_of(example));
        assert!(example.favourite);
        assert_eq!(example.folder.as_deref(), Some("Work"));
        let counter = code_of(by_title(p, "Counter"));
        assert_eq!(
            (
                counter.variant,
                counter.algorithm,
                counter.digits,
                counter.counter
            ),
            (Variant::Hotp, Algorithm::Sha256, 8, 3)
        );
        assert_eq!(code_of(by_title(p, "Steam")).variant, Variant::Steam);
    }
    let bytes = fixture("aegis-encrypted.json");
    assert!(matches!(
        import::read(&bytes, Format::Aegis, None, &Limits::default()),
        Err(CxfError::PasswordNeeded)
    ));
    assert!(matches!(
        import::read(&bytes, Format::Aegis, Some(b"nope"), &Limits::default()),
        Err(CxfError::WrongPassword)
    ));
    // A file that asks for an enormous key derivation is refused before it runs.
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["header"]["slots"][1]["n"] = serde_json::json!(1u64 << 30);
    let err = import::read(
        v.to_string().as_bytes(),
        Format::Aegis,
        Some(b"fixture-aegis"),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("outside what Lockdown runs"),
        "{err}"
    );
    // One changed byte of the sealed vault: refused by the cipher, nothing parsed.
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let db = v["db"].as_str().unwrap().to_string();
    let flipped = format!(
        "{}{}",
        if db.starts_with('A') { "B" } else { "A" },
        &db[1..]
    );
    v["db"] = serde_json::json!(flipped);
    assert!(matches!(
        import::read(
            v.to_string().as_bytes(),
            Format::Aegis,
            Some(b"fixture-aegis"),
            &Limits::default()
        ),
        Err(CxfError::WrongPassword)
    ));
}

#[test]
fn twofas_plain_and_encrypted() {
    assert_eq!(detected("2fas-plain.2fas"), Some(Format::TwoFas));
    assert_eq!(detected("2fas-encrypted.2fas"), Some(Format::TwoFas));
    for p in [
        read("2fas-plain.2fas", Format::TwoFas, None),
        read("2fas-encrypted.2fas", Format::TwoFas, Some(b"fixture-2fas")),
    ] {
        assert_eq!(p.items.len(), 2);
        let example = by_title(&p, "Example");
        is_rfc_key(&code_of(example));
        assert_eq!(example.folder.as_deref(), Some("Personal"));
        assert_eq!(example.modified_at, Some(1_760_000_000));
        assert_eq!(code_of(by_title(&p, "Steam")).variant, Variant::Steam);
    }
    let bytes = fixture("2fas-encrypted.2fas");
    assert!(matches!(
        import::read(&bytes, Format::TwoFas, Some(b"wrong"), &Limits::default()),
        Err(CxfError::WrongPassword)
    ));
}

#[test]
fn ente_plain_text_and_the_libsodium_encrypted_export() {
    assert_eq!(detected("ente-plain.txt"), Some(Format::OtpLinks));
    assert_eq!(detected("ente-encrypted.json"), Some(Format::Ente));
    for p in [
        read("ente-plain.txt", Format::Ente, None),
        read("ente-encrypted.json", Format::Ente, Some(b"fixture-ente")),
    ] {
        assert_eq!(p.items.len(), 2, "{:?}", p.problems);
        is_rfc_key(&code_of(by_title(&p, "Example")));
        assert_eq!(code_of(by_title(&p, "Counter")).counter, 2);
    }
    let bytes = fixture("ente-encrypted.json");
    assert!(matches!(
        import::read(&bytes, Format::Ente, Some(b"wrong"), &Limits::default()),
        Err(CxfError::WrongPassword)
    ));
    assert!(matches!(
        import::read(&bytes, Format::Ente, None, &Limits::default()),
        Err(CxfError::PasswordNeeded)
    ));
}

#[test]
fn proton_and_bitwarden_authenticators() {
    assert_eq!(
        detected("proton-authenticator.json"),
        Some(Format::ProtonAuthenticator)
    );
    let p = read(
        "proton-authenticator.json",
        Format::ProtonAuthenticator,
        None,
    );
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.problems.len(), 1, "the broken entry is reported by name");
    assert!(p.problems[0].contains("Broken"));
    is_rfc_key(&code_of(by_title(&p, "Example")));

    assert_eq!(
        detected("bitwarden-authenticator.json"),
        Some(Format::BitwardenAuthenticator)
    );
    for name in [
        "bitwarden-authenticator.json",
        "bitwarden-authenticator.csv",
    ] {
        let p = read(name, Format::BitwardenAuthenticator, None);
        assert_eq!(p.items.len(), 2, "{name}: {:?}", p.problems);
        is_rfc_key(&code_of(by_title(&p, "Example")));
        is_rfc_key(&code_of(by_title(&p, "Bare key")));
    }
    // A Bitwarden password-manager export is still the password manager's.
    assert_eq!(
        import::detect(&fixture("bitwarden.json"), "bitwarden.json"),
        Some(Format::Bitwarden)
    );
}

#[test]
fn steam_files_put_the_identity_secret_in_a_crown_jewel_and_never_the_session() {
    assert_eq!(detected("steam.maFile"), Some(Format::Steam));
    assert_eq!(detected("steam-sda-encrypted.zip"), Some(Format::Steam));
    let one = read("steam.maFile", Format::Steam, None);
    let folder = read(
        "steam-sda-encrypted.zip",
        Format::Steam,
        Some(b"fixture-sda"),
    );
    let app = read("steam-app-v2.json", Format::Steam, None);
    for (p, accounts) in [(&one, 1), (&folder, 2), (&app, 1)] {
        let codes: Vec<&ImportedItem> = p
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Code)
            .collect();
        let jewels: Vec<&ImportedItem> = p.items.iter().filter(|i| i.crown_jewel).collect();
        assert_eq!((codes.len(), jewels.len()), (accounts, accounts));
        for c in codes {
            let code = code_of(c);
            assert_eq!(code.variant, Variant::Steam);
            is_rfc_key(&code);
            assert!(!c.crown_jewel, "the everyday code isn't a crown jewel");
        }
        for j in jewels {
            assert!(j.fields.iter().any(|f| f.label == "Identity secret"));
            assert!(j.fields.iter().any(|f| f.label == "Revocation code"));
        }
        let text = format!("{:?}", p.items);
        assert!(!text.contains("fixture-access-token") && !text.contains("fixture-refresh-token"));
    }
    assert!(
        folder
            .problems
            .iter()
            .any(|x| x.contains("session was left out"))
    );
    let zip = fixture("steam-sda-encrypted.zip");
    assert!(matches!(
        import::read(&zip, Format::Steam, None, &Limits::default()),
        Err(CxfError::PasswordNeeded)
    ));
    assert!(matches!(
        import::read(
            &zip,
            Format::Steam,
            Some(b"wrong-passkey"),
            &Limits::default()
        ),
        Err(CxfError::WrongPassword)
    ));
    let winauth = read("winauth.txt", Format::WinAuth, None);
    assert_eq!(winauth.items.len(), 2);
    assert_eq!(code_of(by_title(&winauth, "Steam")).variant, Variant::Steam);
}

#[test]
fn every_new_format_is_listed_and_named() {
    for f in [
        Format::GoogleAuthenticator,
        Format::Aegis,
        Format::TwoFas,
        Format::Ente,
        Format::ProtonAuthenticator,
        Format::BitwardenAuthenticator,
        Format::Steam,
        Format::WinAuth,
        Format::OtpLinks,
    ] {
        assert!(Format::ALL.contains(&f));
        assert_eq!(Format::from_id(f.id()), Some(f));
        assert!(!f.how().is_empty() && !f.name().is_empty());
    }
}

/// Broken copies of every sample, straight to each reader with no safety
/// net: a value (a refusal is fine), never a panic.
#[test]
fn broken_files_are_refused_never_panicked_on() {
    let samples: [(&str, Format); 13] = [
        ("google-authenticator.txt", Format::GoogleAuthenticator),
        ("aegis-plain.json", Format::Aegis),
        ("aegis-encrypted.json", Format::Aegis),
        ("2fas-plain.2fas", Format::TwoFas),
        ("2fas-encrypted.2fas", Format::TwoFas),
        ("ente-plain.txt", Format::Ente),
        ("ente-encrypted.json", Format::Ente),
        ("proton-authenticator.json", Format::ProtonAuthenticator),
        (
            "bitwarden-authenticator.json",
            Format::BitwardenAuthenticator,
        ),
        (
            "bitwarden-authenticator.csv",
            Format::BitwardenAuthenticator,
        ),
        ("steam.maFile", Format::Steam),
        ("steam-sda-encrypted.zip", Format::Steam),
        ("winauth.txt", Format::WinAuth),
    ];
    let limits = Limits::default();
    let direct = |bytes: &[u8], f: Format, pw: Option<&[u8]>| -> Result<Parsed, CxfError> {
        match f {
            Format::GoogleAuthenticator => otp::read_google(bytes),
            Format::Aegis => otp::read_aegis(bytes, pw, &limits),
            Format::TwoFas => otp::read_2fas(bytes, pw),
            Format::Ente => otp::read_ente(bytes, pw),
            Format::ProtonAuthenticator => otp::read_proton_authenticator(bytes),
            Format::BitwardenAuthenticator => otp::read_bitwarden_authenticator(bytes, &limits),
            Format::Steam => steam::read(bytes, pw, &limits),
            _ => otp::read_otpauth_lines(bytes, "lines"),
        }
    };
    for (name, format) in samples {
        let bytes = fixture(name);
        let pw: Option<&[u8]> = [
            ("aegis", &b"fixture-aegis"[..]),
            ("2fas", b"fixture-2fas"),
            ("ente", b"fixture-ente"),
            ("sda", b"fixture-sda"),
        ]
        .iter()
        .find(|(k, _)| name.contains(k))
        .map(|(_, p)| *p);
        let mut copies: Vec<Vec<u8>> = Vec::new();
        for cut in (1..bytes.len()).step_by((bytes.len() / 12).max(1)) {
            copies.push(bytes[..cut].to_vec());
        }
        for i in 0..24 {
            let mut b = bytes.clone();
            let at = (i * 7919 + 13) % b.len();
            b[at] ^= 1 << (i % 8);
            copies.push(b);
        }
        copies.push(Vec::new());
        copies.push(vec![0xff; 300]);
        copies.push(b"{\"header\":{\"slots\":[{}]},\"db\":\"x\"}".to_vec());
        for copy in copies {
            let _ = direct(&copy, format, pw);
            let _ = direct(&copy, format, None);
        }
    }
}
