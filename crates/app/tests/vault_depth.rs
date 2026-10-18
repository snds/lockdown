//! Vault depth through the real server (docs/plan/21-vault-depth.md): the
//! account's attachment room is read from the server, a file over it is
//! refused at sync in the server's words and nothing is lost on the
//! device, and a person's own template reaches a second device like any
//! item.

use std::sync::Arc;

use lockdown_app::{
    App, FieldChange, GateInput, LockPolicy, Paths, UserFieldView, UserTemplateInput,
};
use lockdown_server::{AppState, Config, run_in_background};

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";

fn device(dir: &std::path::Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    App::start(paths, None, LockPolicy::default())
}

fn secret_key_of(page: &[u8]) -> String {
    std::str::from_utf8(page)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .map(|v| v.trim().to_string())
        .expect("the kit page names the Secret Key")
}

#[test]
fn the_attachment_room_is_the_servers_and_templates_travel() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        per_login: (100, 10.0),
        attachment_quota: Some(300_000),
        ..Config::default()
    };
    let running = run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap();
    let url = running.url();

    let a = device(dir.path(), "a");
    a.open_or_create(true).unwrap();
    assert_eq!(a.storage_usage().unwrap(), None, "no account yet");
    let registered = a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let secret_key = secret_key_of(registered.kit.as_ref().unwrap().expose());
    let room = a.storage_usage().unwrap().unwrap();
    assert_eq!((room.used, room.quota), (0, Some(300_000)));
    assert_eq!(room.quota_words, "300 kB");

    // A small file fits (padded to 64 KiB plus one chunk's frame).
    let doc = a
        .add_item("document", Some("Lease"), false, vec![])
        .unwrap();
    let small = dir.path().join("small.pdf");
    std::fs::write(&small, vec![1u8; 50_000]).unwrap();
    a.attach_file(&doc, small, None, GateInput::default())
        .unwrap();
    a.sync().unwrap();
    let room = a.storage_usage().unwrap().unwrap();
    assert_eq!(room.used, (64 << 10) + 40);

    // One that doesn't fit is refused at sync, in the server's words, and
    // stays on this device.
    let scans = a
        .add_item("document", Some("Scans"), false, vec![])
        .unwrap();
    let big = dir.path().join("big.bin");
    std::fs::write(&big, vec![2u8; 300_000]).unwrap();
    let att = a
        .attach_file(&scans, big, None, GateInput::default())
        .unwrap();
    let err = a.sync().unwrap_err();
    assert!(
        err.message.contains("for attachments on this server"),
        "{}",
        err.message
    );
    assert_eq!(a.storage_usage().unwrap().unwrap().used, (64 << 10) + 40);
    assert_eq!(
        a.attachment(&scans, &att, GateInput::default())
            .unwrap()
            .expose()
            .len(),
        300_000
    );
    // Taking the file off isn't enough (the history keeps it); deleting
    // the item for good is, and the next sync goes through.
    a.trash(&scans).unwrap();
    a.purge(&scans, GateInput::default()).unwrap();
    a.sync().unwrap();

    // A template made on one device is there on the other.
    let tid = a
        .save_user_template(
            None,
            None,
            UserTemplateInput {
                name: "Gym locker".into(),
                summary: String::new(),
                icon: String::new(),
                fields: vec![UserFieldView {
                    key: "locker".into(),
                    field_id: String::new(),
                    label: "Locker".into(),
                    kind: "text".into(),
                    required: true,
                    hint: String::new(),
                    section: String::new(),
                    secret: false,
                }],
            },
        )
        .unwrap();
    a.add_item(
        &format!("user:{tid}"),
        None,
        false,
        vec![FieldChange::text("tpl.locker", "12")],
    )
    .unwrap();
    a.sync().unwrap();
    let b = device(dir.path(), "b");
    b.account_login(&url, LOGIN, PASSWORD, Some(&secret_key))
        .unwrap();
    b.open_or_create(true).unwrap();
    let theirs = b.user_templates().unwrap();
    assert_eq!(theirs.len(), 1);
    assert_eq!(
        (theirs[0].id.as_str(), theirs[0].name.as_str()),
        (tid.as_str(), "Gym locker")
    );
    let made = b
        .list()
        .unwrap()
        .into_iter()
        .find(|r| r.template == format!("user:{tid}"))
        .unwrap();
    assert_eq!(made.title, "Gym locker");
}
