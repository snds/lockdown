//! The recovery kit as a file in the web vault (docs/plan/38-mac-fixes.md
//! §4), natively against the real server in process: the page the desktop
//! was shown, sent back by a signed-in tab, comes back as the same PDF the
//! desktop writes; a tab that is locked, or a page that isn't a kit page,
//! gets nothing.

use std::sync::Arc;

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
    let state = Arc::new(AppState::in_memory(config).unwrap());
    run_in_background(state).unwrap()
}

fn ok(web: &mut WebVault, cmd: &str, args: Value) -> Value {
    match call(web, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(Reply::Bytes(b)) => panic!("{cmd}: bytes ({} of them), not JSON", b.len()),
        Err(e) => panic!("{cmd}: {e}"),
    }
}

#[test]
fn a_signed_in_tab_gets_the_desktops_pdf_of_the_kit_page_and_a_locked_one_does_not() {
    let running = server();
    let url = running.url();
    let dir = tempfile::tempdir().unwrap();

    // The account as the desktop makes it, with the kit page it shows once.
    let paths =
        lockdown_app::Paths::resolve(Some(dir.path().join("desktop/identity")), None).unwrap();
    let desktop = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    desktop.open_or_create(true).unwrap();
    let registered = desktop
        .account_register(&url, LOGIN, PASSWORD, None)
        .unwrap();
    desktop.sync().unwrap();
    let page = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();

    let mut web = WebVault::new(WebPaths {
        root: dir.path().join("tab/identity"),
        data: dir.path().join("tab/data"),
    });
    ok(
        &mut web,
        "web_sign_in",
        json!({ "server": url, "login": LOGIN, "password": PASSWORD, "secretKey": secret_key }),
    );

    let lines = registered.lines.clone();
    let pdf = match call(&mut web, "kit_pdf", json!({ "lines": lines, "page": page })) {
        Ok(Reply::Bytes(b)) => b,
        other => panic!("kit_pdf: {other:?}"),
    };
    let on_desktop = desktop.kit_pdf(&registered.lines, &page).unwrap();
    assert_eq!(*pdf, *on_desktop, "the same bytes on both");
    let typed = secret_key.replace('-', "");
    let first = format!("({})", &typed[..4]);
    assert_eq!(
        pdf.windows(first.len())
            .filter(|w| *w == first.as_bytes())
            .count(),
        1
    );

    // Not a kit page.
    assert!(call(&mut web, "kit_pdf", json!({ "page": "Dear bank," })).is_err());

    // Locked: nothing.
    ok(&mut web, "lock", json!({}));
    let locked = call(&mut web, "kit_pdf", json!({ "lines": [], "page": page })).unwrap_err();
    assert_eq!(locked.code, lockdown_app::ErrorCode::Locked);
}
