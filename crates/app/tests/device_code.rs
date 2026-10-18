//! TV and console sign-ins through `App` (docs/plan/26-desktop-finish.md
//! §3): a QR read from a camera frame passes the proximity gate, a paste or
//! an image file never does, only pages on the signed list open, the yes
//! goes through the `Approve` gate and the login the person picked comes
//! first on the real page; a passkey QR and a Steam QR stop after the gate.

use lockdown_app::scan::{ScanSource, decode_qr};
use lockdown_app::{App, ErrorCode, GateInput, LockPolicy, Paths};

fn device(dir: &std::path::Path) -> App {
    let paths = Paths::resolve(Some(dir.join("a/snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app
}

fn confirmed() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

/// A grey image of `text`'s QR code: four pixels a module, a quiet zone.
fn qr_image(text: &str) -> (u32, u32, Vec<u8>) {
    let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
    let modules = code.width();
    let (scale, quiet) = (4, 4);
    let side = (modules + 2 * quiet) * scale;
    let colors = code.to_colors();
    let mut luma = vec![255u8; side * side];
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = (x / scale, y / scale);
            if mx >= quiet && my >= quiet && mx < quiet + modules && my < quiet + modules {
                let dark = colors[(my - quiet) * modules + (mx - quiet)] == qrcode::Color::Dark;
                if dark {
                    luma[y * side + x] = 0;
                }
            }
        }
    }
    (side as u32, side as u32, luma)
}

fn camera(app: &App, text: &str) -> lockdown_app::scan::ScanOutcome {
    let (w, h, luma) = qr_image(text);
    app.scan_image(w, h, luma, ScanSource::Camera).unwrap()
}

const TV: &str = "https://www.youtube.com/activate?user_code=WDJB-MJHT";

#[test]
fn a_scanned_device_code_opens_the_real_page_with_the_login_first() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    let login = app
        .add_login(
            "https://www.youtube.com",
            "sean@example.com",
            b"yt-pass-1".to_vec(),
            Some("YouTube"),
        )
        .unwrap();
    app.add_login("https://elsewhere.example", "x", b"pw".to_vec(), None)
        .unwrap();

    // The core decodes the frame itself.
    let (w, h, luma) = qr_image(TV);
    assert_eq!(decode_qr(w, h, &luma).as_deref(), Some(TV));
    assert_eq!(
        decode_qr(w, h, &luma[1..]),
        None,
        "a short frame is refused"
    );

    let out = camera(&app, TV);
    assert_eq!(out.kind, "device_code");
    assert_eq!(out.decision, "approve", "{:?}", out.reason);
    assert_eq!(out.host.as_deref(), Some("www.youtube.com"));
    assert_eq!(out.code.as_deref(), Some("WDJB-MJHT"));
    assert_eq!(out.accounts.len(), 1);
    assert_eq!(out.accounts[0].item_id, login);
    let id = out.activation_id.clone().unwrap();

    // The yes goes through the gate (no account here: the window's confirm).
    let e = app
        .activation_approve(&id, Some(&login), GateInput::default())
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ApprovalNeeded);
    let started = app
        .activation_approve(&id, Some(&login), confirmed())
        .unwrap();
    assert_eq!(
        started.url,
        "https://www.youtube.com/activate?user_code=WDJB-MJHT"
    );
    assert!(started.code_in_url);
    // The login comes first on that page, and the approval was its use.
    let m = app.page_matches(&started.url).unwrap();
    assert_eq!(m.logins[0].item_id, login);
    assert!(m.logins[0].asked);
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == login)
        .unwrap();
    assert_eq!(row.uses, 1);
    // An activation is used once.
    assert!(app.activation_approve(&id, None, confirmed()).is_err());

    // A login that doesn't fit the page is refused even after the gate.
    let again = camera(&app, "https://www.youtube.com/activate?user_code=ABCD-EFGH");
    let other = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id != login)
        .unwrap()
        .id;
    let e = app
        .activation_approve(
            again.activation_id.as_deref().unwrap(),
            Some(&other),
            confirmed(),
        )
        .unwrap_err();
    assert!(
        e.message.contains("isn't one for this page"),
        "{}",
        e.message
    );

    // Cancelled, it's gone.
    let third = camera(&app, "https://www.youtube.com/activate?user_code=QQQQ-RRRR");
    let third = third.activation_id.unwrap();
    app.activation_cancel(&third).unwrap();
    assert!(app.activation_approve(&third, None, confirmed()).is_err());
}

#[test]
fn a_paste_or_an_image_file_never_passes_and_only_listed_pages_open() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    for source in [ScanSource::Pasted, ScanSource::Image] {
        let out = match source {
            ScanSource::Pasted => app.scan_text(TV, source).unwrap(),
            _ => {
                let (w, h, luma) = qr_image(TV);
                app.scan_image(w, h, luma, source).unwrap()
            }
        };
        assert_eq!(out.kind, "device_code");
        assert_eq!(out.decision, "refused");
        assert!(out.reason.unwrap().contains("screen in front of you"));
        assert!(out.activation_id.is_none());
        // The page is still named, so the person sees what it was.
        assert_eq!(out.host.as_deref(), Some("www.youtube.com"));
    }
    // An unlisted page or a look-alike is never opened, scanned or not.
    let out = camera(
        &app,
        "https://www.youtube.com.evil.example/activate?user_code=WDJB-MJHT",
    );
    assert_eq!(out.kind, "unlisted");
    assert!(out.reason.unwrap().contains("youtube.com.evil.example"));
    let out = camera(&app, "just some text");
    assert_eq!(out.kind, "other");
    // No code in the picture.
    let out = app
        .scan_image(64, 64, vec![255; 64 * 64], ScanSource::Camera)
        .unwrap();
    assert_eq!(out.kind, "none");
    assert!(!out.found);
}

#[test]
fn passkey_and_steam_codes_go_through_the_same_gate_then_stop() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    let fido = "FIDO:/10203040506070809001020304050607080900102030405060708090010203040506";
    let out = app.scan_text(fido, ScanSource::Pasted).unwrap();
    assert_eq!(
        (out.kind.as_str(), out.decision.as_str()),
        ("passkey_qr", "refused")
    );
    assert!(out.reason.unwrap().contains("screen in front of you"));
    let out = camera(&app, fido);
    assert_eq!(
        (out.kind.as_str(), out.decision.as_str()),
        ("passkey_qr", "unsupported")
    );
    assert!(out.reason.unwrap().contains("Bluetooth"));

    let steam = "https://s.team/q/1/2345678901234567890";
    let out = app.scan_text(steam, ScanSource::Pasted).unwrap();
    assert_eq!(
        (out.kind.as_str(), out.decision.as_str()),
        ("steam_qr", "refused")
    );
    let out = camera(&app, steam);
    assert_eq!(
        (out.kind.as_str(), out.decision.as_str()),
        ("steam_qr", "unsupported")
    );
    assert!(out.reason.unwrap().contains("Steam"));
}

#[test]
fn the_rate_limit_holds_and_a_lock_ends_every_activation() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    // The same frame seen again is answered from the last outcome, but a
    // flood of different codes meets the limit (five a minute).
    let mut decisions = Vec::new();
    for code in [
        "AAAA-0001",
        "AAAA-0002",
        "AAAA-0003",
        "AAAA-0004",
        "AAAA-0005",
        "AAAA-0006",
    ] {
        let out = camera(
            &app,
            &format!("https://www.youtube.com/activate?user_code={code}"),
        );
        decisions.push(out.decision.clone());
        if out.decision == "refused" {
            assert!(out.reason.unwrap().to_lowercase().contains("too many"));
        }
    }
    assert_eq!(decisions.iter().filter(|d| *d == "approve").count(), 5);
    assert_eq!(decisions.last().map(String::as_str), Some("refused"));

    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    let out = camera(&app, TV);
    let id = out.activation_id.unwrap();
    app.lock().unwrap();
    assert_eq!(
        app.scan_text(TV, ScanSource::Camera).unwrap_err().code,
        ErrorCode::Locked
    );
    app.unlock().unwrap();
    assert!(app.activation_approve(&id, None, confirmed()).is_err());
}
