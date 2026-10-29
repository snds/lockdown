//! The master password on the command line (docs/plan/39-first-run.md
//! §1.5): `init --name` makes a local account the way the app's first run
//! does, every later command needs the master password (from the first
//! stdin line, the variable, or a prompt), an older install gets one with
//! `password set`, and `password reset` replaces a forgotten one with the
//! recovery kit.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const MASTER: &str = "correct horse battery staple";
const NEW_MASTER: &str = "orbit lantern pickle meadow";

fn lockdown(
    root: &Path,
    args: &[&str],
    stdin: Option<&str>,
    env: &[(&str, &str)],
) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    cmd.arg("--root")
        .arg(root)
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .env_remove("LOCKDOWN_MASTER_PASSWORD")
        .env_remove("LOCKDOWN_SECRET_KEY")
        .env_remove("LOCKDOWN_RECOVERY_CODE")
        .env("HOSTNAME", "test-laptop")
        .envs(env.iter().copied())
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

fn kit_of(out: &str) -> (String, String) {
    let value = |label: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap_or_else(|| panic!("no {label} in {out}"))
            .trim()
            .to_string()
    };
    (value("Secret Key:"), value("Recovery code:"))
}

fn add(root: &Path, site: &str, master: &str) -> (bool, String, String) {
    lockdown(
        root,
        &[
            "--master-password-stdin",
            "add",
            "login",
            "--site",
            site,
            "--username",
            "me",
            "--password-stdin",
        ],
        Some(&format!("{master}\nitem-password\n")),
        &[],
    )
}

#[test]
fn init_with_a_name_makes_a_vault_only_the_master_password_opens() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");

    // A weak password is refused before anything is made.
    let (ok, _, err) = lockdown(
        &root,
        &["init", "--name", "Sean", "--password-stdin"],
        Some("sean1234\n"),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("at least fair"), "{err}");

    let (ok, out, err) = lockdown(
        &root,
        &["init", "--name", "Sean", "--password-stdin"],
        Some(&format!("{MASTER}\n")),
        &[],
    );
    assert!(ok, "{err}");
    assert!(out.contains("vault created for Sean"), "{out}");
    let _ = kit_of(&out);
    assert!(!out.contains("Server:"), "{out}");

    // No master password: refused, in words.
    let (ok, _, err) = lockdown(&root, &["list"], None, &[]);
    assert!(!ok);
    assert!(err.contains("locked with a master password"), "{err}");
    // A wrong one.
    let (ok, _, err) = lockdown(
        &root,
        &["--master-password-stdin", "list"],
        Some("not the one\n"),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("That isn't the master password"), "{err}");

    // The first stdin line is the master password; the command's own
    // secret follows it.
    let (ok, out, err) = add(&root, "https://example.com", MASTER);
    assert!(ok, "{err}");
    assert!(out.starts_with("added "), "{out}");
    let (ok, out, err) = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(ok, "{err}");
    assert!(out.contains("example.com"), "{out}");
    // `LOCKDOWN_PASSWORD` keeps its old meaning and never unlocks.
    let (ok, _, _) = lockdown(&root, &["list"], None, &[("LOCKDOWN_PASSWORD", MASTER)]);
    assert!(!ok);
    // The vault syncs with the server inside the command.
    let (ok, out, err) = lockdown(
        &root,
        &["sync"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(ok, "{err}");
    assert!(out.contains("synced"), "{out}");
}

#[test]
fn an_older_install_gets_a_master_password_and_a_forgotten_one_is_reset() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let (ok, _, err) = lockdown(&root, &["init"], None, &[]);
    assert!(ok, "{err}");
    let (ok, out, err) = lockdown(&root, &["kit", "create"], None, &[]);
    assert!(ok, "{err}");
    let (sk, rc) = kit_of(&out);
    let (ok, _, err) = lockdown(
        &root,
        &[
            "add",
            "login",
            "--site",
            "https://a.example",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("pw\n"),
        &[],
    );
    assert!(ok, "{err}");
    // Each command says once that there is no master password yet.
    let (ok, before, err) = lockdown(&root, &["list"], None, &[]);
    assert!(ok, "{err}");
    assert!(
        err.contains(
            "This vault has no master password yet. Run lockdown password set --name NAME"
        ),
        "{err}"
    );

    let (ok, out, err) = lockdown(
        &root,
        &[
            "password",
            "set",
            "--name",
            "Sean",
            "--password-stdin",
            "--kit-stdin",
        ],
        Some(&format!("{MASTER}\n{sk}\n{rc}\n")),
        &[],
    );
    assert!(ok, "{err}");
    assert!(out.contains("master password now locks"), "{out}");
    let (ok, _, _) = lockdown(&root, &["list"], None, &[]);
    assert!(!ok);
    let (ok, after, err) = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(ok, "{err}");
    assert_eq!(after, before);

    // Forgotten: the kit and a new password.
    let (ok, out, err) = lockdown(
        &root,
        &["password", "reset", "--kit-stdin", "--password-stdin"],
        Some(&format!("{sk}\n{rc}\n{NEW_MASTER}\n")),
        &[],
    );
    assert!(ok, "{err}");
    assert!(out.contains("new master password is set"), "{out}");
    let (ok, _, _) = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(!ok);
    let (ok, again, err) = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", NEW_MASTER)],
    );
    assert!(ok, "{err}");
    assert_eq!(again, before);
}
