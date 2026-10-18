//! Per-item exposure tiers through `App` (docs/plan/22-security-depth.md
//! §1): a mailbox login starts at "ask each time" and every value out of it
//! asks; an everyday login fills freely until the person keeps it to the
//! app, after which the browser never hears of it; letting an item ask less
//! goes through the `Reveal` gate; a crown jewel never fills freely and
//! keeps its own gates.

use lockdown_app::browser::{FillOutcome, FillWhat, PageFacts};
use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};

fn fresh() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    (dir, app)
}

fn facts(page: &str) -> PageFacts {
    PageFacts {
        top: page.into(),
        frame: page.into(),
        field_visible: true,
        user_confirmed: false,
    }
}

/// With no account and no platform check, a gate is the person's yes in
/// the app's window (D101): enough to tell asked from not asked.
fn yes() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

#[test]
fn a_mailbox_login_asks_for_every_value_and_an_everyday_one_does_not() {
    let (_dir, app) = fresh();
    let gmail = app
        .add_login(
            "https://accounts.google.com",
            "me@gmail.com",
            b"mail-pw".to_vec(),
            Some("Google"),
        )
        .unwrap();
    let shop = app
        .add_login(
            "https://shop.example",
            "me",
            b"shop-pw".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    let g = app.exposure(&gmail).unwrap();
    assert_eq!((g.tier.as_str(), g.source.as_str()), ("ask", "high-risk"));
    assert!(g.reason.unwrap().contains("google.com"));
    assert_eq!(app.exposure(&shop).unwrap().tier, "fill");

    // Ask each time: a reveal, a copy and a browser fill all ask.
    let asked = app.reveal(&gmail, "pw", GateInput::default()).unwrap_err();
    assert_eq!(asked.code, ErrorCode::ApprovalNeeded);
    assert!(
        app.reveal_to_use(&gmail, "pw", GateInput::default())
            .is_err()
    );
    assert_eq!(
        app.reveal(&gmail, "pw", yes()).unwrap().expose(),
        b"mail-pw"
    );
    let fill = app.fill(
        &gmail,
        FillWhat::Login,
        None,
        facts("https://accounts.google.com/signin"),
    );
    assert_eq!(fill.unwrap_err().code, ErrorCode::ApprovalNeeded);
    // The everyday login asks nothing.
    assert_eq!(
        app.reveal(&shop, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"shop-pw"
    );
    assert!(matches!(
        app.fill(
            &shop,
            FillWhat::Login,
            None,
            facts("https://shop.example/login")
        )
        .unwrap(),
        FillOutcome::Fill(_)
    ));
    // Only the items off the everyday level are badged.
    let badges = app.exposures().unwrap();
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].id, gmail);
}

#[test]
fn an_item_kept_to_the_app_never_reaches_the_browser() {
    let (_dir, app) = fresh();
    let shop = app
        .add_login(
            "https://shop.example",
            "me",
            b"shop-pw".to_vec(),
            Some("Shop"),
        )
        .unwrap();
    assert_eq!(
        app.page_matches("https://shop.example/login")
            .unwrap()
            .logins
            .len(),
        1
    );
    // Keeping it to the app asks more, so it asks nothing.
    let kept = app
        .set_exposure(&shop, Some("app-only"), GateInput::default())
        .unwrap();
    assert_eq!(
        (kept.tier.as_str(), kept.source.as_str()),
        ("app-only", "chosen")
    );
    assert!(
        app.page_matches("https://shop.example/login")
            .unwrap()
            .logins
            .is_empty()
    );
    match app
        .fill(
            &shop,
            FillWhat::Login,
            None,
            facts("https://shop.example/login"),
        )
        .unwrap()
    {
        FillOutcome::Refuse(why) => assert!(why.contains("kept to the Lockdown app"), "{why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    // In the app it is as before.
    assert_eq!(
        app.reveal(&shop, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"shop-pw"
    );
    // The choice syncs as part of the item: a new version, kept by an edit.
    let v = app.history(&shop).unwrap().len();
    assert!(v >= 2);
    app.edit_login(
        &shop,
        Some("Shop, renamed"),
        None,
        None,
        GateInput::default(),
    )
    .unwrap();
    assert_eq!(app.exposure(&shop).unwrap().tier, "app-only");

    // Back to filling freely asks less, so it goes through the gate.
    let gated = app
        .set_exposure(&shop, Some("fill"), GateInput::default())
        .unwrap_err();
    assert_eq!(gated.code, ErrorCode::ApprovalNeeded);
    assert_eq!(app.set_exposure(&shop, None, yes()).unwrap().tier, "fill");
    assert_eq!(
        app.page_matches("https://shop.example/login")
            .unwrap()
            .logins
            .len(),
        1
    );
}

#[test]
fn a_crown_jewel_never_fills_freely_and_a_high_risk_default_can_be_lowered_with_the_gate() {
    let (_dir, app) = fresh();
    let vault_login = app
        .add_item(
            "login",
            Some("Bank"),
            true,
            vec![
                FieldChange::text("user", "me"),
                FieldChange::secret("pw", b"bank-pw"),
                FieldChange::text("url.0", "https://bank.example"),
            ],
        )
        .unwrap();
    let e = app.exposure(&vault_login).unwrap();
    assert_eq!((e.tier.as_str(), e.source.as_str()), ("ask", "crown-jewel"));
    assert!(!e.choices.contains(&"fill".to_string()));
    assert!(app.set_exposure(&vault_login, Some("fill"), yes()).is_err());
    assert_eq!(
        app.set_exposure(&vault_login, Some("app-only"), GateInput::default())
            .unwrap()
            .tier,
        "app-only"
    );

    let gmail = app
        .add_login(
            "https://mail.google.com",
            "me@gmail.com",
            b"mail-pw".to_vec(),
            None,
        )
        .unwrap();
    assert_eq!(
        app.set_exposure(&gmail, Some("fill"), GateInput::default())
            .unwrap_err()
            .code,
        ErrorCode::ApprovalNeeded
    );
    let lowered = app.set_exposure(&gmail, Some("fill"), yes()).unwrap();
    assert_eq!(
        (lowered.tier.as_str(), lowered.source.as_str()),
        ("fill", "chosen")
    );
    assert_eq!(
        app.reveal(&gmail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"mail-pw"
    );
    // An SSH key never reaches the browser and has no level to choose.
    assert!(
        app.set_exposure(&vault_login, Some("bogus"), yes())
            .is_err()
    );
}
