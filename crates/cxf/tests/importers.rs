//! Each importer against a sample file shaped like the real export (every
//! value made up), then every reader against broken copies of those files:
//! cut short, bytes flipped, garbage. A reader may refuse; it may never
//! panic.

use std::path::PathBuf;

use lockdown_cxf::import::{self, Format, Parsed};
use lockdown_cxf::{CxfError, ImportedItem, Limits};
use lockdown_vault::{FieldKind, FieldValue, ItemKind};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

const KDBX_PASSWORD: &[u8] = b"fixture-password";
const BITWARDEN_PASSWORD: &[u8] = b"fixture-password";

fn read(name: &str, format: Format, password: Option<&[u8]>) -> Parsed {
    let bytes = fixture(name);
    assert_eq!(
        import::detect(&bytes, name),
        Some(format),
        "{name} is detected as {format:?}"
    );
    import::read(&bytes, format, password, &Limits::default()).unwrap()
}

fn by_title<'a>(p: &'a Parsed, title: &str) -> &'a ImportedItem {
    p.items
        .iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| panic!("no item '{title}' in {:?}", titles(p)))
}

fn titles(p: &Parsed) -> Vec<&str> {
    p.items.iter().map(|i| i.title.as_str()).collect()
}

fn text(i: &ImportedItem, id: &str) -> Option<String> {
    i.fields
        .iter()
        .find(|f| f.field_id == id)
        .map(|f| match &f.value {
            FieldValue::Text(t) => t.clone(),
            FieldValue::Url { value, .. } => value.clone(),
            FieldValue::Boolean(b) => b.to_string(),
            other => format!("{other:?}"),
        })
}

fn labelled(i: &ImportedItem, label: &str) -> Option<(FieldKind, String)> {
    i.fields
        .iter()
        .find(|f| f.label == label && f.field_id.starts_with("custom."))
        .map(|f| {
            (
                f.kind,
                match &f.value {
                    FieldValue::Text(t) => t.clone(),
                    FieldValue::Url { value, .. } => value.clone(),
                    FieldValue::Boolean(b) => b.to_string(),
                    other => format!("{other:?}"),
                },
            )
        })
}

#[test]
fn one_password_1pux() {
    let p = read("1password.1pux", Format::OnePux, None);
    assert_eq!(p.items.len(), 6, "{:?}", titles(&p));
    assert!(
        p.problems
            .iter()
            .any(|x| x.contains("1 items in 1Password's trash"))
    );
    let login = by_title(&p, "Example");
    assert_eq!(login.kind, ItemKind::Login);
    assert_eq!(text(login, "user").as_deref(), Some("ada@example.com"));
    assert_eq!(text(login, "pw").as_deref(), Some("1p-example-pass"));
    assert_eq!(
        text(login, "url.0").as_deref(),
        Some("https://example.com/login")
    );
    assert_eq!(
        text(login, "url.1").as_deref(),
        Some("https://app.example.com")
    );
    assert!(
        text(login, "code.0")
            .unwrap()
            .starts_with("otpauth://totp/")
    );
    assert_eq!(
        labelled(login, "PIN"),
        Some((FieldKind::Hidden, "2468".into()))
    );
    assert_eq!(
        labelled(login, "Recovery email"),
        Some((FieldKind::Email, "backup@example.com".into()))
    );
    assert_eq!(
        labelled(login, "Signed up"),
        Some((FieldKind::Date, "2021-01-01".into()))
    );
    assert_eq!(text(login, "notes").as_deref(), Some("Shared with nobody."));
    assert_eq!(login.tags, ["personal", "email"]);
    assert_eq!(login.folder.as_deref(), Some("Personal"));
    assert!(login.favourite);
    assert_eq!(login.history.len(), 2, "two old passwords, two versions");
    assert_eq!(
        text_in(&login.history[0], "pw").as_deref(),
        Some("1p-old-1")
    );

    let card = by_title(&p, "Visa");
    assert_eq!(card.kind, ItemKind::Card);
    assert_eq!(
        text(card, "card.number").as_deref(),
        Some("4111111111111111")
    );
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2029-09"));
    assert_eq!(text(card, "card.cvc").as_deref(), Some("123"));
    assert_eq!(
        labelled(card, "issuing bank").map(|x| x.1).as_deref(),
        Some("Example Bank")
    );

    let wifi = by_title(&p, "Home Wi-Fi");
    assert_eq!(wifi.kind, ItemKind::WiFi);
    assert!(wifi.archived);
    assert_eq!(text(wifi, "wifi.ssid").as_deref(), Some("home-net"));
    assert_eq!(
        text(wifi, "wifi.passphrase").as_deref(),
        Some("k7Hq3mWx9rTa4nPe")
    );

    let doc = by_title(&p, "Lease");
    assert_eq!(doc.kind, ItemKind::Document);
    assert_eq!(doc.attachments[0].name, "lease.pdf");
    assert_eq!(
        doc.attachments[0].bytes.as_slice(),
        b"%PDF-1.4 example lease"
    );

    let ssh = by_title(&p, "Laptop key");
    assert!(text(ssh, "ssh.private_key").unwrap().contains("OPENSSH"));

    let medical = by_title(&p, "Medical record");
    assert_eq!(medical.kind, ItemKind::Custom);
    assert!(medical.report[0].contains("category 113"));
    assert_eq!(
        labelled(medical, "healthcare professional")
            .map(|x| x.1)
            .as_deref(),
        Some("Dr Example")
    );
}

fn text_in(fields: &[lockdown_vault::PlainField], id: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.field_id == id)
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            _ => None,
        })
}

fn check_bitwarden(p: &Parsed) {
    assert_eq!(p.items.len(), 5, "{:?}", titles(p));
    assert!(
        p.problems
            .iter()
            .any(|x| x.contains("1 items in Bitwarden's trash"))
    );
    let login = by_title(p, "Example Forum");
    assert_eq!(text(login, "user").as_deref(), Some("ada@example.com"));
    assert_eq!(
        text(login, "pw").as_deref(),
        Some("correct horse battery staple")
    );
    let rules: Vec<(String, String)> = login
        .fields
        .iter()
        .filter_map(|f| match &f.value {
            FieldValue::Url { value, match_rule } => Some((value.clone(), match_rule.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        rules,
        [
            (
                "https://forum.example.com/login".into(),
                "exact_host".into()
            ),
            ("https://example.com".into(), "base_domain".into())
        ]
    );
    assert!(text(login, "code.0").unwrap().contains("JBSWY3DPEHPK3PXP"));
    assert_eq!(
        text(login, "passkey.0.rp_id").as_deref(),
        Some("forum.example.com")
    );
    let cred = login
        .fields
        .iter()
        .find(|f| f.field_id == "passkey.0.credential_id")
        .unwrap();
    assert_eq!(cred.value, FieldValue::Bytes(vec![1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(
        labelled(login, "Answer"),
        Some((FieldKind::Hidden, "Biscuit".into()))
    );
    assert_eq!(
        labelled(login, "Newsletter"),
        Some((FieldKind::Boolean, "true".into()))
    );
    assert!(login.report.iter().any(|r| r.contains("linked field")));
    assert_eq!(login.folder.as_deref(), Some("Work"));
    assert_eq!(login.history.len(), 2);
    assert_eq!(
        text_in(&login.history[0], "pw").as_deref(),
        Some("older-pass-1")
    );

    let note = by_title(p, "Door codes");
    assert_eq!(note.kind, ItemKind::Note);
    assert_eq!(
        text(note, "body").as_deref(),
        Some("Front: 2580\nShed: 1357")
    );
    let card = by_title(p, "Everyday Visa");
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2029-09"));
    let me = by_title(p, "Me");
    assert_eq!(me.kind, ItemKind::Identity);
    assert_eq!(
        text(me, "address.street").as_deref(),
        Some("1 Example Street, Flat 2")
    );
    assert_eq!(
        labelled(me, "Passport number"),
        Some((FieldKind::Hidden, "000000000".into()))
    );
    let ssh = by_title(p, "Build server key");
    assert_eq!(ssh.kind, ItemKind::SshKey);
    assert!(
        text(ssh, "ssh.public_key")
            .unwrap()
            .starts_with("ssh-ed25519")
    );
}

#[test]
fn bitwarden_plain_and_password_protected() {
    check_bitwarden(&read("bitwarden.json", Format::Bitwarden, None));
    let protected = fixture("bitwarden-protected.json");
    assert_eq!(
        import::detect(&protected, "bitwarden_export.json"),
        Some(Format::Bitwarden)
    );
    assert!(!String::from_utf8_lossy(&protected).contains("correct horse"));
    assert!(matches!(
        import::read(&protected, Format::Bitwarden, None, &Limits::default()),
        Err(CxfError::PasswordNeeded)
    ));
    assert!(matches!(
        import::read(
            &protected,
            Format::Bitwarden,
            Some(b"wrong"),
            &Limits::default()
        ),
        Err(CxfError::WrongPassword)
    ));
    let p = import::read(
        &protected,
        Format::Bitwarden,
        Some(BITWARDEN_PASSWORD),
        &Limits::default(),
    )
    .unwrap();
    check_bitwarden(&p);
    // An account-locked export is refused with what to do instead.
    let locked = br#"{"encrypted":true,"encKeyValidation_DO_NOT_EDIT":"2.a|b|c","data":"2.a|b|c"}"#;
    let err = import::read(locked, Format::Bitwarden, None, &Limits::default()).unwrap_err();
    assert!(
        err.to_string().contains("only Bitwarden can open it"),
        "{err}"
    );
    // A file asking for a ruinous key derivation is refused before it runs.
    let heavy = br#"{"encrypted":true,"passwordProtected":true,"salt":"x","kdfType":1,"kdfIterations":3,"kdfMemory":65536,"kdfParallelism":4,"encKeyValidation_DO_NOT_EDIT":"2.a|b|c","data":"2.a|b|c"}"#;
    let err = import::read(heavy, Format::Bitwarden, Some(b"x"), &Limits::default()).unwrap_err();
    assert!(
        err.to_string().contains("outside what Lockdown accepts"),
        "{err}"
    );
}

#[test]
fn proton_pass_zip_json_and_csv() {
    let p = read("proton.zip", Format::Proton, None);
    assert_eq!(p.items.len(), 5, "{:?}", titles(&p));
    assert!(p.problems[0].contains("1 items in Proton Pass's trash"));
    let mail = by_title(&p, "Example Mail");
    assert_eq!(text(mail, "user").as_deref(), Some("ada@example.com"));
    assert_eq!(text(mail, "url.1").as_deref(), Some("https://example.com"));
    assert!(mail.favourite);
    assert_eq!(
        labelled(mail, "Recovery code"),
        Some((FieldKind::Hidden, "RC-1111-2222".into()))
    );
    assert!(text(mail, "code.0").is_some());
    let card = by_title(&p, "Debit card");
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2030-04"));
    let alias = by_title(&p, "Shopping alias");
    assert_eq!(
        text(alias, "user").as_deref(),
        Some("shop.x1y2@passmail.example")
    );
    assert!(!alias.report.is_empty());
    let id = by_title(&p, "Ada");
    assert_eq!(
        labelled(id, "Passport number"),
        Some((FieldKind::Hidden, "000000000".into()))
    );
    assert_eq!(
        text(by_title(&p, "Locker"), "body").as_deref(),
        Some("Locker 42, code 1984.")
    );
    // The bare JSON reads the same.
    let json = fixture("src/proton-data.json");
    assert_eq!(import::detect(&json, "data.json"), Some(Format::Proton));
    let again = import::read(&json, Format::Proton, None, &Limits::default()).unwrap();
    assert_eq!(again.items.len(), 5);
    // And the CSV.
    let csv = read("proton.csv", Format::Proton, None);
    assert_eq!(csv.items.len(), 2);
    assert_eq!(
        text(&csv.items[0], "url.1").as_deref(),
        Some("https://example.com")
    );
    // An encrypted export is refused with what to do.
    let pgp = zip_of(&[("Proton Pass/data.pgp", b"-----BEGIN PGP MESSAGE-----")]);
    let err = import::read(&pgp, Format::Proton, None, &Limits::default()).unwrap_err();
    assert!(err.to_string().contains("without encryption"), "{err}");
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (n, b) in files {
        w.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn keepass_kdbx4() {
    let bytes = fixture("keepass.kdbx");
    assert_eq!(import::detect(&bytes, "x.kdbx"), Some(Format::Kdbx));
    assert!(matches!(
        import::read(&bytes, Format::Kdbx, None, &Limits::default()),
        Err(CxfError::PasswordNeeded)
    ));
    assert!(matches!(
        import::read(&bytes, Format::Kdbx, Some(b"nope"), &Limits::default()),
        Err(CxfError::WrongPassword)
    ));
    let p = import::read(
        &bytes,
        Format::Kdbx,
        Some(KDBX_PASSWORD),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(p.items.len(), 2, "{:?}", titles(&p));
    assert!(p.problems[0].contains("1 entries in KeePass's recycle bin"));
    let bank = by_title(&p, "Example Bank");
    assert_eq!(bank.kind, ItemKind::Login);
    assert_eq!(bank.folder.as_deref(), Some("Internet/Banking"));
    assert_eq!(text(bank, "user").as_deref(), Some("ada"));
    assert_eq!(text(bank, "pw").as_deref(), Some("kdbx-new-pass"));
    assert_eq!(
        text(bank, "url.0").as_deref(),
        Some("https://bank.example.com")
    );
    assert!(text(bank, "code.0").unwrap().starts_with("otpauth://"));
    assert_eq!(
        labelled(bank, "PIN"),
        Some((FieldKind::Hidden, "9753".into()))
    );
    assert_eq!(
        labelled(bank, "Account no"),
        Some((FieldKind::Text, "12-34-56".into()))
    );
    assert_eq!(bank.tags, ["money"]);
    assert_eq!(bank.attachments[0].name, "statement.txt");
    assert_eq!(bank.attachments[0].bytes.as_slice(), b"balance: example");
    assert_eq!(bank.history.len(), 1);
    assert_eq!(
        text_in(&bank.history[0], "pw").as_deref(),
        Some("kdbx-old-pass")
    );
    let note = by_title(&p, "Router notes");
    assert_eq!(note.kind, ItemKind::Note);
    assert_eq!(
        text(note, "body").as_deref(),
        Some("Admin page at 192.168.1.1")
    );
}

#[test]
fn a_kdbx_asking_for_a_huge_key_derivation_is_refused_before_it_runs() {
    let mut bytes = fixture("keepass.kdbx");
    // Find the variant dictionary's "M" (memory) entry and make it 1 TiB.
    let at = bytes
        .windows(5)
        .position(|w| w == [0x05, 0x01, 0x00, 0x00, 0x00])
        .and_then(|p| (bytes.get(p + 5) == Some(&b'M')).then_some(p))
        .expect("the memory entry");
    let value = at + 5 + 1 + 4;
    bytes[value..value + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
    let err = import::read(
        &bytes,
        Format::Kdbx,
        Some(KDBX_PASSWORD),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("heavier than Lockdown allows"),
        "{err}"
    );
}

#[test]
fn dashlane_zip() {
    let p = read("dashlane.zip", Format::Dashlane, None);
    assert_eq!(p.items.len(), 7, "{:?}", titles(&p));
    let video = by_title(&p, "Example Video");
    assert_eq!(text(video, "user").as_deref(), Some("ada@example.com"));
    assert_eq!(
        labelled(video, "Second username").map(|x| x.1).as_deref(),
        Some("ada")
    );
    assert!(text(video, "code.0").is_some());
    assert_eq!(video.folder.as_deref(), Some("Entertainment"));
    assert_eq!(
        text(by_title(&p, "WiFi guest"), "body").as_deref(),
        Some("Network: guest\nPassword: welcome-example")
    );
    let card = by_title(&p, "Travel card");
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2029-09"));
    let bank = by_title(&p, "Savings");
    assert_eq!(bank.kind, ItemKind::BankAccount);
    assert_eq!(
        text(bank, "bank.account_number").as_deref(),
        Some("12345678")
    );
    let passport = p
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Document)
        .unwrap();
    assert_eq!(text(passport, "doc.type").as_deref(), Some("Passport"));
    assert_eq!(text(passport, "expires_at").as_deref(), Some("2030-01-01"));
    let me = by_title(&p, "Ada at home");
    assert_eq!(text(me, "address.postal_code").as_deref(), Some("N1 1AA"));
    // One CSV on its own reads too.
    let one = fixture("src/dashlane-credentials.csv");
    assert_eq!(
        import::detect(&one, "credentials.csv"),
        Some(Format::Dashlane)
    );
    assert_eq!(
        import::read(&one, Format::Dashlane, None, &Limits::default())
            .unwrap()
            .items
            .len(),
        2
    );
}

#[test]
fn keeper_json_and_csv() {
    let p = read("keeper.json", Format::Keeper, None);
    assert_eq!(p.items.len(), 5);
    let bank = by_title(&p, "Example Bank");
    assert_eq!(text(bank, "user").as_deref(), Some("ada.lovelace"));
    assert!(text(bank, "code.0").is_some());
    assert_eq!(
        labelled(bank, "Customer number").map(|x| x.1).as_deref(),
        Some("00112233")
    );
    assert_eq!(
        labelled(bank, "Memorable word").map(|x| x.0),
        Some(FieldKind::Hidden)
    );
    assert_eq!(bank.folder.as_deref(), Some("Finance/Banks"));
    let card = by_title(&p, "Family card");
    assert_eq!(card.kind, ItemKind::Card);
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2028-03"));
    assert_eq!(text(card, "card.holder").as_deref(), Some("Ada Lovelace"));
    assert_eq!(text(card, "card.pin").as_deref(), Some("4321"));
    assert_eq!(card.folder.as_deref(), Some("Family"));
    let key = by_title(&p, "Deploy key");
    assert_eq!(key.kind, ItemKind::SshKey);
    assert!(text(key, "ssh.private_key").is_some());
    assert_eq!(
        text(by_title(&p, "Garage notes"), "body").as_deref(),
        Some("Code is the year we moved in.")
    );
    let odd = by_title(&p, "Hobby");
    assert_eq!(odd.kind, ItemKind::Custom);
    assert!(!odd.report.is_empty());

    let bytes = fixture("keeper.csv");
    let p = import::read(&bytes, Format::Keeper, None, &Limits::default()).unwrap();
    assert_eq!(p.items.len(), 2);
    let mail = by_title(&p, "Example Mail");
    assert_eq!(text(mail, "pw").as_deref(), Some("mail-pass-1"));
    assert_eq!(
        labelled(mail, "Recovery email").map(|x| x.1).as_deref(),
        Some("backup@example.com")
    );
    assert!(text(mail, "code.0").is_some());
    assert_eq!(
        by_title(&p, "Staging").folder.as_deref(),
        Some("Team Shared/Work/Servers")
    );
}

#[test]
fn lastpass_csv() {
    let p = read("lastpass.csv", Format::LastPass, None);
    assert_eq!(p.items.len(), 6, "{:?}", titles(&p));
    let shop = by_title(&p, "Example Shop");
    assert_eq!(text(shop, "pw").as_deref(), Some("p,ss\"word"));
    assert!(shop.favourite);
    assert_eq!(shop.folder.as_deref(), Some("Shopping/Home"));
    assert!(text(shop, "code.0").is_some());
    let card = by_title(&p, "Travel card");
    assert_eq!(card.kind, ItemKind::Card);
    assert_eq!(text(card, "card.expiry").as_deref(), Some("2029-09"));
    assert_eq!(
        text(card, "card.number").as_deref(),
        Some("4111111111111111")
    );
    assert_eq!(
        text(card, "notes").as_deref(),
        Some("Use for travel.\nSecond line of the note.")
    );
    let wifi = by_title(&p, "Home Wi-Fi");
    assert_eq!(wifi.kind, ItemKind::WiFi);
    assert_eq!(text(wifi, "wifi.security").as_deref(), Some("WPA2"));
    assert_eq!(by_title(&p, "Thoughts").kind, ItemKind::Note);
    let health = by_title(&p, "Health plan");
    assert_eq!(health.kind, ItemKind::Document);
    assert_eq!(
        labelled(health, "Member ID").map(|x| x.1).as_deref(),
        Some("HX-0000")
    );
    // A formula in the file is kept as text, not run.
    assert_eq!(
        text(by_title(&p, "Formula row"), "pw").as_deref(),
        Some("=SUM(1+1)")
    );
}

#[test]
fn enpass_json() {
    let p = read("enpass.json", Format::Enpass, None);
    assert_eq!(p.items.len(), 4);
    assert!(p.problems[0].contains("1 items in Enpass's trash"));
    let cloud = by_title(&p, "Example Cloud");
    assert_eq!(text(cloud, "user").as_deref(), Some("ada"));
    assert_eq!(
        labelled(cloud, "E-mail").map(|x| x.1).as_deref(),
        Some("ada@example.com"),
        "a second sign-in name lands as a field of your own"
    );
    assert_eq!(text(cloud, "pw").as_deref(), Some("enp4ss-example"));
    assert_eq!(
        text(cloud, "url.0").as_deref(),
        Some("https://cloud.example.com")
    );
    assert!(text(cloud, "code.0").is_some());
    assert_eq!(
        labelled(cloud, "Recovery PIN"),
        Some((FieldKind::Hidden, "8642".into()))
    );
    assert!(labelled(cloud, "Old field").is_none(), "deleted in Enpass");
    assert_eq!(
        cloud.attachments[0].bytes.as_slice(),
        b"receipt: kettle, 1 unit"
    );
    assert_eq!(cloud.folder.as_deref(), Some("Home"));
    let amex = by_title(&p, "Amex");
    assert!(amex.archived);
    assert_eq!(text(amex, "card.expiry").as_deref(), Some("2027-11"));
    let wifi = by_title(&p, "Home 5G");
    assert_eq!(wifi.kind, ItemKind::WiFi);
    assert_eq!(text(wifi, "wifi.ssid").as_deref(), Some("home-net-5g"));
    assert_eq!(text(wifi, "wifi.security").as_deref(), Some("WPA3"));
}

#[test]
fn a_generic_csv() {
    let p = read("generic.csv", Format::Csv, None);
    assert_eq!(p.items.len(), 1);
    assert_eq!(
        text(&p.items[0], "notes").as_deref(),
        Some("note, with comma")
    );
}

/// A small, seeded generator, so a failure names its input.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0 >> 33
    }
}

fn never_panics(name: &str, bytes: &[u8], format: Format, password: Option<&[u8]>) {
    let limits = Limits::default();
    let mut rng = Lcg(bytes.len() as u64);
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    for cut in (0..bytes.len()).step_by((bytes.len() / 24).max(1)) {
        inputs.push(bytes[..cut].to_vec());
    }
    for _ in 0..40 {
        let mut b = bytes.to_vec();
        for _ in 0..1 + rng.next() % 4 {
            let i = (rng.next() as usize) % b.len().max(1);
            if let Some(x) = b.get_mut(i) {
                *x ^= 1 << (rng.next() % 8);
            }
        }
        inputs.push(b);
    }
    for _ in 0..10 {
        let n = (rng.next() % 512) as usize;
        let mut g: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        if let Some(first) = bytes.first() {
            g.insert(0, *first);
        }
        inputs.push(g);
    }
    for (k, input) in inputs.iter().enumerate() {
        let run = std::panic::catch_unwind(|| {
            // Straight to the format's own reader: no safety net.
            let _ = match format {
                Format::OnePux => import::onepux::read(input, &limits).map(|_| ()),
                Format::Bitwarden => import::bitwarden::read(input, password, &limits).map(|_| ()),
                Format::Proton => import::proton::read(input, &limits).map(|_| ()),
                Format::Kdbx => import::kdbx::read(input, password, &limits).map(|_| ()),
                Format::Dashlane => import::dashlane::read(input, &limits).map(|_| ()),
                Format::Keeper => import::keeper::read_json(input, &limits).map(|_| ()),
                Format::Enpass => import::enpass::read(input, &limits).map(|_| ()),
                _ => import::read(input, format, password, &limits).map(|_| ()),
            };
            let _ = import::detect(input, name);
        });
        assert!(run.is_ok(), "{name}: input #{k} panicked");
    }
}

#[test]
fn every_reader_survives_broken_files() {
    for (name, format, password) in [
        ("1password.1pux", Format::OnePux, None),
        ("bitwarden.json", Format::Bitwarden, None),
        (
            "bitwarden-protected.json",
            Format::Bitwarden,
            Some(BITWARDEN_PASSWORD),
        ),
        ("proton.zip", Format::Proton, None),
        ("proton.csv", Format::Proton, None),
        ("keepass.kdbx", Format::Kdbx, Some(KDBX_PASSWORD)),
        ("dashlane.zip", Format::Dashlane, None),
        ("keeper.json", Format::Keeper, None),
        ("keeper.csv", Format::Keeper, None),
        ("lastpass.csv", Format::LastPass, None),
        ("enpass.json", Format::Enpass, None),
        ("generic.csv", Format::Csv, None),
        (
            "backup-v1.lockdown",
            Format::Lockdown,
            Some(b"correct horse battery staple".as_slice()),
        ),
        ("backup-v1.json", Format::Lockdown, None),
    ] {
        never_panics(name, &fixture(name), format, password);
    }
}

#[test]
fn caps_hold_on_every_format() {
    let tiny = Limits {
        max_bytes: 64,
        ..Limits::default()
    };
    for name in [
        "bitwarden.json",
        "enpass.json",
        "keeper.json",
        "1password.1pux",
        "lastpass.csv",
    ] {
        let bytes = fixture(name);
        let format = import::detect(&bytes, name).unwrap();
        assert!(
            matches!(
                import::read(&bytes, format, None, &tiny),
                Err(CxfError::TooLarge { .. })
            ),
            "{name}"
        );
    }
    // A value over the per-field cap is left out and named.
    let small = Limits {
        max_field_bytes: 16,
        ..Limits::default()
    };
    let p = import::read(&fixture("bitwarden.json"), Format::Bitwarden, None, &small).unwrap();
    let login = by_title(&p, "Example Forum");
    assert!(text(login, "pw").is_none());
    assert!(
        login.report.iter().any(|r| r.contains("longer than")),
        "{:?}",
        login.report
    );
}

/// Writes the binary fixtures that can't be typed by hand: the KeePass
/// database and the password-protected Bitwarden file. Run with
/// `cargo test -p lockdown-cxf --test importers -- --ignored` to remake them.
#[test]
#[ignore]
fn write_binary_fixtures() {
    use keepass::db::{History, Value, fields};
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    let plain = std::fs::read(dir.join("bitwarden.json")).unwrap();
    let sealed = lockdown_cxf::import::bitwarden::seal_for_tests(
        &plain,
        BITWARDEN_PASSWORD,
        "QnVpbGRpbmdBU2FsdA==",
        5000,
    );
    std::fs::write(dir.join("bitwarden-protected.json"), sealed).unwrap();

    let mut db = keepass::Database::new();
    if let keepass::config::KdfConfig::Argon2 {
        iterations,
        memory,
        parallelism,
        ..
    } = &mut db.config.kdf_config
    {
        *iterations = 2;
        *memory = 64 * 1024;
        *parallelism = 1;
    }
    let mut root = db.root_mut();
    let mut internet = root.add_group();
    internet.name = "Internet".into();
    let mut banking = internet.add_group();
    banking.name = "Banking".into();
    let mut e = banking.add_entry();
    e.set_unprotected(fields::TITLE, "Example Bank");
    e.set_unprotected(fields::USERNAME, "ada");
    e.set_protected(fields::PASSWORD, "kdbx-old-pass");
    e.set_unprotected(fields::URL, "https://bank.example.com");
    let old = (*e).clone();
    e.set_protected(fields::PASSWORD, "kdbx-new-pass");
    e.set_protected(
        fields::OTP,
        "otpauth://totp/Example%20Bank:ada?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Bank",
    );
    e.set_protected("PIN", "9753");
    e.set_unprotected("Account no", "12-34-56");
    e.tags.push("money".into());
    e.add_attachment(
        "statement.txt",
        Value::protected(b"balance: example".to_vec()),
    );
    let mut h = History::default();
    h.add_entry(old);
    e.history = Some(h);
    let mut root = db.root_mut();
    let mut n = root.add_entry();
    n.set_unprotected(fields::TITLE, "Router notes");
    n.set_unprotected(fields::NOTES, "Admin page at 192.168.1.1");
    let mut root = db.root_mut();
    let mut bin = root.add_group();
    bin.name = "Recycle Bin".into();
    let bin_id = bin.id();
    let mut gone = bin.add_entry();
    gone.set_unprotected(fields::TITLE, "Deleted");
    db.meta.recyclebin_uuid = Some(bin_id.uuid());
    let mut out = Vec::new();
    db.save(
        &mut out,
        keepass::DatabaseKey::new().with_password(std::str::from_utf8(KDBX_PASSWORD).unwrap()),
    )
    .unwrap();
    std::fs::write(dir.join("keepass.kdbx"), out).unwrap();
}

#[test]
fn a_kdbx_export_opens_in_a_keepass_reader() {
    use lockdown_cxf::ImportedAttachment;
    use lockdown_vault::PlainField;
    let mut login = ImportedItem::new("a", ItemKind::Login, "Forum");
    login.fields.extend([
        PlainField::text("user", "Username", "ada"),
        PlainField::hidden("pw", "Password", "kdbx-out-pass"),
        PlainField::url("url.0", "https://forum.example.com", "exact_host"),
        PlainField::url("url.1", "https://m.forum.example.com", "exact_host"),
        PlainField::new(
            "code.0",
            FieldKind::Totp,
            "One-time code",
            FieldValue::Text("otpauth://totp/F:ada?secret=JBSWY3DPEHPK3PXP".into()),
        ),
        PlainField::hidden("custom.0", "Door code", "1234"),
        PlainField::text("notes", "Notes", "hello"),
    ]);
    login.attachments.push(ImportedAttachment {
        name: "a.txt".into(),
        mime: "text/plain".into(),
        bytes: zeroize::Zeroizing::new(b"attached".to_vec()),
    });
    let mut card = ImportedItem::new("b", ItemKind::Card, "Visa");
    card.fields.push(PlainField::hidden(
        "card.number",
        "Card number",
        "4111111111111111",
    ));
    let file = lockdown_cxf::kdbx_out::write(&[login, card], "out-pass", "Lockdown").unwrap();
    let p = import::read(&file, Format::Kdbx, Some(b"out-pass"), &Limits::default()).unwrap();
    let f = by_title(&p, "Forum");
    assert_eq!(f.folder.as_deref(), Some("Logins"));
    assert_eq!(text(f, "pw").as_deref(), Some("kdbx-out-pass"));
    assert_eq!(
        text(f, "url.1").as_deref(),
        Some("https://m.forum.example.com")
    );
    assert!(text(f, "code.0").is_some());
    assert_eq!(
        labelled(f, "Door code"),
        Some((FieldKind::Hidden, "1234".into()))
    );
    assert_eq!(f.attachments[0].bytes.as_slice(), b"attached");
    let c = by_title(&p, "Visa");
    assert_eq!(text(c, "pw").as_deref(), Some("4111111111111111"));
}
