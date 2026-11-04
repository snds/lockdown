//! Crown jewels in locked backups (D138 as decided by Sean, 2026-11-04;
//! docs/plan/48-crown-jewel-backups.md): a scheduled backup and a locked
//! Lockdown export carry every crown jewel, sealed under the backup's
//! password, while "Include crown jewels" is on (the default); a restore
//! brings them back as crown jewels, behind the password to reveal; with
//! the switch off, they're left out and counted as before; a plain export
//! still leaves them out (D121, D142, D144).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};

const PHRASE: &str =
    "abandon ability able about above absent absorb abstract absurd abuse access accident";
const CODES: &str = "8f3k-2m9q 7r4t-1w6z";
const BANK_PW: &[u8] = b"vault-door-71";
const BACKUP_PW: &[u8] = b"backup password 1";
const MASTER: &str = "correct horse battery staple";

fn fresh(dir: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app
}

/// A fresh install with a master password (a local account), so the
/// `Reveal` gate asks for that password.
fn with_master(dir: &Path, name: &str) -> App {
    lockdown_server::local::install();
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.first_run_create("Sean", MASTER).unwrap();
    app
}

fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

fn password(p: &str) -> GateInput {
    GateInput {
        password: Some(p.into()),
        confirmed: false,
    }
}

/// A login, a wallet (a crown jewel by its kind), recovery codes and a
/// bank login marked a crown jewel. None is opted in.
struct Vault {
    wallet: String,
    codes: String,
    bank: String,
}

fn fill(app: &App) -> Vault {
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
    let codes = app
        .add_item(
            "note",
            Some("Mail recovery codes"),
            true,
            vec![FieldChange::text("body", CODES)],
        )
        .unwrap();
    let bank = app
        .add_item(
            "login",
            Some("Bank"),
            true,
            vec![
                FieldChange::text("user", "ada@bank"),
                FieldChange::secret("pw", BANK_PW),
                FieldChange::text("url.0", "https://bank.example.com"),
            ],
        )
        .unwrap();
    for id in [&wallet, &codes, &bank] {
        let d = app.show(id, false).unwrap();
        assert!(d.crown_jewel && !d.export_opt_in, "{id}");
    }
    Vault {
        wallet,
        codes,
        bank,
    }
}

fn backup_files(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lockdown"))
        .collect();
    out.sort();
    out
}

fn no_plaintext(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for secret in ["abandon", "8f3k", "vault-door", "Cold wallet", "Bank"] {
        assert!(!text.contains(secret), "{secret} is readable in the file");
    }
}

/// Restores `bytes` into `app` with the backup password; the preview's
/// crown-jewel count, then the report.
fn restore(app: &App, bytes: Vec<u8>, name: &str) -> (usize, usize) {
    let p = app
        .import_preview(bytes, "auto", name, Some(BACKUP_PW.to_vec()), None)
        .unwrap();
    let crown = p.crown_jewels;
    let r = app.import_commit(p.token).unwrap();
    (crown, r.imported)
}

/// The restored crown jewels are crown jewels: the class, the opt-in still
/// off, the password to reveal (not a confirm), the bank login never handed
/// to a page, and a plain export still leaving them out.
fn restored_as_crown_jewels(b: &App, v: &Vault) {
    for id in [&v.wallet, &v.codes, &v.bank] {
        let d = b.show(id, false).unwrap();
        assert!(d.crown_jewel, "{id} came back a crown jewel");
        assert!(!d.export_opt_in, "{id} came back not opted in");
    }
    let row = b
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == v.wallet)
        .unwrap();
    assert!(row.crown_jewel);
    // Revealing needs the master password: nothing without it, nothing
    // with a wrong one, the phrase with the right one.
    let err = b.reveal(&v.wallet, "seed", yes()).unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded, "{err}");
    let err = b
        .reveal(&v.wallet, "seed", password("not my password at all"))
        .unwrap_err();
    assert!(err.message.contains("didn't match"), "{err}");
    assert_eq!(
        b.reveal(&v.wallet, "seed", password(MASTER))
            .unwrap()
            .expose(),
        PHRASE.as_bytes()
    );
    // The bank login: a page sees that it exists, never its user or address.
    let m = b.page_matches("https://bank.example.com/login").unwrap();
    let bank = m
        .logins
        .iter()
        .find(|l| l.item_id == v.bank)
        .expect("listed");
    assert!(bank.crown_jewel && bank.username.is_empty() && bank.saved.is_none());
    // The opt-in stayed off, so a plain export from here leaves all three out.
    let plain = b.export_preview("cxf", None).unwrap();
    assert_eq!(plain.crown_jewels_left_out, 3);
}

#[test]
fn a_scheduled_backup_carries_crown_jewels_and_restores_them_as_crown_jewels() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("backups");
    std::fs::create_dir_all(&folder).unwrap();
    let a = fresh(dir.path(), "a");
    let v = fill(&a);

    let s = a.backup_status().unwrap();
    assert!(s.include_crown_jewels, "on by default");
    assert_eq!((s.crown_jewels_left_out, s.crown_jewels_waiting), (0, 0));
    let s = a
        .backup_setup(
            folder.display().to_string(),
            Some(BACKUP_PW.to_vec()),
            24,
            5,
            yes(),
        )
        .unwrap();
    assert_eq!(s.backups.len(), 1);
    assert_eq!(
        (s.crown_jewels_left_out, s.crown_jewels_waiting),
        (0, 0),
        "nothing left out, nothing waiting"
    );

    // The worker's own tick writes the next one when it is due: forget when
    // the last ran, then wait for the file without calling the app (a call
    // would hold the tick off).
    let data = PathBuf::from(a.probe().unwrap().data);
    std::fs::remove_file(data.join("backup-state.cbor")).unwrap();
    let start = Instant::now();
    while backup_files(&folder).len() < 2 {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the tick didn't run"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let scheduled = backup_files(&folder).pop().unwrap();
    let bytes = std::fs::read(&scheduled).unwrap();
    assert!(bytes.starts_with(b"LDBACKUP"));
    no_plaintext(&bytes);

    // A wrong backup password reveals nothing, not even a count.
    let b = with_master(dir.path(), "b");
    let before = b.list().unwrap().len();
    let err = b
        .import_preview(
            bytes.clone(),
            "auto",
            "x.lockdown",
            Some(b"wrong password".to_vec()),
            None,
        )
        .unwrap_err();
    assert!(err.message.contains("didn't open"), "{err}");
    assert!(
        lockdown_cxf::archive::open(&bytes, b"wrong password", &lockdown_cxf::Limits::backup())
            .is_err()
    );
    assert_eq!(b.list().unwrap().len(), before);

    // The password alone opens it on a fresh vault: all four items, the
    // three crown jewels as crown jewels.
    let name = scheduled.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(restore(&b, bytes, &name), (3, 4));
    restored_as_crown_jewels(&b, &v);

    // On this device the kept key opens it, and it all lands unchanged.
    let p = a.backup_restore_preview(&name, None).unwrap();
    assert_eq!((p.total, p.crown_jewels, p.unchanged), (4, 3, 4));
}

#[test]
fn a_locked_export_carries_crown_jewels_and_a_plain_one_still_leaves_them_out() {
    let dir = tempfile::tempdir().unwrap();
    let a = fresh(dir.path(), "a");
    let v = fill(&a);

    // A locked Lockdown backup: every crown jewel, and it says so.
    let locked = a.export_preview_for("lockdown", None, true).unwrap();
    assert_eq!(
        (
            locked.items,
            locked.crown_jewels_included,
            locked.crown_jewels_left_out
        ),
        (4, 3, 0)
    );
    assert!(locked.crown_jewels_all && locked.left_out.is_empty());
    let file = dir.path().join("vault.lockdown");
    let done = a
        .export_to(
            file.clone(),
            "lockdown",
            Some(BACKUP_PW.to_vec()),
            false,
            None,
            yes(),
        )
        .unwrap();
    assert!(done.preview.encrypted && done.preview.crown_jewels_all);
    assert_eq!(done.preview.crown_jewels_included, 3);
    let bytes = std::fs::read(&file).unwrap();
    no_plaintext(&bytes);
    // Inside: every crown jewel, the document marked, nothing left out.
    let plain =
        lockdown_cxf::archive::open(&bytes, BACKUP_PW, &lockdown_cxf::Limits::backup()).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&plain).unwrap();
    assert_eq!(doc["crown_jewels"], "all");
    assert_eq!(doc["left_out"].as_array().unwrap().len(), 0);
    let crown: Vec<&serde_json::Value> = doc["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["class"] == "crown_jewel")
        .collect();
    assert_eq!(crown.len(), 3);
    assert!(
        crown.iter().all(|i| i["fields"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["id"] != "lockdown.export")),
        "no opt-in is written for an item the person didn't opt in"
    );

    let b = with_master(dir.path(), "b");
    assert_eq!(restore(&b, bytes, "vault.lockdown"), (3, 4));
    restored_as_crown_jewels(&b, &v);

    // Every plain file and every other format: D121 as before.
    for (format, sealed) in [
        ("lockdown", false),
        ("cxf", false),
        ("csv", false),
        ("kdbx", true),
    ] {
        let p = a.export_preview_for(format, None, sealed).unwrap();
        assert!(!p.crown_jewels_all, "{format}");
        let left: Vec<&str> = p.left_out.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(p.crown_jewels_left_out, 3, "{format}: {left:?}");
        assert_eq!(p.crown_jewels_included, 0, "{format}");
    }
    let out = dir.path().join("plain.json");
    for format in ["lockdown", "cxf", "csv"] {
        let done = a
            .export_to(out.clone(), format, None, true, None, yes())
            .unwrap();
        assert_eq!(done.preview.crown_jewels_left_out, 3, "{format}");
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("pw-1"), "{format}");
        for secret in ["abandon", "8f3k", "vault-door"] {
            assert!(!text.contains(secret), "{format} carried {secret}");
        }
        if format == "lockdown" {
            let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(
                doc.get("crown_jewels").is_none(),
                "a plain file isn't marked"
            );
            assert_eq!(doc["left_out"].as_array().unwrap().len(), 3);
        }
    }
    // A plain Lockdown file asked to carry them all still won't: the
    // narrowing is in the core, not in the caller.
    let p = a.export_preview_for("lockdown", None, false).unwrap();
    assert_eq!(p.crown_jewels_left_out, 3);
    // An opted-in crown jewel still leaves in a plain file (D121).
    a.set_export_opt_in(&v.wallet, true, yes()).unwrap();
    let p = a.export_preview("cxf", None).unwrap();
    assert_eq!((p.crown_jewels_included, p.crown_jewels_left_out), (1, 2));
}

#[test]
fn with_the_switch_off_crown_jewels_are_left_out_and_counted_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("backups");
    std::fs::create_dir_all(&folder).unwrap();
    let a = fresh(dir.path(), "a");
    let v = fill(&a);
    a.backup_setup(
        folder.display().to_string(),
        Some(BACKUP_PW.to_vec()),
        24,
        10,
        yes(),
    )
    .unwrap();

    // The switch goes through the gate both ways, and the webview's
    // settings call can't flip it.
    let err = a
        .backup_crown_jewels(false, GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    let mut settings = a.settings().unwrap();
    settings.backup.include_crown_jewels = false;
    assert!(
        a.set_settings(settings)
            .unwrap()
            .backup
            .include_crown_jewels
    );
    let s = a.backup_crown_jewels(false, yes()).unwrap();
    assert!(!s.include_crown_jewels);
    assert_eq!((s.crown_jewels_left_out, s.crown_jewels_waiting), (3, 0));
    assert!(!a.settings().unwrap().backup.include_crown_jewels);

    // A backup and a locked export now leave all three out, by title.
    let s = a.backup_now().unwrap();
    let name = s.backups[0].name.clone();
    let locked = a.export_preview_for("lockdown", None, true).unwrap();
    assert!(!locked.crown_jewels_all);
    assert_eq!(locked.crown_jewels_left_out, 3);
    let mut titles: Vec<&str> = locked.left_out.iter().map(|l| l.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["Bank", "Cold wallet", "Mail recovery codes"]);
    let b = fresh(dir.path(), "b");
    let bytes = std::fs::read(folder.join(&name)).unwrap();
    let p = b
        .import_preview(bytes, "auto", &name, Some(BACKUP_PW.to_vec()), None)
        .unwrap();
    assert_eq!((p.total, p.crown_jewels), (1, 0));
    assert!(
        p.problems
            .iter()
            .any(|x| x.contains("3 items were left out")),
        "{:?}",
        p.problems
    );

    // The per-item opt-in works as before: one in, two counted.
    a.set_export_opt_in(&v.wallet, true, yes()).unwrap();
    let s = a.backup_status().unwrap();
    assert_eq!(s.crown_jewels_left_out, 2);

    // On again: the screen says once that the next backup carries the two
    // the newest one lacks, until that backup runs.
    let s = a.backup_crown_jewels(true, yes()).unwrap();
    assert!(s.include_crown_jewels);
    assert_eq!((s.crown_jewels_left_out, s.crown_jewels_waiting), (0, 2));
    let s = a.backup_now().unwrap();
    assert_eq!(s.crown_jewels_waiting, 0);
    let p = a.backup_restore_preview(&s.backups[0].name, None).unwrap();
    assert_eq!((p.total, p.crown_jewels), (4, 3));

    // Locked: the switch can't be changed.
    a.lock().unwrap();
    assert_eq!(
        a.backup_crown_jewels(false, yes()).unwrap_err().code,
        ErrorCode::Locked
    );
}
