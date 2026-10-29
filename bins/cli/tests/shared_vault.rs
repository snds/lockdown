//! The command line on a device whose persona owns a shared vault others
//! wrote to (docs/plan/37-cli-popup.md §1; doc 36 §6 gap 3). The store
//! lists vaults by id, and ids are random, so the shared vault can come
//! first; the command line used to open that one with only this identity's
//! devices trusted and failed. It now opens the persona's vaults the way the
//! app does: the persona's own first vault by the app's order, a shared
//! vault trusting its writers as this device pinned them, and a vault whose
//! writers this device hasn't met waiting instead of failing.
//!
//! The people are `lockdown_app::App`s against the real server in process
//! (the command line has no sharing commands); the command line is the
//! real binary, run on Alice's own identity root and data directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

fn server() -> RunningServer {
    let config = Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap()
}

fn root_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(name).join("snds").join("identity")
}

fn person(dir: &Path, name: &str, server: &RunningServer) -> App {
    let paths = Paths::resolve(Some(root_of(dir, name)), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
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

/// Runs `lockdown` on an identity root (and a data dir when given); the
/// hostname is the test process's, so the device keeps the name the app
/// gave it.
fn cli(root: &Path, data: Option<&Path>, args: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    cmd.arg("--root").arg(root);
    if let Some(data) = data {
        cmd.arg("--data").arg(data);
    }
    let out = cmd
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .stdin(Stdio::null())
        .output()
        .expect("the lockdown binary runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(root: &Path, data: Option<&Path>, args: &[&str]) -> String {
    let (ok, out, err) = cli(root, data, args);
    assert!(ok, "lockdown {args:?} failed:\n{out}\n{err}");
    out
}

/// A copy of a data directory without the persona's sharing record: the
/// same device's store and keys, but none of the pins the app made.
fn copy_without_pins(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("sharing-") || !entry.file_type().unwrap().is_file() {
            continue;
        }
        std::fs::copy(entry.path(), to.join(&name)).unwrap();
    }
}

#[test]
fn the_cli_opens_the_personas_own_vault_when_a_shared_vault_others_wrote_to_sorts_first() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let bob = person(dir.path(), "bob", &srv);

    // Vault ids are random and the store lists them by id: new vaults are
    // made until one sorts in front of the person's first vault, and that
    // one is shared with Bob, who writes to it. A persona holds at most 20
    // vaults, so a person whose first vault sorts very low (1 time in 20)
    // is set aside and another one made.
    let (who, alice, personal, family, made) = (1..=10)
        .find_map(|round| {
            let who = format!("alice{round}");
            let alice = person(dir.path(), &who, &srv);
            let personal = alice.vaults().unwrap()[0].id.clone();
            for made in 1..20 {
                let Ok(id) = alice.create_vault(&format!("Family {made}")) else {
                    break;
                };
                if id < personal {
                    return Some((who, alice, personal, id, made));
                }
            }
            None
        })
        .expect("one of ten people gets a vault that sorts first");
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-one".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_vault(&family, "bob@example.com", "edit", pw())
        .unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();
    let bobs = bob
        .add_item_in(&family, "note", Some("Bob's note"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    alice.sync().unwrap();
    assert!(alice.list().unwrap().iter().any(|r| r.id == bobs));
    alice.lock().unwrap();
    drop(alice);

    // The command line on Alice's laptop: the persona's own first vault,
    // whatever the ids say, with the shared vault open beside it.
    let root = root_of(dir.path(), &who);
    let out = ok(&root, None, &["list"]);
    assert!(out.contains(&mail) && out.contains("Mail"), "{out}");
    assert!(!out.contains(&bobs), "{out}");
    let out = ok(&root, None, &["status"]);
    assert!(out.contains(&format!("vault          {personal}")), "{out}");
    assert!(
        out.contains(&format!(
            "other vaults   {made} more open; the command line works in the vault above"
        )),
        "{out}"
    );
    assert!(!out.contains("waiting  "), "{out}");
    ok(&root, None, &["init"]);
    let out = ok(&root, None, &["status"]);
    assert!(out.contains(&format!("vault          {personal}")), "{out}");

    // The same device before it met Bob (its sharing record left out):
    // the shared vault waits instead of failing the command, and says so.
    let data = dir.path().join("alice-unmet");
    copy_without_pins(&dir.path().join(&who).join("snds").join("lockdown"), &data);
    let out = ok(&root, Some(&data), &["list"]);
    assert!(out.contains(&mail), "{out}");
    let out = ok(&root, Some(&data), &["status"]);
    assert!(out.contains(&format!("vault          {personal}")), "{out}");
    assert!(
        out.contains(
            "waiting        1 shared vault has changes from people this device hasn't met yet. \
             It opens once the Lockdown app syncs and checks them against the server's log."
        ),
        "{out}"
    );
}
