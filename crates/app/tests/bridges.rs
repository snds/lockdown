//! The bridges slice through `App`, end to end on a real vault
//! (docs/plan/23-bridges.md): authenticator and Steam files through the
//! import preview, the federated sign-in map from the extension's
//! observations, fill fixes kept on the login, aliases through a local
//! stand-in for SimpleLogin behind the `Use` gate, alias monitoring
//! against the in-process server, the guided incident flows and their
//! sealed progress, the emergency sheet, set-up cards, change-password
//! recipes, the clock check, and exports locked to an age key.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lockdown_app::bridges::federation::FederatedEvidence;
use lockdown_app::bridges::fixes::FillFix;
use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_security::site_data::{FieldFix, FieldMatch};
use lockdown_server::{AppState, Config, run_in_background};

fn fresh() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    (dir, app)
}

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../cxf/tests/fixtures")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn confirmed() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

/// Every field of every item as JSON text, as a shell could see it.
fn everything_shown(app: &App) -> String {
    let mut out = String::new();
    for row in app.list().unwrap() {
        out.push_str(&serde_json::to_string(&app.show(&row.id, false).unwrap()).unwrap());
    }
    out
}

#[test]
fn authenticator_and_steam_files_land_as_codes_and_the_identity_secret_as_a_crown_jewel() {
    let (_dir, app) = fresh();
    assert!(app.import_formats().iter().any(|f| f.id == "aegis"));
    assert_eq!(
        app.import_detect(&fixture("2fas-plain.2fas"), "x.2fas")
            .as_deref(),
        Some("2fas")
    );
    let preview = app
        .import_preview(
            fixture("aegis-encrypted.json"),
            "auto",
            "aegis.json",
            Some(b"fixture-aegis".to_vec()),
            None,
        )
        .unwrap();
    assert_eq!(preview.total, 3);
    assert!(preview.problems.iter().any(|p| p.contains("motp")));
    app.import_commit(preview.token).unwrap();
    assert_eq!(app.codes().unwrap().len(), 3);

    let steam = app
        .import_preview(
            fixture("steam-sda-encrypted.zip"),
            "steam",
            "maFiles.zip",
            Some(b"fixture-sda".to_vec()),
            None,
        )
        .unwrap();
    assert_eq!((steam.total, steam.crown_jewels), (4, 2));
    let report = app.import_commit(steam.token).unwrap();
    assert_eq!(report.imported, 4);
    let rows = app.list().unwrap();
    let recovery: Vec<_> = rows
        .iter()
        .filter(|r| r.title.starts_with("Steam Guard recovery"))
        .collect();
    assert_eq!(recovery.len(), 2);
    assert!(recovery.iter().all(|r| r.crown_jewel));
    assert!(
        rows.iter()
            .filter(|r| r.title.starts_with("Steam ("))
            .all(|r| !r.crown_jewel)
    );
    // The identity secret is a crown jewel's field: revealing it takes the gate.
    let detail = app.show(&recovery[0].id, false).unwrap();
    let field = detail
        .fields
        .iter()
        .find(|f| f.label == "Identity secret")
        .unwrap();
    let err = app
        .reveal(&recovery[0].id, &field.field_id, GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    assert!(!everything_shown(&app).contains("fixture-access-token"));
}

#[test]
fn the_federated_map_records_what_the_provider_confirms_and_asks_about_the_rest() {
    let (_dir, app) = fresh();
    let shop = app
        .add_item(
            "login",
            Some("Shop"),
            false,
            vec![
                FieldChange::text("url.0", "https://www.shop.example/login"),
                FieldChange::text("user", "sean"),
            ],
        )
        .unwrap();
    let google = app
        .add_login(
            "https://accounts.google.com",
            "sean@gmail.com",
            b"g-pass-1".to_vec(),
            Some("Google"),
        )
        .unwrap();
    // The provider's own page, returning to the same site: recorded at once.
    let seen = app
        .federation_seen(FederatedEvidence {
            provider: "google".into(),
            page: "https://www.shop.example/login".into(),
            auth_url: Some(
                "https://accounts.google.com/o/oauth2/v2/auth?client_id=1&redirect_uri=https%3A%2F%2Fwww.shop.example%2Fauth%2Fcallback".into(),
            ),
        })
        .unwrap();
    assert_eq!(seen.recorded, vec![shop.clone()]);
    let detail = app.show(&shop, false).unwrap();
    let text = |id: &str| {
        detail
            .fields
            .iter()
            .find(|f| f.field_id == id)
            .and_then(|f| f.value.clone())
            .unwrap_or_default()
    };
    assert_eq!(text("sign_in_provider"), "Google");
    assert_eq!(text("sign_in_method"), "Another account");
    assert!(
        detail
            .fields
            .iter()
            .all(|f| !f.field_id.starts_with("lockdown.")),
        "Lockdown's own field is never shown"
    );
    let map = app.federation_map().unwrap();
    let g = &map.providers[0];
    assert_eq!((g.id.as_str(), g.sites.len()), ("google", 1));
    assert_eq!(g.account.as_ref().unwrap().item_id, google);
    assert!(g.warnings.iter().any(|w| w.contains("no two-step sign-in")));
    assert!(
        g.warnings
            .iter()
            .any(|w| w.contains("no password of its own"))
    );

    // A button click alone, on a site with no login: waits for a yes.
    let seen = app
        .federation_seen(FederatedEvidence {
            provider: "apple".into(),
            page: "https://news.example/".into(),
            auth_url: None,
        })
        .unwrap();
    let pending = seen.pending.unwrap();
    assert!(seen.recorded.is_empty());
    // The same again is the same observation.
    assert_eq!(
        app.federation_seen(FederatedEvidence {
            provider: "apple".into(),
            page: "https://www.news.example/x".into(),
            auth_url: None,
        })
        .unwrap()
        .pending,
        Some(pending)
    );
    let map = app.federation_map().unwrap();
    assert_eq!(map.pending.len(), 1);
    assert_eq!(map.pending[0].provider_name, "Apple");
    let made = app.federation_accept(pending, None).unwrap();
    assert!(app.federation_map().unwrap().pending.is_empty());
    let apple = app.federation_map().unwrap();
    let apple = apple.providers.iter().find(|p| p.id == "apple").unwrap();
    assert_eq!(apple.sites[0].item_id, made);
    assert!(apple.warnings[0].contains("no Apple login saved"));

    // A hostile return address is refused; a crown jewel is never touched.
    assert!(app
        .federation_seen(FederatedEvidence {
            provider: "google".into(),
            page: "https://bank.example".into(),
            auth_url: Some("https://accounts.google.com/o/oauth2/auth?redirect_uri=https%3A%2F%2Fevil.example%2F".into()),
        })
        .is_err());
    app.add_item(
        "login",
        Some("Bank"),
        true,
        vec![
            FieldChange::text("url.0", "https://bank.example"),
            FieldChange::text("user", "s"),
        ],
    )
    .unwrap();
    let seen = app
        .federation_seen(FederatedEvidence {
            provider: "google".into(),
            page: "https://bank.example".into(),
            auth_url: Some("https://accounts.google.com/o/oauth2/auth?redirect_uri=https%3A%2F%2Fbank.example%2F".into()),
        })
        .unwrap();
    assert!(seen.recorded.is_empty());
    // Pending observations go with the lock.
    assert_eq!(app.federation_map().unwrap().pending.len(), 1);
    app.lock().unwrap();
    app.unlock().unwrap();
    assert!(app.federation_map().unwrap().pending.is_empty());
    // Picking the provider's account by hand.
    let other = app
        .add_login(
            "https://accounts.google.com",
            "work@gmail.com",
            b"g-pass-2".to_vec(),
            Some("Work Google"),
        )
        .unwrap();
    let map = app.federation_map().unwrap();
    assert!(map.providers[0].account.is_none());
    assert_eq!(map.providers[0].candidates.len(), 2);
    app.federation_set_account("google", &other).unwrap();
    assert_eq!(
        app.federation_map().unwrap().providers[0]
            .account
            .as_ref()
            .unwrap()
            .item_id,
        other
    );
}

#[test]
fn fill_fixes_live_on_the_login_and_join_the_community_list() {
    let (_dir, app) = fresh();
    let a = app
        .add_login(
            "https://shop.example/login",
            "sean",
            b"pw-a-1".to_vec(),
            None,
        )
        .unwrap();
    let b = app
        .add_login(
            "https://www.shop.example",
            "other",
            b"pw-b-1".to_vec(),
            None,
        )
        .unwrap();
    let fix = FillFix {
        never: false,
        fields: vec![FieldFix {
            matcher: FieldMatch {
                name: Some("login_id".into()),
                ..FieldMatch::default()
            },
            role: "username".into(),
        }],
    };
    assert_eq!(
        app.site_fix_set_for("https://shop.example/signin", fix.clone())
            .unwrap(),
        2
    );
    let view = app.site_fix_for("https://shop.example/login").unwrap();
    assert_eq!(view.fields.len(), 1);
    assert_eq!(view.fields[0].source, "yours");
    assert_eq!(view.yours, 2);
    assert_eq!(app.site_fix(&a).unwrap(), fix);
    // An ordinary edit keeps it, and nothing shows it.
    app.edit_login(&b, None, Some("renamed"), None, GateInput::default())
        .unwrap();
    assert_eq!(app.site_fix(&b).unwrap(), fix);
    assert!(!everything_shown(&app).contains("login_id"));
    // Never fill here, then cleared.
    app.site_fix_set(
        &a,
        FillFix {
            never: true,
            fields: vec![],
        },
    )
    .unwrap();
    assert!(app.site_fix_for("https://shop.example").unwrap().never);
    app.site_fix_set(&a, FillFix::default()).unwrap();
    app.site_fix_set(&b, FillFix::default()).unwrap();
    let view = app.site_fix_for("https://shop.example").unwrap();
    assert!(!view.never && view.fields.is_empty());
    // A bad fix and a site with no login are refused.
    assert!(
        app.site_fix_set(
            &a,
            FillFix {
                never: false,
                fields: vec![FieldFix {
                    matcher: FieldMatch::default(),
                    role: "username".into()
                }]
            }
        )
        .is_err()
    );
    assert!(
        app.site_fix_set_for("https://nowhere.example", fix)
            .is_err()
    );
}

type Seen = Arc<Mutex<Vec<(String, String)>>>;

/// A stand-in for SimpleLogin: records the path and the key header.
fn simplelogin() -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let (mut key, mut len) = (String::new(), 0usize);
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                let lower = h.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("authentication:") {
                    key = v.trim().to_string();
                }
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            let _ = reader.read_exact(&mut body);
            log.lock().unwrap().push((
                line.split_whitespace().nth(1).unwrap_or("").to_string(),
                key,
            ));
            let body = format!(r#"{{"alias":"shop.word{n}@simplelogin.com"}}"#);
            let _ = write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, seen)
}

#[test]
fn an_alias_is_made_through_the_use_gate_with_a_crown_jewel_key_and_joins_the_monitored_set() {
    let (_dir, app) = fresh();
    let (base, seen) = simplelogin();
    let overview = app.alias_overview().unwrap();
    assert_eq!(overview.services.len(), 6);
    let item = app
        .alias_service_add(
            "simple_login",
            b"sl-test-key-123".to_vec(),
            Some(&base),
            None,
        )
        .unwrap();
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == item)
        .unwrap();
    assert!(row.crown_jewel, "the API key is a crown jewel");
    assert_eq!(
        app.alias_overview().unwrap().configured[0].service,
        "simple_login"
    );
    // No gate answered: nothing leaves.
    let err = app
        .alias_generate(&item, "https://shop.example/signup", GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    assert!(seen.lock().unwrap().is_empty());
    let made = app
        .alias_generate(&item, "https://shop.example/signup", confirmed())
        .unwrap();
    assert_eq!(made.address, "shop.word0@simplelogin.com");
    assert_eq!(made.service_name, "SimpleLogin");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen[0],
        (
            "/api/alias/random/new?hostname=shop.example".into(),
            "sl-test-key-123".into()
        )
    );
    assert_eq!(app.alias_overview().unwrap().configured[0].made, 1);
    // The key never shows; the address joins the monitored set, with a
    // saved user name on an alias domain.
    assert!(!everything_shown(&app).contains("sl-test-key-123"));
    app.add_login(
        "https://forum.example",
        "q1@duck.com",
        b"pw-forum".to_vec(),
        None,
    )
    .unwrap();
    let m = app.alias_monitoring().unwrap();
    assert_eq!(m.addresses, ["q1@duck.com", "shop.word0@simplelogin.com"]);
    assert!(!m.enabled && !m.has_account);
    assert!(
        app.alias_monitoring_set(true).is_err(),
        "no account, no server to watch them"
    );
}

#[test]
fn alias_monitoring_tells_the_server_and_off_empties_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state).unwrap();
    let url = running.url();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.account_register(&url, "me@example.com", "correct horse battery staple", None)
        .unwrap();
    app.add_login(
        "https://forum.example",
        "q1@duck.com",
        b"pw-forum".to_vec(),
        None,
    )
    .unwrap();
    let on = app.alias_monitoring_set(true).unwrap();
    assert!(on.enabled && on.has_account);
    assert!(app.settings().unwrap().alias_monitoring);
    // A shell can't switch it on or off through the plain settings call.
    let mut s = app.settings().unwrap();
    s.alias_monitoring = false;
    assert!(app.set_settings(s).unwrap().alias_monitoring);
    let off = app.alias_monitoring_set(false).unwrap();
    assert!(!off.enabled);
    // The clock check against the same server: in step.
    let clock = app.clock_check().unwrap();
    assert!(clock.offset_secs.abs() <= 3, "{clock:?}");
    assert!(!clock.correcting);
    assert_eq!(app.clock_status().unwrap().unwrap(), clock);
}

#[test]
fn the_clock_check_needs_an_account() {
    let (_dir, app) = fresh();
    assert!(app.clock_status().unwrap().is_none());
    assert!(
        app.clock_check()
            .unwrap_err()
            .message
            .contains("no account")
    );
}

#[test]
fn the_incident_flows_fill_in_from_the_vault_and_keep_progress_sealed() {
    let (dir, app) = fresh();
    let shop = app
        .add_login(
            "https://shop.example",
            "sean@example.com",
            b"same-pass-1".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    let forum = app
        .add_login(
            "https://forum.example",
            "sean@example.com",
            b"same-pass-1".to_vec(),
            Some("Forum"),
        )
        .unwrap();
    app.add_login(
        "https://mail.example.com",
        "sean@example.com",
        b"mail-pass-9X!q".to_vec(),
        Some("Mailbox"),
    )
    .unwrap();
    app.add_code(
        "otpauth://totp/Shop:sean?secret=JBSWY3DPEHPK3PXP&issuer=Shop",
        None,
        Some(&shop),
    )
    .unwrap();
    let list = app.incidents().unwrap();
    assert_eq!(list.len(), 7);
    let flow = app.incident("site-breach", Some(&shop)).unwrap();
    assert_eq!(flow.picked.as_deref(), Some(shop.as_str()));
    let same = flow.steps.iter().find(|s| s.id == "same").unwrap();
    assert_eq!(
        same.items
            .iter()
            .map(|i| i.item_id.as_str())
            .collect::<Vec<_>>(),
        [forum.as_str()]
    );
    assert_eq!(flow.steps[0].items[0].item_id, shop);
    let flow = app
        .incident_mark("site-breach", "change", None, true)
        .unwrap();
    assert!(flow.steps[0].done && !flow.finished);
    let flow = app
        .incident_mark("site-breach", "same", Some(&forum), true)
        .unwrap();
    assert!(flow.steps[1].items[0].done);
    // Progress survives a lock, sealed: the file says nothing in the clear.
    app.lock().unwrap();
    app.unlock().unwrap();
    assert!(app.incident("site-breach", None).unwrap().steps[0].done);
    let file = std::fs::read_dir(dir.path().join("snds/lockdown"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with("incidents-"))
        .unwrap();
    let bytes = std::fs::read(file.path()).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(&shop));
    // Seed leak lists every code; email takeover splits the mailbox from the rest.
    let seed = app.incident("seed-leak", None).unwrap();
    assert_eq!(seed.steps[0].items.len(), 1);
    let email = app
        .incident("email-takeover", Some("sean@example.com"))
        .unwrap();
    let mailbox = &email
        .steps
        .iter()
        .find(|s| s.id == "password")
        .unwrap()
        .items;
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox[0].title, "Mailbox");
    let using = &email
        .steps
        .iter()
        .find(|s| s.id == "accounts")
        .unwrap()
        .items;
    assert_eq!(using.len(), 2);
    assert!(email.choices.iter().any(|c| c.value == "sean@example.com"));
    // Picking for a flow that needs nothing, a step that isn't, are refused.
    assert!(app.incident("infostealer", Some("x")).is_err());
    assert!(
        app.incident_mark("site-breach", "nope", None, true)
            .is_err()
    );
    let reset = app.incident_reset("site-breach").unwrap();
    assert!(reset.started_at.is_none() && reset.steps.iter().all(|s| !s.done));
}

#[test]
fn the_emergency_sheet_says_where_things_are_and_holds_no_secret() {
    let (_dir, app) = fresh();
    app.add_login(
        "https://shop.example",
        "sean",
        b"super-secret-pw-1".to_vec(),
        None,
    )
    .unwrap();
    let kit = app.kit_create(false, None).unwrap();
    let page = String::from_utf8(kit.kit.expose().to_vec()).unwrap_or_default();
    let sheet = app.emergency_sheet().unwrap();
    let text = serde_json::to_string(&sheet).unwrap();
    assert!(!text.contains("super-secret-pw-1"));
    for line in page
        .lines()
        .filter(|l| l.starts_with("Secret Key") || l.starts_with("Recovery code"))
    {
        let value = line.split_once(':').unwrap().1.trim();
        if value.len() > 12 {
            assert!(!text.contains(value), "{value} is on the sheet");
        }
    }
    let headings: Vec<&str> = sheet.sections.iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(
        headings,
        [
            "Your account",
            "Your devices",
            "Your backups",
            "Emergency contacts",
            "What to do"
        ]
    );
    assert!(text.contains("lives on this computer only"));
    assert!(sheet.blanks.iter().any(|b| b.contains("recovery kit")));
}

#[test]
fn set_up_cards_and_change_password_recipes_come_from_the_signed_data() {
    let (_dir, app) = fresh();
    let github = app
        .add_login(
            "https://github.com/login",
            "sean",
            b"pw-gh-1".to_vec(),
            Some("GitHub"),
        )
        .unwrap();
    let guide = app.setup_guide(&github).unwrap().unwrap();
    assert_eq!(guide.site, "GitHub");
    assert!(guide.passkeys.is_some());
    assert!(guide.attribution.iter().any(|a| a.licence == "CC BY 4.0"));
    let note = app
        .add_item(
            "note",
            Some("n"),
            false,
            vec![FieldChange::text("body", "x")],
        )
        .unwrap();
    assert!(app.setup_guide(&note).unwrap().is_none());
    let sources = app.guide_sources();
    assert_eq!(sources.sources.len(), 2);
    let mut s = app.settings().unwrap();
    s.change_password_check = false;
    app.set_settings(s).unwrap();
    let link = app.security_change_link(&github).unwrap();
    assert_eq!(link.url, "https://github.com/settings/security");
}

#[test]
fn an_export_locked_to_an_age_key_opens_only_with_the_private_key() {
    let (dir, app) = fresh();
    app.add_login(
        "https://shop.example",
        "sean",
        b"pw-shop-7".to_vec(),
        Some("Shop"),
    )
    .unwrap();
    let id = lockdown_cxf::age_file::Identity::generate().unwrap();
    let path = dir.path().join("vault.lockdown.age");
    assert!(
        app.export_to_recipients(path.clone(), vec!["not-a-key".into()], None, confirmed())
            .unwrap_err()
            .message
            .contains("isn't an age public key")
    );
    let err = app
        .export_to_recipients(
            path.clone(),
            vec![id.recipient().encode()],
            None,
            GateInput::default(),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    let done = app
        .export_to_recipients(
            path.clone(),
            vec![id.recipient().encode()],
            None,
            confirmed(),
        )
        .unwrap();
    assert!(done.preview.encrypted);
    let bytes = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("pw-shop-7"));
    // Back in on a second vault: the preview needs the private key.
    let (_d2, other) = fresh();
    assert_eq!(
        other.import_detect(&bytes, "vault.lockdown.age").as_deref(),
        Some("lockdown")
    );
    assert!(
        other
            .import_preview(bytes.clone(), "auto", "v.age", None, None)
            .is_err()
    );
    let preview = other
        .import_preview(
            bytes,
            "auto",
            "v.age",
            Some(id.encode().as_bytes().to_vec()),
            None,
        )
        .unwrap();
    assert_eq!(preview.total, 1);
}
