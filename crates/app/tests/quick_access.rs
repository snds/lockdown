//! Quick access through `App` (docs/plan/26-desktop-finish.md §1): the
//! shared index ranks, forgives a typo, never finds a secret, follows
//! every change, and is gone on lock; its code and fill actions keep the
//! gates and count as uses.

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

fn ids(app: &App, q: &str) -> Vec<String> {
    app.search(q, 50)
        .unwrap()
        .into_iter()
        .map(|h| h.item.id)
        .collect()
}

#[test]
fn one_index_ranks_forgives_typos_and_never_holds_a_secret() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    let github = app
        .add_login(
            "https://github.com",
            "sean",
            b"correct-horse-battery".to_vec(),
            Some("GitHub"),
        )
        .unwrap();
    let mail = app
        .add_login(
            "https://mail.example.com",
            "sean@example.com",
            b"pw-mail".to_vec(),
            Some("Mail"),
        )
        .unwrap();
    let bank = app
        .add_login(
            "https://bank.example",
            "owner",
            b"pw-bank".to_vec(),
            Some("Bank"),
        )
        .unwrap();

    // Every word in any order; the whole host and its parts.
    assert_eq!(ids(&app, "sean git"), std::slice::from_ref(&github));
    assert_eq!(ids(&app, "github.com"), std::slice::from_ref(&github));
    // A slip is forgiven, and says so.
    let hits = app.search("gtihub", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item.id, github);
    assert!(!hits[0].exact);
    // A password is never a search word.
    assert!(ids(&app, "correct-horse-battery").is_empty());
    assert!(ids(&app, "horse").is_empty());

    // The index follows a change: a new title is found, the old one isn't.
    app.edit_login(&mail, Some("Post office"), None, None, GateInput::default())
        .unwrap();
    assert_eq!(ids(&app, "post"), std::slice::from_ref(&mail));
    assert!(ids(&app, "mail").contains(&mail), "still by its host");

    // A crown jewel by title and kind only (D113).
    let bank = app.set_crown_jewel(&bank, true, confirmed()).unwrap();
    assert_eq!(ids(&app, "bank"), std::slice::from_ref(&bank));
    assert!(ids(&app, "owner").is_empty());

    // Trashed items leave search.
    app.trash(&mail).unwrap();
    assert!(ids(&app, "post").is_empty());

    // An empty search lists everything live, a favourite first.
    app.organize(&github, None, None, Some(true)).unwrap();
    let all = ids(&app, "");
    assert_eq!(all.first(), Some(&github));
    assert_eq!(all.len(), 2);

    // Locked, there is no index.
    app.lock().unwrap();
    assert_eq!(app.search("git", 5).unwrap_err().code, ErrorCode::Locked);
}

#[test]
fn quick_codes_and_fills_keep_their_gates_and_count_as_uses() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path());
    let code = app
        .add_code(
            "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example",
            Some("Example code"),
            None,
        )
        .unwrap();
    let field = app
        .codes()
        .unwrap()
        .into_iter()
        .find(|c| c.item_id == code)
        .unwrap()
        .field_id;
    let shown = app.quick_code(&code, &field, GateInput::default()).unwrap();
    assert_eq!(shown.expose().len(), 6);
    assert!(shown.expose().iter().all(u8::is_ascii_digit));
    let row = app
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.id == code)
        .unwrap();
    assert_eq!(row.uses, 1, "a code shown is a use (D151)");

    // Two logins for one site: the one quick access asked for comes first.
    let first = app
        .add_login(
            "https://example.com/login",
            "a",
            b"pw-a".to_vec(),
            Some("Alpha"),
        )
        .unwrap();
    let second = app
        .add_login(
            "https://example.com/login",
            "b",
            b"pw-b".to_vec(),
            Some("Beta"),
        )
        .unwrap();
    let started = app.quick_fill(&second).unwrap();
    assert_eq!(started.url, "https://example.com/login");
    assert_eq!(started.host, "example.com");
    let m = app.page_matches("https://example.com/login").unwrap();
    assert_eq!(m.logins[0].item_id, second);
    assert!(m.logins[0].asked);
    assert!(m.logins.iter().any(|l| l.item_id == first && !l.asked));
    // Elsewhere it changes nothing.
    let other = app.page_matches("https://elsewhere.example/").unwrap();
    assert!(other.logins.is_empty());

    // Never a crown jewel, never an item kept to the app, never a non-login.
    app.set_exposure(&first, Some("app-only"), confirmed())
        .unwrap();
    let e = app.quick_fill(&first).unwrap_err();
    assert!(e.message.contains("kept to the app"), "{}", e.message);
    assert!(app.quick_fill(&code).is_err());
    let jewel = app.set_crown_jewel(&second, true, confirmed()).unwrap();
    let e = app.quick_fill(&jewel).unwrap_err();
    assert!(e.message.contains("protected"), "{}", e.message);

    // A lock forgets the request.
    let third = app
        .add_login("https://shop.example/", "c", b"pw-c".to_vec(), Some("Shop"))
        .unwrap();
    app.quick_fill(&third).unwrap();
    app.lock().unwrap();
    app.unlock().unwrap();
    let m = app.page_matches("https://shop.example/").unwrap();
    assert!(!m.logins[0].asked);
}
