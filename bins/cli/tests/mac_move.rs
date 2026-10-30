//! The one-time move out of the Mac's Group Containers
//! (docs/plan/41-mac-paths.md §2.3), over a real install: a vault made by
//! the command line at an older build's places opens, with its items, at
//! the new places once moved. The move itself is `lockdown_paths`'s; this
//! runs on every platform because it only moves folders.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use lockdown_app::paths::{Dirs, Settled, old_mac_dirs, settle_from};

const MASTER: &str = "correct horse battery staple";

fn lockdown(root: &Path, args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lockdown"))
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .env_remove("LOCKDOWN_MASTER_PASSWORD")
        .env("HOSTNAME", "test-mac")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the lockdown binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_vault_made_in_group_containers_opens_in_application_support_after_the_move() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("Users/sam");
    let old = old_mac_dirs(&home);
    let support = home.join("Library/Application Support/snds");
    let new = Dirs {
        root: support.join("identity"),
        data: support.join("lockdown"),
    };

    // An older build: the identity in the app group, the data beside it.
    let (ok, _, err) = lockdown(
        &old.root,
        &["init", "--name", "Sam", "--password-stdin"],
        &format!("{MASTER}\n"),
    );
    assert!(ok, "{err}");
    assert!(old.data.join("device-key.sealed.cbor").exists());
    let (ok, _, err) = lockdown(
        &old.root,
        &[
            "--master-password-stdin",
            "add",
            "login",
            "--site",
            "https://moved.example",
            "--username",
            "sam",
            "--password-stdin",
        ],
        &format!("{MASTER}\nitem-password\n"),
    );
    assert!(ok, "{err}");

    match settle_from(&old, &new) {
        Settled::Moved {
            identity: true,
            data: true,
            old_data_left: None,
        } => {}
        other => panic!("{other:?}"),
    }
    assert!(!old.data.exists());

    // The same vault, the same master password, at the new places.
    let (ok, out, err) = lockdown(
        &new.root,
        &["--master-password-stdin", "list"],
        &format!("{MASTER}\n"),
    );
    assert!(ok, "{err}");
    assert!(out.contains("moved.example"), "{out}");

    // Once only.
    assert_eq!(settle_from(&old, &new), Settled::Checked);
}
