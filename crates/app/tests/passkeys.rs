//! Passkeys made and used through the core (docs/plan/19-passkeys.md),
//! checked the way a relying party checks them: the attestation object is
//! parsed, the COSE key read out, and every assertion verified with it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use lockdown_app::browser::{FillWhat, PageFacts};
use lockdown_app::passkeys::{
    Asserted, B64, CreateRequest, Created, GetRequest, Outcome, PasskeyPage, Signal,
    UserVerification,
};
use lockdown_app::{App, ApprovalKind, Approver, GateInput, LockPolicy, Paths};
use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};

fn fresh() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    (dir, app)
}

fn page(url: &str, confirmed: bool) -> PasskeyPage {
    PasskeyPage {
        top: url.into(),
        frame: url.into(),
        visible: true,
        user_confirmed: confirmed,
        related: None,
    }
}

fn create_req(rp: Option<&str>, user: &str, handle: &[u8]) -> CreateRequest {
    CreateRequest {
        rp_id: rp.map(str::to_string),
        rp_name: "Example".into(),
        user_id: B64(handle.to_vec()),
        user_name: user.into(),
        user_display_name: "Sean".into(),
        challenge: B64(b"create-challenge".to_vec()),
        algs: vec![-7, -257],
        user_verification: UserVerification::Preferred,
        cred_props: true,
        ..CreateRequest::default()
    }
}

fn get_req(rp: Option<&str>) -> GetRequest {
    GetRequest {
        rp_id: rp.map(str::to_string),
        challenge: B64(b"get-challenge".to_vec()),
        ..GetRequest::default()
    }
}

fn done<T: std::fmt::Debug>(o: Outcome<T>) -> T {
    match o {
        Outcome::Done(t) => t,
        other => panic!("expected done, got {other:?}"),
    }
}

fn refusal<T: std::fmt::Debug>(o: Outcome<T>) -> (String, String) {
    match o {
        Outcome::Refused(r) => (r.name.dom_name().to_string(), r.message),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Makes a passkey through both steps (the ask, then the deliberate action).
fn make(app: &App, url: &str, req: CreateRequest) -> Created {
    assert!(matches!(
        app.passkey_create(page(url, false), req.clone(), None)
            .unwrap(),
        Outcome::Confirm(_)
    ));
    done(app.passkey_create(page(url, true), req, None).unwrap())
}

/// Signs in through both steps.
fn sign_in(app: &App, url: &str, req: GetRequest, item: &str, cred: &B64) -> Asserted {
    assert!(matches!(
        app.passkey_get(page(url, false), req.clone(), item, cred.clone(), None).unwrap(),
        Outcome::Confirm(r) if r.contains("deliberate")
    ));
    done(
        app.passkey_get(page(url, true), req, item, cred.clone(), None)
            .unwrap(),
    )
}

/// The relying party reads the attestation: the credential id, the
/// uncompressed public point and the flag byte.
fn rp_registers(c: &Created, rp_id: &str) -> (Vec<u8>, Vec<u8>, u8) {
    let v: ciborium::Value = ciborium::from_reader(c.attestation_object.0.as_slice()).unwrap();
    let map = v.as_map().unwrap();
    assert_eq!(map[0].1.as_text(), Some("none"));
    let auth = map[2].1.as_bytes().unwrap().clone();
    assert_eq!(auth, c.authenticator_data.0);
    assert_eq!(
        &auth[..32],
        ring::digest::digest(&ring::digest::SHA256, rp_id.as_bytes()).as_ref()
    );
    assert_eq!(
        &auth[33..37],
        &[0, 0, 0, 0],
        "the sign count is always zero"
    );
    assert_eq!(&auth[37..53], &lockdown_passkey::LOCKDOWN_AAGUID);
    let len = u16::from_be_bytes([auth[53], auth[54]]) as usize;
    let cred = auth[55..55 + len].to_vec();
    let cose: ciborium::Value = ciborium::from_reader(&auth[55 + len..]).unwrap();
    let m = cose.as_map().unwrap();
    let coord = |k: i64| {
        m.iter()
            .find(|(a, _)| a.as_integer() == Some(k.into()))
            .and_then(|(_, b)| b.as_bytes().cloned())
            .unwrap()
    };
    let mut point = vec![4u8];
    point.extend(coord(-2));
    point.extend(coord(-3));
    (cred, point, auth[32])
}

/// The relying party verifies an assertion with the registered key.
fn rp_verifies(a: &Asserted, point: &[u8], rp_id: &str, origin: &str) -> u8 {
    let cd: serde_json::Value = serde_json::from_slice(&a.client_data_json.0).unwrap();
    assert_eq!(cd["type"], "webauthn.get");
    assert_eq!(cd["origin"], origin);
    assert_eq!(cd["crossOrigin"], false);
    assert_eq!(
        lockdown_passkey::encode::from_b64url(cd["challenge"].as_str().unwrap()).unwrap(),
        b"get-challenge"
    );
    let auth = &a.authenticator_data.0;
    assert_eq!(auth.len(), 37);
    assert_eq!(
        &auth[..32],
        ring::digest::digest(&ring::digest::SHA256, rp_id.as_bytes()).as_ref()
    );
    assert_eq!(&auth[33..37], &[0, 0, 0, 0]);
    let mut signed = auth.clone();
    signed.extend(ring::digest::digest(&ring::digest::SHA256, &a.client_data_json.0).as_ref());
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, point)
        .verify(&signed, &a.signature.0)
        .expect("the assertion verifies with the registered key");
    auth[32]
}

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const BE: u8 = 0x08;
const BS: u8 = 0x10;
const AT: u8 = 0x40;

#[test]
fn a_passkey_is_made_on_the_login_and_signs_in_as_the_rp_expects() {
    let (_dir, app) = fresh();
    let login = app
        .add_login(
            "https://example.com",
            "sean@example.com",
            b"pw".to_vec(),
            Some("Example"),
        )
        .unwrap();
    let url = "https://example.com/account/security";
    let plan = app
        .passkey_plan(
            page(url, false),
            create_req(None, "sean@example.com", b"user-1"),
        )
        .unwrap()
        .unwrap();
    assert_eq!(plan.item_id.as_deref(), Some(login.as_str()));
    assert_eq!(plan.title, "Example");
    assert!(!plan.excluded);

    let made = make(&app, url, create_req(None, "sean@example.com", b"user-1"));
    assert_eq!(
        made.item_id, login,
        "the passkey joins the login it belongs to"
    );
    assert_eq!(made.public_key_algorithm, -7);
    assert!(
        !made.user_verified,
        "preferred, with no platform check: presence only"
    );
    assert_eq!(made.resident_key, Some(true));
    let (cred, point, flags) = rp_registers(&made, "example.com");
    assert_eq!(cred, made.credential_id.0);
    assert_eq!(flags, UP | BE | BS | AT);
    assert_eq!(
        &made.public_key.0[26..],
        point.as_slice(),
        "getPublicKey() is the same key"
    );
    let cd: serde_json::Value = serde_json::from_slice(&made.client_data_json.0).unwrap();
    assert_eq!(cd["type"], "webauthn.create");
    assert_eq!(cd["origin"], "https://example.com");

    // The row now shows a passkey method; the key never comes back out.
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == login)
        .unwrap();
    assert_eq!(row.passkeys, 1);
    assert!(row.methods.contains(&"passkey".to_string()));
    assert!(row.views.contains(&"passkeys".to_string()));
    let err = app
        .reveal(&login, "passkey.0.key", GateInput::default())
        .unwrap_err();
    assert!(err.message.contains("never shown"), "{}", err.message);

    // Sign in: offered, then signed after the second deliberate action.
    let opts = app
        .passkey_options(page(url, false), get_req(None))
        .unwrap()
        .unwrap();
    assert_eq!(opts.rp_id, "example.com");
    assert_eq!(opts.options.len(), 1);
    assert_eq!(opts.options[0].user_name, "sean@example.com");
    let a = sign_in(&app, url, get_req(None), &login, &made.credential_id);
    assert_eq!(a.user_handle.0, b"user-1");
    assert_eq!(
        rp_verifies(&a, &point, "example.com", "https://example.com"),
        UP | BE | BS
    );

    // The management view.
    let view = &app.passkeys(&login).unwrap()[0];
    assert_eq!(view.provider, "Lockdown");
    assert_eq!(
        view.aaguid.as_deref(),
        Some("07017dfd-2717-4c97-92c2-b3d6513f46ce")
    );
    assert_eq!(view.algorithm, "ES256 (P-256)");
    assert!(view.backup_eligible && view.backed_up);
    assert!(view.created.is_some() && view.last_used.is_some());
    assert!(!view.only_sign_in, "the login still has its password");
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == login)
        .unwrap();
    assert_eq!(row.uses, 1, "a passkey sign-in is a use (D151)");
}

#[test]
fn where_a_new_passkey_goes_and_what_is_refused() {
    let (_dir, app) = fresh();
    let url = "https://shop.example.org/register";
    // No login fits: a new one for the page's origin.
    let made = make(&app, url, create_req(Some("example.org"), "newbie", b"u-1"));
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == made.item_id)
        .unwrap();
    assert_eq!(row.kind, "login");
    assert_eq!(row.title, "Example");
    assert_eq!(row.site, "https://shop.example.org");
    assert_eq!(row.username, "newbie");
    // The same user handle again replaces it in place (WebAuthn §6.3.2 step 7).
    let again = make(&app, url, create_req(Some("example.org"), "newbie", b"u-1"));
    assert_eq!(again.item_id, made.item_id);
    assert_ne!(again.credential_id, made.credential_id);
    assert_eq!(app.passkeys(&made.item_id).unwrap().len(), 1);
    // An excluded credential: InvalidStateError.
    let mut req = create_req(Some("example.org"), "newbie", b"u-2");
    req.exclude = vec![again.credential_id.clone()];
    let (name, _) = refusal(app.passkey_create(page(url, true), req, None).unwrap());
    assert_eq!(name, "InvalidStateError");
    // Another site's RP ID, a public suffix, a plain-http page, a frame.
    for (url, rp) in [
        ("https://shop.example.org/", Some("example.com")),
        ("https://shop.example.org/", Some("org")),
        ("http://shop.example.org/", None),
    ] {
        let (name, _) = refusal(
            app.passkey_create(page(url, true), create_req(rp, "x", b"u"), None)
                .unwrap(),
        );
        assert_eq!(name, "SecurityError", "{url} {rp:?}");
    }
    let framed = PasskeyPage {
        top: "https://news.test/".into(),
        ..page(url, true)
    };
    let (name, words) = refusal(
        app.passkey_create(framed, create_req(None, "x", b"u"), None)
            .unwrap(),
    );
    assert_eq!(name, "NotAllowedError");
    assert!(words.contains("not from a frame"));
    // Covered: nothing is made, even when confirmed.
    let covered = PasskeyPage {
        visible: false,
        ..page(url, true)
    };
    assert!(matches!(
        app.passkey_create(covered, create_req(None, "y", b"v"), None)
            .unwrap(),
        Outcome::Refused(_)
    ));
    // Only ES256 is made.
    let mut eddsa = create_req(None, "z", b"w");
    eddsa.algs = vec![-8];
    assert_eq!(
        refusal(app.passkey_create(page(url, true), eddsa, None).unwrap()).0,
        "NotSupportedError"
    );
    // A bad user handle.
    assert_eq!(
        refusal(
            app.passkey_create(page(url, true), create_req(None, "z", &[0; 65]), None)
                .unwrap()
        )
        .0,
        "TypeError"
    );
}

#[test]
fn a_crown_jewel_login_is_never_offered_to_a_page() {
    let (_dir, app) = fresh();
    let url = "https://bank.example/";
    let made = make(&app, url, create_req(None, "me", b"me"));
    let id = app
        .set_crown_jewel(
            &made.item_id,
            true,
            GateInput {
                password: None,
                confirmed: true,
            },
        )
        .unwrap();
    let opts = app
        .passkey_options(page(url, false), get_req(None))
        .unwrap()
        .unwrap();
    assert!(opts.options.is_empty());
    let (name, _) = refusal(
        app.passkey_get(
            page(url, true),
            get_req(None),
            &id,
            made.credential_id,
            None,
        )
        .unwrap(),
    );
    assert_eq!(name, "NotAllowedError");
}

#[test]
fn the_allow_list_and_the_rp_id_narrow_what_is_offered() {
    let (_dir, app) = fresh();
    let url = "https://login.example.com/";
    let a = make(&app, url, create_req(Some("example.com"), "a", b"a"));
    let b = make(&app, url, create_req(Some("example.com"), "b", b"b"));
    let all = app
        .passkey_options(page(url, false), get_req(Some("example.com")))
        .unwrap()
        .unwrap();
    assert_eq!(all.options.len(), 2);
    let mut only_b = get_req(Some("example.com"));
    only_b.allow = vec![b.credential_id.clone()];
    let opts = app
        .passkey_options(page(url, false), only_b.clone())
        .unwrap()
        .unwrap();
    assert_eq!(opts.options.len(), 1);
    assert_eq!(opts.options[0].credential_id, b.credential_id);
    // A passkey outside the allow list isn't signed with.
    assert_eq!(
        refusal(
            app.passkey_get(
                page(url, true),
                only_b,
                &a.item_id,
                a.credential_id.clone(),
                None
            )
            .unwrap()
        )
        .0,
        "NotAllowedError"
    );
    // The host's own RP ID is another relying party: nothing for it.
    let host_only = app
        .passkey_options(page(url, false), get_req(None))
        .unwrap()
        .unwrap();
    assert_eq!(host_only.rp_id, "login.example.com");
    assert!(host_only.options.is_empty());
}

/// A platform check that says yes or no and counts its prompts.
struct Platform {
    yes: bool,
    asked: Arc<AtomicU32>,
}

impl Approver for Platform {
    fn confirm(&self, kind: ApprovalKind, _reason: &str) -> Result<bool, String> {
        assert_eq!(kind, ApprovalKind::Approve);
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(self.yes)
    }

    fn method(&self) -> &'static str {
        "Touch ID"
    }
}

#[test]
fn user_verification_runs_the_approve_gate_and_sets_uv_only_when_a_check_ran() {
    // No platform check, no account: required is refused, never faked.
    let (_dir, app) = fresh();
    let url = "https://example.com/";
    let mut req = create_req(None, "me", b"me");
    req.user_verification = UserVerification::Required;
    let (name, words) = refusal(app.passkey_create(page(url, true), req, None).unwrap());
    assert_eq!(name, "NotAllowedError");
    assert!(words.contains("check it's you"), "{words}");
    // The shell approved it in its own window with the password: UV.
    let mut req = create_req(None, "me", b"me");
    req.user_verification = UserVerification::Required;
    let made = done(
        app.passkey_create(page(url, true), req, Some("password".into()))
            .unwrap(),
    );
    assert!(made.user_verified);
    assert_eq!(rp_registers(&made, "example.com").2 & UV, UV);

    // With a platform check: preferred asks it; discouraged doesn't.
    let (_dir2, app) = fresh();
    let asked = Arc::new(AtomicU32::new(0));
    app.set_approver(Box::new(Platform {
        yes: true,
        asked: asked.clone(),
    }))
    .unwrap();
    let made = make(&app, url, create_req(None, "me", b"me"));
    assert!(made.user_verified);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    let mut get = get_req(None);
    get.user_verification = UserVerification::Discouraged;
    let a = sign_in(&app, url, get, &made.item_id, &made.credential_id);
    assert!(!a.user_verified);
    assert_eq!(asked.load(Ordering::SeqCst), 1, "discouraged: no prompt");
    let mut get = get_req(None);
    get.user_verification = UserVerification::Required;
    let a = sign_in(&app, url, get, &made.item_id, &made.credential_id);
    assert!(a.user_verified);
    assert_eq!(a.authenticator_data.0[32] & UV, UV);

    // The platform check says no: NotAllowedError, nothing signed.
    let (_dir3, app) = fresh();
    app.set_approver(Box::new(Platform {
        yes: false,
        asked: Arc::new(AtomicU32::new(0)),
    }))
    .unwrap();
    let (name, words) = refusal(
        app.passkey_create(page(url, true), create_req(None, "me", b"me"), None)
            .unwrap(),
    );
    assert_eq!(name, "NotAllowedError");
    assert!(words.contains("Touch ID"), "{words}");
    assert!(app.list().unwrap().is_empty(), "nothing was saved");
}

#[test]
fn a_passkey_made_on_one_device_signs_in_on_another_after_a_sync() {
    use lockdown_server::{AppState, Config, run_in_background};
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state).unwrap();
    let url = running.url();
    let device = |name: &str| {
        let paths =
            Paths::resolve(Some(dir.path().join(name).join("snds/identity")), None).unwrap();
        App::start(paths, None, LockPolicy::default())
    };
    let a = device("a");
    a.open_or_create(true).unwrap();
    let registered = a
        .account_register(&url, "me@example.com", "correct horse battery staple", None)
        .unwrap();
    let kit = String::from_utf8(registered.kit.unwrap().expose().to_vec()).unwrap();
    let secret_key = kit
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .unwrap();
    let site = "https://example.com/";
    let made = make(&a, site, create_req(None, "me", b"me"));
    let point = rp_registers(&made, "example.com").1;
    a.sync().unwrap();

    let b = device("b");
    b.account_login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        Some(&secret_key),
    )
    .unwrap();
    b.open_or_create(true).unwrap();
    let opts = b
        .passkey_options(page(site, false), get_req(None))
        .unwrap()
        .unwrap();
    assert_eq!(
        opts.options.len(),
        1,
        "the passkey came over with its login"
    );
    let o = &opts.options[0];
    assert_eq!(o.credential_id, made.credential_id);
    let assertion = sign_in(&b, site, get_req(None), &o.item_id, &o.credential_id);
    assert_eq!(
        rp_verifies(&assertion, &point, "example.com", "https://example.com"),
        UP | BE | BS,
        "the same key signs on the second device, and the count stays zero on both"
    );
}

#[test]
fn signals_hide_with_undo_and_rename_but_never_delete() {
    let (_dir, app) = fresh();
    let url = "https://example.com/";
    let one = make(&app, url, create_req(None, "me", b"me"));
    let two = make(&app, url, create_req(None, "me-too", b"me-too"));
    let page_url = "https://example.com/settings";

    // signalUnknownCredential: hidden, not offered, flagged; undo brings it back.
    let changed = app
        .passkey_signal(
            page_url,
            Signal::UnknownCredential {
                rp_id: "example.com".into(),
                credential_id: one.credential_id.clone(),
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(changed, 1);
    let opts = app
        .passkey_options(page(url, false), get_req(None))
        .unwrap()
        .unwrap();
    assert_eq!(opts.options.len(), 1);
    assert_eq!(opts.options[0].credential_id, two.credential_id);
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == one.item_id)
        .unwrap();
    assert!(row.warnings.contains(&"passkey-hidden-by-site".to_string()));
    assert!(row.views.contains(&"attention".to_string()));
    let view = &app.passkeys(&one.item_id).unwrap()[0];
    assert!(
        view.hidden
            .as_ref()
            .unwrap()
            .reason
            .contains("no longer knows")
    );
    assert_eq!(
        app.passkeys(&one.item_id).unwrap().len(),
        1,
        "hidden, not deleted"
    );
    app.passkey_restore(&one.item_id, one.credential_id.clone())
        .unwrap();
    assert_eq!(
        app.passkey_options(page(url, false), get_req(None))
            .unwrap()
            .unwrap()
            .options
            .len(),
        2
    );

    // signalAllAcceptedCredentials: unlisted ones hide; listed again, they return.
    let accepted = |ids: Vec<B64>| Signal::AllAcceptedCredentials {
        rp_id: "example.com".into(),
        user_id: B64(b"me".to_vec()),
        all_accepted_credential_ids: ids,
    };
    assert_eq!(
        app.passkey_signal(page_url, accepted(vec![]))
            .unwrap()
            .unwrap(),
        1
    );
    assert!(app.passkeys(&one.item_id).unwrap()[0].hidden.is_some());
    assert!(
        app.passkeys(&two.item_id).unwrap()[0].hidden.is_none(),
        "another user's passkey is untouched"
    );
    assert_eq!(
        app.passkey_signal(page_url, accepted(vec![one.credential_id.clone()]))
            .unwrap()
            .unwrap(),
        1
    );
    assert!(app.passkeys(&one.item_id).unwrap()[0].hidden.is_none());

    // signalCurrentUserDetails: the names change in a new version.
    let details = Signal::CurrentUserDetails {
        rp_id: "example.com".into(),
        user_id: B64(b"me".to_vec()),
        name: "renamed@example.com".into(),
        display_name: "Renamed".into(),
    };
    assert_eq!(
        app.passkey_signal(page_url, details.clone())
            .unwrap()
            .unwrap(),
        1
    );
    assert_eq!(
        app.passkeys(&one.item_id).unwrap()[0].user_name,
        "renamed@example.com"
    );
    assert_eq!(
        app.passkey_signal(page_url, details).unwrap().unwrap(),
        0,
        "nothing new"
    );

    // Another site can't signal about example.com's passkeys.
    let r = app
        .passkey_signal(
            "https://evil.example/",
            Signal::UnknownCredential {
                rp_id: "example.com".into(),
                credential_id: two.credential_id.clone(),
            },
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(r.name.dom_name(), "SecurityError");
    assert!(app.passkeys(&two.item_id).unwrap()[0].hidden.is_none());
}

#[test]
fn deleting_warns_when_it_is_the_only_way_in() {
    let (_dir, app) = fresh();
    let url = "https://example.net/";
    let made = make(&app, url, create_req(None, "solo", b"solo"));
    let view = &app.passkeys(&made.item_id).unwrap()[0];
    assert!(view.only_sign_in, "a passkey-only login");
    app.passkey_delete(
        &made.item_id,
        made.credential_id.clone(),
        GateInput::default(),
    )
    .unwrap();
    assert!(app.passkeys(&made.item_id).unwrap().is_empty());
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == made.item_id)
        .unwrap();
    assert_eq!(row.passkeys, 0);
    // History keeps the version that held it.
    assert!(app.history(&made.item_id).unwrap().len() >= 3);
}

#[test]
fn the_upgrade_after_a_password_sign_in_needs_lockdowns_own_recent_fill() {
    let (_dir, app) = fresh();
    let login = app
        .add_login("https://example.com", "me", b"pw".to_vec(), None)
        .unwrap();
    let url = "https://example.com/welcome";
    let mut req = create_req(None, "me", b"me");
    req.conditional = true;
    // Nothing filled yet: refused without a word to the page beyond NotAllowed.
    assert_eq!(
        refusal(
            app.passkey_create(page(url, false), req.clone(), None)
                .unwrap()
        )
        .0,
        "NotAllowedError"
    );
    // Lockdown fills the password here; the upgrade then goes through, with
    // no prompt: UP and UV clear.
    let facts = PageFacts {
        top: "https://example.com/login".into(),
        frame: "https://example.com/login".into(),
        field_visible: true,
        user_confirmed: false,
    };
    app.fill(&login, FillWhat::Login, None, facts).unwrap();
    let made = done(
        app.passkey_create(page(url, false), req.clone(), None)
            .unwrap(),
    );
    assert_eq!(made.item_id, login);
    assert_eq!(rp_registers(&made, "example.com").2, BE | BS | AT);
    // Another user name on that site: no.
    let mut other = req;
    other.user_name = "someone-else".into();
    other.user_id = B64(b"else".to_vec());
    assert!(matches!(
        app.passkey_create(page(url, false), other, None).unwrap(),
        Outcome::Refused(_)
    ));
    // A lock forgets the fill.
    app.lock().unwrap();
    app.unlock().unwrap();
    let mut again = create_req(None, "me", b"me-2");
    again.conditional = true;
    assert!(matches!(
        app.passkey_create(page(url, false), again, None).unwrap(),
        Outcome::Refused(_)
    ));
}

#[test]
fn a_made_passkey_survives_a_cxf_export_and_import_and_still_signs() {
    let (_dir, app) = fresh();
    let url = "https://example.com/";
    let made = make(&app, url, create_req(None, "me", b"me"));
    let point = rp_registers(&made, "example.com").1;
    let file = app
        .export_items(
            "cxf",
            GateInput {
                password: None,
                confirmed: true,
            },
        )
        .unwrap();
    let (_dir2, other) = fresh();
    other.import_items(file.expose().to_vec(), "cxf").unwrap();
    let opts = other
        .passkey_options(page(url, false), get_req(None))
        .unwrap()
        .unwrap();
    assert_eq!(opts.options.len(), 1);
    let o = &opts.options[0];
    assert_eq!(o.credential_id, made.credential_id);
    let a = sign_in(&other, url, get_req(None), &o.item_id, &o.credential_id);
    assert_eq!(a.user_handle.0, b"me");
    rp_verifies(&a, &point, "example.com", "https://example.com");
    // Lockdown's own CXF carries the provider id, the dates and the flags in
    // its item extension, so they come back too.
    let view = &other.passkeys(&o.item_id).unwrap()[0];
    let before = &app.passkeys(&made.item_id).unwrap()[0];
    assert_eq!(view.provider, "Lockdown");
    assert_eq!(view.created, before.created);
    assert!(view.backup_eligible && view.backed_up);
}
