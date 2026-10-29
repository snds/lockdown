//! The Security screen's post-quantum status line, end to end against the
//! real server (docs/plan/30-pq-status-line.md): "can't check" before the
//! server confirmed anything, "on" for a lone up-to-date device, "waiting
//! for" a device from an older build by its Devices-screen name and id,
//! "on" again once that device reports level 2, and "can't check" when the
//! server is gone. Plus the wire shape against its schema file (D62).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use ecosystem_identity::FileKeystore;
use lockdown_app::pq_status::{PqDevice, PqStatusView};
use lockdown_app::{App, LockPolicy, Paths};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use serde_json::Value;

const LOGIN: &str = "me@example.com";
const PASSWORD: &str = "correct horse battery staple";
const SCHEMA: &str = include_str!("../schema/pq-status.schema.json");

fn server_on_disk(dir: &Path) -> (Arc<AppState>, RunningServer) {
    let storage =
        lockdown_server::storage::SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let state = Arc::new(AppState::new(Box::new(storage), keys, Config::default()));
    let running = run_in_background(state.clone()).unwrap();
    (state, running)
}

/// Changes the identity on disk while the app is locked, as another app
/// holding this device's key would (here: an older device's record).
fn on_disk<T>(
    app: &App,
    edit: impl FnOnce(&mut ecosystem_identity::Identity, &FileKeystore) -> T,
) -> T {
    let probe = app.probe().unwrap();
    let keystore =
        FileKeystore::open_or_create(&Path::new(&probe.data).join("device-key.cbor")).unwrap();
    let mut identity =
        lockdown_vault::identity::open_identity(Path::new(&probe.root), &keystore, None)
            .unwrap()
            .unwrap();
    edit(&mut identity, &keystore)
}

#[test]
fn the_line_follows_the_rollout_and_never_says_on_unconfirmed() {
    let dir = tempfile::tempdir().unwrap();
    let (_state, running) = server_on_disk(dir.path());
    let url = running.url();
    let paths = Paths::resolve(Some(dir.path().join("a/snds/identity")), None).unwrap();
    let a = App::start(paths, None, LockPolicy::default());
    a.open_or_create(true).unwrap();

    // No account: nothing to check against.
    let s = a.pq_status().unwrap();
    assert_eq!((s.state, s.reason), ("unknown", Some("no_account")));
    assert_eq!(s.headline, "Quantum-safe signatures: can't check right now");

    // With an account, before a sync confirmed the records: still unknown.
    a.account_register(&url, LOGIN, PASSWORD, None).unwrap();
    let s = a.pq_status().unwrap();
    assert_eq!((s.state, s.reason), ("unknown", Some("unreached")));

    // A lone, up-to-date device, confirmed by the server: on.
    a.sync().unwrap();
    let s = a.pq_status().unwrap();
    assert_eq!((s.state, s.reason), ("on", None));
    assert_eq!(s.headline, "Quantum-safe signatures: on");
    assert!(s.without_level_1.is_empty() && s.without_level_2.is_empty());

    // A device from an older build (it reads the account's key, level 1,
    // but not post-quantum heads) joins the records.
    a.lock().unwrap();
    let old = on_disk(&a, |identity, keystore| {
        lockdown_vault::test_support::add_device_at_level(
            identity,
            keystore,
            "Lockdown on the old laptop",
            Some(1),
        )
        .unwrap()
    });
    a.unlock().unwrap();
    // Unlocking is offline: unknown until the sync.
    assert_eq!(a.pq_status().unwrap().reason, Some("unreached"));
    a.sync().unwrap();
    let s = a.pq_status().unwrap();
    assert_eq!(s.state, "waiting");
    assert_eq!(
        s.headline,
        "Quantum-safe signatures: waiting for Lockdown on the old laptop"
    );
    assert!(
        s.explanation
            .starts_with("This device needs the latest Lockdown")
    );
    assert!(s.without_level_1.is_empty());
    let expected = PqDevice {
        id: old.to_string(),
        name: "Lockdown on the old laptop".into(),
    };
    assert_eq!(s.without_level_2, std::slice::from_ref(&expected));
    // Named as the Devices screen names it.
    let listed = a.device_list().unwrap();
    let row = listed.iter().find(|d| d.id == expected.id).unwrap();
    assert_eq!(row.name, expected.name);

    // The old device updates and reports level 2: on.
    a.lock().unwrap();
    on_disk(&a, |identity, keystore| {
        lockdown_vault::test_support::set_device_level(identity, keystore, old, Some(2)).unwrap()
    });
    a.unlock().unwrap();
    a.sync().unwrap();
    let s = a.pq_status().unwrap();
    assert_eq!((s.state, s.reason), ("on", None));

    // The server goes away: the next sync fails, and the line can't say on.
    drop(running);
    assert!(a.sync().is_err());
    let s = a.pq_status().unwrap();
    assert_eq!((s.state, s.reason), ("unknown", Some("unreached")));
    assert!(s.explanation.contains("hasn't reached your server"));
}

// --- the wire shape (D62) ------------------------------------------------------------

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every emitted field is declared, every required one is emitted, and
/// enums hold (the same check as `crates/matcher/tests/wire_schema.rs`).
fn check(schema: &Value, instance: &Value, path: &str) {
    let declared = keys(&schema["properties"]);
    let emitted = keys(instance);
    for k in &emitted {
        assert!(
            declared.contains(k),
            "{path}: `{k}` is emitted but not in the schema"
        );
    }
    for r in schema["required"].as_array().unwrap() {
        let r = r.as_str().unwrap();
        assert!(
            emitted.contains(r),
            "{path}: `{r}` is required but not emitted"
        );
    }
    for (k, v) in instance.as_object().unwrap() {
        if let Some(values) = schema["properties"][k]["enum"].as_array() {
            assert!(
                values.contains(v),
                "{path}.{k}: {v} is not in the schema's enum"
            );
        }
    }
}

fn view(state: &'static str, reason: Option<&'static str>) -> PqStatusView {
    let d = |n: &str| PqDevice {
        id: "0b5e2f0c-8a51-4c45-9d0b-5c4d6f1e2a3b".into(),
        name: n.into(),
    };
    PqStatusView {
        state,
        reason,
        headline: "Quantum-safe signatures: waiting for Old phone".into(),
        explanation: "These devices need the latest Lockdown.",
        without_level_1: vec![d("Old phone")],
        without_level_2: vec![d("Kitchen tablet")],
    }
}

#[test]
fn the_wire_shape_matches_its_schema_file() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let all_reasons = schema["properties"]["reason"]["enum"]
        .as_array()
        .unwrap()
        .clone();
    for (state, reason) in [
        ("on", None),
        ("waiting", None),
        ("unknown", Some("no_account")),
        ("unknown", Some("unreached")),
        ("unknown", Some("silent")),
        ("unknown", Some("not_published")),
        ("unknown", Some("managed_elsewhere")),
    ] {
        let v = serde_json::to_value(view(state, reason)).unwrap();
        check(&schema, &v, "PqStatusView");
        for list in ["without_level_1", "without_level_2"] {
            for d in v[list].as_array().unwrap() {
                check(
                    &schema["$defs"]["PqDevice"],
                    d,
                    &format!("PqStatusView.{list}"),
                );
            }
        }
    }
    // Every reason the schema names is one the core can say, and null.
    assert_eq!(all_reasons.len(), 6);
    // The exact keys, so a rename shows up here and in the desktop's types.
    let v = serde_json::to_value(view("on", None)).unwrap();
    assert_eq!(
        keys(&v).into_iter().collect::<Vec<_>>(),
        [
            "explanation",
            "headline",
            "reason",
            "state",
            "without_level_1",
            "without_level_2"
        ]
    );
    assert_eq!(v["reason"], Value::Null);
    assert_eq!(
        keys(&v["without_level_1"][0])
            .into_iter()
            .collect::<Vec<_>>(),
        ["id", "name"]
    );
}
