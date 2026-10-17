//! Drives the `lockdown` binary end to end on a temp root: init → add →
//! list → show → status, each a separate process, so the second and every
//! later invocation opens the vault through the identity root and the
//! device keystore alone, with no re-init, no kit and no master password.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn lockdown(root: &Path, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    cmd.arg("--root")
        .arg(root)
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .env("HOSTNAME", "test-laptop")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("the lockdown binary runs");
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

#[test]
fn init_add_list_show_status_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds").join("identity");
    let data = dir.path().join("snds").join("lockdown");

    // Before init: nothing is created by a read.
    let (ok, _, err) = lockdown(&root, &["list"], None);
    assert!(!ok);
    assert!(err.contains("lockdown init"), "{err}");
    assert!(!root.exists());
    assert!(!data.exists());

    // init creates the identity and the vault.
    let (ok, out, err) = lockdown(&root, &["init"], None);
    assert!(ok, "{err}");
    assert!(out.starts_with("identity created, vault created"), "{out}");
    assert!(out.contains("managed by lockdown"), "{out}");
    assert!(
        out.contains("device         Lockdown on test-laptop"),
        "{out}"
    );
    assert!(out.contains("vault head     seq 1 "), "{out}");
    assert!(root.join("FORMAT").exists());
    assert!(data.join("vault.sqlite").exists());
    assert!(data.join("device-key.cbor").exists());
    assert!(data.join("remembered.cbor").exists());

    // add: the password comes from stdin, never an argument.
    let (ok, out, err) = lockdown(
        &root,
        &[
            "add",
            "login",
            "--site",
            "https://accounts.example.com/login",
            "--username",
            "me@example.com",
            "--password-stdin",
        ],
        Some("correct horse battery staple\n"),
    );
    assert!(ok, "{err}");
    let id = out
        .trim()
        .strip_prefix("added ")
        .expect("an id")
        .to_string();
    assert_eq!(id.len(), 36);

    // list: a new process opens the vault without init.
    let (ok, out, err) = lockdown(&root, &["list"], None);
    assert!(ok, "{err}");
    assert!(out.contains(&id), "{out}");
    assert!(out.contains("login"), "{out}");
    assert!(out.contains("accounts.example.com"), "{out}");
    assert!(!out.contains("correct horse"), "{out}");

    // show: masked by default, revealed on request.
    let (ok, out, err) = lockdown(&root, &["show", &id], None);
    assert!(ok, "{err}");
    assert!(out.contains("username: me@example.com"), "{out}");
    assert!(
        out.contains("website:  https://accounts.example.com/login (exact_host)"),
        "{out}"
    );
    assert!(out.contains("password: •"), "{out}");
    assert!(!out.contains("correct horse"), "{out}");
    let (ok, out, _) = lockdown(&root, &["show", &id, "--reveal"], None);
    assert!(ok);
    assert!(
        out.contains("password: correct horse battery staple"),
        "{out}"
    );

    // A second add from the environment, then status shows head and count.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    let out = cmd
        .arg("--root")
        .arg(&root)
        .args([
            "add",
            "login",
            "--site",
            "https://mail.example.org",
            "--username",
            "me",
            "--title",
            "Mail",
        ])
        .env("LOCKDOWN_PASSWORD", "hunter2")
        .env("HOSTNAME", "test-laptop")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (ok, out, err) = lockdown(&root, &["status"], None);
    assert!(ok, "{err}");
    assert!(out.contains("vault head     seq 3 hash "), "{out}");
    assert!(out.contains("items          2 live"), "{out}");
    let (_, out, _) = lockdown(&root, &["list"], None);
    assert!(out.contains("Mail  https://mail.example.org"), "{out}");

    // init again: attaches and opens, creates nothing new.
    let (ok, out, err) = lockdown(&root, &["init"], None);
    assert!(ok, "{err}");
    assert!(out.starts_with("identity attached, vault opened"), "{out}");
    assert!(out.contains("vault head     seq 3 "), "{out}");

    // No password source: refused before anything is opened.
    let (ok, _, err) = lockdown(
        &root,
        &[
            "add",
            "login",
            "--site",
            "https://x.example",
            "--username",
            "u",
        ],
        None,
    );
    assert!(!ok);
    assert!(err.contains("LOCKDOWN_PASSWORD"), "{err}");

    // A wrong persona selector is an error with the labels in it.
    let (ok, _, err) = lockdown(&root, &["--persona", "Work", "list"], None);
    assert!(!ok);
    assert!(err.contains("persona"), "{err}");
}
