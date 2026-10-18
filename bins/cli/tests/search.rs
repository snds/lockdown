//! `lockdown search` over the shared quick-access index
//! (docs/plan/26-desktop-finish.md §1): ranked, a typo forgiven and
//! marked, and never a password.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn lockdown(root: &Path, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
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
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn search_finds_by_words_forgives_a_typo_and_never_a_password() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds").join("identity");
    assert!(lockdown(&root, &["init"], None).0);
    for (site, user, pw) in [
        ("https://github.com/login", "octo", "horse-battery-staple"),
        ("https://mail.example.com", "me@example.com", "mail-pass"),
    ] {
        let (ok, _, err) = lockdown(
            &root,
            &[
                "add",
                "login",
                "--site",
                site,
                "--username",
                user,
                "--password-stdin",
            ],
            Some(&format!("{pw}\n")),
        );
        assert!(ok, "{err}");
    }
    let (ok, out, err) = lockdown(&root, &["search", "octo", "git"], None);
    assert!(ok, "{err}");
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(
        out.contains("github.com") && !out.contains("close match"),
        "{out}"
    );
    let (_, out, _) = lockdown(&root, &["search", "gtihub"], None);
    assert!(
        out.contains("github.com") && out.contains("close match"),
        "{out}"
    );
    let (_, out, _) = lockdown(&root, &["search", "horse"], None);
    assert!(
        out.trim().is_empty(),
        "a password is never a search word: {out}"
    );
    let (_, out, _) = lockdown(&root, &["search", "--limit", "1"], None);
    assert_eq!(out.lines().count(), 1);
}
