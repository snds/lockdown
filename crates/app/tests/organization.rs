//! Organization end to end through `App` (docs/plan/15-organization.md):
//! several vaults, folders, tags, favourites, the archive, moves between
//! vaults and into and out of the crown-jewel class, the duplicate finder
//! and its merge, history and restore, attachments, expiry reminders, and
//! the same state reaching a second device through the real server.

use std::sync::Arc;

use lockdown_app::{App, ErrorCode, FieldChange, GateInput, ItemSummary, LockPolicy, Paths};
use lockdown_server::{AppState, Config, run_in_background};

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn row<'a>(rows: &'a [ItemSummary], id: &str) -> &'a ItemSummary {
    rows.iter().find(|r| r.id == id).expect("the row")
}

fn confirmed() -> GateInput {
    GateInput {
        password: None,
        confirmed: true,
    }
}

#[test]
fn vaults_folders_tags_favourites_and_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path(), "a");
    app.open_or_create(true).unwrap();
    let vaults = app.vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    assert!(vaults[0].primary);
    let personal = vaults[0].id.clone();

    // A second vault, its own key branch, named; and an item made in it.
    let family = app.create_vault("Family").unwrap();
    assert!(app.create_vault("  ").is_err());
    let wifi = app
        .add_item_in(
            &family,
            "wi-fi",
            None,
            false,
            vec![FieldChange::text("wifi.ssid", "home")],
        )
        .unwrap();
    let mail = app
        .add_login("https://mail.example.com", "me", b"pw-one".to_vec(), None)
        .unwrap();
    let rows = app.list().unwrap();
    assert_eq!(row(&rows, &wifi).vault_id, family);
    assert_eq!(row(&rows, &mail).vault_id, personal);
    let vaults = app.vaults().unwrap();
    assert_eq!(vaults.len(), 2);
    assert_eq!(vaults[1].name, "Family");
    assert_eq!(vaults[1].items, 1);

    // Folders nest; filing, tags and a favourite land on the item.
    let work = app.add_folder(&personal, "Work", None).unwrap();
    let clients = app.add_folder(&personal, "Clients", Some(&work)).unwrap();
    assert!(
        app.add_folder(&family, "x", Some(&work)).is_err(),
        "another vault's folder"
    );
    app.organize(
        &mail,
        Some(Some(clients.clone())),
        Some(vec!["work/email".into(), "daily".into()]),
        Some(true),
    )
    .unwrap();
    assert!(
        app.organize(&wifi, Some(Some(clients.clone())), None, None)
            .is_err(),
        "a folder of another vault"
    );
    let rows = app.list().unwrap();
    let r = row(&rows, &mail);
    assert_eq!(r.folder.as_deref(), Some(clients.as_str()));
    assert_eq!(r.tags, ["daily", "work/email"]);
    assert!(r.favourite);
    assert!(r.views.iter().any(|v| v == "favourites"));
    let folders = &app.vaults().unwrap()[0].folders;
    assert_eq!(
        folders.iter().find(|f| f.id == clients).unwrap().path,
        "Work / Clients"
    );
    // The detail carries the organization, never as fields.
    let detail = app.show(&mail, false).unwrap();
    assert!(detail.place.organization.favourite);
    assert!(!detail.fields.iter().any(|f| f.field_id.starts_with("org.")));

    // A tag renamed everywhere, nested ones too; a folder removed moves its
    // items up.
    assert_eq!(app.rename_tag("work", Some("job")).unwrap(), 1);
    assert_eq!(
        app.list()
            .unwrap()
            .iter()
            .find(|r| r.id == mail)
            .unwrap()
            .tags,
        ["daily", "job/email"]
    );
    assert_eq!(app.remove_folder(&personal, &clients).unwrap(), 1);
    assert_eq!(
        row(&app.list().unwrap(), &mail).folder.as_deref(),
        Some(work.as_str())
    );

    // An edit from an editor that knows nothing of the organization keeps it.
    let fields: Vec<FieldChange> = vec![
        FieldChange::keep("url.0"),
        FieldChange::text("user", "me2"),
        FieldChange::keep("pw"),
    ];
    app.edit_item(&mail, None, fields, GateInput::default())
        .unwrap();
    assert!(row(&app.list().unwrap(), &mail).favourite);

    // The archive: out of every view but its own and out of the codes.
    app.archive(&mail).unwrap();
    let r = row(&app.list().unwrap(), &mail).clone();
    assert_eq!(r.state, "archived");
    assert_eq!(r.views, ["archived"]);
    app.unarchive(&mail).unwrap();
    assert_eq!(row(&app.list().unwrap(), &mail).state, "live");

    // A conflict settled by keeping the copy: the copy takes the title back.
    let orig = app
        .add_login(
            "https://bank.example.com",
            "me",
            b"a".to_vec(),
            Some("Bank"),
        )
        .unwrap();
    let copy = app
        .add_login(
            "https://bank.example.com",
            "me",
            b"b".to_vec(),
            Some("Bank (conflict copy from laptop, 2026-10-17)"),
        )
        .unwrap();
    app.resolve_conflict(&copy, &orig).unwrap();
    let rows = app.list().unwrap();
    assert_eq!(row(&rows, &orig).state, "trashed");
    assert_eq!(row(&rows, &copy).title, "Bank");
    assert!(!row(&rows, &copy).conflict_copy);

    // A lock and an unlock open both vaults again, names and all.
    app.lock().unwrap();
    app.unlock().unwrap();
    let vaults = app.vaults().unwrap();
    assert_eq!(vaults.len(), 2);
    assert_eq!(vaults[0].id, personal, "the first vault stays first");
    assert_eq!(vaults[1].name, "Family");
    app.rename_vault(&family, "Household").unwrap();
    app.set_trash_days(&family, 7).unwrap();
    assert!(app.set_trash_days(&family, 0).is_err());
    let v = &app.vaults().unwrap()[1];
    assert_eq!((v.name.as_str(), v.trash_days), ("Household", 7));
}

#[test]
fn moves_classes_history_files_duplicates_and_reminders() {
    let dir = tempfile::tempdir().unwrap();
    let app = device(dir.path(), "a");
    app.open_or_create(true).unwrap();
    let family = app.create_vault("Family").unwrap();
    let personal = app.vaults().unwrap()[0].id.clone();
    let shop = app
        .add_login("https://shop.example.com", "me", b"first".to_vec(), None)
        .unwrap();
    let folder = app.add_folder(&personal, "Shops", None).unwrap();
    app.organize(&shop, Some(Some(folder)), Some(vec!["buy".into()]), None)
        .unwrap();
    app.attach(
        &shop,
        "receipt.txt",
        "text/plain",
        b"paid".to_vec(),
        GateInput::default(),
    )
    .unwrap();
    // A reveal is a use, so the item has a record on this device.
    app.reveal(&shop, "pw", GateInput::default()).unwrap();
    assert_eq!(row(&app.list().unwrap(), &shop).uses, 1);

    // A move seals it again in the other vault, with its history and its
    // file; the folder (per vault) is dropped, the tags stay, the use follows.
    let before = app.history(&shop).unwrap().len();
    let moved = app.move_item(&shop, &family, GateInput::default()).unwrap();
    let rows = app.list().unwrap();
    assert!(!rows.iter().any(|r| r.id == shop));
    let r = row(&rows, &moved);
    assert_eq!(r.vault_id, family);
    assert_eq!(r.folder, None);
    assert_eq!(r.tags, ["buy"]);
    assert_eq!(r.uses, 1);
    assert_eq!(r.attachments, 1);
    assert!(app.history(&moved).unwrap().len() >= before);
    let detail = app.show(&moved, false).unwrap();
    let file = &detail.place.attachments[0];
    assert_eq!(file.name, "receipt.txt");
    let bytes = app
        .attachment(&moved, &file.id, GateInput::default())
        .unwrap();
    assert_eq!(bytes.expose(), b"paid");

    // Into the crown-jewel class freely; out of it only through the gate.
    let jewel = app
        .set_crown_jewel(&moved, true, GateInput::default())
        .unwrap();
    assert!(row(&app.list().unwrap(), &jewel).crown_jewel);
    let err = app
        .set_crown_jewel(&jewel, false, GateInput::default())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    let plain = app.set_crown_jewel(&jewel, false, confirmed()).unwrap();
    assert!(!row(&app.list().unwrap(), &plain).crown_jewel);
    // A wallet is always a crown jewel.
    let wallet = app
        .add_item(
            "crypto-wallet",
            Some("Cold"),
            false,
            vec![FieldChange::secret("seed", b"abandon ability able about")],
        )
        .unwrap();
    assert!(app.show(&wallet, false).unwrap().place.crown_jewel_fixed);
    let err = app
        .set_crown_jewel(&wallet, false, confirmed())
        .unwrap_err();
    assert!(
        err.message.contains("always a crown jewel"),
        "{}",
        err.message
    );

    // History: an edit, then back to the first version.
    app.edit_item(
        &plain,
        Some("Shop (new)"),
        vec![
            FieldChange::keep("url.0"),
            FieldChange::keep("user"),
            FieldChange::secret("pw", b"second"),
        ],
        GateInput::default(),
    )
    .unwrap();
    let history = app.history(&plain).unwrap();
    assert!(history[0].current);
    assert!(
        history[0].changed.iter().any(|c| c == "Title"),
        "{:?}",
        history[0].changed
    );
    assert!(
        history[0].changed.iter().any(|c| c == "Password"),
        "{:?}",
        history[0].changed
    );
    let first = history.last().unwrap().n;
    let old = app.show_version(&plain, first).unwrap();
    assert!(
        old.fields
            .iter()
            .find(|f| f.field_id == "pw")
            .unwrap()
            .value
            .is_none(),
        "masked"
    );
    app.restore_version(&plain, first, GateInput::default())
        .unwrap();
    assert_eq!(
        app.reveal(&plain, "pw", GateInput::default())
            .unwrap()
            .expose(),
        b"first"
    );
    // Files come off in a new version; the history keeps them.
    app.detach(
        &plain,
        &app.show(&plain, false).unwrap().place.attachments[0].id,
        GateInput::default(),
    )
    .unwrap();
    assert!(
        app.show(&plain, false)
            .unwrap()
            .place
            .attachments
            .is_empty()
    );

    // Duplicates: the same site and username twice (merge applies), and the
    // same password on two different sites.
    let a = app
        .add_login(
            "https://forum.example.org",
            "sam",
            b"same-pw".to_vec(),
            None,
        )
        .unwrap();
    let b = app
        .add_login(
            "https://www.forum.example.org/login",
            "SAM",
            b"other".to_vec(),
            None,
        )
        .unwrap();
    let c = app
        .add_login("https://news.example.net", "x", b"same-pw".to_vec(), None)
        .unwrap();
    let groups = app.duplicates().unwrap();
    let same_login = groups.iter().find(|g| g.reason == "same-login").unwrap();
    assert!(same_login.mergeable);
    assert_eq!(same_login.ids.len(), 2);
    assert!(same_login.ids.contains(&a) && same_login.ids.contains(&b));
    let same_pw = groups.iter().find(|g| g.reason == "same-password").unwrap();
    assert!(same_pw.ids.contains(&a) && same_pw.ids.contains(&c));
    // No group ever carries a value.
    let json = serde_json::to_string(&groups).unwrap();
    assert!(!json.contains("same-pw\"") && !json.contains("other"));
    app.merge_duplicates(&a, vec![b.clone()], GateInput::default())
        .unwrap();
    let rows = app.list().unwrap();
    assert_eq!(row(&rows, &b).state, "trashed");
    let kept = app.show(&a, false).unwrap();
    let urls = kept.fields.iter().filter(|f| f.kind == "url").count();
    assert_eq!(urls, 2, "both addresses kept");
    assert!(
        !app.duplicates()
            .unwrap()
            .iter()
            .any(|g| g.reason == "same-login")
    );
    // A trashed item deletes for good on request.
    app.purge(&b, GateInput::default()).unwrap();
    assert!(!app.list().unwrap().iter().any(|r| r.id == b));
    assert!(
        app.purge(&a, GateInput::default()).is_err(),
        "only from the trash"
    );

    // An expiry reminder: a card that lapses this month, by its lead.
    app.add_item(
        "card",
        Some("Visa"),
        false,
        vec![
            FieldChange::secret("card.number", b"4111111111111111"),
            FieldChange::text("card.expiry", &this_month()),
        ],
    )
    .unwrap();
    let reminders = app.expiry_reminders().unwrap();
    assert_eq!(reminders.len(), 1);
    assert_eq!(reminders[0].kind, "card");
    assert!(reminders[0].days >= 0 && reminders[0].days <= 31);
}

/// `YYYY-MM` of today (UTC).
fn this_month() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // Civil date from days (Howard Hinnant).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}")
}

#[test]
fn a_second_vault_its_folders_and_the_archive_reach_another_device() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state).unwrap();
    let url = running.url();

    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    let registered = a
        .account_register(&url, "me@example.com", "correct horse battery staple", None)
        .unwrap();
    let page = std::str::from_utf8(registered.kit.as_ref().unwrap().expose())
        .unwrap()
        .to_string();
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    let family = a.create_vault("Family").unwrap();
    let folder = a.add_folder(&family, "Streaming", None).unwrap();
    let tv = a
        .add_item_in(
            &family,
            "login",
            Some("TV"),
            false,
            vec![
                FieldChange::text("url.0", "https://tv.example.com"),
                FieldChange::text("org.folder", &folder),
            ],
        )
        .unwrap();
    a.sync().unwrap();

    // B joins and finds both vaults, the name, the folder and the filing.
    let b = device(dir.path(), "b");
    b.account_login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        Some(&secret_key),
    )
    .unwrap();
    b.open_or_create(true).unwrap();
    b.sync().unwrap();
    let vaults = b.vaults().unwrap();
    assert_eq!(vaults.len(), 2, "{vaults:?}");
    let fam = vaults.iter().find(|v| v.id == family).unwrap();
    assert_eq!(fam.name, "Family");
    assert_eq!(fam.folders[0].name, "Streaming");
    let rows = b.list().unwrap();
    assert_eq!(row(&rows, &tv).folder.as_deref(), Some(folder.as_str()));

    // B archives it and stars it; A sees both after a sync.
    b.organize(&tv, None, None, Some(true)).unwrap();
    b.archive(&tv).unwrap();
    b.sync().unwrap();
    a.sync().unwrap();
    let r = row(&a.list().unwrap(), &tv).clone();
    assert_eq!(r.state, "archived");
    assert!(r.favourite);
}
