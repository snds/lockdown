//! No shipped path makes an unprotected computer
//! (docs/plan/56-no-plain-joins.md).
//!
//! - `account_login` and `recover` on a computer that hasn't joined refuse
//!   with `not_set_up`, in words that name the sealed join, and write
//!   nothing, even against a server where the account exists and the
//!   password is right;
//! - a computer an old join left half done (a plain key file, no identity)
//!   is refused the same way, and the sealed join then seals those keys;
//! - on a computer that holds the identity, `account_login` is a fresh
//!   session as before (the account dialog), and `recover` refuses as it
//!   always did;
//! - the old unsealed join exists only as the test fixtures
//!   `account_login_plain` and `recover_plain`, behind the `test-support`
//!   feature, which only `[dev-dependencies]` in the workspace turn on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ecosystem_identity::keystore::DeviceKeystore;
use lockdown_app::local_unlock::{self, KeyFile};
use lockdown_app::{
    App, ErrorCode, JOIN_SEALED_LOGIN, JOIN_SEALED_RECOVER, LockPolicy, Paths,
    local_unlock::DeviceKeys,
};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";

fn server() -> RunningServer {
    let state = AppState::in_memory(Config {
        per_address: (500, 100.0),
        per_login: (500, 100.0),
        ..Config::default()
    })
    .unwrap();
    run_in_background(Arc::new(state)).unwrap()
}

fn paths(dir: &Path, name: &str) -> Paths {
    Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap()
}

/// Every file under `dir`, relative and sorted (an empty list when it
/// doesn't exist).
fn tree(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, at: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The laptop that made the account; returns it and the kit's Secret Key
/// and recovery code.
fn laptop(dir: &Path, url: &str) -> (App, String, String) {
    let app = App::start(paths(dir, "laptop"), None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    let reg = app.account_register(url, LOGIN, PASSWORD, None).unwrap();
    let page = String::from_utf8(reg.kit.unwrap().expose().to_vec()).unwrap();
    let line = |label: &str| {
        page.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap()
            .trim()
            .to_string()
    };
    app.add_login("https://mail.example.com", "me", b"pw-one".to_vec(), None)
        .unwrap();
    app.sync().unwrap();
    (app, line("Secret Key:"), line("Recovery code:"))
}

#[test]
fn the_old_joins_refuse_on_an_empty_computer_and_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (_laptop, sk, rc) = laptop(dir.path(), &url);
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    let folder = dir.path().join("phone");

    // The right password and Secret Key for an account that exists: the
    // refusal is the core's, not the server's.
    let e = phone
        .account_login(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotSetUp, "{e:?}");
    assert_eq!(e.message, JOIN_SEALED_LOGIN);
    assert!(e.message.contains("account_login_sealed"));
    assert!(e.detail.is_none());
    let e = phone
        .account_login(&url, LOGIN, PASSWORD, None)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotSetUp, "{e:?}");

    let e = phone.recover(&url, LOGIN, PASSWORD, &sk, &rc).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotSetUp, "{e:?}");
    assert_eq!(e.message, JOIN_SEALED_RECOVER);
    assert!(e.message.contains("recover_sealed"));

    assert_eq!(tree(&folder), Vec::<String>::new(), "nothing written");
    let probe = phone.probe().unwrap();
    assert!(!probe.identity_present && !probe.device_key_present);

    // The sealed join the words name works on the same computer.
    phone
        .account_login_sealed(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap();
    phone.open_joined(PASSWORD).unwrap();
    let data = PathBuf::from(phone.probe().unwrap().data);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    assert_eq!(phone.list().unwrap().len(), 1);
}

#[test]
fn a_half_done_plain_join_is_refused_and_the_sealed_join_seals_it() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (_laptop, sk, rc) = laptop(dir.path(), &url);
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    let data = PathBuf::from(phone.probe().unwrap().data);
    // What an old join left when it stopped before the identity: the plain
    // key file and nothing else.
    std::fs::create_dir_all(&data).unwrap();
    let plain = DeviceKeys::open_or_create_plain(&data).unwrap();
    let before = std::fs::read(data.join(local_unlock::PLAIN_FILE)).unwrap();

    let e = phone
        .account_login(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::NotSetUp, JOIN_SEALED_LOGIN)
    );
    let e = phone.recover(&url, LOGIN, PASSWORD, &sk, &rc).unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::NotSetUp, JOIN_SEALED_RECOVER)
    );
    assert_eq!(
        std::fs::read(data.join(local_unlock::PLAIN_FILE)).unwrap(),
        before,
        "the refusal leaves the old file as it was"
    );
    assert!(!phone.probe().unwrap().identity_present);

    // The sealed recovery carries on as the same device and seals it.
    phone
        .recover_sealed(&url, LOGIN, PASSWORD, &sk, &rc)
        .unwrap();
    assert_eq!(local_unlock::key_file(&data), KeyFile::Sealed);
    assert!(!data.join(local_unlock::PLAIN_FILE).exists());
    let opened = DeviceKeys::open_with_password(&data, PASSWORD).unwrap();
    assert_eq!(opened.device_pub(), plain.device_pub());
    phone.open_joined(PASSWORD).unwrap();
}

#[test]
fn on_a_computer_that_holds_the_identity_nothing_changed() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (laptop, sk, rc) = laptop(dir.path(), &url);

    // The account dialog's sign-in: a fresh session, and the vault syncs.
    laptop.account_login(&url, LOGIN, PASSWORD, None).unwrap();
    laptop.sync().unwrap();

    // A sealed computer too.
    let phone = App::start(paths(dir.path(), "phone"), None, LockPolicy::default());
    phone
        .account_login_sealed(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap();
    phone.open_joined(PASSWORD).unwrap();
    phone.account_login(&url, LOGIN, PASSWORD, None).unwrap();
    phone.sync().unwrap();

    // `recover` refuses where an identity is, in the words it always had.
    let e = laptop.recover(&url, LOGIN, PASSWORD, &sk, &rc).unwrap_err();
    assert_eq!(e.code, ErrorCode::Refused, "{e:?}");
    assert!(
        e.message.contains("an ecosystem identity already exists"),
        "{}",
        e.message
    );
}

#[test]
fn the_fixture_makes_the_old_plain_join_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let srv = server();
    let url = srv.url();
    let (_laptop, sk, rc) = laptop(dir.path(), &url);

    let old = App::start(paths(dir.path(), "old"), None, LockPolicy::default());
    old.account_login_plain(&url, LOGIN, PASSWORD, Some(&sk))
        .unwrap();
    let data = PathBuf::from(old.probe().unwrap().data);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Plain);
    old.open_or_create(true).unwrap();
    assert_eq!(old.list().unwrap().len(), 1);

    let recovered = App::start(paths(dir.path(), "recovered"), None, LockPolicy::default());
    recovered
        .recover_plain(&url, LOGIN, PASSWORD, &sk, &rc)
        .unwrap();
    let data = PathBuf::from(recovered.probe().unwrap().data);
    assert_eq!(local_unlock::key_file(&data), KeyFile::Plain);
    recovered.open_or_create(true).unwrap();
    assert_eq!(recovered.list().unwrap().len(), 1);
}

/// Each `[section]` of a manifest and the lines under it.
fn sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![(String::new(), Vec::new())];
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            out.push((
                t.trim_matches(|c| c == '[' || c == ']').to_string(),
                Vec::new(),
            ));
        } else if !t.starts_with('#') {
            out.last_mut().unwrap().1.push(t.to_string());
        }
    }
    out
}

/// The workspace's member manifests, from the root manifest's `members`.
fn member_manifests(root: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let start = text.find("members").expect("a members list");
    let list = &text[start..text[start..].find(']').unwrap() + start];
    let mut out: Vec<PathBuf> = list
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|m| root.join(m).join("Cargo.toml"))
        .collect();
    out.sort();
    out
}

#[test]
fn only_dev_dependencies_turn_on_the_fixture() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.ancestors().nth(2).unwrap();
    let manifests = member_manifests(root);
    assert!(
        manifests
            .iter()
            .any(|m| m.ends_with("apps/desktop/src-tauri/Cargo.toml")),
        "{manifests:?}"
    );

    // lockdown-app's own features: `test-support` is off by default and no
    // other feature turns it on.
    let own = std::fs::read_to_string(crate_dir.join("Cargo.toml")).unwrap();
    let features = sections(&own)
        .into_iter()
        .find(|(name, _)| name == "features")
        .unwrap()
        .1;
    assert!(features.iter().any(|l| l.starts_with("test-support = []")));
    for line in &features {
        if !line.starts_with("test-support") {
            assert!(!line.contains("test-support"), "{line}");
        }
    }

    // Every manifest that names `test-support` on lockdown-app does so
    // only under `[dev-dependencies]`: a build of the desktop, the command
    // line or the server never has the fixture.
    let mut dev = Vec::new();
    for manifest in &manifests {
        let text = std::fs::read_to_string(manifest).unwrap();
        for (section, lines) in sections(&text) {
            for line in lines {
                if line.starts_with("lockdown-app") && line.contains("test-support") {
                    assert!(
                        section == "dev-dependencies" || section.ends_with(".dev-dependencies"),
                        "{} turns on test-support under [{section}]",
                        manifest.display()
                    );
                    dev.push(manifest.strip_prefix(root).unwrap().display().to_string());
                }
            }
        }
    }
    dev.sort();
    assert_eq!(dev, ["bins/cli/Cargo.toml", "crates/app/Cargo.toml"]);
}
