//! Sharing end to end through `App` against the real server
//! (`docs/plan/18-sharing.md`): people pinned through the key-transparency
//! log and their fingerprints, a vault shared by right (view, fill-only,
//! edit) with the core refusing what a right doesn't allow, a member
//! removed and the vault moved to new keys, an item shared on its own and
//! withdrawn, share links and request links, a household, and emergency
//! access with its wait, its stop and its release.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use lockdown_app::browser::{FillOutcome, FillWhat, PageFacts};
use lockdown_app::passkeys::{
    B64, CreateRequest, GetRequest, Outcome, PasskeyPage, UserVerification,
};
use lockdown_app::sharing::SendOptions;
use lockdown_app::{App, ErrorCode, FieldChange, GateInput, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";

struct Server {
    running: RunningServer,
    clock: Arc<AtomicI64>,
}

fn server() -> Server {
    let config = Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    let mut state = AppState::in_memory(config).unwrap();
    let clock = Arc::new(AtomicI64::new(lockdown_vault::Timestamp::now().0));
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    Server {
        running: run_in_background(Arc::new(state)).unwrap(),
        clock,
    }
}

fn person(dir: &std::path::Path, name: &str, server: &Server) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    app.account_register(
        &server.running.url(),
        &format!("{name}@example.com"),
        PASSWORD,
        None,
    )
    .unwrap();
    app.sync().unwrap();
    app
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

fn facts() -> PageFacts {
    PageFacts {
        top: "https://mail.example.com/login".into(),
        frame: "https://mail.example.com/login".into(),
        field_visible: true,
        user_confirmed: false,
    }
}

#[test]
fn a_vault_shared_by_right_with_fingerprints_rotation_and_fill_only() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let carol = person(dir.path(), "carol", &srv);
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-one".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    alice.sync().unwrap();
    let vault = alice.vaults().unwrap()[0].id.clone();

    // Bob looked up through the log; his fingerprint is the one he sees for himself.
    let b = alice.lookup_contact("bob@example.com").unwrap();
    assert!(!b.verified);
    let bob_own = bob.sharing_overview().unwrap().fingerprint;
    assert_eq!(b.fingerprint, bob_own);
    assert!(alice.lookup_contact("nobody@example.com").is_err());
    assert!(alice.verify_contact(&b.account_id, "12345 67890").is_err());
    assert!(
        alice
            .verify_contact(&b.account_id, &bob_own)
            .unwrap()
            .verified
    );
    alice.lookup_contact("carol@example.com").unwrap();

    // Sharing hands over the Vault Key, so it asks first.
    let asked = alice
        .share_vault(&vault, "bob@example.com", "view", GateInput::default())
        .unwrap_err();
    assert_eq!(asked.code, ErrorCode::ApprovalNeeded);
    let view = alice
        .share_vault(&vault, "bob@example.com", "view", pw())
        .unwrap();
    assert!(view.shared);
    assert_eq!(view.members.len(), 2);
    alice
        .share_vault(&vault, "carol@example.com", "fill-only", pw())
        .unwrap();

    // Bob reads and copies, but can't change anything.
    bob.sync().unwrap();
    let row = bob
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == mail)
        .expect("the shared login");
    assert_eq!(row.access.as_deref(), Some("view"));
    assert_eq!(
        bob.reveal(&mail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-one"
    );
    let badge = bob
        .vaults()
        .unwrap()
        .into_iter()
        .find(|v| v.id == vault)
        .and_then(|v| v.sharing)
        .unwrap();
    assert_eq!(
        (badge.kind.as_str(), badge.right.as_str()),
        ("shared", "view")
    );
    assert_eq!(badge.owner.as_deref(), Some("alice@example.com"));
    let edit = vec![
        FieldChange::keep("url.0"),
        FieldChange::text("user", "bob"),
        FieldChange::keep("pw"),
    ];
    assert!(
        bob.edit_item(&mail, None, edit.clone(), GateInput::default())
            .is_err()
    );
    assert!(bob.trash(&mail).is_err());
    assert!(
        bob.add_item_in(&vault, "note", Some("x"), false, Vec::new())
            .is_err()
    );

    // Carol may only fill: no reveal, no copy, nothing shown, no code view;
    // the browser fill still works.
    carol.sync().unwrap();
    let row = carol
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == mail)
        .unwrap();
    assert_eq!(row.access.as_deref(), Some("fill-only"));
    assert!(row.copy.is_none());
    assert!(carol.reveal(&mail, "pw", GateInput::default()).is_err());
    assert!(
        carol
            .reveal_to_use(&mail, "pw", GateInput::default())
            .is_err()
    );
    let shown = carol.show(&mail, true).unwrap();
    assert!(shown.fields.iter().all(|f| f.secret.is_none()));
    match carol.fill(&mail, FillWhat::Login, None, facts()).unwrap() {
        FillOutcome::Fill(values) => {
            let pw = values.iter().find(|v| v.role == "pw").unwrap();
            assert_eq!(pw.value.expose(), b"pw-one");
        }
        other => panic!("{other:?}"),
    }

    // Bob made an editor: his change reaches Alice.
    alice
        .set_member_right(&vault, &b.account_id, "edit")
        .unwrap();
    bob.sync().unwrap();
    bob.edit_item(&mail, None, edit, GateInput::default())
        .unwrap();
    bob.sync().unwrap();
    alice.sync().unwrap();
    assert_eq!(
        alice
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == mail)
            .unwrap()
            .username,
        "bob"
    );

    // Carol removed: the vault moves to new keys; she loses it, Bob keeps it.
    let carol_account = alice
        .vault_sharing(&vault)
        .unwrap()
        .members
        .into_iter()
        .find(|m| m.login == "carol@example.com")
        .unwrap()
        .account_id;
    let after = alice.remove_member(&vault, &carol_account).unwrap();
    assert_eq!(after.members.len(), 2);
    assert!(after.history.iter().any(|h| h.words.contains("new key")));
    carol.sync().unwrap();
    assert!(!carol.list().unwrap().iter().any(|r| r.id == mail));
    bob.sync().unwrap();
    assert_eq!(
        bob.reveal(&mail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-one"
    );
    let n = bob
        .edit_item(
            &mail,
            None,
            vec![
                FieldChange::keep("url.0"),
                FieldChange::text("user", "bob2"),
                FieldChange::keep("pw"),
            ],
            GateInput::default(),
        )
        .unwrap();
    assert!(n > 1);
    bob.sync().unwrap();
    alice.sync().unwrap();
    assert!(
        alice
            .list()
            .unwrap()
            .iter()
            .any(|r| r.id == mail && r.username == "bob2")
    );
}

#[test]
fn an_item_shared_on_its_own_to_fill_only_and_withdrawn() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-one".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    let other = alice
        .add_login(
            "https://bank.example.com",
            "alice",
            b"pw-two".to_vec(),
            Some("Bank"),
        )
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    assert!(
        alice
            .share_item(&mail, "bob@example.com", "edit", pw())
            .is_err()
    );
    let sent = alice
        .share_item(&mail, "bob@example.com", "fill-only", pw())
        .unwrap();
    bob.sync().unwrap();
    let rows = bob.list().unwrap();
    let row = rows.iter().find(|r| r.id == mail).expect("the shared item");
    assert_eq!(row.access.as_deref(), Some("fill-only"));
    assert!(!rows.iter().any(|r| r.id == other), "only that item");
    assert!(bob.reveal(&mail, "pw", GateInput::default()).is_err());
    // The extension's popup offers Copy only for a row's copy target, and
    // its copy goes through `reveal_to_use`: neither exists for fill-only.
    assert!(row.copy.is_none());
    assert!(
        bob.reveal_to_use(&mail, "pw", GateInput::default())
            .is_err()
    );
    assert!(matches!(
        bob.fill(&mail, FillWhat::Login, None, facts()).unwrap(),
        FillOutcome::Fill(_)
    ));
    let overview = bob.sharing_overview().unwrap();
    assert_eq!(overview.received.len(), 1);
    assert_eq!(overview.received[0].from, "alice@example.com");

    // Alice's edit reaches Bob's copy; then she withdraws it.
    alice
        .edit_item(
            &mail,
            None,
            vec![
                FieldChange::keep("url.0"),
                FieldChange::text("user", "alice2"),
                FieldChange::keep("pw"),
            ],
            GateInput::default(),
        )
        .unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();
    assert!(
        bob.list()
            .unwrap()
            .iter()
            .any(|r| r.id == mail && r.username == "alice2")
    );
    alice.unshare_item(&sent.share_id).unwrap();
    bob.sync().unwrap();
    assert!(!bob.list().unwrap().iter().any(|r| r.id == mail));
    // Alice still reads her item under its new key.
    assert_eq!(
        alice
            .reveal(&mail, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"pw-one"
    );
}

#[test]
fn share_links_and_request_links_between_two_people() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);

    // Text, with a password and two views.
    let link = alice
        .send_text(
            "Door code",
            b"4711".to_vec(),
            Some(b"blue".to_vec()),
            SendOptions {
                days: 2,
                max_views: Some(2),
            },
        )
        .unwrap();
    let link_text = String::from_utf8(link.expose().to_vec()).unwrap();
    assert!(link_text.contains("/s/") && link_text.contains('#'));
    assert!(
        bob.open_send(link.expose(), None).is_err(),
        "needs the password"
    );
    assert!(bob.open_send(link.expose(), Some(b"red".to_vec())).is_err());
    let opened = bob
        .open_send(link.expose(), Some(b"blue".to_vec()))
        .unwrap();
    assert_eq!(opened.kind, "text");
    assert_eq!(bob.opened_value(0).unwrap().expose(), b"4711");
    let saved = bob.save_opened().unwrap();
    assert!(bob.list().unwrap().iter().any(|r| r.id == saved));
    assert_eq!(alice.sends().unwrap()[0].views, 1);

    // An item's fields; a crown jewel never goes.
    let mail = alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw-one".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    let options = SendOptions {
        days: 1,
        max_views: None,
    };
    let uses = |app: &App| {
        app.list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == mail)
            .map(|r| r.uses)
            .unwrap_or_default()
    };
    let before = uses(&alice);
    let link = alice.send_item(&mail, None, options.clone(), pw()).unwrap();
    // Putting an item in a link is one use, as a copy is (D151).
    assert_eq!(uses(&alice), before + 1);
    let opened = bob.open_send(link.expose(), None).unwrap();
    assert_eq!(opened.title, "Mail");
    let pw_at = opened.values.iter().position(|v| v.secret).unwrap();
    assert_eq!(bob.opened_value(pw_at).unwrap().expose(), b"pw-one");
    let id = alice.sends().unwrap()[0].send_id.clone();
    for s in alice.sends().unwrap() {
        alice.delete_send(&s.send_id).unwrap();
    }
    assert!(alice.sends().unwrap().iter().all(|s| s.send_id != id));
    assert!(bob.open_send(link.expose(), None).is_err(), "deleted");

    // A request: Bob asks, Alice answers, only Bob opens it, once.
    let request = bob.create_request("the router password", 3).unwrap();
    assert_eq!(
        alice.request_label(request.expose()).unwrap(),
        "the router password"
    );
    alice
        .answer_request(request.expose(), "Router", b"hunter2".to_vec())
        .unwrap();
    assert!(
        alice
            .answer_request(request.expose(), "Router", b"again".to_vec())
            .is_err()
    );
    let mine = bob.requests().unwrap();
    assert!(mine[0].answered_at.is_some());
    let answer = bob.open_answer(&mine[0].request_id).unwrap();
    assert_eq!(answer.title, "Router");
    assert_eq!(bob.opened_value(0).unwrap().expose(), b"hunter2");
    assert!(alice.open_answer(&mine[0].request_id).is_err());
    bob.delete_request(&mine[0].request_id).unwrap();
}

#[test]
fn a_household_and_emergency_access_with_its_wait_stop_and_release() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let personal = alice.vaults().unwrap()[0].id.clone();
    let wifi = alice
        .add_item_in(
            &personal,
            "wi-fi",
            None,
            false,
            vec![FieldChange::text("wifi.ssid", "home")],
        )
        .unwrap();

    // A household: Alice organizes it, Bob joins as an editor.
    let home = alice.create_household("Home").unwrap();
    let b = alice.lookup_contact("bob@example.com").unwrap();
    alice
        .add_to_household(&home, "bob@example.com", "edit", pw())
        .unwrap();
    bob.sync().unwrap();
    let households = bob.sharing_overview().unwrap().households;
    assert_eq!(households.len(), 1);
    assert_eq!(households[0].name, "Home");
    assert_eq!(households[0].organizer, "alice@example.com");
    assert!(!households[0].organizing);
    let note = bob
        .add_item_in(
            &home,
            "note",
            Some("Bins"),
            false,
            vec![FieldChange::text("body", "Tuesday")],
        )
        .unwrap();
    bob.sync().unwrap();
    alice.sync().unwrap();
    assert!(alice.list().unwrap().iter().any(|r| r.id == note));

    // An emergency contact's fingerprint must be compared first.
    assert!(
        alice
            .add_emergency_contact("bob@example.com", 2, vec![personal.clone()], pw())
            .is_err()
    );
    let bob_own = bob.sharing_overview().unwrap().fingerprint;
    alice.verify_contact(&b.account_id, &bob_own).unwrap();
    let grants = alice
        .add_emergency_contact("bob@example.com", 2, vec![personal.clone()], pw())
        .unwrap();
    assert_eq!(grants[0].state, "invited");
    let grant = grants[0].grant_id.clone();

    bob.emergency_step(&grant, "accept").unwrap();
    let mine = bob.emergency_step(&grant, "request").unwrap();
    assert_eq!(mine[0].state, "requested");
    assert!(
        bob.emergency_open(&grant).is_err(),
        "the wait hasn't run out"
    );
    let theirs = alice.sharing_overview().unwrap();
    assert_eq!(theirs.waiting_on_you, 1);
    alice.emergency_step(&grant, "stop").unwrap();
    bob.emergency_step(&grant, "request").unwrap();
    srv.clock.fetch_add(3 * 86_400, Ordering::SeqCst);
    let opened = bob.emergency_open(&grant).unwrap();
    assert_eq!(opened[0].state, "released");
    let row = bob
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == wifi)
        .expect("released to read");
    assert_eq!(row.access.as_deref(), Some("view"));
    assert!(bob.trash(&wifi).is_err());
    let history = &alice.emergency().unwrap()[0].history;
    let words: Vec<&str> = history.iter().map(|h| h.words.as_str()).collect();
    assert!(words.iter().any(|w| w.contains("stopped")), "{words:?}");
    assert!(words.iter().any(|w| w.contains("released")), "{words:?}");

    // Revoked: the vault closes for Bob at his next sync.
    alice.emergency_step(&grant, "revoke").unwrap();
    bob.sync().unwrap();
    assert!(!bob.list().unwrap().iter().any(|r| r.id == wifi));
}

fn passkey_page(url: &str, confirmed: bool) -> PasskeyPage {
    PasskeyPage {
        top: url.into(),
        frame: url.into(),
        visible: true,
        user_confirmed: confirmed,
        related: None,
    }
}

/// A passkey on a fill-only item signs in (a passkey only ever "fills"),
/// but its key is never revealed, and the member can't delete it.
#[test]
fn a_fill_only_passkey_signs_in_but_is_never_revealed_or_changed() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let alice = person(dir.path(), "alice", &srv);
    let bob = person(dir.path(), "bob", &srv);
    let url = "https://shop.example.com/login";
    let req = CreateRequest {
        rp_id: None,
        rp_name: "Shop".into(),
        user_id: B64(b"alice-handle".to_vec()),
        user_name: "alice".into(),
        user_display_name: "Alice".into(),
        challenge: B64(b"create-challenge".to_vec()),
        algs: vec![-7],
        user_verification: UserVerification::Discouraged,
        ..CreateRequest::default()
    };
    assert!(matches!(
        alice
            .passkey_create(passkey_page(url, false), req.clone(), None)
            .unwrap(),
        Outcome::Confirm(_)
    ));
    let Outcome::Done(created) = alice
        .passkey_create(passkey_page(url, true), req, None)
        .unwrap()
    else {
        panic!("the passkey wasn't made");
    };
    alice.sync().unwrap();
    alice.lookup_contact("bob@example.com").unwrap();
    alice
        .share_item(&created.item_id, "bob@example.com", "fill-only", pw())
        .unwrap();
    bob.sync().unwrap();
    let get = GetRequest {
        rp_id: None,
        challenge: B64(b"get-challenge".to_vec()),
        ..GetRequest::default()
    };
    let signed = bob
        .passkey_get(
            passkey_page(url, true),
            get,
            &created.item_id,
            created.credential_id.clone(),
            None,
        )
        .unwrap();
    assert!(
        matches!(signed, Outcome::Done(ref a) if a.credential_id == created.credential_id),
        "a fill-only passkey still signs in: {signed:?}"
    );
    assert!(
        bob.reveal(&created.item_id, "passkey.0.key", GateInput::default())
            .is_err()
    );
    assert!(
        bob.passkey_delete(&created.item_id, created.credential_id.clone(), pw())
            .is_err(),
        "fill only can't change the item"
    );
}
