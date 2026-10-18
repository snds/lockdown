//! The endpoint end to end: a real `App` over a temporary vault, the
//! connection loop over the identity crate's in-memory transport, and once
//! over the real Unix socket with the real process-identity check.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ecosystem_identity::handshake::{
    AllowAll, ChannelTransport, PeerChannel, PeerTransport, Scripted,
};
use lockdown_app::{App, GateInput, LockPolicy, Paths, Revealed};
use lockdown_native_messaging::caller::{Caller, Family, HostHello};
use lockdown_native_messaging::{DEV_CHROMIUM_ID, FIREFOX_ID};
use serde_json::{Value, json};

use super::pairing::proof;
use super::*;

/// A clipboard that remembers how long each copy was, never the value.
#[derive(Default)]
struct FakeClipboard {
    copies: Mutex<Vec<usize>>,
}

impl CopySink for FakeClipboard {
    fn copy(&self, value: Revealed) -> Result<u64, String> {
        self.copies.lock().unwrap().push(value.len());
        Ok(30)
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    app: App,
    clipboard: Arc<FakeClipboard>,
    endpoint: Endpoint,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let data = paths.data.clone();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    let clipboard = Arc::new(FakeClipboard::default());
    let config = EndpointConfig {
        data: data.clone(),
        host_binary: data.join("missing-host"),
        app_exe: std::env::current_exe().unwrap(),
    };
    let endpoint = Endpoint::start(app.clone(), clipboard.clone(), config).unwrap();
    Rig {
        _dir: dir,
        app,
        clipboard,
        endpoint,
    }
}

fn chrome() -> Caller {
    Caller {
        family: Family::Chromium,
        extension_id: DEV_CHROMIUM_ID.into(),
    }
}

/// The host's side of a connection, driven by the test.
struct Host {
    ch: ecosystem_identity::handshake::channel::InMemoryChannel,
    next: u64,
}

impl Host {
    fn call(&mut self, op: &str, mut args: Value) -> Value {
        self.next += 1;
        args["id"] = json!(self.next);
        args["op"] = json!(op);
        self.ch.send(args.to_string().as_bytes()).unwrap();
        let answer: Value =
            serde_json::from_slice(&self.ch.recv(Duration::from_secs(10)).unwrap()).unwrap();
        assert_eq!(answer["id"], self.next, "{answer}");
        answer
    }

    fn ok(&mut self, op: &str, args: Value) -> Value {
        let a = self.call(op, args);
        assert_eq!(a["ok"], true, "{op}: {a}");
        a["result"].clone()
    }

    fn err(&mut self, op: &str, args: Value) -> String {
        let a = self.call(op, args);
        assert_eq!(a["ok"], false, "{op}: {a}");
        a["error"]["code"].as_str().unwrap().to_string()
    }
}

/// Connects through the in-memory transport and reads the first frame.
fn connect(
    shared: Arc<Shared>,
    verifier: Box<dyn ecosystem_identity::handshake::PeerVerifier + Send>,
    caller: Caller,
) -> (Host, Value) {
    let t = ChannelTransport::new();
    let server = t.clone();
    std::thread::spawn(move || {
        let ch = server.accept(Duration::from_secs(10)).unwrap();
        serve(&shared, verifier.as_ref(), ch);
    });
    let mut ch = t.connect(Duration::from_secs(10)).unwrap();
    // An untrusted peer is refused before its hello is read, and the
    // channel may already be closed when the hello goes out: the refusal
    // is still the first frame to read.
    let _ = ch.send(&serde_json::to_vec(&HostHello::new(caller)).unwrap());
    let first: Value = serde_json::from_slice(&ch.recv(Duration::from_secs(10)).unwrap()).unwrap();
    (Host { ch, next: 0 }, first)
}

const CLIENT: &str = "aaaabbbbccccddddeeeeffff00001111";
const KEY: [u8; 32] = [42; 32];

fn hex(b: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(b)
}

/// Pairs `CLIENT` through the protocol and the app's answer, and signs in.
fn paired(rig: &Rig) -> Host {
    let (mut host, ready) = connect(
        Arc::clone(&rig.endpoint.shared),
        Box::new(AllowAll),
        chrome(),
    );
    assert_eq!(ready["event"], "ready");
    let challenge = data_encoding::HEXLOWER
        .decode(ready["challenge"].as_str().unwrap().as_bytes())
        .unwrap();
    let begun = host.ok(
        "pair_begin",
        json!({ "client_id": CLIENT, "client_key": hex(&KEY), "name": "Chrome on Linux" }),
    );
    let code = begun["code"].as_str().unwrap().to_string();
    assert_eq!(
        host.ok("pair_status", json!({ "client_id": CLIENT }))["state"],
        "pending"
    );
    let pending = rig.endpoint.pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].code, code,
        "the app shows the code the extension shows"
    );
    // No platform check and no account here: the gate is the window's confirm.
    assert!(
        rig.endpoint
            .answer(&pending[0].request_id, true, GateInput::default())
            .is_err()
    );
    rig.endpoint
        .answer(
            &pending[0].request_id,
            true,
            GateInput {
                password: None,
                confirmed: true,
            },
        )
        .unwrap();
    assert_eq!(
        host.ok("pair_status", json!({ "client_id": CLIENT }))["state"],
        "paired"
    );
    let p = hex(&proof(&KEY, &challenge, CLIENT));
    assert_eq!(
        host.ok("auth", json!({ "client_id": CLIENT, "proof": p }))["paired"],
        true
    );
    host
}

#[test]
fn nothing_is_answered_before_pairing_and_a_wrong_proof_stays_unpaired() {
    let rig = rig();
    let (mut host, ready) = connect(
        Arc::clone(&rig.endpoint.shared),
        Box::new(AllowAll),
        chrome(),
    );
    assert_eq!(ready["event"], "ready");
    let status = host.ok("status", json!({}));
    assert_eq!(status["locked"], false);
    assert_eq!(status["paired"], false);
    for op in [
        "matches",
        "fill",
        "search",
        "copy",
        "wallet",
        "offer_save",
        "basic_auth",
    ] {
        assert_eq!(host.err(op, json!({})), "not_paired", "{op}");
    }
    assert_eq!(
        host.err("auth", json!({ "client_id": CLIENT, "proof": "00" })),
        "not_paired"
    );
    assert_eq!(host.err("teleport", json!({})), "not_paired");
    assert_eq!(rig.clipboard.copies.lock().unwrap().len(), 0);
}

#[test]
fn a_paired_browser_matches_fills_copies_and_saves_through_the_core() {
    let rig = rig();
    let id = rig
        .app
        .add_login("https://example.com", "me", b"hunter22".to_vec(), None)
        .unwrap();
    let mut host = paired(&rig);
    assert_eq!(host.err("teleport", json!({})), "unknown_op");

    let m = host.ok("matches", json!({ "frame": "https://example.com/login" }));
    assert_eq!(m["logins"][0]["item_id"], id);
    assert_eq!(m["logins"][0]["decision"], "exact");
    let page = "https://example.com/login";
    let fill = host.ok(
        "fill",
        json!({ "item_id": id, "what": "login", "top": page, "frame": page, "field_visible": true }),
    );
    assert_eq!(fill["verdict"], "fill");
    assert_eq!(
        fill["values"],
        json!([{ "role": "user", "value": "me" }, { "role": "pw", "value": "hunter22" }])
    );
    let covered = host.ok(
        "fill",
        json!({ "item_id": id, "what": "login", "top": page, "frame": page, "field_visible": false, "user_confirmed": true }),
    );
    assert_eq!(covered["verdict"], "refuse");
    assert!(covered.get("values").is_none());
    let elsewhere = "https://evil.test/";
    assert_eq!(
        host.ok(
            "fill",
            json!({ "item_id": id, "what": "login", "top": elsewhere, "frame": elsewhere, "field_visible": true }),
        )["verdict"],
        "refuse"
    );

    let found = host.ok("search", json!({ "query": "exam me" }));
    assert_eq!(found["items"][0]["item_id"], id);
    assert_eq!(found["items"][0]["host"], "example.com");
    assert_eq!(found["items"][0]["copy"]["label"], "password");
    assert!(!found.to_string().contains("hunter22"));
    assert_eq!(
        host.ok("search", json!({ "query": "nothing-like-it" }))["items"],
        json!([])
    );
    let copied = host.ok("copy", json!({ "item_id": id }));
    assert_eq!(copied["clears_in_secs"], 30);
    assert_eq!(*rig.clipboard.copies.lock().unwrap(), vec![8]);

    // Basic auth: exactly one login for this exact host.
    let basic = host.ok(
        "basic_auth",
        json!({ "url": "https://example.com/private/" }),
    );
    assert_eq!(basic["verdict"], "fill");
    assert_eq!(basic["values"][1]["value"], "hunter22");
    assert_eq!(
        host.ok("basic_auth", json!({ "url": "https://www.example.com/" }))["verdict"],
        "none"
    );

    // A new sign-in elsewhere: offered, saved, then the same sign-in is "same".
    let offer = host.ok(
        "offer_save",
        json!({ "page": "https://new.test/signup", "username": "sean", "password": "pa55word" }),
    );
    assert_eq!(offer["kind"], "new");
    let offer_id = offer["offer_id"].as_str().unwrap().to_string();
    let saved = host.ok("save", json!({ "offer_id": offer_id }));
    assert_eq!(saved["kind"], "new");
    assert_eq!(host.err("save", json!({ "offer_id": offer_id })), "expired");
    let again = host.ok(
        "offer_save",
        json!({ "page": "https://new.test/login", "username": "sean", "password": "pa55word" }),
    );
    assert_eq!(again["kind"], "same");
    // A changed password on the first site: an update of that login.
    let update = host.ok(
        "offer_save",
        json!({ "page": page, "username": "me", "password": "n3w-pass" }),
    );
    assert_eq!(update["kind"], "update");
    let dismissed = update["offer_id"].as_str().unwrap().to_string();
    host.ok("dismiss", json!({ "offer_id": dismissed }));
    assert_eq!(
        host.err("save", json!({ "offer_id": dismissed })),
        "expired"
    );

    // Unpaired in the app: the next request is refused.
    rig.endpoint.unpair(CLIENT).unwrap();
    assert_eq!(host.err("search", json!({ "query": "" })), "not_paired");
    assert!(rig.endpoint.paired().is_empty());
}

#[test]
fn locked_is_said_plainly() {
    let rig = rig();
    let mut host = paired(&rig);
    rig.app.lock().unwrap();
    assert_eq!(host.ok("status", json!({}))["locked"], true);
    assert_eq!(
        host.err("matches", json!({ "frame": "https://example.com" })),
        "locked"
    );
}

#[test]
fn another_extension_or_an_untrusted_program_is_refused_at_once() {
    let rig = rig();
    let other = Caller {
        family: Family::Chromium,
        extension_id: "a".repeat(32),
    };
    let (_host, first) = connect(Arc::clone(&rig.endpoint.shared), Box::new(AllowAll), other);
    assert_eq!(first["event"], "refused");
    let (_host, first) = connect(
        Arc::clone(&rig.endpoint.shared),
        Box::new(Scripted::new([Err("not the host".to_string())])),
        Caller {
            family: Family::Firefox,
            extension_id: FIREFOX_ID.into(),
        },
    );
    assert_eq!(first["event"], "refused");
    assert!(
        first["message"]
            .as_str()
            .unwrap()
            .contains("isn't Lockdown's browser host")
    );
}

#[test]
fn without_the_host_installed_the_endpoint_says_why_and_still_lists_pairings() {
    let rig = rig();
    let status = rig.endpoint.status();
    assert!(!status.listening);
    assert!(status.problem.unwrap().contains("isn't installed"));
    assert!(rig.endpoint.pending().is_empty());
}

#[cfg(unix)]
#[test]
fn over_the_real_socket_the_host_binary_is_checked_by_process_identity() {
    use ecosystem_identity::handshake::unix::UnixSocketTransport;
    use lockdown_native_messaging::endpoint::EndpointFile;

    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let data = paths.data.clone();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    // This test binary plays both the app and the host: the endpoint allows
    // exactly this executable, by path and hash, on the other end.
    let me = std::env::current_exe().unwrap();
    let config = EndpointConfig {
        data: data.clone(),
        host_binary: me.clone(),
        app_exe: me.clone(),
    };
    let endpoint = Endpoint::start(app, Arc::new(FakeClipboard::default()), config).unwrap();
    assert!(
        endpoint.status().listening,
        "{:?}",
        endpoint.status().problem
    );
    let file = EndpointFile::read(&data).unwrap();
    assert_eq!(file.app_exe, me);
    let mut ch = UnixSocketTransport::at(&file.socket)
        .connect(Duration::from_secs(5))
        .expect("the endpoint's socket accepts a connection");
    ch.send(&serde_json::to_vec(&HostHello::new(chrome())).unwrap())
        .unwrap();
    // Before it answers, the endpoint hashes the connecting executable: this
    // test binary, hundreds of MB with the shell feature. The workspace
    // builds sha2 optimised, so that takes about a second; the wait is
    // generous so a slow runner reads as a pass, or as this message.
    let started = std::time::Instant::now();
    let ready = ch.recv(Duration::from_secs(90)).unwrap_or_else(|e| {
        panic!(
            "no `ready` from the endpoint {:?} after the hello ({e}): the process-identity check of {} (path and SHA-256) didn't finish",
            started.elapsed(),
            me.display()
        )
    });
    let ready: Value = serde_json::from_slice(&ready).unwrap();
    assert_eq!(ready["event"], "ready", "{ready}");
    ch.send(br#"{"id":1,"op":"status"}"#).unwrap();
    let status: Value = serde_json::from_slice(
        &ch.recv(Duration::from_secs(30))
            .expect("the endpoint answers `status` once it said ready"),
    )
    .unwrap();
    assert_eq!(status["result"]["paired"], false);
    drop(endpoint);
    assert!(
        EndpointFile::read(&data).is_err(),
        "the endpoint file goes with the app"
    );
}

/// The passkey ops over the protocol (docs/plan/19-passkeys.md §4): a plan,
/// the create's two steps, the offer, the get's two steps, a Signal, and
/// the setting that turns them off. Nothing secret is in any answer.
#[test]
fn passkeys_are_made_and_used_over_the_protocol_with_nothing_secret_in_an_answer() {
    let rig = rig();
    let login = rig
        .app
        .add_login(
            "https://example.com",
            "sean",
            b"pw".to_vec(),
            Some("Example"),
        )
        .unwrap();
    let mut host = paired(&rig);
    assert_eq!(host.ok("status", json!({}))["passkeys"], true);
    let page = |confirmed: bool| json!({ "top": "https://example.com/a", "frame": "https://example.com/a", "visible": true, "user_confirmed": confirmed });
    let create = json!({
        "rp_name": "Example", "user_id": "dXNlci0x", "user_name": "sean", "user_display_name": "Sean",
        "challenge": "Y2hhbGxlbmdl", "algs": [-7], "user_verification": "preferred"
    });
    let plan = host.ok(
        "passkey_plan",
        json!({ "page": page(false), "request": create }),
    );
    assert_eq!(plan["outcome"], "done");
    assert_eq!(plan["result"]["item_id"], login);
    let first = host.ok(
        "passkey_create",
        json!({ "page": page(false), "request": create }),
    );
    assert_eq!(first["outcome"], "confirm");
    let made = host.ok(
        "passkey_create",
        json!({ "page": page(true), "request": create }),
    );
    assert_eq!(made["outcome"], "done", "{made}");
    let made = &made["result"];
    assert_eq!(made["item_id"], login);
    assert_eq!(made["public_key_algorithm"], -7);
    for absent in ["pkcs8", "key", "private_key"] {
        assert!(made.get(absent).is_none(), "{absent} never crosses");
    }
    let cred = made["credential_id"].as_str().unwrap().to_string();

    let get = json!({ "challenge": "Z2V0", "user_verification": "discouraged" });
    let offered = host.ok(
        "passkey_options",
        json!({ "page": page(false), "request": get }),
    );
    assert_eq!(offered["result"]["options"][0]["credential_id"], cred);
    let args = |confirmed: bool| json!({ "page": page(confirmed), "request": get, "item_id": login, "credential_id": cred, "user_name": "sean" });
    assert_eq!(host.ok("passkey_get", args(false))["outcome"], "confirm");
    let signed = host.ok("passkey_get", args(true));
    assert_eq!(signed["outcome"], "done", "{signed}");
    assert_eq!(signed["result"]["user_handle"], "dXNlci0x");
    assert!(signed["result"]["signature"].as_str().unwrap().len() > 60);

    // A page from another site gets a SecurityError, by name.
    let evil = json!({ "page": { "top": "https://evil.test/", "frame": "https://evil.test/" }, "request": { "rp_id": "example.com", "challenge": "eA" } });
    let r = host.ok("passkey_options", evil);
    assert_eq!(r["outcome"], "refused");
    assert_eq!(r["name"], "SecurityError");

    // A Signal hides it.
    let s = host.ok(
        "passkey_signal",
        json!({ "page": "https://example.com/", "signal": { "signal": "unknown_credential", "rp_id": "example.com", "credential_id": cred } }),
    );
    assert_eq!(s["result"]["changed"], 1);
    let offered = host.ok(
        "passkey_options",
        json!({ "page": page(false), "request": get }),
    );
    assert_eq!(offered["result"]["options"].as_array().unwrap().len(), 0);

    // Turned off in Settings: the extension is told so and steps aside.
    let mut settings = rig.app.settings().unwrap();
    settings.browser_passkeys = false;
    rig.app.set_settings(settings).unwrap();
    assert_eq!(host.ok("status", json!({}))["passkeys"], false);
    assert_eq!(
        host.err(
            "passkey_options",
            json!({ "page": page(false), "request": get })
        ),
        "passkeys_off"
    );
}

#[test]
fn passkey_ops_need_the_pairing_first() {
    let rig = rig();
    let (mut host, _) = connect(
        Arc::clone(&rig.endpoint.shared),
        Box::new(AllowAll),
        chrome(),
    );
    for op in [
        "passkey_options",
        "passkey_plan",
        "passkey_create",
        "passkey_get",
        "passkey_signal",
    ] {
        assert_eq!(host.err(op, json!({})), "not_paired", "{op}");
    }
}

#[test]
fn a_request_waiting_for_the_app_is_answered_through_the_gate() {
    let rig = rig();
    let id = rig
        .endpoint
        .shared
        .approvals
        .put(CLIENT, "Chrome on Linux", "Sign in to example.com as sean")
        .unwrap();
    let pending = rig.endpoint.passkey_pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, "Sign in to example.com as sean");
    // The gate here is the window's confirm (no account, no platform check):
    // nothing confirmed yet is refused and the request still waits.
    assert!(
        rig.endpoint
            .passkey_answer(&id, true, GateInput::default())
            .is_err()
    );
    assert_eq!(rig.endpoint.passkey_pending().len(), 1);
    rig.endpoint
        .passkey_answer(
            &id,
            true,
            GateInput {
                password: None,
                confirmed: true,
            },
        )
        .unwrap();
    assert_eq!(
        rig.endpoint
            .shared
            .approvals
            .wait(&id, Duration::from_secs(1))
            .as_deref(),
        Some("window"),
        "the method that approved it goes back to the core, which sets UV only for a real check"
    );
}

#[test]
fn the_bridges_ops_need_the_pairing_and_answer_through_the_core() {
    let rig = rig();
    let (mut stranger, _) = connect(
        Arc::clone(&rig.endpoint.shared),
        Box::new(AllowAll),
        chrome(),
    );
    for op in [
        "federation_hints",
        "federation_seen",
        "site_fixes",
        "fix_set",
        "change_link",
        "alias_options",
        "alias_create",
    ] {
        assert_eq!(stranger.err(op, json!({})), "not_paired", "{op}");
    }
    let shop = rig
        .app
        .add_item(
            "login",
            Some("Shop"),
            false,
            vec![lockdown_app::FieldChange::text(
                "url.0",
                "https://shop.example",
            )],
        )
        .unwrap();
    let jewel = rig
        .app
        .add_item(
            "login",
            Some("Bank"),
            true,
            vec![lockdown_app::FieldChange::text(
                "url.0",
                "https://bank.example",
            )],
        )
        .unwrap();
    let mut host = paired(&rig);
    let hints = host.ok("federation_hints", json!({}));
    assert!(
        hints["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "google")
    );
    let seen = host.ok(
        "federation_seen",
        json!({
            "provider": "google",
            "page": "https://shop.example/login",
            "auth_url": "https://accounts.google.com/o/oauth2/auth?redirect_uri=https%3A%2F%2Fshop.example%2Fcb"
        }),
    );
    assert_eq!(seen["recorded"][0], shop.as_str());
    assert_eq!(
        host.err(
            "federation_seen",
            json!({ "provider": "nobody", "page": "https://shop.example" })
        ),
        "refused"
    );
    // A fix set from the popup comes back for the page.
    let fix = json!({ "never": false, "fields": [{ "match": { "name": "login_id" }, "role": "username" }] });
    assert_eq!(
        host.ok(
            "fix_set",
            json!({ "frame": "https://shop.example/x", "fix": fix })
        )["logins"],
        1
    );
    let fixes = host.ok(
        "site_fixes",
        json!({ "frame": "https://shop.example/login" }),
    );
    assert_eq!(fixes["fields"][0]["role"], "username");
    assert_eq!(fixes["fields"][0]["source"], "yours");
    // The checked change-password link; a crown jewel's is the app's.
    let mut s = rig.app.settings().unwrap();
    s.change_password_check = false;
    rig.app.set_settings(s).unwrap();
    let link = host.ok("change_link", json!({ "item_id": shop }));
    assert_eq!(
        link["url"],
        "https://shop.example/.well-known/change-password"
    );
    assert_eq!(
        host.err("change_link", json!({ "item_id": jewel })),
        "refused"
    );
    // No alias service set up: none offered. One set up: making an alias
    // asks for the Use gate, which only the platform check answers here.
    assert!(
        host.ok("alias_options", json!({}))["services"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let svc = rig
        .app
        .alias_service_add(
            "duck_duck_go",
            b"ddg-key".to_vec(),
            Some("http://127.0.0.1:9"),
            None,
        )
        .unwrap();
    let opts = host.ok("alias_options", json!({}));
    assert_eq!(opts["services"][0]["item_id"], svc.as_str());
    assert_eq!(opts["services"][0]["name"], "DuckDuckGo");
    assert!(!opts.to_string().contains("ddg-key"));
    assert_eq!(
        host.err(
            "alias_create",
            json!({ "item_id": svc, "frame": "https://shop.example/signup" })
        ),
        "approval_needed"
    );
}
