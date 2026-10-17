//! Two devices as two `lockdown` processes against a server on a random
//! port: register → sync → a second device enrols through the account →
//! sync → conflicting edits → sync → the conflict copy shows in `list`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use lockdown_server::{AppState, Config, run_in_background};

struct Device {
    root: std::path::PathBuf,
    name: &'static str,
}

impl Device {
    fn run(&self, args: &[&str], stdin: Option<&str>) -> (bool, String, String) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
        cmd.arg("--root")
            .arg(&self.root)
            .args(args)
            .env_remove("LOCKDOWN_PASSWORD")
            .env("HOSTNAME", self.name)
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

    fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let (ok, out, err) = self.run(args, stdin);
        assert!(ok, "lockdown {args:?} failed:\n{out}\n{err}");
        out
    }
}

fn device(dir: &Path, name: &'static str) -> Device {
    Device {
        root: dir.join(name).join("snds").join("identity"),
        name,
    }
}

const PASSWORD: &str = "correct horse battery staple\n";

#[test]
fn two_devices_register_sync_conflict_and_list_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let server =
        run_in_background(Arc::new(AppState::in_memory(Config::default()).unwrap())).unwrap();
    let url = server.url();
    let a = device(dir.path(), "laptop");
    let b = device(dir.path(), "phone");

    // A: identity, account, a login, sync.
    a.ok(&["init"], None);
    let (ok, _, err) = a.run(&["sync"], None);
    assert!(!ok);
    assert!(err.contains("account register"), "{err}");
    let out = a.ok(
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    assert!(out.contains("registered"), "{out}");
    let out = a.ok(
        &[
            "add",
            "login",
            "--site",
            "https://bank.example",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("first secret\n"),
    );
    let id = out.trim().strip_prefix("added ").unwrap().to_string();
    let out = a.ok(&["sync"], None);
    assert!(
        out.contains("pushed 1, pulled 0, merged 0; vault head seq 1"),
        "{out}"
    );
    let out = a.ok(&["status"], None);
    assert!(out.contains("account        me@example.com at"), "{out}");
    assert!(
        out.contains("synced         seq 1 (item revision 1)"),
        "{out}"
    );
    assert!(out.contains("vault head     seq 1 "), "{out}");

    // Registering again under the same name is refused.
    let (ok, _, err) = a.run(
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    assert!(!ok);
    assert!(err.contains("already an account"), "{err}");

    // B: nothing local. A wrong password is just "didn't go through".
    let (ok, _, err) = b.run(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some("wrong horse\n"),
    );
    assert!(!ok);
    assert!(err.contains("didn't go through"), "{err}");
    assert!(!b.root.exists(), "a failed login creates nothing");
    let out = b.ok(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    assert!(out.contains("now enrolled"), "{out}");
    let out = b.ok(&["sync"], None);
    assert!(
        out.contains("pushed 0, pulled 0, merged 0; vault head seq 1"),
        "{out}"
    );
    let out = b.ok(&["list"], None);
    assert!(out.contains(&id), "{out}");
    assert!(out.contains("bank.example"), "{out}");
    let out = b.ok(&["show", &id, "--reveal"], None);
    assert!(out.contains("password: first secret"), "{out}");
    let out = b.ok(&["status"], None);
    assert!(out.contains("device         phone"), "{out}");
    assert!(out.contains("devices        2 in the identity"), "{out}");

    // Both change the password. A syncs first.
    a.ok(&["edit", &id, "--password-stdin"], Some("laptop secret\n"));
    let out = a.ok(&["sync"], None);
    assert!(
        out.contains("pushed 1, pulled 0, merged 0; vault head seq 2"),
        "{out}"
    );
    b.ok(&["edit", &id, "--password-stdin"], Some("phone secret\n"));
    let out = b.ok(&["sync"], None);
    assert!(out.contains("merged 1"), "{out}");
    assert!(out.contains("conflict copy"), "{out}");
    assert!(out.contains("vault head seq 3"), "{out}");
    let out = b.ok(&["list"], None);
    let copies: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("(conflict copy from phone, "))
        .collect();
    assert_eq!(copies.len(), 1, "{out}");
    assert_eq!(out.lines().count(), 2, "{out}");
    let out = b.ok(&["show", &id, "--reveal"], None);
    assert!(
        out.contains("password: laptop secret"),
        "the server's value stays in the item: {out}"
    );
    let copy_id = copies[0].split_whitespace().next().unwrap();
    let out = b.ok(&["show", copy_id, "--reveal"], None);
    assert!(out.contains("password: phone secret"), "{out}");

    // A pulls the copy; its list shows both, and its head caught up.
    let out = a.ok(&["sync"], None);
    assert!(
        out.contains("pushed 0, pulled 1, merged 0; vault head seq 3"),
        "{out}"
    );
    let out = a.ok(&["list"], None);
    assert!(out.contains("(conflict copy from phone, "), "{out}");
    assert_eq!(out.lines().count(), 2, "{out}");
    let out = a.ok(&["status"], None);
    assert!(out.contains("vault head     seq 3 "), "{out}");
    assert!(
        out.contains("devices        1 in the identity, 2 learned"),
        "{out}"
    );

    // Nothing to do syncs as nothing to do, on both.
    for d in [&a, &b] {
        let out = d.ok(&["sync"], None);
        assert!(
            out.contains("pushed 0, pulled 0, merged 0; vault head seq 3"),
            "{out}"
        );
    }
}
