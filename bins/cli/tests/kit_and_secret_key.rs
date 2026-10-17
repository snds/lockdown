//! The Secret Key in the account slot (D70, closing D60): an account slot
//! written the old way (password alone) is upgraded when the kit is made,
//! the old envelope leaves a tombstone, and from then on the server's
//! database plus the password no longer open the account on a new device;
//! the kit's Secret Key is needed too. Also: the kit as a file, and
//! registering when a kit already exists.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;

use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_crypto::identity::keystore::DeviceKeystore;
use lockdown_crypto::identity::records::KeyRefKind;
use lockdown_crypto::identity::{FileKeystore, Identity};
use lockdown_server::{AppState, Config, run_in_background};
use lockdown_sync::HttpTransport;

struct Device {
    root: std::path::PathBuf,
    name: &'static str,
}

impl Device {
    fn data(&self) -> std::path::PathBuf {
        self.root.parent().unwrap().join("lockdown")
    }

    fn run(
        &self,
        args: &[&str],
        stdin: Option<&str>,
        env: &[(&str, &str)],
    ) -> (bool, String, String) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
        cmd.arg("--root")
            .arg(&self.root)
            .args(args)
            .env_remove("LOCKDOWN_PASSWORD")
            .env_remove("LOCKDOWN_SECRET_KEY")
            .env_remove("LOCKDOWN_RECOVERY_CODE")
            .env("HOSTNAME", self.name)
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

    fn ok(&self, args: &[&str], stdin: Option<&str>, env: &[(&str, &str)]) -> String {
        let (ok, out, err) = self.run(args, stdin, env);
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

const PASSWORD: &str = "correct horse battery staple";

fn kit_field(out: &str, label: &str) -> String {
    out.lines()
        .find(|l| l.starts_with(label))
        .unwrap_or_else(|| panic!("no {label} in {out}"))
        .split_whitespace()
        .last()
        .unwrap()
        .to_string()
}

/// An account made the way D60 did: the slot wrapped under the export key
/// alone, written through the library the way the earlier CLI did.
fn old_style_account(laptop: &Device, url: &str) {
    std::fs::create_dir_all(laptop.data()).unwrap();
    let keystore =
        Rc::new(FileKeystore::open_or_create(&laptop.data().join("device-key.cbor")).unwrap());
    let mut identity =
        Identity::create(&laptop.root, "lockdown", "laptop", keystore.as_ref()).unwrap();
    identity.adopt("lockdown").unwrap();
    let signing = keystore.clone();
    let binding = DeviceBinding {
        device_id: keystore.device_id(),
        device_pub: keystore.device_pub(),
        sign: Box::new(move |m| {
            DeviceKeystore::sign(signing.as_ref(), m).map_err(lockdown_crypto::Error::from)
        }),
    };
    let mut auth =
        OpaqueAccountAuth::new(HttpTransport::new(url).unwrap(), "me@example.com", binding);
    identity
        .enable_account_unlock(&mut auth, PASSWORD, None)
        .unwrap();
}

#[test]
fn an_old_slot_is_upgraded_when_the_kit_is_made_and_the_password_alone_no_longer_enrols() {
    let dir = tempfile::tempdir().unwrap();
    // Many logins under one name in a few seconds: the brake (D57) is
    // loosened for this test only.
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    let server = run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap();
    let url = server.url();
    let a = device(dir.path(), "laptop");
    let b = device(dir.path(), "phone");
    let c = device(dir.path(), "tablet");
    old_style_account(&a, &url);

    // The CLI meets the old account: it logs in, publishes the records,
    // and says what is still open.
    a.ok(&["init"], None, &[]);
    let out = a.ok(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(out.contains("password alone"), "{out}");
    assert!(out.contains("kit create"), "{out}");
    a.ok(
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
        &[],
    );
    a.ok(&["sync"], None, &[]);

    // Meanwhile a new device enrols with the password alone: the old slot
    // lets it (D60). It stays enrolled.
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
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(out.contains("now enrolled"), "{out}");
    b.ok(&["sync"], None, &[]);

    // The kit needs the account password; without it nothing changes.
    let (ok, _, err) = a.run(&["kit", "create"], None, &[]);
    assert!(!ok);
    assert!(err.contains("account password"), "{err}");
    let (ok, _, err) = a.run(
        &["kit", "create", "--password-stdin"],
        Some("wrong horse\n"),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("didn't go through"), "{err}");
    let out = a.ok(&["status"], None, &[]);
    assert!(out.contains("recovery kit: None"), "{out}");

    // Made, saved to a file only the person can read, never printed.
    let kit_path = dir.path().join("kit.txt");
    let out = a.ok(
        &[
            "kit",
            "create",
            "--password-stdin",
            "--save",
            kit_path.to_str().unwrap(),
        ],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(out.contains("recovery kit saved to"), "{out}");
    assert!(
        !out.contains("A1-"),
        "the kit is not printed when saved: {out}"
    );
    let kit = std::fs::read_to_string(&kit_path).unwrap();
    assert!(kit.contains("LOCKDOWN RECOVERY KIT"), "{kit}");
    let secret_key = kit_field(&kit, "Secret Key:");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&kit_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let (ok, _, err) = a.run(
        &[
            "kit",
            "create",
            "--replace",
            "--password-stdin",
            "--save",
            kit_path.to_str().unwrap(),
        ],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("exists already"), "{err}");
    let out = a.ok(&["status"], None, &[]);
    assert!(out.contains("recovery kit: Created"), "{out}");

    // The old slot was retired through the crate's tombstone path: the
    // identity store's own manifest records the removal, and the account
    // envelope there now is a new one.
    let loaded = lockdown_crypto::identity::Store::at(&a.root)
        .load()
        .unwrap();
    let profile_id = loaded.contents.profile().unwrap().profile_id;
    let account_path = format!(
        "envelopes/{}.account.{profile_id}.cbor",
        loaded
            .contents
            .envelopes
            .values()
            .find(|e| e.wrapped_by.kind == KeyRefKind::Account)
            .unwrap()
            .key_id
    );
    assert!(
        loaded
            .manifest
            .tombstones
            .iter()
            .any(|t| t.path == account_path),
        "no tombstone for {account_path} in {:?}",
        loaded.manifest.tombstones
    );

    // Now the server's database and the password are not enough: a third
    // device is told it needs the Secret Key, and with it enrols.
    let (ok, _, err) = c.run(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("needs the Secret Key"), "{err}");
    assert!(!c.root.exists());
    let (ok, _, err) = c.run(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
        &[("LOCKDOWN_SECRET_KEY", "A1-00000-00000-00000-00000-000000")],
    );
    assert!(!ok);
    assert!(err.contains("check the Secret Key"), "{err}");
    let out = c.ok(
        &[
            "account",
            "login",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
        &[("LOCKDOWN_SECRET_KEY", &secret_key)],
    );
    assert!(out.contains("now enrolled"), "{out}");
    let out = c.ok(&["sync"], None, &[]);
    assert!(out.contains("vault head seq 1"), "{out}");
    let out = c.ok(&["list"], None, &[]);
    assert!(out.contains("bank.example"), "{out}");

    // The device that enrolled before the kit keeps working and learns
    // the new slot on its next login; the kit's Secret Key opens it there
    // too. Typing the Secret Key again is only needed where a slot is
    // still the old one, which none is now.
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
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(!out.contains("password alone"), "{out}");
    a.ok(&["sync"], None, &[]);
    let out = a.ok(&["device", "list"], None, &[]);
    assert_eq!(out.lines().count(), 3, "laptop, phone and tablet: {out}");
}

#[test]
fn registering_with_a_kit_already_made_needs_its_secret_key() {
    let dir = tempfile::tempdir().unwrap();
    let server =
        run_in_background(Arc::new(AppState::in_memory(Config::default()).unwrap())).unwrap();
    let url = server.url();
    let a = device(dir.path(), "laptop");
    a.ok(&["init"], None, &[]);
    let out = a.ok(&["kit", "create"], None, &[]);
    assert!(out.contains("no account on this device yet"), "{out}");
    let secret_key = kit_field(&out, "Secret Key:");
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
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("give its Secret Key"), "{err}");
    let out = a.ok(
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
        Some(&format!("{PASSWORD}\n{secret_key}\n")),
        &[],
    );
    assert!(out.contains("registered"), "{out}");
    assert!(
        !out.contains("LOCKDOWN RECOVERY KIT"),
        "no second kit: {out}"
    );
    // And that slot wants the Secret Key on a new device.
    let b = device(dir.path(), "phone");
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
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(!ok);
    assert!(err.contains("needs the Secret Key"), "{err}");
}
