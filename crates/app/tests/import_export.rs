//! Import, export and backups through the app (docs/plan/14-import-export.md):
//! every sample file previews and lands; crown jewels leave only when
//! opted in (D121); a sealed Lockdown backup restores everything, a second
//! restore merges; scheduled backups write sealed files and keep the
//! newest N.

use std::path::{Path, PathBuf};

use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};

fn fresh(dir: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../cxf/tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

const PHRASE: &str =
    "abandon ability able about above absent absorb abstract absurd abuse access accident";

#[test]
fn every_sample_file_previews_then_lands() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh(dir.path(), "a");
    for (name, password, total) in [
        ("1password.1pux", None, 6),
        ("bitwarden.json", None, 5),
        (
            "bitwarden-protected.json",
            Some(b"fixture-password".to_vec()),
            5,
        ),
        ("proton.zip", None, 5),
        ("keepass.kdbx", Some(b"fixture-password".to_vec()), 2),
        ("dashlane.zip", None, 7),
        ("keeper.json", None, 5),
        ("lastpass.csv", None, 6),
        ("enpass.json", None, 4),
    ] {
        let before = app.list().unwrap().len();
        let preview = app
            .import_preview(fixture(name), "auto", name, password)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(preview.total, total, "{name}");
        assert_eq!(preview.new, total, "{name}");
        assert!(!preview.by_kind.is_empty());
        assert_eq!(app.list().unwrap().len(), before, "a preview keeps nothing");
        let report = app.import_commit(preview.token).unwrap();
        assert_eq!(report.imported, total, "{name}");
        assert_eq!(app.list().unwrap().len(), before + total, "{name}");
        // The token is spent.
        assert!(app.import_commit(preview.token).is_err());
    }
    // A password-protected file asks for its password first.
    let err = app
        .import_preview(fixture("keepass.kdbx"), "auto", "x.kdbx", None)
        .unwrap_err();
    assert!(err.message.contains("protected with a password"), "{err}");
    // The folder and tags a source had are kept as fields of your own.
    let rows = app.list().unwrap();
    let forum = rows.iter().find(|r| r.title == "Example Forum").unwrap();
    let detail = app.show(&forum.id, false).unwrap();
    assert!(
        detail
            .fields
            .iter()
            .any(|f| f.label == "Folder" && f.value.as_deref() == Some("Work"))
    );
    // A 1Password archived item lands archived; a document keeps its file.
    assert!(
        rows.iter()
            .any(|r| r.title == "Home Wi-Fi" && r.kind == "wi-fi" && r.state == "archived"),
        "1Password's archived network"
    );
}

#[test]
fn crown_jewels_stay_out_of_exports_unless_opted_in() {
    let dir = tempfile::tempdir().unwrap();
    let app = fresh(dir.path(), "a");
    app.add_login(
        "https://forum.example.com",
        "ada",
        b"pw-1".to_vec(),
        Some("Forum"),
    )
    .unwrap();
    let wallet = app
        .add_item(
            "crypto-wallet",
            Some("Cold wallet"),
            false,
            vec![FieldChange::secret("seed", PHRASE.as_bytes())],
        )
        .unwrap();
    let preview = app.export_preview("cxf").unwrap();
    assert_eq!(preview.items, 1);
    assert_eq!(preview.crown_jewels_left_out, 1);
    assert_eq!(preview.left_out[0].title, "Cold wallet");

    let out = dir.path().join("export.json");
    // A plain file needs the person's yes first, then the gate.
    let err = app
        .export_to(out.clone(), "cxf", None, false, yes())
        .unwrap_err();
    assert!(err.message.contains("won't be protected"), "{err}");
    let err = app
        .export_to(out.clone(), "cxf", None, true, GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    let done = app
        .export_to(out.clone(), "cxf", None, true, yes())
        .unwrap();
    assert_eq!(done.preview.crown_jewels_left_out, 1);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("pw-1"));
    assert!(!text.contains("abandon"), "the phrase stayed in the vault");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&out).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "readable by the person only");
    }

    // Opting in asks first; then the wallet leaves, and an edit keeps it opted in.
    let err = app
        .set_export_opt_in(&wallet, true, GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    app.set_export_opt_in(&wallet, true, yes()).unwrap();
    assert!(app.show(&wallet, false).unwrap().export_opt_in);
    assert!(
        app.show(&wallet, false)
            .unwrap()
            .fields
            .iter()
            .all(|f| !f.field_id.starts_with("lockdown.")),
        "the opt-in is a flag on the item, not a field to draw"
    );
    app.edit_item(
        &wallet,
        Some("Cold wallet 2"),
        vec![FieldChange::keep("seed")],
        yes(),
    )
    .unwrap();
    assert!(app.show(&wallet, false).unwrap().export_opt_in);
    let preview = app.export_preview("cxf").unwrap();
    assert_eq!(
        (preview.crown_jewels_left_out, preview.crown_jewels_included),
        (0, 1)
    );
    app.export_to(out.clone(), "cxf", None, true, yes())
        .unwrap();
    assert!(std::fs::read_to_string(&out).unwrap().contains("abandon"));
    // Opting out asks nothing.
    app.set_export_opt_in(&wallet, false, GateInput::default())
        .unwrap();
    assert!(!app.show(&wallet, false).unwrap().export_opt_in);
    // A standard item has nothing to opt in.
    let forum = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.title == "Forum")
        .unwrap();
    assert!(app.set_export_opt_in(&forum.id, true, yes()).is_err());
}

#[test]
fn a_sealed_backup_restores_everything_and_a_second_restore_merges() {
    let dir = tempfile::tempdir().unwrap();
    let a = fresh(dir.path(), "a");
    // Content with history, files, an archived item and a trashed one.
    let p = a
        .import_preview(fixture("1password.1pux"), "auto", "x.1pux", None)
        .unwrap();
    a.import_commit(p.token).unwrap();
    let forum = a
        .add_login(
            "https://forum.example.com",
            "ada",
            b"pw-1".to_vec(),
            Some("Forum"),
        )
        .unwrap();
    a.edit_login(
        &forum,
        None,
        None,
        Some(b"pw-2".to_vec()),
        GateInput::default(),
    )
    .unwrap();
    let note = a
        .add_item(
            "note",
            Some("Draft"),
            false,
            vec![FieldChange::text("body", "x")],
        )
        .unwrap();
    a.trash(&note).unwrap();
    let wallet = a
        .add_item(
            "crypto-wallet",
            Some("Cold wallet"),
            false,
            vec![FieldChange::secret("seed", PHRASE.as_bytes())],
        )
        .unwrap();
    a.set_export_opt_in(&wallet, true, yes()).unwrap();

    let file = dir.path().join("vault.lockdown");
    let preview = a.export_preview("lockdown").unwrap();
    assert!(preview.attachments >= 1, "{preview:?}");
    let done = a
        .export_to(
            file.clone(),
            "lockdown",
            Some(b"backup password 1".to_vec()),
            false,
            yes(),
        )
        .unwrap();
    assert!(done.preview.encrypted);
    let bytes = std::fs::read(&file).unwrap();
    assert!(bytes.starts_with(b"LDBACKUP"));
    assert!(!String::from_utf8_lossy(&bytes).contains("pw-2"));

    // Into a vault on another identity: everything back, by id.
    let b = fresh(dir.path(), "b");
    let wrong = b
        .import_preview(
            bytes.clone(),
            "auto",
            "vault.lockdown",
            Some(b"nope".to_vec()),
        )
        .unwrap_err();
    assert!(wrong.message.contains("didn't open"), "{wrong}");
    let p = b
        .import_preview(
            bytes.clone(),
            "auto",
            "vault.lockdown",
            Some(b"backup password 1".to_vec()),
        )
        .unwrap();
    assert_eq!(p.format, "lockdown");
    assert_eq!(p.new, p.total);
    b.import_commit(p.token).unwrap();
    let rows_a = a.list().unwrap();
    let rows_b = b.list().unwrap();
    assert_eq!(rows_a.len(), rows_b.len());
    for r in &rows_a {
        let other = rows_b.iter().find(|x| x.id == r.id).expect("same id");
        assert_eq!(
            (&other.title, &other.kind, &other.state),
            (&r.title, &r.kind, &r.state)
        );
        assert_eq!(other.crown_jewel, r.crown_jewel);
    }
    assert_eq!(
        b.reveal(&forum, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-2"
    );
    assert_eq!(
        b.show(&forum, false).unwrap().version,
        2,
        "history came too"
    );
    assert!(b.show(&wallet, false).unwrap().crown_jewel);
    assert!(b.show(&wallet, false).unwrap().export_opt_in);
    // The files came back byte for byte: B's own plain export carries them.
    let check = dir.path().join("check.json");
    b.export_to(check.clone(), "lockdown", None, true, yes())
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&check).unwrap()).unwrap();
    let lease = doc["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["fields"][0]["value"]["text"] == "Lease")
        .unwrap();
    assert_eq!(lease["attachments"][0]["name"], "lease.pdf");
    assert_eq!(lease["attachments"][0]["size"], 22);

    // Restoring again changes nothing; after an edit, the backup's copy
    // comes back as a new version.
    let p = b
        .import_preview(
            bytes.clone(),
            "lockdown",
            "vault.lockdown",
            Some(b"backup password 1".to_vec()),
        )
        .unwrap();
    assert_eq!((p.new, p.updated), (0, 0), "{p:?}");
    b.import_commit(p.token).unwrap();
    assert_eq!(b.list().unwrap().len(), rows_a.len());
    b.edit_login(
        &forum,
        None,
        None,
        Some(b"pw-3".to_vec()),
        GateInput::default(),
    )
    .unwrap();
    let p = b
        .import_preview(
            bytes,
            "lockdown",
            "vault.lockdown",
            Some(b"backup password 1".to_vec()),
        )
        .unwrap();
    assert_eq!(p.updated, 1);
    let report = b.import_commit(p.token).unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(
        b.reveal(&forum, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-2"
    );
    assert_eq!(
        b.show(&forum, false).unwrap().version,
        4,
        "nothing rewritten"
    );
}

#[test]
fn kdbx_and_csv_exports_say_what_they_leave() {
    let dir = tempfile::tempdir().unwrap();
    let a = fresh(dir.path(), "a");
    a.add_login(
        "https://forum.example.com",
        "ada",
        b"pw-1".to_vec(),
        Some("Forum"),
    )
    .unwrap();
    a.add_item(
        "wi-fi",
        Some("Home"),
        false,
        vec![
            FieldChange::text("wifi.ssid", "home-net"),
            FieldChange::secret("wifi.passphrase", b"k7Hq3mWx9rTa4nPe"),
        ],
    )
    .unwrap();
    let csv = a.export_preview("csv").unwrap();
    assert_eq!(csv.items, 1);
    assert!(
        csv.dropped.iter().any(|d| d.contains("aren't logins")),
        "{:?}",
        csv.dropped
    );
    let out = dir.path().join("vault.kdbx");
    assert!(
        a.export_to(out.clone(), "kdbx", None, true, yes()).is_err(),
        "a KeePass file needs a password"
    );
    a.export_to(
        out.clone(),
        "kdbx",
        Some(b"keepass-pw".to_vec()),
        false,
        yes(),
    )
    .unwrap();
    let b = fresh(dir.path(), "b");
    let p = b
        .import_preview(
            std::fs::read(&out).unwrap(),
            "auto",
            "vault.kdbx",
            Some(b"keepass-pw".to_vec()),
        )
        .unwrap();
    assert_eq!(p.total, 2);
}

#[test]
fn backups_are_sealed_kept_to_n_and_restore_on_this_device_without_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("backups");
    std::fs::create_dir_all(&folder).unwrap();
    let a = fresh(dir.path(), "a");
    a.add_login(
        "https://forum.example.com",
        "ada",
        b"pw-1".to_vec(),
        Some("Forum"),
    )
    .unwrap();
    a.add_item(
        "crypto-wallet",
        Some("Cold wallet"),
        false,
        vec![FieldChange::secret("seed", PHRASE.as_bytes())],
    )
    .unwrap();
    let s = a.backup_status().unwrap();
    assert!(!s.enabled && !s.key_set);

    let folder_s = folder.display().to_string();
    let err = a
        .backup_setup(folder_s.clone(), None, 24, 2, yes())
        .unwrap_err();
    assert!(err.message.contains("choose a backup password"), "{err}");
    let err = a
        .backup_setup(folder_s.clone(), Some(b"short".to_vec()), 24, 2, yes())
        .unwrap_err();
    assert!(err.message.contains("at least 8"), "{err}");
    let err = a
        .backup_setup(
            "relative/dir".into(),
            Some(b"backup-password".to_vec()),
            24,
            2,
            yes(),
        )
        .unwrap_err();
    assert!(err.message.contains("isn't a folder"), "{err}");
    let s = a
        .backup_setup(
            folder_s.clone(),
            Some(b"backup-password".to_vec()),
            24,
            2,
            yes(),
        )
        .unwrap();
    assert!(s.enabled && s.key_set);
    assert_eq!(s.backups.len(), 1, "one written at once");
    assert_eq!(s.crown_jewels_left_out, 1, "the wallet isn't opted in");
    assert!(s.last_success_at.is_some() && s.last_error.is_none());
    assert_eq!(
        s.next_due_at,
        s.last_success_at.map(|t| t + 24 * 3600),
        "the next one a day later"
    );
    a.backup_now().unwrap();
    let s = a.backup_now().unwrap();
    assert_eq!(s.backups.len(), 2, "kept to the newest two");
    // Something else in the folder is never touched.
    std::fs::write(folder.join("mine.txt"), b"keep me").unwrap();
    a.backup_now().unwrap();
    assert!(folder.join("mine.txt").exists());
    // No plaintext anywhere in the folder.
    for e in std::fs::read_dir(&folder).unwrap().flatten() {
        let b = std::fs::read(e.path()).unwrap();
        let t = String::from_utf8_lossy(&b);
        assert!(
            !t.contains("pw-1") && !t.contains("Forum"),
            "{}",
            e.path().display()
        );
        assert!(!e.path().to_string_lossy().ends_with(".partial"));
    }
    // The webview's settings call can't move backups.
    let mut settings = a.settings().unwrap();
    settings.backup.folder = Some("/tmp/elsewhere".into());
    settings.backup.enabled = false;
    let saved = a.set_settings(settings).unwrap();
    assert_eq!(saved.backup.folder.as_deref(), Some(folder_s.as_str()));
    assert!(saved.backup.enabled);

    // Restore on this device: the kept key opens it.
    let name = a.backup_status().unwrap().backups[0].name.clone();
    let p = a.backup_restore_preview(&name, None).unwrap();
    assert_eq!(p.total, 1);
    assert_eq!(p.unchanged, 1);
    assert!(a.backup_restore_preview("../../etc/passwd", None).is_err());
    // Elsewhere: the password alone.
    let b = fresh(dir.path(), "b");
    let bytes = std::fs::read(folder.join(&name)).unwrap();
    let p = b
        .import_preview(bytes, "auto", &name, Some(b"backup-password".to_vec()))
        .unwrap();
    b.import_commit(p.token).unwrap();
    assert_eq!(b.list().unwrap().len(), 1);

    // Off, and the key forgotten: the files stay.
    let s = a.backup_off(true).unwrap();
    assert!(!s.enabled && !s.key_set);
    assert!(!s.backups.is_empty());
    // Locked: no backup calls.
    a.lock().unwrap();
    assert_eq!(a.backup_now().unwrap_err().code, ErrorCode::Locked);
}
