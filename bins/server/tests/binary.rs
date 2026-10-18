//! The `lockdown-server` binary as an operator runs it:
//!
//! - plain HTTP on an address other than this machine's is refused unless
//!   `--insecure-http` says a proxy does TLS;
//! - with `--tls-cert`/`--tls-key` it serves hybrid TLS, `healthcheck`
//!   passes, `--redirect-http` sends plain HTTP to HTTPS, and a renewed
//!   certificate is served after SIGHUP;
//! - `backup keygen | create | verify | restore` round-trips a live
//!   server's data, and a server started on the restored directory has the
//!   same accounts and the same key-transparency log key; a wrong Backup Key
//!   or a changed byte is refused.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_sync::{HttpTransport, Transport};
use lockdown_tls::Trust;
use lockdown_tls::testing::pki;
use lockdown_vault::Id;

const BIN: &str = env!("CARGO_BIN_EXE_lockdown-server");

struct Server {
    child: Child,
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the binary with `args` and waits for its "listening on" line.
fn start(args: &[&str], data: &Path) -> Server {
    let mut child = Command::new(BIN)
        .args(args)
        .arg("--data")
        .arg(data)
        .env_remove("LOCKDOWN_LISTEN")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let url = loop {
        let line = lines.next().expect("the server printed something").unwrap();
        if let Some(rest) = line.split("listening on ").nth(1) {
            break rest.split_whitespace().next().unwrap().to_string();
        }
    };
    // Keep draining stderr so the server never blocks on it.
    std::thread::spawn(move || for _ in lines {});
    Server { child, url }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn register(url: &str, trust: &Trust, login: &str) {
    let seed = SigningKey::generate().unwrap().inner().to_bytes();
    let binding = DeviceBinding {
        device_id: Id::new(),
        device_pub: SigningKey::from_bytes(seed).verifying_key().to_bytes(),
        sign: Box::new(move |m| Ok(SigningKey::from_bytes(seed).sign(m))),
    };
    let mut auth = OpaqueAccountAuth::new(HttpTransport::with_trust(url, trust).unwrap(), login, binding);
    auth.register_account(&Id::new(), "correct horse battery staple")
        .unwrap();
    auth.sign_in("correct horse battery staple").unwrap();
}

#[test]
fn plain_http_away_from_this_machine_needs_insecure_http() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .args(["--listen", "0.0.0.0:0", "--data"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--insecure-http"), "{err}");

    let s = start(&["--listen", "0.0.0.0:0", "--insecure-http"], dir.path());
    assert!(s.url.starts_with("http://0.0.0.0:"));
}

#[test]
fn tls_healthcheck_redirect_and_a_certificate_renewed_on_sighup() {
    let dir = tempfile::tempdir().unwrap();
    let certs = dir.path().join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    let first = pki(&["localhost", "127.0.0.1"]);
    let files = first.write(&certs);
    let redirect = free_port();
    let redirect_addr = format!("127.0.0.1:{redirect}");
    let s = start(
        &[
            "--listen",
            "127.0.0.1:0",
            "--tls-cert",
            files.cert.to_str().unwrap(),
            "--tls-key",
            files.key.to_str().unwrap(),
            "--redirect-http",
            &redirect_addr,
            "--tls-reload-secs",
            "0",
        ],
        &dir.path().join("data"),
    );
    assert!(s.url.starts_with("https://127.0.0.1:"), "{}", s.url);
    let trust = Trust::Only(vec![first.ca_der.clone()]);
    register(&s.url, &trust, "me@example.com");

    // The container health check.
    let listen = s.url.trim_start_matches("https://");
    let health = Command::new(BIN)
        .args(["healthcheck", "--listen", listen, "--tls-cert"])
        .arg(&files.cert)
        .output()
        .unwrap();
    assert!(health.status.success(), "{}", String::from_utf8_lossy(&health.stderr));

    // Plain HTTP on the redirect port goes to the same path over HTTPS.
    let plain = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = plain
        .get(format!("http://{redirect_addr}/v1/kt/sth?x=1"))
        .header("host", format!("localhost:{redirect}"))
        .send()
        .unwrap();
    assert_eq!(res.status().as_u16(), 308);
    let port = s.url.rsplit(':').next().unwrap();
    assert_eq!(
        res.headers().get("location").unwrap().to_str().unwrap(),
        format!("https://localhost:{port}/v1/kt/sth?x=1")
    );

    // Renew: new files in place, SIGHUP, and new clients see the new CA.
    let second = pki(&["localhost", "127.0.0.1"]);
    std::fs::write(&files.cert, &second.chain_pem).unwrap();
    std::fs::write(&files.key, &second.key_pem).unwrap();
    let pid = s.child.id().to_string();
    assert!(Command::new("kill").args(["-HUP", &pid]).status().unwrap().success());
    let trusts_second = Trust::Only(vec![second.ca_der.clone()]);
    let mut renewed = false;
    for _ in 0..100 {
        if HttpTransport::with_trust(&s.url, &trusts_second)
            .unwrap()
            .kt_sth()
            .is_ok()
        {
            renewed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(renewed, "the renewed certificate is served after SIGHUP");
    assert!(HttpTransport::with_trust(&s.url, &trust).unwrap().kt_sth().is_err());
}

fn backup_cmd(args: &[&str], key: &str) -> std::process::Output {
    let mut child = Command::new(BIN)
        .arg("backup")
        .args(args)
        .env_remove("LOCKDOWN_BACKUP_KEY")
        .env_remove("LOCKDOWN_DATA")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{key}\n").as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_backup_restores_the_same_server_and_refuses_a_wrong_key_or_a_changed_byte() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let blobs = dir.path().join("blobs");
    let s = start(
        &["--listen", "127.0.0.1:0", "--blob-dir", blobs.to_str().unwrap()],
        &data,
    );
    register(&s.url, &Trust::Platform, "me@example.com");
    register(&s.url, &Trust::Platform, "you@example.com");
    let before = HttpTransport::new(&s.url).unwrap().kt_sth().unwrap();
    // A blob file as an attachment upload would leave it.
    let vault_dir = blobs.join(Id::new().to_string());
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::write(vault_dir.join("ab".repeat(32)), vec![9u8; 3_000_000]).unwrap();

    let keygen = backup_cmd(&["keygen"], "");
    assert!(keygen.status.success());
    let key = String::from_utf8(keygen.stdout).unwrap().trim().to_string();
    assert!(key.starts_with("LDBK-"));

    // While the server runs.
    let file = dir.path().join("server.ldbk");
    let made = backup_cmd(
        &[
            "create",
            "--data",
            data.to_str().unwrap(),
            "--blob-dir",
            blobs.to_str().unwrap(),
            "--out",
            file.to_str().unwrap(),
        ],
        &key,
    );
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let printed = String::from_utf8_lossy(&made.stdout);
    assert!(printed.contains("3 files (1 attachments)"), "{printed}");
    drop(s);
    // Never over an existing file.
    let again = backup_cmd(
        &["create", "--data", data.to_str().unwrap(), "--out", file.to_str().unwrap()],
        &key,
    );
    assert!(!again.status.success());

    let verified = backup_cmd(&["verify", "--in", file.to_str().unwrap()], &key);
    assert!(verified.status.success(), "{}", String::from_utf8_lossy(&verified.stderr));
    assert!(String::from_utf8_lossy(&verified.stdout).contains("2 accounts"));

    let wrong = backup_cmd(
        &["verify", "--in", file.to_str().unwrap()],
        BackupKeyText::other().as_str(),
    );
    assert!(!wrong.status.success());
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("wrong Backup Key"));

    let mut bytes = std::fs::read(&file).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 1;
    let bent = dir.path().join("bent.ldbk");
    std::fs::write(&bent, &bytes).unwrap();
    assert!(!backup_cmd(&["verify", "--in", bent.to_str().unwrap()], &key).status.success());
    let cut = dir.path().join("cut.ldbk");
    std::fs::write(&cut, &std::fs::read(&file).unwrap()[..n - 100]).unwrap();
    assert!(!backup_cmd(&["verify", "--in", cut.to_str().unwrap()], &key).status.success());

    // Restore into an empty directory; a full one is refused.
    let refused = backup_cmd(
        &["restore", "--in", file.to_str().unwrap(), "--data", data.to_str().unwrap()],
        &key,
    );
    assert!(!refused.status.success());
    let fresh = dir.path().join("restored");
    let restored = backup_cmd(
        &["restore", "--in", file.to_str().unwrap(), "--data", fresh.to_str().unwrap()],
        &key,
    );
    assert!(restored.status.success(), "{}", String::from_utf8_lossy(&restored.stderr));
    let blob_back = fresh
        .join("blobs")
        .join(vault_dir.file_name().unwrap())
        .join("ab".repeat(32));
    assert_eq!(std::fs::read(blob_back).unwrap().len(), 3_000_000);

    let back = start(
        &["--listen", "127.0.0.1:0", "--blob-dir", fresh.join("blobs").to_str().unwrap()],
        &fresh,
    );
    let after = HttpTransport::new(&back.url).unwrap().kt_sth().unwrap();
    assert_eq!(after.log_key, before.log_key, "the same log key every client pinned");
    assert_eq!(after.sth.size, before.sth.size);
    // The OPAQUE setup came back too: the account still signs in.
    let seed = SigningKey::generate().unwrap().inner().to_bytes();
    let binding = DeviceBinding {
        device_id: Id::new(),
        device_pub: SigningKey::from_bytes(seed).verifying_key().to_bytes(),
        sign: Box::new(move |m| Ok(SigningKey::from_bytes(seed).sign(m))),
    };
    let mut auth =
        OpaqueAccountAuth::new(HttpTransport::new(&back.url).unwrap(), "me@example.com", binding);
    assert!(auth.sign_in("correct horse battery staple").is_ok());
}

/// Another valid key, for the wrong-key check.
struct BackupKeyText;

impl BackupKeyText {
    fn other() -> String {
        lockdown_server::backup::BackupKey::generate()
            .unwrap()
            .display()
            .to_string()
    }
}
