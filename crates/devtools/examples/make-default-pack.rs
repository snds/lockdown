//! Writes the default demo pack, `crates/devtools/demo/default-pack.json`
//! (docs/plan/40-devtools.md §3.5):
//!
//! ```sh
//! cargo run -p lockdown-devtools --features demo --example make-default-pack
//! ```
//!
//! The items are built as Lockdown items from the core's own templates and
//! written to CXF by the core's own exporter (`lockdown_cxf::from_items`),
//! the same document an export makes. The file is the pack; this program
//! is only how the default one was made, and it makes the same file every
//! time (fixed dates and ids, no randomness: keys are made when the demo
//! opens). Everything in it is invented: only `example.com`, `example.org`
//! and `.test` addresses, the card networks' published test numbers, the
//! BIP-39 test phrase and the RFC 6238 test secret.

use std::collections::BTreeMap;

use lockdown_cxf::archive::DocField;
use lockdown_cxf::{ImportedItem, from_items};
use lockdown_devtools::demo::pack::{
    Extras, FORMAT, Fresh, PACK_V, Pack, PackDismissal, PackFile, PackTemplate, TemplateItem,
};
use lockdown_vault::{
    FieldKind, FieldValue, ItemKind, PlainField, TEMPLATE_FIELD, template_variant,
};
use serde_json::json;

/// 2026-10-01T00:00:00Z: every date in the pack is fixed.
const BASE: u64 = 1_790_812_800;
const DAY: u64 = 86_400;

/// RFC 6238's test secret ("12345678901234567890"), base32.
const RFC6238: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

/// The BIP-39 test vector.
const BIP39: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

struct Builder {
    items: Vec<ImportedItem>,
    extras: BTreeMap<String, Extras>,
    n: usize,
}

/// A field from the kind's (or variant's) template: its label, section and
/// storage kind are the core's.
fn templated(kind: ItemKind, variant: &str, id: &str, value: &str) -> PlainField {
    let template = template_variant(kind, variant).expect("a template");
    let t = template
        .field(id)
        .unwrap_or_else(|| panic!("{} has no field {id}", template.name));
    let storage = t.control.storage();
    let value = match storage {
        FieldKind::Url => FieldValue::Url {
            value: value.into(),
            match_rule: "exact_host".into(),
        },
        FieldKind::Boolean => FieldValue::Boolean(value == "true"),
        _ => FieldValue::Text(value.into()),
    };
    let mut f = PlainField::new(id, storage, t.label, value);
    if !t.section.is_empty() {
        f.section = Some(t.section.into());
    }
    f
}

fn custom(n: usize, label: &str, kind: FieldKind, value: &str) -> PlainField {
    let value = match kind {
        FieldKind::Url => FieldValue::Url {
            value: value.into(),
            match_rule: "exact_host".into(),
        },
        FieldKind::Boolean => FieldValue::Boolean(value == "true"),
        _ => FieldValue::Text(value.into()),
    };
    let mut f = PlainField::new(format!("custom.{n}"), kind, label, value);
    f.section = Some("More".into());
    f
}

/// Optional parts of an item.
#[derive(Default)]
struct Opts<'a> {
    folder: Option<&'a str>,
    tags: &'a [&'a str],
    favourite: bool,
    extra_fields: Vec<PlainField>,
    extras: Extras,
    age_days: u64,
}

impl Builder {
    fn add(
        &mut self,
        kind: ItemKind,
        variant: &str,
        title: &str,
        values: &[(&str, &str)],
        o: Opts<'_>,
    ) -> String {
        self.n += 1;
        let id = format!("demo-{:03}", self.n);
        let mut item = ImportedItem::new(id.clone(), kind, title);
        let mut fields = vec![PlainField::text("title", "title", title)];
        if !variant.is_empty() {
            fields.push(PlainField::text(TEMPLATE_FIELD, "Template", variant));
        }
        for (fid, v) in values {
            if fid.starts_with("passkey.") {
                let label = match fid.rsplit('.').next() {
                    Some("rp_id") => "Website",
                    Some("user_name") => "User name",
                    _ => "Display name",
                };
                let mut f = PlainField::text(fid, label, v);
                f.section = Some("Passkey".into());
                fields.push(f);
            } else {
                fields.push(templated(kind, variant, fid, v));
            }
        }
        fields.extend(o.extra_fields);
        item.fields = fields;
        item.folder = o.folder.map(str::to_string);
        item.tags = o.tags.iter().map(|t| t.to_string()).collect();
        item.favourite = o.favourite;
        let made = BASE - (o.age_days + 1) * DAY;
        item.created_at = Some(made);
        item.modified_at = Some(made + DAY / 2);
        self.items.push(item);
        if o.extras != Extras::default() {
            self.extras.insert(id.clone(), o.extras);
        }
        id
    }
}

fn hist(versions: &[&[(&str, FieldKind, &str, &str)]]) -> Vec<Vec<DocField>> {
    versions
        .iter()
        .map(|v| {
            v.iter()
                .map(|(id, kind, label, value)| {
                    let value = match kind {
                        FieldKind::Url => FieldValue::Url {
                            value: (*value).into(),
                            match_rule: "exact_host".into(),
                        },
                        _ => FieldValue::Text((*value).into()),
                    };
                    DocField::from_plain(&PlainField::new(*id, *kind, *label, value))
                })
                .collect()
        })
        .collect()
}

fn main() {
    let mut b = Builder {
        items: Vec::new(),
        extras: BTreeMap::new(),
        n: 0,
    };
    use ItemKind as K;

    // --- the welcome note ----------------------------------------------------
    b.add(
        K::Note,
        "",
        "Welcome to the Lockdown demo",
        &[(
            "body",
            "# Welcome to the demo\n\nEverything here is invented: every site is an `example.com`, `example.org` or `.test` address, the cards are the networks' test numbers, and the keys were made when the demo opened.\n\nPoke at anything. Edit, delete, move, tag. **Leave the demo** wipes it, and **Developer → Reset demo data** builds it again.\n\n- Every kind of item, and each kind's variants\n- Folders inside folders, tags, favourites and a crown jewel\n- Weak and reused passwords for Watchtower\n- An expired card, a passport about to expire, a code with a 60-second period\n- Long titles, other scripts, right-to-left text and emoji\n- Items in Deleted, an archived item, and items with history",
        )],
        Opts {
            favourite: true,
            tags: &["demo"],
            ..Opts::default()
        },
    );

    // --- logins: the hub kind --------------------------------------------------
    b.add(
        K::Login,
        "",
        "Example Mail",
        &[
            ("user", "sam.rivera@example.com"),
            ("pw", "correct-Horse-battery-staple-42"),
            ("code.0", &format!("otpauth://totp/Example%20Mail:sam.rivera%40example.com?secret={RFC6238}&issuer=Example%20Mail&algorithm=SHA1&digits=6&period=30")),
            ("url.0", "https://mail.example.com"),
            ("notes", "The primary inbox. Its earlier passwords are in its history."),
        ],
        Opts {
            folder: Some("Email"),
            tags: &["email", "important"],
            favourite: true,
            extras: Extras {
                history: hist(&[
                    &[
                        ("user", FieldKind::Text, "Username", "sam.rivera@example.com"),
                        ("pw", FieldKind::Hidden, "Password", "Summer2019!"),
                        ("url.0", FieldKind::Url, "Website", "https://mail.example.com"),
                    ],
                    &[
                        ("user", FieldKind::Text, "Username", "sam.rivera@example.com"),
                        ("pw", FieldKind::Hidden, "Password", "Winter-2022-mail"),
                        ("url.0", FieldKind::Url, "Website", "https://mail.example.com"),
                    ],
                    &[
                        ("user", FieldKind::Text, "Username", "sam.rivera@example.com"),
                        ("pw", FieldKind::Hidden, "Password", "Spring-2024-mail-longer"),
                        ("url.0", FieldKind::Url, "Website", "https://mail.example.com"),
                        ("notes", FieldKind::Text, "Notes", "Changed after the 2024 notice."),
                    ],
                ]),
                ..Extras::default()
            },
            age_days: 900,
            ..Opts::default()
        },
    );
    b.add(
        K::Login,
        "",
        "Primary email recovery account",
        &[
            ("user", "sam.recovery@example.org"),
            ("pw", "Tr0ub4dor&3-recovery-only"),
            ("url.0", "https://accounts.example.org"),
            ("notes", "The account every other one recovers through: a crown jewel, so using it asks for Touch ID or the password."),
        ],
        Opts {
            folder: Some("Email"),
            tags: &["email", "recovery"],
            extras: Extras {
                crown_jewel: true,
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    // Several logins for one site.
    for (who, user, pw) in [
        ("Sam", "sam.rivera", "bank-Sam-7f3k-Q9x2"),
        ("Alex", "alex.rivera", "bank-Alex-2m8p-L4w7"),
        ("the business", "rivera-studio", "bank-Studio-9z1c-K6v3"),
    ] {
        b.add(
            K::Login,
            "",
            &format!("Example Bank ({who})"),
            &[
                ("user", user),
                ("pw", pw),
                ("url.0", "https://bank.example.com/login"),
            ],
            Opts {
                folder: Some("Finance"),
                tags: &["finance"],
                ..Opts::default()
            },
        );
    }
    // Watchtower: weak, reused, and both.
    let weak = b.add(
        K::Login,
        "",
        "Recipe forum",
        &[
            ("user", "samcooks"),
            ("pw", "password"),
            ("url.0", "https://recipes.example.org"),
        ],
        Opts {
            tags: &["weak-password"],
            extras: Extras {
                dismiss: vec![PackDismissal {
                    finding: "weak:pw".into(),
                    snooze_days: Some(30),
                }],
                ..Extras::default()
            },
            age_days: 2400,
            ..Opts::default()
        },
    );
    let _ = weak;
    for (title, site, pw) in [
        ("Old photo site", "https://photos.example.test", "123456"),
        (
            "Neighbourhood board",
            "https://board.example.test",
            "letmein",
        ),
        ("Garden club", "https://garden.example.org", "Summer2019!"),
        ("Book swap", "https://books.example.org", "Summer2019!"),
        ("Bike shop", "https://bikes.example.com", "Summer2019!"),
    ] {
        b.add(
            K::Login,
            "",
            title,
            &[
                ("user", "sam.rivera@example.com"),
                ("pw", pw),
                ("url.0", site),
            ],
            Opts {
                tags: &["watchtower"],
                age_days: 1800,
                ..Opts::default()
            },
        );
    }
    b.add(
        K::Login,
        "",
        "Gym members' area",
        &[
            ("user", "sam.rivera@example.com"),
            ("pw", "Summer2019!"),
            ("url.0", "https://members.example-gym.test"),
        ],
        Opts {
            tags: &["watchtower"],
            extras: Extras {
                dismiss: vec![PackDismissal {
                    finding: "reused:pw".into(),
                    snooze_days: None,
                }],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    // Lookalike addresses.
    b.add(
        K::Login,
        "",
        "Example Pay",
        &[
            ("user", "sam.rivera@example.com"),
            ("pw", "pay-Ex4mple-real-one"),
            ("url.0", "https://examplepay.test"),
        ],
        Opts {
            folder: Some("Finance"),
            ..Opts::default()
        },
    );
    b.add(
        K::Login,
        "",
        "Examp1e Pay (a lookalike)",
        &[
            ("user", "sam.rivera@example.com"),
            ("pw", "pay-lookalike-saved-by-mistake"),
            ("url.0", "https://examp1epay.test"),
        ],
        Opts {
            tags: &["watchtower"],
            ..Opts::default()
        },
    );
    // Passkeys: beside a password, alone in a login, and several.
    b.add(
        K::Login,
        "",
        "Example Shop",
        &[
            ("user", "sam.rivera@example.com"),
            ("pw", "shop-Pa55-with-passkey-too"),
            ("url.0", "https://shop.example.com"),
            ("passkey.0.rp_id", "shop.example.com"),
            ("passkey.0.user_name", "sam.rivera@example.com"),
            ("passkey.0.user_display_name", "Sam Rivera"),
        ],
        Opts {
            folder: Some("Shopping"),
            extras: Extras {
                fresh: vec![Fresh::Passkey],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Login,
        "",
        "Example Social",
        &[
            ("sign_in_method", "Passkey"),
            ("url.0", "https://social.example.org"),
            ("passkey.0.rp_id", "social.example.org"),
            ("passkey.0.user_name", "samr"),
            ("passkey.0.user_display_name", "Sam R."),
            ("passkey.1.rp_id", "social.example.org"),
            ("passkey.1.user_name", "samr-work"),
            ("passkey.1.user_display_name", "Sam (work)"),
        ],
        Opts {
            extras: Extras {
                fresh: vec![Fresh::Passkey],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    // Signed in with another account.
    b.add(
        K::Login,
        "",
        "Example Notes (signed in with Example Mail)",
        &[
            ("sign_in_method", "Another account"),
            ("sign_in_provider", "Example Mail"),
            ("url.0", "https://notes.example.com"),
        ],
        Opts::default(),
    );
    // A code with a 60-second period and 8 digits.
    b.add(
        K::Login,
        "",
        "Example Cloud console",
        &[
            ("user", "sam@example.test"),
            ("pw", "cloud-Console-8d-60s"),
            ("code.0", &format!("otpauth://totp/Example%20Cloud:sam?secret={RFC6238}&issuer=Example%20Cloud&digits=8&period=60")),
            ("url.0", "https://console.cloud.example.test"),
            ("url.1", "https://status.cloud.example.test"),
        ],
        Opts {
            folder: Some("Work/Engineering"),
            tags: &["work"],
            ..Opts::default()
        },
    );
    // Every optional field empty, and every field filled.
    b.add(
        K::Login,
        "",
        "A login with nothing but its title",
        &[],
        Opts::default(),
    );
    b.add(
        K::Login,
        "",
        "A login with every field filled",
        &[
            ("user", "everything@example.com"),
            ("pw", "every-Field-filled-1"),
            ("code.0", &format!("otpauth://totp/Everything:everything?secret={RFC6238}&issuer=Everything")),
            ("sign_in_method", "Password"),
            ("sign_in_provider", "Example Mail"),
            ("url.0", "https://everything.example.com"),
            ("url.1", "https://www.everything.example.com"),
            ("url.2", "https://m.everything.example.com"),
            ("pw.rule", "minlength: 12; maxlength: 64; required: digit; required: upper;"),
            ("notes", "Notes **in Markdown**, with a list:\n\n1. one\n2. two\n\nand a [link](https://everything.example.com/help)."),
        ],
        Opts {
            folder: None,
            tags: &["complete", "demo", "work/clients"],
            favourite: true,
            extra_fields: vec![
                custom(0, "Security question", FieldKind::Hidden, "The name of the first street: Example Road"),
                custom(1, "Account number", FieldKind::Text, "ACC-000-123"),
                custom(2, "Support", FieldKind::Url, "https://help.everything.example.com"),
                custom(3, "Support email", FieldKind::Email, "help@everything.example.com"),
                custom(4, "Support phone", FieldKind::Phone, "+1 555 0100"),
                custom(5, "Renews", FieldKind::Date, "2027-03-01"),
                custom(6, "Auto-renew", FieldKind::Boolean, "true"),
            ],
            ..Opts::default()
        },
    );
    // Long title, address and notes.
    let long_title = format!(
        "A very long title that keeps going {} until it has to wrap or be cut off somewhere sensible",
        "and going ".repeat(12)
    );
    let long_url = format!(
        "https://a-very-long-subdomain-name-for-testing.example.com/{}?q={}&utm_source=demo",
        "deep/path/segment/".repeat(10),
        "x".repeat(120)
    );
    let long_notes = (1..=60)
        .map(|n| format!("Line {n} of a long note: the quick brown fox jumps over the lazy dog, again and again, so the detail has to scroll."))
        .collect::<Vec<_>>()
        .join("\n");
    b.add(
        K::Login,
        "",
        &long_title,
        &[
            (
                "user",
                &format!("{}@example.com", "long.username".repeat(6)),
            ),
            ("pw", &"a-long-password-".repeat(8)),
            ("url.0", &long_url),
            ("notes", &long_notes),
        ],
        Opts {
            tags: &["edge-case"],
            ..Opts::default()
        },
    );
    // Other scripts, right-to-left text and emoji.
    for (title, user, site, note) in [
        (
            "Почта (пример)",
            "семён@example.com",
            "https://pochta.example.com",
            "Кириллица в заголовке и заметке.",
        ),
        (
            "例のサイト",
            "山田@example.com",
            "https://rei.example.jp.test",
            "日本語のメモ。",
        ),
        (
            "示例邮箱",
            "王@example.com",
            "https://youxiang.example.test",
            "中文备注。",
        ),
        (
            "بريد المثال",
            "سامي@example.com",
            "https://bareed.example.test",
            "ملاحظة باللغة العربية تُكتب من اليمين إلى اليسار، مع رقم 42 في وسطها.",
        ),
        (
            "דואר לדוגמה",
            "שמואל@example.com",
            "https://doar.example.test",
            "הערה בעברית, מימין לשמאל.",
        ),
        (
            "उदाहरण बैंक",
            "सैम@example.com",
            "https://udaharan.example.test",
            "हिन्दी में टिप्पणी।",
        ),
        (
            "ร้านค้าตัวอย่าง",
            "สมชาย@example.com",
            "https://ran.example.test",
            "บันทึกภาษาไทย",
        ),
        (
            "예시 계정",
            "샘@example.com",
            "https://yesi.example.test",
            "한국어 메모입니다.",
        ),
        (
            "🔐 Emoji vault 🎉🦊",
            "fox🦊@example.com",
            "https://emoji.example.test",
            "Emoji everywhere: 👍🏽 👨‍👩‍👧 🏳️‍🌈 🇳🇿",
        ),
        (
            "Mixed: Café ↔ مقهى ↔ カフェ",
            "café@example.org",
            "https://cafe.example.org",
            "Left-to-right then right-to-left: Hello, مرحبا, שלום, done.",
        ),
    ] {
        b.add(
            K::Login,
            "",
            title,
            &[
                ("user", user),
                ("pw", "scripts-Test-pass-1"),
                ("url.0", site),
                ("notes", note),
            ],
            Opts {
                folder: Some("International"),
                tags: &["i18n"],
                ..Opts::default()
            },
        );
    }
    // Archived and deleted.
    b.add(
        K::Login,
        "",
        "Old job intranet (archived)",
        &[
            ("user", "srivera"),
            ("pw", "old-Job-intranet-2019"),
            ("url.0", "https://intranet.oldjob.example.test"),
        ],
        Opts {
            extras: Extras {
                state: Some("archived".into()),
                ..Extras::default()
            },
            age_days: 2000,
            ..Opts::default()
        },
    );
    for title in ["Deleted login", "Deleted streaming service"] {
        b.add(
            K::Login,
            "",
            title,
            &[
                ("user", "sam.rivera@example.com"),
                ("pw", "deleted-But-restorable"),
                ("url.0", "https://deleted.example.org"),
            ],
            Opts {
                extras: Extras {
                    state: Some("trashed".into()),
                    ..Extras::default()
                },
                ..Opts::default()
            },
        );
    }

    // --- passkey, code ---------------------------------------------------------
    b.add(
        K::Passkey,
        "",
        "Example Docs passkey",
        &[
            ("passkey.0.rp_id", "docs.example.com"),
            ("passkey.0.user_name", "sam.rivera@example.com"),
            ("passkey.0.user_display_name", "Sam Rivera"),
        ],
        Opts {
            extras: Extras {
                fresh: vec![Fresh::Passkey],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Code,
        "",
        "Example VPN",
        &[(
            "code.0",
            &format!("otpauth://totp/Example%20VPN:sam?secret={RFC6238}&issuer=Example%20VPN"),
        )],
        Opts {
            folder: Some("Work/Engineering"),
            tags: &["work"],
            ..Opts::default()
        },
    );
    b.add(
        K::Code,
        "",
        "Example Admin (8 digits, 60 s)",
        &[("code.0", &format!("otpauth://totp/Example%20Admin:sam?secret={RFC6238}&issuer=Example%20Admin&digits=8&period=60&algorithm=SHA1"))],
        Opts::default(),
    );
    b.add(
        K::Code,
        "",
        "Example Token (counter)",
        &[("code.0", &format!("otpauth://hotp/Example%20Token:sam?secret={RFC6238}&issuer=Example%20Token&counter=0"))],
        Opts::default(),
    );

    // --- Wi-Fi and the router --------------------------------------------------
    b.add(
        K::WiFi,
        "",
        "Home Wi-Fi",
        &[
            ("wifi.ssid", "Rivera Home"),
            ("wifi.passphrase", "home-Wifi-passphrase-2026"),
            ("wifi.security", "WPA2/WPA3"),
        ],
        Opts {
            folder: Some("Home/Network"),
            favourite: true,
            ..Opts::default()
        },
    );
    b.add(
        K::WiFi,
        "",
        "Guest network",
        &[("wifi.ssid", "Rivera Guest"), ("wifi.security", "Open")],
        Opts {
            folder: Some("Home/Network"),
            ..Opts::default()
        },
    );
    b.add(
        K::WiFi,
        "",
        "Lab network (hidden, enterprise)",
        &[
            ("wifi.ssid", "lab-net"),
            ("wifi.passphrase", "lab-Enterprise-pass"),
            ("wifi.security", "Enterprise"),
            ("wifi.hidden", "true"),
            ("wifi.eap_identity", "sam@lab.example.test"),
        ],
        Opts {
            folder: Some("Home/Network/Lab"),
            ..Opts::default()
        },
    );
    b.add(
        K::WiFi,
        "router",
        "Home router",
        &[
            ("wifi.ssid", "Rivera Home"),
            ("wifi.passphrase", "home-Wifi-passphrase-2026"),
            ("wifi.security", "WPA2/WPA3"),
            ("router.admin", "https://router.home.test"),
            ("user", "admin"),
            ("pw", "router-Admin-pass"),
            ("router.model", "Example Router 3000"),
        ],
        Opts {
            folder: Some("Home/Network"),
            ..Opts::default()
        },
    );

    // --- keys and credentials ----------------------------------------------
    b.add(
        K::SshKey,
        "",
        "Build server key",
        &[
            ("ssh.key_type", "Ed25519"),
            ("ssh.hosts", "build.example.test\ndeploy.example.test"),
        ],
        Opts {
            folder: Some("Work/Engineering/Servers"),
            tags: &["work"],
            extras: Extras {
                fresh: vec![Fresh::SshKey],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::SshKey,
        "pgp",
        "Sam's PGP key (placeholder)",
        &[
            ("pgp.private_key", "-----BEGIN PGP PRIVATE KEY BLOCK-----\n\nThis is a placeholder, not a key. The demo makes no PGP keys.\n-----END PGP PRIVATE KEY BLOCK-----"),
            ("pgp.public_key", "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nPlaceholder.\n-----END PGP PUBLIC KEY BLOCK-----"),
            ("pgp.fingerprint", "0000 1111 2222 3333 4444  5555 6666 7777 8888 9999"),
            ("pgp.user_id", "Sam Rivera <sam.rivera@example.com>"),
            ("pgp.algorithm", "Ed25519"),
            ("expires_at", "2028-01-01"),
        ],
        Opts::default(),
    );
    b.add(
        K::ApiCredential,
        "",
        "Example API token",
        &[
            ("api.key", "demo_tok_0123456789abcdef0123456789abcdef"),
            ("api.secret", "demo_secret_fedcba9876543210"),
            ("api.type", "Bearer token"),
            ("api.host", "api.example.com"),
            ("api.scopes", "read write"),
            ("expires_at", "2026-12-31"),
            (
                "api.rotation",
                "Rotate every 90 days from the developer console.",
            ),
        ],
        Opts {
            folder: Some("Work/Engineering"),
            tags: &["work"],
            ..Opts::default()
        },
    );
    b.add(
        K::CryptoWallet,
        "",
        "Test wallet (BIP-39 test phrase)",
        &[
            ("seed", BIP39),
            ("wallet.password", "wallet-Test-pass"),
            ("wallet.name", "Example Wallet"),
            ("wallet.network", "Testnet"),
            (
                "wallet.address",
                "tb1qexampleexampleexampleexampleexamplex0",
            ),
        ],
        Opts {
            tags: &["crown-jewel"],
            extras: Extras {
                crown_jewel: true,
                ..Extras::default()
            },
            ..Opts::default()
        },
    );

    // --- documents and their variants --------------------------------------
    b.add(
        K::Document,
        "",
        "Lease agreement",
        &[
            ("doc.type", "Contract"),
            ("doc.number", "LEASE-2026-001"),
            ("doc.issuer", "Example Lettings"),
            ("doc.issued", "2026-01-15"),
            ("expires_at", "2027-01-14"),
        ],
        Opts {
            folder: Some("Home"),
            extras: Extras {
                files: vec![
                    PackFile {
                        name: "lease-summary.txt".into(),
                        mime: "text/plain".into(),
                        text: Some("Lease summary (demo placeholder)\nTenant: Sam Rivera\nTerm: twelve months\nNothing here is real.\n".into()),
                        size: None,
                    },
                    PackFile {
                        name: "lease-scan.pdf".into(),
                        mime: "application/pdf".into(),
                        text: None,
                        size: Some(48 * 1024),
                    },
                ],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Document,
        "",
        "Large archive (near the import limit)",
        &[("doc.type", "Other"), ("notes", "A placeholder file just under the 32 MiB a single attachment may be in an import or restore.")],
        Opts {
            tags: &["edge-case"],
            extras: Extras {
                files: vec![PackFile {
                    name: "large-archive.bin".into(),
                    mime: "application/octet-stream".into(),
                    text: None,
                    size: Some(31 * 1024 * 1024),
                }],
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Document,
        "medical-record",
        "Medical record",
        &[
            ("med.patient", "Sam Rivera"),
            ("med.provider", "Example Clinic"),
            ("med.record_number", "MR-000123"),
            ("med.blood_type", "O+"),
            ("med.allergies", "Penicillin (demo)"),
            ("med.emergency_contact", "Alex Rivera"),
            ("med.emergency_phone", "+1 555 0101"),
        ],
        Opts {
            folder: Some("Health"),
            ..Opts::default()
        },
    );
    b.add(
        K::Document,
        "insurance",
        "Home insurance",
        &[
            ("insurance.type", "Home"),
            ("insurance.insurer", "Example Mutual"),
            ("insurance.policy_number", "HM-2026-4455"),
            ("insurance.holder", "Sam Rivera"),
            ("insurance.start", "2026-02-01"),
            ("expires_at", "2027-02-01"),
            ("insurance.phone", "+1 555 0102"),
            ("insurance.url", "https://claims.example.org"),
        ],
        Opts {
            folder: Some("Home"),
            ..Opts::default()
        },
    );
    b.add(
        K::Document,
        "recovery-codes",
        "Example Mail recovery codes",
        &[
            ("recovery.account", "Example Mail"),
            (
                "recovery_codes",
                "1111-2222\n3333-4444\n5555-6666\n7777-8888\n9999-0000",
            ),
            ("recovery.made", "2026-03-10"),
        ],
        Opts {
            folder: Some("Email"),
            ..Opts::default()
        },
    );
    b.add(
        K::Document,
        "outdoor-licence",
        "Fishing licence",
        &[
            ("outdoor.holder", "Sam Rivera"),
            ("outdoor.number", "FL-778899"),
            ("outdoor.activities", "Freshwater fishing"),
            ("outdoor.authority", "Example Parks"),
            ("expires_at", "2026-11-15"),
        ],
        Opts::default(),
    );

    // --- identities and their variants -------------------------------------
    b.add(
        K::Identity,
        "",
        "Sam Rivera",
        &[
            ("name.given", "Sam"),
            ("name.middle", "Jordan"),
            ("name.family", "Rivera"),
            ("identity.email", "sam.rivera@example.com"),
            ("address.tel", "+1 555 0100"),
            ("address.street", "1 Example Road"),
            ("address.city", "Exampleton"),
            ("address.territory", "EX"),
            ("address.postal_code", "00000"),
            ("address.country", "Exampleland"),
        ],
        Opts {
            favourite: true,
            folder: None,
            ..Opts::default()
        },
    );
    b.add(
        K::Identity,
        "address",
        "Work address",
        &[
            ("address.street", "100 Example Avenue, Floor 3"),
            ("address.city", "Exampleton"),
            ("address.postal_code", "00001"),
            ("address.country", "Exampleland"),
            ("address.tel", "+1 555 0110"),
        ],
        Opts::default(),
    );
    b.add(
        K::Identity,
        "business",
        "Rivera Studio",
        &[
            ("business.name", "Rivera Studio Ltd"),
            ("business.department", "Design"),
            ("business.tax_id", "EX-VAT-000000"),
            ("business.registration", "00000000"),
            ("identity.email", "hello@studio.example.com"),
            ("business.website", "https://studio.example.com"),
        ],
        Opts {
            extras: Extras {
                vault: Some("Work".into()),
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Identity,
        "passport",
        "Passport (expires soon)",
        &[
            ("passport.number", "X00000000"),
            ("passport.type", "P"),
            ("passport.country", "Exampleland"),
            ("passport.issued", "2016-12-01"),
            ("expires_at", "2026-11-20"),
            ("name.given", "Sam Jordan"),
            ("name.family", "Rivera"),
            ("passport.nationality", "Examplelander"),
            ("passport.birth_date", "1990-01-01"),
            ("passport.sex", "X"),
        ],
        Opts {
            folder: Some("Travel"),
            ..Opts::default()
        },
    );
    b.add(
        K::Identity,
        "driver-licence",
        "Driver's licence",
        &[
            ("dl.number", "D000-0000-0000"),
            ("dl.class", "B"),
            ("dl.country", "Exampleland"),
            ("expires_at", "2031-05-01"),
            ("name.given", "Sam"),
            ("name.family", "Rivera"),
        ],
        Opts::default(),
    );
    b.add(
        K::Identity,
        "national-id",
        "National ID card",
        &[
            ("id.number", "ID-0000000"),
            ("id.country", "Exampleland"),
            ("expires_at", "2030-01-01"),
            ("name.given", "Sam"),
            ("name.family", "Rivera"),
        ],
        Opts::default(),
    );
    b.add(
        K::Identity,
        "vehicle",
        "Car",
        &[
            ("vehicle.make", "Example Motors"),
            ("vehicle.model", "Model E"),
            ("vehicle.year", "2022"),
            ("vehicle.plate", "EX 0000"),
            ("vehicle.vin", "00000000000000000"),
            ("expires_at", "2027-04-30"),
        ],
        Opts::default(),
    );
    b.add(
        K::Identity,
        "contact",
        "Alex Rivera",
        &[
            ("name.given", "Alex"),
            ("name.family", "Rivera"),
            ("identity.email", "alex.rivera@example.com"),
            ("address.tel", "+1 555 0101"),
            ("contact.birthday", "1991-06-15"),
        ],
        Opts::default(),
    );

    // --- cards and a bank account -------------------------------------------
    for (title, number, brand, expiry, cvc, folder, fav) in [
        (
            "Example Visa",
            "4242424242424242",
            "Visa",
            "2030-12",
            "123",
            "Finance/Cards",
            true,
        ),
        (
            "Expired Mastercard",
            "5555555555554444",
            "Mastercard",
            "2023-08",
            "456",
            "Finance/Cards",
            false,
        ),
        (
            "Example Amex",
            "378282246310005",
            "American Express",
            "2029-03",
            "1234",
            "Finance/Cards",
            false,
        ),
        (
            "Visa debit",
            "4000056655665556",
            "Visa",
            "2028-07",
            "789",
            "Finance/Cards",
            false,
        ),
    ] {
        b.add(
            K::Card,
            "",
            title,
            &[
                ("card.number", number),
                ("card.holder", "SAM J RIVERA"),
                ("card.expiry", expiry),
                ("card.cvc", cvc),
                ("card.type", brand),
            ],
            Opts {
                folder: Some(folder),
                favourite: fav,
                tags: &["finance"],
                ..Opts::default()
            },
        );
    }
    b.add(
        K::Card,
        "",
        "A card with only its number",
        &[("card.number", "6011111111111117")],
        Opts::default(),
    );
    b.add(
        K::BankAccount,
        "",
        "Example Savings",
        &[
            ("bank.name", "Example Bank"),
            ("bank.holder", "Sam Rivera"),
            ("bank.account_number", "000123456789"),
            ("bank.routing", "110000000"),
            ("bank.iban", "GB82WEST12345698765432"),
            ("bank.swift", "EXAMPLE1"),
            ("bank.branch", "Exampleton High Street"),
            ("bank.pin", "0000"),
        ],
        Opts {
            folder: Some("Finance"),
            tags: &["finance"],
            ..Opts::default()
        },
    );

    // --- servers, databases, licences, memberships, custom -----------------
    b.add(
        K::Server,
        "",
        "Build server",
        &[
            ("server.host", "build.example.test"),
            ("server.port", "22"),
            ("user", "deploy"),
            ("pw", "build-Server-pass-v2"),
            ("server.admin", "https://build.example.test/admin"),
        ],
        Opts {
            folder: Some("Work/Engineering/Servers"),
            tags: &["work"],
            extras: Extras {
                vault: Some("Work".into()),
                history: hist(&[&[
                    (
                        "server.host",
                        FieldKind::Text,
                        "Address",
                        "build-old.example.test",
                    ),
                    ("user", FieldKind::Text, "Username", "deploy"),
                    ("pw", FieldKind::Hidden, "Password", "build-Server-pass-v1"),
                ]]),
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::Server,
        "email-account",
        "Example mail account (IMAP)",
        &[
            ("email.address", "sam.rivera@example.com"),
            ("pw", "imap-App-password"),
            ("email.provider", "Example Mail"),
            ("email.imap_host", "imap.example.com"),
            ("email.imap_port", "993"),
            ("email.smtp_host", "smtp.example.com"),
            ("email.smtp_port", "465"),
            ("email.security", "SSL/TLS"),
        ],
        Opts {
            folder: Some("Email"),
            ..Opts::default()
        },
    );
    b.add(
        K::Database,
        "",
        "Orders database",
        &[
            ("db.type", "PostgreSQL"),
            ("db.host", "db.example.test"),
            ("db.port", "5432"),
            ("db.name", "orders"),
            ("user", "orders_app"),
            ("pw", "db-Orders-pass"),
            ("db.options", "sslmode=require"),
        ],
        Opts {
            folder: Some("Work/Engineering/Databases"),
            tags: &["work"],
            extras: Extras {
                vault: Some("Work".into()),
                ..Extras::default()
            },
            ..Opts::default()
        },
    );
    b.add(
        K::SoftwareLicence,
        "",
        "Example Editor licence",
        &[
            ("licence.key", "EXMPL-12345-ABCDE-67890-FGHIJ"),
            ("licence.licensed_to", "Sam Rivera"),
            ("licence.email", "sam.rivera@example.com"),
            ("licence.version", "5.x"),
            ("licence.publisher", "Example Software"),
            ("licence.url", "https://download.example.com/editor"),
            ("licence.purchased", "2024-02-02"),
        ],
        Opts::default(),
    );
    b.add(
        K::Membership,
        "",
        "Example Gym",
        &[
            ("member.organization", "Example Gym"),
            ("member.name", "Sam Rivera"),
            ("member.number", "GYM-000777"),
            ("member.pin", "4321"),
            ("member.since", "2022-01-10"),
            ("expires_at", "2026-12-31"),
            ("member.url", "https://members.example-gym.test"),
        ],
        Opts::default(),
    );
    b.add(
        K::Custom,
        "",
        "Locker combination",
        &[("notes", "The locker by the back door.")],
        Opts {
            extra_fields: vec![
                custom(0, "Combination", FieldKind::Hidden, "12-34-56"),
                custom(1, "Locker", FieldKind::Text, "B17"),
            ],
            ..Opts::default()
        },
    );

    // --- notes -----------------------------------------------------------------
    b.add(
        K::Note,
        "",
        "A long note",
        &[("body", &(1..=200).map(|n| format!("Paragraph {n}: the demo's long note, so scrolling and search inside a note get exercised.")).collect::<Vec<_>>().join("\n\n"))],
        Opts {
            tags: &["edge-case"],
            ..Opts::default()
        },
    );
    b.add(
        K::Note,
        "",
        "ملاحظة من اليمين إلى اليسار",
        &[("body", "هذه ملاحظة كاملة باللغة العربية. تتضمن قائمة:\n\n- البند الأول\n- البند الثاني\n\nونهاية.")],
        Opts {
            tags: &["i18n"],
            ..Opts::default()
        },
    );
    b.add(K::Note, "", "An empty note", &[], Opts::default());

    // --- bulk: enough to scroll and search ----------------------------------
    let services = [
        "Stream", "Music", "News", "Travel", "Weather", "Maps", "Photos", "Video", "Games",
        "Fitness", "Recipes", "Books", "Podcasts", "Calendar", "Tasks", "Chat", "Forum", "Market",
        "Tickets", "Rides",
    ];
    for (i, s) in services.iter().enumerate() {
        for (j, tld) in ["example.com", "example.org", "test"].iter().enumerate() {
            let host = if *tld == "test" {
                format!("{}.example.test", s.to_lowercase())
            } else {
                format!("{}{}.{tld}", s.to_lowercase(), j)
            };
            b.add(
                K::Login,
                "",
                &format!("Example {s} {}", j + 1),
                &[
                    (
                        "user",
                        &format!("sam+{}{}@example.com", s.to_lowercase(), j),
                    ),
                    ("pw", &format!("bulk-{s}-{i:02}{j}-Pass!")),
                    ("url.0", &format!("https://{host}")),
                ],
                Opts {
                    folder: Some(if j == 2 { "Bulk/Test sites" } else { "Bulk" }),
                    tags: &["bulk"],
                    age_days: (i * 37 + j * 11) as u64,
                    ..Opts::default()
                },
            );
        }
    }
    for n in 1..=12 {
        b.add(
            K::Note,
            "",
            &format!("Meeting notes {n:02}"),
            &[("body", &format!("Notes from meeting {n}: agenda, decisions and follow-ups. Searchable word: pineapple{n}."))],
            Opts {
                folder: Some("Bulk/Notes"),
                tags: &["bulk"],
                ..Opts::default()
            },
        );
    }
    for n in 1..=8 {
        b.add(
            K::Code,
            "",
            &format!("Example service code {n}"),
            &[(
                "code.0",
                &format!("otpauth://totp/Service{n}:sam?secret={RFC6238}&issuer=Service{n}"),
            )],
            Opts {
                folder: Some("Bulk/Codes"),
                tags: &["bulk"],
                ..Opts::default()
            },
        );
    }

    // --- a template of Sam's own, and items made from it -----------------------
    let templates = vec![PackTemplate {
        template: json!({
            "name": "Bike",
            "summary": "A bike's frame number and lock",
            "icon": "pedal_bike",
            "fields": [
                { "key": "frame", "label": "Frame number", "kind": "text", "required": true },
                { "key": "lock", "label": "Lock code", "kind": "hidden" },
                { "key": "shop", "label": "Bought at", "kind": "url" },
                { "key": "bought", "label": "Bought on", "kind": "date" }
            ]
        }),
        items: vec![
            TemplateItem {
                title: "Road bike".into(),
                values: BTreeMap::from([
                    ("frame".into(), "WTU000000001".into()),
                    ("lock".into(), "4-8-1-5".into()),
                    ("shop".into(), "https://bikes.example.com".into()),
                    ("bought".into(), "2025-04-12".into()),
                ]),
            },
            TemplateItem {
                title: "Kid's bike".into(),
                values: BTreeMap::from([("frame".into(), "WTU000000002".into())]),
            },
        ],
    }];

    let header = from_items(&b.items, "example.com", "Lockdown demo pack", BASE).expect("CXF");
    let pack = Pack {
        format: FORMAT.into(),
        v: PACK_V,
        name: "Lockdown QA pack".into(),
        about: "Every kind and variant, the edge cases QA needs, and enough items to scroll. Nothing in it is real.".into(),
        cxf: serde_json::to_value(&header).expect("CXF as JSON"),
        vaults: vec!["Work".into()],
        items: b.extras,
        templates,
    };
    pack.check().expect("the pack checks");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/default-pack.json");
    std::fs::write(&out, pack.to_bytes()).expect("written");
    println!("{} items, written to {}", pack.item_count(), out.display());
}
