//! Lockdown's real client against Lockdown's real server over hybrid
//! post-quantum TLS: two `lockdown` processes and a server on a random port
//! with a certificate from a private CA the clients pin through
//! `LOCKDOWN_CA_CERT`. Register (the kit is made) → add → sync → a second
//! device recovers with the kit and gets the item → it edits and syncs →
//! the first syncs and sees the edit. Every handshake the server took was
//! X25519MLKEM768; none fell back to a classical group. Without the pinned
//! CA, the same client refuses the server.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use lockdown_server::{AppState, Config, run_tls_in_background};
use lockdown_tls::testing::pki;
use lockdown_tls::{Groups, ReloadingCert};

struct Device {
    root: PathBuf,
    name: &'static str,
    ca: Option<PathBuf>,
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
        match &self.ca {
            Some(ca) => cmd.env("LOCKDOWN_CA_CERT", ca),
            None => cmd.env_remove("LOCKDOWN_CA_CERT"),
        };
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

fn device(dir: &Path, name: &'static str, ca: Option<PathBuf>) -> Device {
    Device {
        root: dir.join(name).join("snds").join("identity"),
        name,
        ca,
    }
}

const PASSWORD: &str = "correct horse battery staple\n";

#[test]
fn two_cli_devices_sync_through_the_server_over_hybrid_tls() {
    let dir = tempfile::tempdir().unwrap();
    let files = pki(&["localhost", "127.0.0.1"]).write(dir.path());
    let cert = ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(Groups::HybridOnly)),
    )
    .unwrap();
    // Hybrid only on the server: a classical handshake can't even happen.
    let tls = Arc::new(lockdown_tls::server_config(cert, Groups::HybridOnly).unwrap());
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    let server =
        run_tls_in_background(Arc::new(AppState::in_memory(config).unwrap()), tls).unwrap();
    let url = server.url();
    assert!(url.starts_with("https://"));

    let a = device(dir.path(), "laptop", Some(files.ca.clone()));
    let b = device(dir.path(), "phone", Some(files.ca.clone()));
    a.ok(&["init"], None);
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
    assert!(out.contains("pushed 1"), "{out}");

    // The second device comes back with the kit, over the same TLS.
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
    assert!(out.contains("pulled (1 live items)"), "{out}");
    let out = b.ok(&["show", &id, "--reveal"], None);
    assert!(out.contains("password: first secret"), "{out}");
    b.ok(&["edit", &id, "--password-stdin"], Some("phone secret\n"));
    let out = b.ok(&["sync"], None);
    assert!(out.contains("pushed 1"), "{out}");
    a.ok(&["sync"], None);
    let out = a.ok(&["show", &id, "--reveal"], None);
    assert!(out.contains("password: phone secret"), "{out}");
    let out = a.ok(&["audit"], None);
    assert!(!out.is_empty());

    let counts = server.tls_stats.as_ref().unwrap().counts();
    assert!(counts.hybrid >= 6, "{counts:?}");
    assert_eq!(counts.classical, 0, "{counts:?}");

    // The same client without the pinned CA refuses the server: the
    // certificate is from a CA the system doesn't know.
    let c = device(dir.path(), "stranger", None);
    c.ok(&["init"], None);
    let (ok, _, err) = c.run(
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "them@example.com",
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    assert!(!ok);
    assert!(
        err.to_lowercase().contains("certificate") || err.contains("couldn't reach"),
        "{err}"
    );
    // And plain http to another machine is refused before any request.
    let (ok, _, err) = c.run(
        &[
            "account",
            "register",
            "--server",
            "http://192.0.2.1:8787",
            "--login",
            "them@example.com",
            "--password-stdin",
        ],
        Some(PASSWORD),
    );
    assert!(!ok);
    assert!(err.contains("plain http"), "{err}");
}
