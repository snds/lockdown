//! `lockdown sharing` (docs/plan/55-member-key-change.md): the command line
//! lists who reset their account and waits on its person, as an owner, to
//! let them back in, and answers yes for one of them.
//!
//! The people are `lockdown_app::App`s against the real server in process;
//! the server publishes new keys for Bob under his account, as an
//! operator's reset would. Alice's command line is the real binary, run on
//! her own identity root.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use ecosystem_identity::{FileKeystore, Identity};
use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::storage::{IdentityAccept, RecordRow, SqliteStorage, Storage};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_vault::Id;
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};

const PASSWORD: &str = "correct horse battery staple";

fn server(dir: &Path) -> RunningServer {
    let storage = SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let config = Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::new(Box::new(storage), keys, config))).unwrap()
}

fn root_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(name).join("snds").join("identity")
}

fn person(dir: &Path, name: &str, server: &RunningServer) -> App {
    let paths = Paths::resolve(Some(root_of(dir, name)), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app.account_register(
        &server.url(),
        &format!("{name}@example.com"),
        PASSWORD,
        None,
    )
    .unwrap();
    app.sync().unwrap();
    app
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

fn cli(root: &Path, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
    use std::io::Write as _;
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    cmd.arg("--root").arg(root);
    let mut child = cmd
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the lockdown binary runs");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Publishes a fresh identity as `login`'s, continuing their chain and
/// logged like any manifest (the server's own hand).
fn reset(db: &Path, dir: &Path, login: &str) -> Identity {
    let keystore = FileKeystore::open_or_create(&dir.join("bob2-key.cbor")).unwrap();
    let mut id = Identity::create(&dir.join("bob2"), "lockdown", "bob2", &keystore).unwrap();
    lockdown_vault::pq::roll_out(&mut id, &keystore).unwrap();
    let conn = rusqlite::Connection::open(db).unwrap();
    let account: String = conn
        .query_row(
            "SELECT account_id FROM accounts WHERE login = ?1",
            [login],
            |r| r.get(0),
        )
        .unwrap();
    let prev: Vec<u8> = conn
        .query_row(
            "SELECT body FROM identity_heads WHERE account_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&account],
            |r| r.get(0),
        )
        .unwrap();
    let prev = IdentityManifest::from_bytes(&prev).unwrap();
    let mut set = RecordSet::from_records(
        id.profile(),
        id.personas().cloned(),
        id.devices().cloned(),
        id.envelopes().cloned(),
        id.consents().cloned(),
        Some(&prev),
        Vec::new(),
        id.device_id(),
    )
    .unwrap();
    set.sign(&lockdown_vault::identity::identity_signing_key(&id, &keystore).unwrap())
        .unwrap();
    let pq = lockdown_vault::pq::identity_pq_key(&id, &keystore).unwrap();
    lockdown_vault::pq::sign_manifest(&mut set.manifest, &pq).unwrap();
    let rows: Vec<RecordRow> = set
        .records
        .iter()
        .map(|(path, body)| RecordRow {
            path: path.clone(),
            hash: lockdown_crypto::hash::hash(body),
            body: body.clone(),
        })
        .collect();
    let body = set.manifest.to_bytes().unwrap();
    SqliteStorage::open(db)
        .unwrap()
        .accept_identity(
            Id::parse(&account).unwrap(),
            IdentityAccept {
                manifest: &set.manifest,
                body: &body,
                records: &rows,
                isk_pub: &id.profile().isk_pub,
                revoked: &[],
                revocations: &[],
                handed_over_to: Some(id.profile().profile_id),
            },
        )
        .unwrap()
        .unwrap();
    id
}

#[test]
fn the_cli_lists_who_waits_to_be_let_back_in_and_answers_yes() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server(dir.path());
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let vault = alice.create_household("Family").unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&vault, "bob@example.com", "edit", pw())
        .unwrap();
    bob.sync().unwrap();
    alice.lock().unwrap();
    let root = root_of(dir.path(), "alice");

    // Nobody waits yet.
    let (ok, out, err) = cli(&root, &["sharing"], None);
    assert!(ok, "{out}\n{err}");
    assert_eq!(out.trim(), "nobody is waiting on you to let them back in");

    // Bob's account is reset: the command line's sync asks.
    let bob2 = reset(
        &dir.path().join("server.sqlite"),
        dir.path(),
        "bob@example.com",
    );
    let (ok, out, err) = cli(&root, &["sharing"], None);
    assert!(ok, "{out}\n{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "bob@example.com reset their account. Let them back in?",
            "  to let them back in: lockdown sharing let-back-in bob@example.com",
        ]
    );
    // Someone who isn't waiting: refused in words.
    let (ok, _, err) = cli(
        &root,
        &["sharing", "let-back-in", "carol@example.com"],
        Some(PASSWORD),
    );
    assert!(!ok);
    assert!(err.contains("isn't waiting on you"), "{err}");
    // The wrong account password: the gate refuses it.
    let (ok, _, err) = cli(
        &root,
        &[
            "sharing",
            "let-back-in",
            "bob@example.com",
            "--password-stdin",
        ],
        Some("not it\n"),
    );
    assert!(!ok, "{err}");
    // Yes: done, and nobody waits any more.
    let (ok, out, err) = cli(
        &root,
        &[
            "sharing",
            "let-back-in",
            "bob@example.com",
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
    );
    assert!(ok, "{out}\n{err}");
    assert_eq!(
        out.trim(),
        "bob@example.com can open what you share with them again"
    );
    let (_, out, _) = cli(&root, &["sharing"], None);
    assert_eq!(out.trim(), "nobody is waiting on you to let them back in");
    // The app sees it the same way, and its pin moved to the new keys.
    alice.unlock().unwrap();
    assert!(alice.key_resets().unwrap().is_empty());
    let pinned = alice
        .sharing_overview()
        .unwrap()
        .contacts
        .into_iter()
        .find(|c| c.login == "bob@example.com")
        .unwrap();
    let fresh = lockdown_vault::contact::fingerprint_bytes(
        bob2.profile().profile_id,
        &bob2.profile().isk_pub,
        bob2.personas().next().unwrap().persona_id,
        &bob2.personas().next().unwrap().kem_pub,
    )
    .unwrap();
    assert_eq!(pinned.fingerprint, lockdown_vault::contact::digits(&fresh));
}
