//! `lockdown security`: the Security screen's post-quantum line in the
//! terminal (docs/plan/30-pq-status-line.md), as a `lockdown` process
//! against a server on a random port. No account: can't check. A lone
//! device: on. A device from an older build in the records: waiting for it,
//! by name and id. That device at level 2: on. The server gone: can't
//! check, with why, and never on.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use ecosystem_identity::FileKeystore;
use lockdown_server::{AppState, Config, run_in_background};

fn lockdown(root: &Path, args: &[&str], stdin: Option<&str>) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lockdown"))
        .arg("--root")
        .arg(root)
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
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(
        out.status.success(),
        "lockdown {args:?} failed:\n{stdout}\n{stderr}"
    );
    stdout
}

/// Writes into the identity the way another holder of this device's key
/// would (here: an older device's record), between two commands.
fn on_disk<T>(
    root: &Path,
    edit: impl FnOnce(&mut ecosystem_identity::Identity, &FileKeystore) -> T,
) -> T {
    let data: PathBuf = root.parent().unwrap().join("lockdown");
    let keystore = FileKeystore::open_or_create(&data.join("device-key.cbor")).unwrap();
    let mut identity = lockdown_vault::identity::open_identity(root, &keystore, None)
        .unwrap()
        .unwrap();
    edit(&mut identity, &keystore)
}

#[test]
fn security_prints_the_same_line_as_the_screen() {
    let dir = tempfile::tempdir().unwrap();
    let server =
        run_in_background(Arc::new(AppState::in_memory(Config::default()).unwrap())).unwrap();
    let url = server.url();
    let root = dir.path().join("laptop/snds/identity");
    lockdown(&root, &["init"], None);

    let out = lockdown(&root, &["security"], None);
    assert!(
        out.starts_with("Quantum-safe signatures: can't check right now\n"),
        "{out}"
    );
    assert!(out.contains("isn't signed in to one"), "{out}");

    lockdown(
        &root,
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some("correct horse battery staple\n"),
    );
    let out = lockdown(&root, &["security"], None);
    assert!(out.starts_with("Quantum-safe signatures: on\n"), "{out}");

    let old = on_disk(&root, |identity, keystore| {
        lockdown_vault::test_support::add_device_at_level(identity, keystore, "Old laptop", None)
            .unwrap()
    });
    let out = lockdown(&root, &["security"], None);
    assert!(
        out.starts_with("Quantum-safe signatures: waiting for Old laptop\n"),
        "{out}"
    );
    assert!(
        out.contains("This device needs the latest Lockdown"),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "  {old}  Old laptop  (needs the latest Lockdown to read your account's quantum-safe key)"
        )),
        "{out}"
    );

    on_disk(&root, |identity, keystore| {
        lockdown_vault::test_support::set_device_level(identity, keystore, old, Some(2)).unwrap()
    });
    let out = lockdown(&root, &["security"], None);
    assert!(out.starts_with("Quantum-safe signatures: on\n"), "{out}");

    drop(server);
    let out = lockdown(&root, &["security"], None);
    assert!(
        out.starts_with("Quantum-safe signatures: can't check right now\n"),
        "{out}"
    );
    assert!(out.contains("The server couldn't be checked: "), "{out}");
}
