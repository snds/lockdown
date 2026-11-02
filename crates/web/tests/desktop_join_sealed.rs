//! A desktop joining an account that asks for a security key, sealed
//! through the wait (docs/plan/45-desktop-join-sealed.md), natively against
//! the real server. The key is added and then used on the server's own
//! pages, which the web vault serves, as the desktop opens them in the
//! system browser.
//!
//! - `account_login_sealed` waits for the key with the new device's keys
//!   only in memory: nothing is written while it waits, and an abandoned
//!   wait leaves the computer empty;
//! - once the key answers, `second_factor_wait` seals the keys under the
//!   password before the identity names them; the computer holds only the
//!   sealed key file and `open_joined` opens it with the password.

use std::sync::Arc;

use lockdown_app::local_unlock::{self, KeyFile};
use lockdown_app::{App, ErrorCode, LockPolicy, Paths};
use lockdown_passkey::{
    Ceremony, ClientData, Flags, MakeCredential, get_assertion, make_credential,
};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_web::{Reply, WebPaths, WebVault, call};
use serde_json::{Value, json};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

fn server() -> RunningServer {
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap()
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn bytes_of(v: &Value) -> Vec<u8> {
    serde_json::from_value(v.clone()).unwrap()
}

fn pending_of(url: &str) -> String {
    url.rsplit('/').next().unwrap().to_string()
}

fn computer(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn data_of(app: &App) -> std::path::PathBuf {
    std::path::PathBuf::from(app.probe().unwrap().data)
}

/// Every file under a computer's identity root and data folder.
fn everything(dir: &std::path::Path, name: &str) -> Vec<String> {
    fn walk(at: &std::path::Path, out: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(at) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push(p.display().to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&dir.join(name), &mut out);
    out.sort();
    out
}

#[test]
fn a_desktop_join_that_waits_for_a_security_key_is_sealed_when_it_answers() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let origin = url.trim_end_matches('/').to_string();

    // The laptop that made the account, with an item and a security key.
    let laptop = computer(dir.path(), "laptop");
    laptop.open_or_create_plain().unwrap();
    let registered = laptop
        .account_register(&url, LOGIN, PASSWORD, None)
        .unwrap();
    let kit = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let sk = kit
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    let item = laptop
        .add_login("https://mail.example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    laptop.sync().unwrap();
    let add = laptop.security_key_add_page().unwrap();
    let mut browser = WebVault::new(WebPaths {
        root: dir.path().join("browser/identity"),
        data: dir.path().join("browser/data"),
    })
    .serving(&url);
    let create = ok(
        &mut browser,
        "page_key_request",
        json!({ "pendingId": pending_of(&add) }),
    );
    let made = make_credential(&MakeCredential {
        rp_id: create["rp_id"].as_str().unwrap(),
        client_data: &ClientData {
            ceremony: Ceremony::Create,
            challenge: bytes_of(&create["challenge"]),
            origin: origin.clone(),
            top_origin: None,
        },
        flags: Flags {
            up: true,
            uv: true,
            be: false,
            bs: false,
        },
        algs: &[lockdown_passkey::COSE_ES256],
    })
    .unwrap();
    ok(
        &mut browser,
        "page_key_finish",
        json!({
            "pendingId": pending_of(&add),
            "answer": {
                "name": "Blue key",
                "clientDataJson": made.client_data_json.as_bytes(),
                "attestationObject": made.attestation_object,
            },
        }),
    );

    // A computer whose wait is abandoned: nothing was ever written there.
    let gone = computer(dir.path(), "gone");
    let waiting = gone
        .account_login_sealed(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap_err();
    assert_eq!(waiting.code, ErrorCode::ApprovalNeeded);
    assert!(local_unlock::key_file(&data_of(&gone)) == KeyFile::None);
    drop(gone);
    assert!(
        everything(dir.path(), "gone").is_empty(),
        "{:?}",
        everything(dir.path(), "gone")
    );

    // The phone: the password passes, then it waits for the key, with
    // nothing written meanwhile.
    let phone = computer(dir.path(), "phone");
    let waiting = phone
        .account_login_sealed(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap_err();
    assert_eq!(waiting.code, ErrorCode::ApprovalNeeded);
    assert_eq!(waiting.detail.as_deref(), Some("security_key"));
    assert_eq!(phone.second_factor_wait().unwrap(), None);
    assert!(
        everything(dir.path(), "phone").is_empty(),
        "{:?}",
        everything(dir.path(), "phone")
    );

    // The key is used on the page the desktop opens.
    let page = phone.second_factor_page().unwrap();
    assert!(page.contains("/vault/#/second-factor/"), "{page}");
    let get = ok(
        &mut browser,
        "page_sign_in_request",
        json!({ "pendingId": pending_of(&page) }),
    );
    let signed = get_assertion(
        &made.pkcs8,
        get["rp_id"].as_str().unwrap(),
        &ClientData {
            ceremony: Ceremony::Get,
            challenge: bytes_of(&get["challenge"]),
            origin: origin.clone(),
            top_origin: None,
        },
        Flags {
            up: true,
            ..Flags::default()
        },
    )
    .unwrap();
    ok(
        &mut browser,
        "page_sign_in_answer",
        json!({
            "pendingId": pending_of(&page),
            "answer": {
                "credentialId": made.credential_id,
                "clientDataJson": signed.client_data_json.as_bytes(),
                "authenticatorData": signed.authenticator_data,
                "signature": signed.signature,
            },
        }),
    );
    let lines = phone
        .second_factor_wait()
        .unwrap()
        .expect("the key answered");
    assert!(!lines.is_empty());

    // Sealed in that one step: only the sealed key file, the identity, the
    // login on the lock screen.
    let data = data_of(&phone);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    let probe = phone.probe().unwrap();
    assert!(probe.identity_present && probe.master_password);
    assert_eq!(probe.name.as_deref(), Some(LOGIN));
    assert!(
        phone.unlock().is_err(),
        "the device keystore alone opens nothing"
    );
    assert!(phone.open_joined("not the password at all").is_err());
    let status = phone.open_joined(PASSWORD).unwrap();
    assert!(status.account.is_some());
    assert!(phone.list().unwrap().iter().any(|i| i.id == item));
    phone.sync().unwrap();
    laptop.sync().unwrap();
    assert_eq!(laptop.device_list().unwrap().len(), 2);
}
