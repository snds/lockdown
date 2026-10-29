//! A test fixture for the extension's end-to-end suite
//! (extension/test/extension.spec.ts): the real desktop endpoint and the
//! real core over a throwaway vault, with no window. It seeds a few items,
//! writes the Chromium host manifest into a browser profile directory,
//! prints what happens as JSON lines on stdout, and takes the person's
//! part (approve a pairing, read a code) from lines on stdin.
//!
//! ```text
//! browser-endpoint --dir <scratch> --host <lockdown-native-host> --profile <chromium user data dir> [--platform-check]
//! ```
//!
//! With `--platform-check`, a stand-in for Touch ID or Windows Hello is
//! installed as the app's platform check: it answers what the last
//! `platform yes|no` line said and prints a `platform_check` event each
//! time it is asked, so the passkey suite (extension/test/passkeys.spec.ts)
//! can see user verification happen.
//!
//! The bridges suite (extension/test/bridges.spec.ts) adds logins, an
//! alias service pointed at its own fixture server, and reads back the
//! sign-in map and a login's fill fix the same way.
//!
//! Never built into the app: it is an example of `lockdown-desktop`, and
//! it prints a test vault's own test values when asked, which the app
//! never does.

use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lockdown_app::passkeys::B64;
use lockdown_app::{
    App, ApprovalKind, Approver, FieldChange, GateInput, LockPolicy, Paths, Revealed,
};
use lockdown_desktop_lib::browser::{CopySink, Endpoint, EndpointConfig};
use lockdown_native_messaging::manifest::{Browser, manifest};
use serde_json::json;

static OUT: Mutex<()> = Mutex::new(());

fn emit(v: serde_json::Value) {
    let _g = OUT.lock();
    println!("{v}");
}

struct PrintingClipboard;

impl CopySink for PrintingClipboard {
    fn copy(&self, value: Revealed) -> Result<u64, String> {
        emit(json!({ "event": "copied", "len": value.len() }));
        Ok(30)
    }
}

/// The stand-in platform check.
struct TestCheck {
    yes: Arc<AtomicBool>,
}

impl Approver for TestCheck {
    fn confirm(&self, kind: ApprovalKind, reason: &str) -> Result<bool, String> {
        let yes = self.yes.load(Ordering::SeqCst);
        emit(
            json!({ "event": "platform_check", "kind": format!("{kind:?}"), "reason": reason, "answer": yes }),
        );
        Ok(yes)
    }

    fn method(&self) -> &'static str {
        "the test check"
    }
}

fn arg(name: &str) -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let at = args
        .iter()
        .position(|a| a == name)
        .unwrap_or_else(|| panic!("{name} is required"));
    PathBuf::from(&args[at + 1])
}

fn main() {
    let dir = arg("--dir");
    let host = arg("--host");
    let profile = arg("--profile");
    let paths = Paths::resolve(Some(dir.join("snds/identity")), None).expect("paths");
    let data = paths.data.clone();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).expect("a vault");
    let platform = Arc::new(AtomicBool::new(true));
    if std::env::args().any(|a| a == "--platform-check") {
        app.set_approver(Box::new(TestCheck {
            yes: Arc::clone(&platform),
        }))
        .unwrap();
    }

    let local = app
        .add_login(
            "http://localhost",
            "sean@example.com",
            b"correct-horse-1".to_vec(),
            Some("Local site"),
        )
        .unwrap();
    app.add_code(
        "otpauth://totp/Local:sean?secret=JBSWY3DPEHPK3PXP&issuer=Local",
        None,
        Some(&local),
    )
    .unwrap();
    let framed = app
        .add_login(
            "http://127.0.0.1",
            "framed@example.com",
            b"framed-pass-2".to_vec(),
            Some("Loopback"),
        )
        .unwrap();
    let paypal = app
        .add_login(
            "http://www.paypal.com",
            "pal",
            b"paypal-pass-3".to_vec(),
            Some("PayPal"),
        )
        .unwrap();
    let basic = app
        .add_login(
            "http://basic.test",
            "basic-user",
            b"basic-pass-4".to_vec(),
            Some("Basic site"),
        )
        .unwrap();
    let card = app
        .add_item(
            "card",
            Some("Everyday card"),
            false,
            vec![
                FieldChange::secret("card.number", b"4242424242424242"),
                FieldChange::text("card.holder", "Sean Sands"),
                FieldChange::text("card.expiry", "2030-08"),
                FieldChange::secret("card.cvc", b"123"),
            ],
        )
        .unwrap();
    let identity = app
        .add_item(
            "identity",
            Some("Home"),
            false,
            vec![
                FieldChange::text("name.given", "Sean"),
                FieldChange::text("name.family", "Sands"),
                FieldChange::text("identity.email", "sean@example.com"),
                FieldChange::text("address.city", "Springfield"),
                FieldChange::text("address.postal_code", "12345"),
            ],
        )
        .unwrap();

    // The host manifest where a Chromium with `--user-data-dir=<profile>`
    // looks for it on Linux.
    let nm = profile.join("NativeMessagingHosts");
    std::fs::create_dir_all(&nm).unwrap();
    let body = serde_json::to_vec_pretty(&manifest(Browser::Chromium, &host)).unwrap();
    std::fs::write(
        nm.join(format!("{}.json", lockdown_native_messaging::HOST_NAME)),
        body,
    )
    .unwrap();

    let config = EndpointConfig {
        data: data.clone(),
        host_binary: host,
        app_exe: std::env::current_exe().unwrap(),
    };
    let endpoint =
        Arc::new(Endpoint::start(app.clone(), Arc::new(PrintingClipboard), config).unwrap());
    let status = endpoint.status();
    emit(json!({
        "event": "ready",
        "listening": status.listening,
        "problem": status.problem,
        "data": data,
        "items": { "local": local, "framed": framed, "paypal": paypal, "basic": basic, "card": card, "identity": identity },
    }));

    let watcher = Arc::clone(&endpoint);
    std::thread::spawn(move || {
        let mut seen = std::collections::HashSet::new();
        loop {
            for p in watcher.pending() {
                if seen.insert(p.request_id.clone()) {
                    emit(
                        json!({ "event": "pairing", "request_id": p.request_id, "code": p.code, "name": p.name }),
                    );
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["approve", id] => {
                let r = endpoint.answer(
                    id,
                    true,
                    GateInput {
                        password: None,
                        confirmed: true,
                    },
                );
                emit(json!({ "event": "answered", "ok": r.is_ok() }));
            }
            ["deny", id] => {
                let r = endpoint.answer(id, false, GateInput::default());
                emit(json!({ "event": "answered", "ok": r.is_ok() }));
            }
            ["code", item] => {
                let now = app
                    .code_now(item, "code.0", false, GateInput::default())
                    .unwrap();
                let next = app
                    .code_now(item, "code.0", true, GateInput::default())
                    .unwrap();
                emit(json!({
                    "event": "code",
                    "code": String::from_utf8_lossy(now.expose()),
                    "next": String::from_utf8_lossy(next.expose()),
                }));
            }
            ["password", item] => {
                let pw = app.reveal_to_use(item, "pw", GateInput::default()).unwrap();
                emit(json!({ "event": "password", "value": String::from_utf8_lossy(pw.expose()) }));
            }
            ["list"] => {
                let rows: Vec<_> = app
                    .list()
                    .unwrap()
                    .into_iter()
                    .map(|r| json!({ "id": r.id, "title": r.title, "username": r.username, "site": r.site }))
                    .collect();
                emit(json!({ "event": "list", "items": rows }));
            }
            ["lock"] => {
                app.lock().unwrap();
                emit(json!({ "event": "locked" }));
            }
            ["unlock"] => {
                app.unlock().unwrap();
                emit(json!({ "event": "unlocked" }));
            }
            ["platform", answer] => {
                platform.store(*answer == "yes", Ordering::SeqCst);
                emit(json!({ "event": "platform", "yes": *answer == "yes" }));
            }
            ["passkeys", item] => {
                let views = app.passkeys(item).unwrap_or_default();
                emit(json!({ "event": "passkeys", "passkeys": views }));
            }
            ["restore", item, credential] => {
                let ok = lockdown_passkey_b64(credential)
                    .map(|c| app.passkey_restore(item, c).is_ok())
                    .unwrap_or(false);
                emit(json!({ "event": "restored", "ok": ok }));
            }
            ["browser-passkeys", on] => {
                let mut settings = app.settings().unwrap();
                settings.browser_passkeys = *on == "on";
                app.set_settings(settings).unwrap();
                emit(json!({ "event": "browser_passkeys", "on": *on == "on" }));
            }
            // The bridges suite (extension/test/bridges.spec.ts).
            ["login", url, user, pw, title @ ..] => {
                let title = title.join(" ");
                let r = app.add_login(url, user, pw.as_bytes().to_vec(), Some(&title));
                emit(json!({ "event": "login", "id": r.ok() }));
            }
            ["alias-service", service, base] => {
                let r =
                    app.alias_service_add(service, b"fixture-alias-key".to_vec(), Some(base), None);
                emit(json!({ "event": "alias_service", "id": r.ok() }));
            }
            ["aliases"] => {
                emit(json!({ "event": "aliases", "overview": app.alias_overview().ok() }));
            }
            ["federation"] => {
                emit(json!({ "event": "federation", "map": app.federation_map().ok() }));
            }
            ["fix", item] => {
                emit(json!({ "event": "fix", "fix": app.site_fix(item).ok() }));
            }
            ["change-check", on] => {
                let mut settings = app.settings().unwrap();
                settings.change_password_check = *on == "on";
                app.set_settings(settings).unwrap();
                emit(json!({ "event": "change_check", "on": *on == "on" }));
            }
            // docs/plan/26-desktop-finish.md: a standalone code item, a code
            // as the camera read it (the core's scan flow after the decode,
            // which the core's own tests drive from a real frame), the
            // person's yes, and quick access's "Fill in the browser".
            ["add-code", uri, title @ ..] => {
                let r = app.add_code(uri, Some(&title.join(" ")), None);
                emit(json!({ "event": "code_added", "id": r.ok() }));
            }
            ["scan-camera", text] => {
                let r = app.scan_text(text, lockdown_app::scan::ScanSource::Camera);
                emit(json!({ "event": "scanned", "outcome": r.ok() }));
            }
            ["activation-approve", id, item] => {
                let gate = GateInput {
                    password: None,
                    confirmed: true,
                };
                let r = app.activation_approve(id, Some(item), gate);
                emit(json!({ "event": "activation", "started": r.ok() }));
            }
            ["quick-fill", item] => {
                let r = app.quick_fill(item);
                emit(json!({ "event": "quick_fill", "started": r.ok() }));
            }
            // The automatic upgrade's notices, as the app's toast takes them.
            ["upgrades"] => {
                emit(
                    json!({ "event": "upgrades", "upgrades": app.passkey_upgrades().unwrap_or_default() }),
                );
            }
            ["quit"] => break,
            _ => emit(json!({ "event": "unknown", "line": line })),
        }
    }
}

/// A credential id as the page and the app write it (base64url).
fn lockdown_passkey_b64(text: &str) -> Option<B64> {
    serde_json::from_value(json!(text)).ok()
}
