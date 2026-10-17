//! Two devices as two `lockdown` processes against a server on a random
//! port: register (the recovery kit is made) → sync → a second device
//! recovers with the kit on an empty root and gets the vault → sync →
//! conflicting edits → sync → the conflict copy shows in `list` → the
//! first device revokes the second → the second can neither write nor
//! open, while its history before the revocation still stands.

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
    // This walkthrough logs in many times in a few seconds (register,
    // kit verify, recover attempts, re-register); the login brake (D57) is
    // loosened so the test isn't throttled.
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    let server = run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap();
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
    assert!(out.contains("LOCKDOWN RECOVERY KIT"), "{out}");
    let field = |label: &str| -> String {
        out.lines()
            .find(|l| l.starts_with(label))
            .unwrap_or_else(|| panic!("no {label} in {out}"))
            .split_whitespace()
            .last()
            .unwrap()
            .to_string()
    };
    let secret_key = field("Secret Key:");
    let recovery_code = field("Recovery code:");
    assert!(secret_key.starts_with("A1-"), "{secret_key}");
    assert_eq!(recovery_code.len(), 39, "{recovery_code}");
    let out = a.ok(&["status"], None);
    assert!(out.contains("recovery kit: Created"), "{out}");

    // The kit, typed back: a wrong code is refused, the right one opens,
    // and with the password the Secret Key is checked against the slot.
    let (ok, _, err) = a.run(
        &["kit", "verify", "--kit-stdin"],
        Some(&format!("{secret_key}\n{}0000\n", &recovery_code[..35])),
    );
    assert!(!ok);
    assert!(err.contains("recovery code didn't open"), "{err}");
    let out = a.ok(
        &[
            "kit",
            "verify",
            "--kit-stdin",
            "--password-stdin",
            "--drill",
        ],
        Some(&format!("{PASSWORD}{secret_key}\n{recovery_code}\n")),
    );
    assert!(out.contains("recovery code: opens this identity"), "{out}");
    assert!(out.contains("Secret Key: opens the account slot"), "{out}");
    assert!(out.contains("drill recorded"), "{out}");
    let out = a.ok(&["status"], None);
    assert!(out.contains("recovery kit: Drilled"), "{out}");
    let (ok, _, err) = a.run(&["kit", "create"], None);
    assert!(!ok);
    assert!(err.contains("already has a recovery kit"), "{err}");

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

    // Registering again under the same name is refused. The identity now
    // has a kit, so its Secret Key is required and then the server says
    // the name is taken.
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
    assert!(err.contains("give its Secret Key"), "{err}");
    let (ok, _, err) = a.run(
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
            "--secret-key-stdin",
        ],
        Some(&format!("{PASSWORD}{secret_key}\n")),
    );
    assert!(!ok);
    assert!(err.contains("already an account"), "{err}");

    // B: nothing local. A wrong password is just "didn't go through"; a
    // wrong recovery code is refused after the login, with nothing written
    // and nothing the server could learn from it.
    let (ok, _, err) = b.run(
        &[
            "recover",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
            "--kit-stdin",
        ],
        Some(&format!("wrong horse\n{secret_key}\n{recovery_code}\n")),
    );
    assert!(!ok);
    assert!(err.contains("didn't go through"), "{err}");
    assert!(!b.root.exists(), "a failed login creates nothing");
    let (ok, _, err) = b.run(
        &[
            "recover",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
            "--kit-stdin",
        ],
        Some(&format!(
            "{PASSWORD}{secret_key}\n{}0000\n",
            &recovery_code[..35]
        )),
    );
    assert!(!ok);
    assert!(err.contains("recovery kit didn't open"), "{err}");
    assert!(!b.root.exists(), "a wrong kit creates nothing");
    let out = b.ok(
        &[
            "recover",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
            "--kit-stdin",
        ],
        Some(&format!("{PASSWORD}{secret_key}\n{recovery_code}\n")),
    );
    assert!(
        out.contains("recovered: this device (Lockdown on phone)"),
        "{out}"
    );
    assert!(out.contains("pulled (1 live items)"), "{out}");
    let out = b.ok(&["list"], None);
    assert!(out.contains(&id), "the vault came with the recovery: {out}");
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
    assert!(out.contains("device         Lockdown on phone"), "{out}");
    assert!(
        out.contains("devices        2 in the identity (0 revoked); identity manifest seq 3"),
        "{out}"
    );

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
        .filter(|l| l.contains("(conflict copy from Lockdown on phone, "))
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
    assert!(
        out.contains("(conflict copy from Lockdown on phone, "),
        "{out}"
    );
    assert_eq!(out.lines().count(), 2, "{out}");
    let out = a.ok(&["status"], None);
    assert!(out.contains("vault head     seq 3 "), "{out}");
    assert!(
        out.contains("devices        2 in the identity (0 revoked); identity manifest seq 3"),
        "the phone's record reached the laptop through the synced set: {out}"
    );

    // Nothing to do syncs as nothing to do, on both.
    for d in [&a, &b] {
        let out = d.ok(&["sync"], None);
        assert!(
            out.contains("pushed 0, pulled 0, merged 0; vault head seq 3"),
            "{out}"
        );
    }

    // Key transparency from the shell: `sync` reported the log; `audit`
    // prints every entry of the account verified; a checkpoint exported on
    // the laptop verifies on the phone; `status` shows the view.
    let out = a.ok(&["status"], None);
    assert!(out.contains("transparency   log of "), "{out}");
    let out = a.ok(&["audit"], None);
    assert!(out.contains("tree head       "), "{out}");
    assert!(out.contains("device "), "{out}");
    assert!(out.contains("identity manifest "), "{out}");
    assert!(out.contains("vault "), "{out}");
    assert!(out.contains("entries of this account verified"), "{out}");
    let cp_file = dir.path().join("laptop-checkpoint.txt");
    let out = a.ok(&["checkpoint", "--out", cp_file.to_str().unwrap()], None);
    assert!(out.contains("checkpoint written to"), "{out}");
    let page = std::fs::read_to_string(&cp_file).unwrap();
    assert!(page.starts_with("LOCKDOWN CHECKPOINT"), "{page}");
    assert!(page.contains("Fingerprint:     "), "{page}");
    let (ok, _, err) = a.run(&["checkpoint", "--out", cp_file.to_str().unwrap()], None);
    assert!(!ok);
    assert!(err.contains("exists already"), "{err}");
    let out = b.ok(&["checkpoint", "--verify", cp_file.to_str().unwrap()], None);
    assert!(out.contains("one log, no fork"), "{out}");
    assert!(out.contains("exported by     Lockdown on laptop"), "{out}");
    assert!(
        out.contains("the checkpoint agrees with this device"),
        "{out}"
    );
    // A page with a bent fingerprint line still verifies by its bytes; a
    // page with bent bytes does not.
    let bent = page.replace("Checkpoint:      ", "Checkpoint:      00");
    let bent_file = dir.path().join("bent.txt");
    std::fs::write(&bent_file, bent).unwrap();
    let (ok, _, err) = b.run(
        &["checkpoint", "--verify", bent_file.to_str().unwrap()],
        None,
    );
    assert!(!ok);
    assert!(
        err.contains("didn't decode") || err.contains("aren't readable"),
        "{err}"
    );

    // A sees both devices and revokes the phone.
    let out = a.ok(&["device", "list"], None);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.contains("[this device]"), "{out}");
    let phone_line = out
        .lines()
        .find(|l| l.contains("Lockdown on phone"))
        .unwrap();
    let phone_id = phone_line.split_whitespace().next().unwrap().to_string();
    let (ok, _, err) = a.run(&["device", "revoke", "nobody"], None);
    assert!(!ok);
    assert!(err.contains("no device 'nobody'"), "{err}");
    let out = a.ok(&["device", "revoke", &phone_id], None);
    assert!(
        out.contains(&format!("device {phone_id} (Lockdown on phone) revoked")),
        "{out}"
    );
    assert!(out.contains("heads after seq 3 are refused"), "{out}");
    let out = a.ok(&["device", "list"], None);
    assert!(out.contains("[revoked 20"), "{out}");
    let out = a.ok(&["status"], None);
    assert!(
        out.contains("devices        2 in the identity (1 revoked); identity manifest seq 4"),
        "{out}"
    );

    // B edits and syncs: the server refuses, B learns it is out, and B's
    // local copy no longer opens. A's copy still holds everything B
    // signed before, verified.
    b.ok(&["edit", &id, "--password-stdin"], Some("phone again\n"));
    let (ok, _, err) = b.run(&["sync"], None);
    assert!(!ok);
    assert!(err.contains("revoked"), "{err}");
    let (ok, _, err) = b.run(&["list"], None);
    assert!(!ok);
    assert!(err.contains("revoked"), "{err}");
    let out = a.ok(&["sync"], None);
    assert!(
        out.contains("pushed 0, pulled 0, merged 0; vault head seq 3"),
        "{out}"
    );
    let out = a.ok(&["list"], None);
    assert!(
        out.contains("(conflict copy from Lockdown on phone, "),
        "{out}"
    );
    assert_eq!(out.lines().count(), 2, "{out}");
    let out = a.ok(&["show", &id, "--reveal"], None);
    assert!(out.contains("password: laptop secret"), "{out}");
}
