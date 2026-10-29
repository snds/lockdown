//! The demo end to end (docs/plan/40-devtools.md §3): the default pack
//! reads and round-trips; a bad pack is refused with the reader's
//! sentence; the demo holds every item kind; it never touches the real
//! profile, the keystore or the network; leaving and resetting leave no
//! trace. One test drives the app, because the network hold the demo sets
//! is for the whole process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_devtools::demo::pack::{DEFAULT, Pack};
use lockdown_devtools::demo::{CameFrom, Demo, WHY, default_pack, demo_dir};
use lockdown_devtools::reset::{MemoryKeystore, ServiceStore};
use lockdown_vault::{ItemKind, TEMPLATE_FIELD, TEMPLATES, template_for};

fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

/// Every file under `dir` with its bytes, but `<dir>/demo`.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in read.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == "demo") && d == dir {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.clone(), std::fs::read(&p).unwrap_or_default());
            }
        }
    }
    out
}

#[test]
fn the_default_pack_reads_and_round_trips() {
    let pack = Pack::parse(DEFAULT).unwrap();
    assert_eq!(pack.name, "Lockdown QA pack");
    assert!(pack.item_count() >= 150, "{}", pack.item_count());
    // The file is exactly what the pack writes, and reads back the same.
    assert_eq!(Pack::parse(&pack.to_bytes()).unwrap(), pack);
    assert_eq!(
        pack.to_bytes(),
        DEFAULT,
        "the file is in the writer's shape"
    );
    // Its CXF document survives Lockdown's own import and export.
    let header = pack.header().unwrap();
    let items = lockdown_cxf::to_items(&header).unwrap();
    let again = lockdown_cxf::from_items(
        &items,
        &header.exporter_rp_id,
        &header.exporter_display_name,
        header.timestamp,
    )
    .unwrap();
    let back = lockdown_cxf::to_items(&again).unwrap();
    assert_eq!(back.len(), items.len());
    for (a, b) in items.iter().zip(&back) {
        assert_eq!(
            (a.kind, &a.title, &a.tags, a.favourite),
            (b.kind, &b.title, &b.tags, b.favourite)
        );
        let ids = |i: &lockdown_cxf::ImportedItem| {
            let mut v: Vec<(String, lockdown_vault::FieldValue)> = i
                .fields
                .iter()
                .map(|f| (f.field_id.clone(), f.value.clone()))
                .collect();
            v.sort_by(|x, y| x.0.cmp(&y.0));
            v
        };
        assert_eq!(ids(a), ids(b), "{}", a.title);
    }
}

#[test]
fn the_default_pack_is_safe_and_has_every_kind_and_variant() {
    let pack = default_pack();
    let text = String::from_utf8(DEFAULT.to_vec()).unwrap();
    // Only example.com, example.org and .test addresses.
    let items = lockdown_cxf::to_items(&pack.header().unwrap()).unwrap();
    for item in &items {
        for f in &item.fields {
            if let lockdown_vault::FieldValue::Url { value, .. } = &f.value {
                let host = value
                    .split("://")
                    .nth(1)
                    .and_then(|r| r.split(['/', '?', ':']).next())
                    .unwrap_or("");
                assert!(
                    host == "example.com"
                        || host.ends_with(".example.com")
                        || host == "example.org"
                        || host.ends_with(".example.org")
                        || host.ends_with(".test"),
                    "{}: {value}",
                    item.title
                );
            }
        }
    }
    // No key material in the file: keys are made when the demo opens.
    assert!(!text.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert!(
        !items
            .iter()
            .flat_map(|i| &i.fields)
            .any(|f| f.field_id.ends_with(".key") && f.field_id.starts_with("passkey."))
    );
    // The test vectors are there.
    assert!(text.contains("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
    assert!(text.contains("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"));
    assert!(text.contains("4242424242424242"));
    // Every creatable template, its own and each variant, has an item.
    let have: BTreeSet<(ItemKind, String)> = items
        .iter()
        .map(|i| {
            let variant = i
                .fields
                .iter()
                .find(|f| f.field_id == TEMPLATE_FIELD)
                .and_then(|f| match &f.value {
                    lockdown_vault::FieldValue::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            (i.kind, variant)
        })
        .collect();
    for t in TEMPLATES.iter().filter(|t| !t.kind.hidden()) {
        assert!(
            have.contains(&(t.kind, t.variant.to_string())),
            "no item of {} {}",
            t.kind.as_str(),
            t.variant
        );
    }
}

#[test]
fn a_bad_pack_is_refused_with_the_readers_sentence() {
    let refused = |bytes: &[u8]| Pack::parse(bytes).unwrap_err();
    assert!(refused(b"not json").starts_with("The pack isn't JSON: "));
    let e = refused(br#"{"format":"lockdown-demo-pack","v":1,"name":"x","cxf":{},"extra":1}"#);
    assert!(
        e.starts_with("The pack didn't read: unknown field `extra`"),
        "{e}"
    );
    let e = refused(br#"{"format":"lockdown-demo-pack","v":9,"name":"x","cxf":{}}"#);
    assert_eq!(e, "The pack is version 9; this build reads up to 1.");
    let e = refused(br#"{"format":"lockdown-demo-pack","v":1,"name":"x","cxf":{"version":1}}"#);
    assert!(
        e.starts_with("The pack's CXF document didn't read: "),
        "{e}"
    );
    // An extra naming an item the document lacks.
    let mut pack = default_pack();
    pack.items.insert("no-such-item".into(), Default::default());
    assert_eq!(
        Pack::parse(&pack.to_bytes()).unwrap_err(),
        "The pack's item 'no-such-item': there is no CXF item with that id."
    );
    // A placeholder file over the limit.
    let mut pack = default_pack();
    let (id, extras) = pack
        .items
        .iter_mut()
        .find(|(_, e)| !e.files.is_empty())
        .unwrap();
    let id = id.clone();
    extras.files[0].text = None;
    extras.files[0].size = Some(1 << 40);
    let e = Pack::parse(&pack.to_bytes()).unwrap_err();
    assert!(
        e.starts_with(&format!("The pack's item '{id}': the file")),
        "{e}"
    );
    // A bare CXF document is a pack with no extras.
    let cxf = serde_json::to_vec(&default_pack().cxf).unwrap();
    let bare = Pack::parse(&cxf).unwrap();
    assert_eq!(bare.name, "A CXF file");
    assert!(bare.items.is_empty());
}

#[test]
fn the_demo_is_whole_separate_offline_and_leaves_no_trace() {
    // The OS keystore stands in as a double the app writes to.
    let keystore: &'static MemoryKeystore = Box::leak(Box::default());
    lockdown_app::secrets::install(Box::new(ServiceStore(keystore)));

    let dir = tempfile::tempdir().unwrap();
    let home = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(home.clone(), None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    let mine = app
        .add_login(
            "https://mine.example.com",
            "me",
            b"my-own-password".to_vec(),
            None,
        )
        .unwrap();
    let mut settings = app.settings().unwrap();
    settings.clipboard_clear_secs = 45;
    app.set_settings(settings).unwrap();
    app.lock().unwrap();
    let before_data = snapshot(&home.data);
    let before_root = snapshot(&home.root);

    // A demo folder left by a run that quit inside it is wiped.
    std::fs::create_dir_all(demo_dir(&home.data).join("stale")).unwrap();
    let demo = Demo::new(app.clone(), &home);
    assert!(!demo_dir(&home.data).exists());

    let started = std::time::Instant::now();
    let opened = demo.open(default_pack()).unwrap();
    eprintln!("the demo opened in {:?}", started.elapsed());
    assert_eq!(opened.came_from, CameFrom::Vault);
    assert_eq!(app.confinement().unwrap().as_deref(), Some(WHY));
    assert!(opened.loaded.notes.is_empty(), "{:?}", opened.loaded.notes);

    // Every kind the core can make is in it.
    let rows = app.list().unwrap();
    assert!(rows.len() >= 150, "{}", rows.len());
    let kinds: BTreeSet<String> = rows.iter().map(|r| r.kind.clone()).collect();
    let vaults = app.vaults().unwrap();
    for kind in ItemKind::ALL {
        let present = match kind {
            // The vault's own record: made with its folders.
            ItemKind::VaultInfo => vaults.iter().any(|v| !v.folders.is_empty()),
            // A template of the person's own.
            ItemKind::Template => !app.user_templates().unwrap().is_empty(),
            // No path in the core makes one yet; when one exists, this
            // fails and the pack must grow.
            ItemKind::Consent => {
                assert!(!template_for(kind).creatable);
                continue;
            }
            k => kinds.contains(k.as_str()),
        };
        assert!(present, "no {} in the demo", kind.as_str());
    }
    assert_eq!(vaults.len(), 2, "Personal and Work");
    // Trash, crown jewels, history, files, fresh keys and Watchtower.
    assert!(rows.iter().filter(|r| r.state == "trashed").count() >= 2);
    assert!(rows.iter().any(|r| r.state == "archived"));
    assert!(rows.iter().filter(|r| r.crown_jewel).count() >= 2);
    let mail = rows.iter().find(|r| r.title == "Example Mail").unwrap();
    assert_eq!(
        app.history(&mail.id).unwrap().len(),
        4,
        "three earlier versions and now"
    );
    let shop = rows.iter().find(|r| r.title == "Example Shop").unwrap();
    assert_eq!(shop.passkeys, 1);
    let social = rows.iter().find(|r| r.title == "Example Social").unwrap();
    assert_eq!(social.passkeys, 2);
    let ssh = rows.iter().find(|r| r.title == "Build server key").unwrap();
    let detail = app.show(&ssh.id, false).unwrap();
    let public = detail
        .fields
        .iter()
        .find(|f| f.field_id == "ssh.public_key")
        .and_then(|f| f.value.clone())
        .unwrap_or_default();
    assert!(public.starts_with("ssh-ed25519 "), "{public}");
    let big = rows
        .iter()
        .find(|r| r.title.starts_with("Large archive"))
        .unwrap();
    let big = app.show(&big.id, false).unwrap();
    assert_eq!(big.place.attachments.len(), 1);
    assert_eq!(big.place.attachments[0].size, 31 * 1024 * 1024);
    let report = app.security_report().unwrap();
    let kinds_found: BTreeSet<&str> = report.findings.iter().map(|f| f.kind.as_str()).collect();
    assert!(kinds_found.contains("weak"), "{kinds_found:?}");
    assert!(kinds_found.contains("reused"), "{kinds_found:?}");

    // Read-write: an edit works; the network and exports don't.
    let id = app
        .add_login(
            "https://new.example.org",
            "demo",
            b"made-in-the-demo".to_vec(),
            None,
        )
        .unwrap();
    app.trash(&id).unwrap();
    let refused = app.export_items("cxf", yes()).unwrap_err();
    assert_eq!(
        refused.message,
        "The demo never syncs, shares, exports, backs up or goes online."
    );
    let probe = lockdown_sync::network_held();
    assert_eq!(probe.as_deref(), Some(WHY));
    assert!(
        app.account_register(
            "https://127.0.0.1:9",
            "demo@example.com",
            "a password",
            None
        )
        .is_err()
    );

    // Reset builds it again from the pack: the edit is gone.
    let rebuilt = demo.reset().unwrap();
    assert_eq!(rebuilt.loaded.items, opened.loaded.items);
    let rows = app.list().unwrap();
    assert!(!rows.iter().any(|r| r.title == "new.example.org"));
    let new_ssh = rows.iter().find(|r| r.title == "Build server key").unwrap();
    let new_public = app
        .show(&new_ssh.id, false)
        .unwrap()
        .fields
        .iter()
        .find(|f| f.field_id == "ssh.public_key")
        .and_then(|f| f.value.clone())
        .unwrap_or_default();
    assert_ne!(new_public, public, "keys are made fresh each time");

    // Leaving: back home, locked, the network released, nothing left.
    assert_eq!(demo.leave().unwrap(), CameFrom::Vault);
    assert!(!demo_dir(&home.data).exists());
    assert_eq!(app.confinement().unwrap(), None);
    assert_eq!(lockdown_sync::network_held(), None);
    assert!(app.lock_state().unwrap().locked);
    assert_eq!(
        snapshot(&home.data),
        before_data,
        "the real data is untouched"
    );
    assert_eq!(
        snapshot(&home.root),
        before_root,
        "the real identity is untouched"
    );
    assert!(keystore.all().is_empty(), "the keystore was never written");
    app.unlock().unwrap();
    let rows = app.list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, mine);
    assert_eq!(app.settings().unwrap().clipboard_clear_secs, 45);

    // From first run, leaving goes back to first run.
    let fresh = tempfile::tempdir().unwrap();
    let nothing = Paths::resolve(Some(fresh.path().join("snds/identity")), None).unwrap();
    let app = App::start(nothing.clone(), None, LockPolicy::default());
    let demo = Demo::new(app.clone(), &nothing);
    // Another pack, small: a bare CXF document of one item.
    let mut one = lockdown_cxf::ImportedItem::new("b25lMQ", ItemKind::Note, "Just one note");
    one.fields = vec![lockdown_vault::PlainField::text(
        "title",
        "title",
        "Just one note",
    )];
    let cxf = lockdown_cxf::from_items(&[one], "example.com", "Test", 0).unwrap();
    let small = Pack::parse(&serde_json::to_vec(&cxf).unwrap()).unwrap();
    let opened = demo.open(small).unwrap();
    assert_eq!(opened.came_from, CameFrom::FirstRun);
    assert_eq!(opened.loaded.items, 1);
    assert_eq!(app.list().unwrap().len(), 1);
    assert_eq!(demo.leave().unwrap(), CameFrom::FirstRun);
    let probe = app.probe().unwrap();
    assert!(!probe.identity_present && !probe.device_key_present && !probe.account_present);
    assert!(!demo_dir(&nothing.data).exists());
}
