//! The web vault's part of security depth, natively against the real
//! server (docs/plan/22-security-depth.md §2–§5): a security key added on
//! the page the desktop opens, then used in a tab's own sign-in; a sign-in
//! under the stronger stretching a desktop raised the account to; a tab
//! that joins from the desktop with the QR code it reads; and the
//! reminders and key checks over the same command table.

use std::sync::Arc;

use lockdown_app::GateInput;
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

fn page(dir: &std::path::Path, name: &str, url: &str) -> WebVault {
    WebVault::new(WebPaths {
        root: dir.join(name).join("identity"),
        data: dir.join(name).join("data"),
    })
    .serving(url)
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

fn err(web: &mut WebVault, cmd: &str, args: Value) -> lockdown_app::AppError {
    match call(web, cmd, args) {
        Ok(r) => panic!("{cmd}: expected a refusal, got {r:?}"),
        Err(e) => e,
    }
}

/// The desktop that made the account; returns it and the Secret Key.
fn desktop(dir: &std::path::Path, url: &str) -> (lockdown_app::App, String) {
    let paths = lockdown_app::Paths::resolve(Some(dir.join("desktop/identity")), None).unwrap();
    let app = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    app.open_or_create(true).unwrap();
    let registered = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
    app.sync().unwrap();
    let kit = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let sk = kit
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    (app, sk)
}

fn bytes_of(v: &Value) -> Vec<u8> {
    serde_json::from_value(v.clone()).unwrap()
}

fn pending_of(url: &str) -> String {
    url.rsplit('/').next().unwrap().to_string()
}

#[test]
fn a_key_added_on_the_page_guards_a_tabs_sign_in_under_raised_stretching() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let origin = url.trim_end_matches('/').to_string();
    let (desk, sk) = desktop(dir.path(), &url);
    // A sign-in on the desktop raises the account's stretching first.
    let lines = desk.account_login(&url, LOGIN, PASSWORD, None).unwrap();
    assert!(
        lines.iter().any(|l| l.contains("stretching rose")),
        "{lines:?}"
    );

    // The desktop opens the server's page in the system browser; the page
    // (no account in it) registers the key with the browser's WebAuthn.
    let add = desk.security_key_add_page().unwrap();
    assert!(add.contains("/vault/#/security-key/"), "{add}");
    let mut browser = page(dir.path(), "browser", &url);
    let create = ok(
        &mut browser,
        "page_key_request",
        json!({ "pendingId": pending_of(&add) }),
    );
    assert_eq!(create["rp_id"], "127.0.0.1");
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
    let info = ok(
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
    assert_eq!(info["name"], "Blue key");
    assert_eq!(desk.security_keys().unwrap().keys.len(), 1);

    // A tab signs in: the password passes (under the raised stretching),
    // then it waits for the key.
    let mut tab = page(dir.path(), "tab", &url);
    let waiting = err(
        &mut tab,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": PASSWORD, "secretKey": sk }),
    );
    assert_eq!(
        waiting.detail.as_deref(),
        Some("security_key"),
        "{}",
        waiting.message
    );
    assert_eq!(ok(&mut tab, "web_sign_in_continue", json!({})), Value::Null);
    let get = ok(&mut tab, "web_sign_in_waiting", json!({}));
    let assertion = |challenge: Vec<u8>| {
        get_assertion(
            &made.pkcs8,
            get["rp_id"].as_str().unwrap(),
            &ClientData {
                ceremony: Ceremony::Get,
                challenge,
                origin: origin.clone(),
                top_origin: None,
            },
            Flags {
                up: true,
                ..Flags::default()
            },
        )
        .unwrap()
    };
    // A wrong answer is refused and the sign-in keeps waiting.
    let wrong = assertion(b"another challenge".to_vec());
    let refused = err(
        &mut tab,
        "web_sign_in_answer",
        json!({ "answer": {
            "credentialId": made.credential_id,
            "clientDataJson": wrong.client_data_json.as_bytes(),
            "authenticatorData": wrong.authenticator_data,
            "signature": wrong.signature,
        }}),
    );
    assert!(!refused.message.is_empty());
    let right = assertion(bytes_of(&get["challenge"]));
    let status = ok(
        &mut tab,
        "web_sign_in_answer",
        json!({ "answer": {
            "credentialId": made.credential_id,
            "clientDataJson": right.client_data_json.as_bytes(),
            "authenticatorData": right.authenticator_data,
            "signature": right.signature,
        }}),
    );
    assert!(status["account"].is_object(), "{status}");
    assert_eq!(ok(&mut tab, "web_state", json!({}))["locked"], false);
    // Every unlock here is the password: the reminder never applies.
    let reminders = ok(&mut tab, "reminders", json!({}));
    assert_eq!(reminders["password"]["applies"], false);
    // Unlocking is a sign-in by a device the account knows: no key asked.
    ok(&mut tab, "lock", json!({}));
    ok(&mut tab, "web_unlock", json!({ "password": PASSWORD }));
    // The key list reads the same from the tab; removing one is gated.
    assert_eq!(
        ok(&mut tab, "security_keys", json!({}))["keys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let id = desk.security_keys().unwrap().keys[0].id.clone();
    let gated = err(&mut tab, "security_key_remove", json!({ "id": id }));
    assert_eq!(gated.code, lockdown_app::ErrorCode::ApprovalNeeded);
}

#[test]
fn a_tab_joins_from_the_desktop_with_its_qr_code_and_no_kit() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (desk, _) = desktop(dir.path(), &url);
    let id = desk
        .add_login("https://shop.example", "me", b"pw".to_vec(), Some("Shop"))
        .unwrap();
    desk.sync().unwrap();

    // A QR code for another server is refused before anything is sent.
    let offered = desk.link_offer().unwrap();
    let mut tab = page(dir.path(), "tab", &url);
    let elsewhere = offered.qr.replace("127.0.0.1", "elsewhere.example");
    assert!(
        err(&mut tab, "web_join_start", json!({ "invite": elsewhere }))
            .message
            .contains("not")
    );
    let joining = ok(&mut tab, "web_join_start", json!({ "invite": offered.qr }));
    assert_eq!(joining["state"], "waiting");
    let asked = desk.link_poll().unwrap();
    let shown = ok(&mut tab, "web_join_poll", json!({}));
    assert_eq!(shown["sas"].as_str(), asked.sas.as_deref());
    desk.link_approve(GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    })
    .unwrap();
    let approved = ok(&mut tab, "web_join_poll", json!({}));
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["secret_key"], "sent");
    let status = ok(
        &mut tab,
        "web_join_finish",
        json!({ "login": LOGIN, "password": PASSWORD }),
    );
    assert!(status["account"].is_object());
    let items = ok(&mut tab, "list", json!({}));
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == id.as_str())
    );
    // The tab locks and unlocks like any signed-in tab.
    ok(&mut tab, "lock", json!({}));
    ok(&mut tab, "web_unlock", json!({ "password": PASSWORD }));
    assert_eq!(desk.device_list().unwrap().len(), 2);

    // Exposure and the legacy switch go through the same table.
    let e = ok(&mut tab, "exposure", json!({ "id": id }));
    assert_eq!(e["tier"], "fill");
    let kept = ok(
        &mut tab,
        "set_exposure",
        json!({ "id": id, "tier": "app-only" }),
    );
    assert_eq!(kept["tier"], "app-only");
    let gated = err(
        &mut tab,
        "set_legacy_opt_in",
        json!({ "id": id, "on": true }),
    );
    assert_eq!(gated.code, lockdown_app::ErrorCode::ApprovalNeeded);
    let gate = json!({ "password": PASSWORD, "confirmed": false });
    let refused = err(
        &mut tab,
        "set_legacy_opt_in",
        json!({ "id": id, "on": true, "gate": gate }),
    );
    assert!(
        refused.message.contains("crown jewel"),
        "{}",
        refused.message
    );
}
