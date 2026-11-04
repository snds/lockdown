//! A shared vault member's keys change (docs/plan/55-member-key-change.md),
//! end to end against the real server, three identities and a server that
//! does what a hostile one can: publish new keys for someone under their
//! account, and sign heads with devices of those keys.
//!
//! - A planned change (a hand-over the old keys signed) moves every pin
//!   silently, and what was held back clears by itself.
//! - A reset nothing vouches for leaves every pin where it was; only the
//!   vault's owner is asked, a manager who isn't the owner isn't, and the
//!   owner's yes lets them back in; the others follow the owner's change.
//! - A server that fakes a reset, or forges a hand-over, never gets the
//!   vault key: nothing is wrapped to keys nobody vouched for.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use ecosystem_identity::{FileKeystore, Identity};
use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_server::storage::{IdentityAccept, RecordRow, SqliteStorage, Storage};
use lockdown_server::{AppState, Config, RunningServer, run_in_background};
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};
use lockdown_vault::key_change::{HandoverKeys, KeyHandover, publish_handover};
use lockdown_vault::keys::VaultKey;
use lockdown_vault::sharing::{Change, MembershipEvent, ShareEnvelope};
use lockdown_vault::{Id, ManifestHead, Timestamp};

const PASSWORD: &str = "correct horse battery staple";
const BOB: &str = "bob@example.com";
const QUESTION: &str = "bob@example.com reset their account. Let them back in?";
const HELD: &str = "Some recent changes from bob@example.com couldn't be verified, so Lockdown left them out. You don't need to do anything.";

fn server_on_disk(dir: &Path) -> RunningServer {
    let storage = SqliteStorage::open(&dir.join("server.sqlite")).unwrap();
    let keys =
        lockdown_server::keys::ServerKeys::load_or_create(&dir.join("server-key.cbor")).unwrap();
    let config = Config {
        per_login: (1000, 1000.0),
        per_address: (1000, 1000.0),
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::new(Box::new(storage), keys, config))).unwrap()
}

fn app(dir: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(dir.join(name).join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain().unwrap();
    app
}

fn register(app: &App, url: &str, name: &str) {
    app.account_register(url, &format!("{name}@example.com"), PASSWORD, None)
        .unwrap();
    app.sync().unwrap();
}

fn pw() -> GateInput {
    GateInput {
        password: Some(PASSWORD.into()),
        confirmed: false,
    }
}

/// An identity of Bob's that no app runs: the keys a reset (or a planned
/// change) gives him, or the ones a hostile server makes up for him.
struct Keys {
    identity: Identity,
    keystore: Rc<FileKeystore>,
}

impl Keys {
    fn new(dir: &Path, name: &str) -> Self {
        let keystore =
            Rc::new(FileKeystore::open_or_create(&dir.join(format!("{name}-key.cbor"))).unwrap());
        let mut identity =
            Identity::create(&dir.join(name), "lockdown", name, keystore.as_ref()).unwrap();
        lockdown_vault::pq::roll_out(&mut identity, keystore.as_ref()).unwrap();
        Self { identity, keystore }
    }

    fn persona(&self) -> Id {
        self.identity.personas().next().unwrap().persona_id
    }

    fn opens(&self, env: &ShareEnvelope, sender: &lockdown_crypto::VerifyingKey) -> bool {
        let Ok(kem) = self.identity.persona_kem_secret(self.persona()) else {
            return false;
        };
        env.open::<VaultKey>(&kem, &env.place, sender).is_ok()
    }
}

/// Opens an app's identity while it is locked, to use its keys directly.
fn with_identity<T>(app: &App, f: impl FnOnce(&Identity, &FileKeystore) -> T) -> T {
    app.lock().unwrap();
    let probe = app.probe().unwrap();
    let keystore =
        FileKeystore::open_or_create(&Path::new(&probe.data).join("device-key.cbor")).unwrap();
    let identity = lockdown_vault::identity::open_identity(Path::new(&probe.root), &keystore, None)
        .unwrap()
        .unwrap();
    let out = f(&identity, &keystore);
    app.unlock().unwrap();
    out
}

/// A hand-over from the keys `old`'s app holds to `new`, signed by both.
fn handover(old: &App, new: &Keys) -> KeyHandover {
    with_identity(old, |identity, keystore| {
        let mut h = KeyHandover::new(
            HandoverKeys::of_identity(identity).unwrap(),
            HandoverKeys::of_identity(&new.identity).unwrap(),
            std::collections::BTreeMap::from([(
                identity.personas().next().unwrap().persona_id,
                new.persona(),
            )]),
        );
        h.sign_from(
            &lockdown_vault::identity::identity_signing_key(identity, keystore).unwrap(),
            Some(&lockdown_vault::pq::identity_pq_key(identity, keystore).unwrap()),
        )
        .unwrap();
        h.sign_to(
            &lockdown_vault::identity::identity_signing_key(&new.identity, new.keystore.as_ref())
                .unwrap(),
            Some(
                &lockdown_vault::pq::identity_pq_key(&new.identity, new.keystore.as_ref()).unwrap(),
            ),
        )
        .unwrap();
        h
    })
}

/// The server's own hand on its database.
struct Hand(PathBuf);

impl Hand {
    fn conn(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.0).unwrap()
    }

    fn account(&self, login: &str) -> Id {
        let id: String = self
            .conn()
            .query_row(
                "SELECT account_id FROM accounts WHERE login = ?1",
                [login],
                |r| r.get(0),
            )
            .unwrap();
        Id::parse(&id).unwrap()
    }

    /// Publishes `keys` as the account's identity, continuing its chain and
    /// logged like any manifest: what an operator's reset would do, and
    /// what a hostile server can do on its own.
    fn publish(&self, account: Id, keys: &Keys) {
        self.publish_identity(account, &keys.identity, keys.keystore.as_ref());
    }

    fn publish_identity(&self, account: Id, id: &Identity, keystore: &FileKeystore) {
        let prev: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT body FROM identity_heads WHERE account_id = ?1 ORDER BY seq DESC LIMIT 1",
                [account.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        let prev = IdentityManifest::from_bytes(&prev).unwrap();
        let mut set = RecordSet::from_records(
            id.profile(),
            id.personas().cloned(),
            id.devices().cloned(),
            id.envelopes().cloned(),
            id.consents().cloned(),
            Some(&prev),
            Vec::new(),
            id.device_id(),
        )
        .unwrap();
        set.sign(&lockdown_vault::identity::identity_signing_key(id, keystore).unwrap())
            .unwrap();
        let pq = lockdown_vault::pq::identity_pq_key(id, keystore).unwrap();
        lockdown_vault::pq::sign_manifest(&mut set.manifest, &pq).unwrap();
        let rows: Vec<RecordRow> = set
            .records
            .iter()
            .map(|(path, body)| RecordRow {
                path: path.clone(),
                hash: lockdown_crypto::hash::hash(body),
                body: body.clone(),
            })
            .collect();
        let body = set.manifest.to_bytes().unwrap();
        SqliteStorage::open(&self.0)
            .unwrap()
            .accept_identity(
                account,
                IdentityAccept {
                    manifest: &set.manifest,
                    body: &body,
                    records: &rows,
                    isk_pub: &id.profile().isk_pub,
                    revoked: &[],
                    revocations: &[],
                    handed_over_to: Some(id.profile().profile_id),
                },
            )
            .unwrap()
            .unwrap();
    }

    /// The vault's newest head, made by one of Bob's devices and not yet
    /// taken by anyone else, signed again by a device of `keys`: a head
    /// those keys wrote (its members list is sealed for its place in the
    /// chain, so it opens like any).
    fn sign_newest_as(&self, vault: &str, keys: &Keys) {
        let (seq, body): (i64, Vec<u8>) = self
            .conn()
            .query_row(
                "SELECT seq, body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
                [vault],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let mut head = ManifestHead::from_bytes(&body).unwrap();
        head.signer = keys.identity.device_id();
        head.signed_at = Timestamp::now();
        head.pq_sig = None;
        head.sign(&keys.keystore).unwrap();
        let hash = head.hash().unwrap();
        let conn = self.conn();
        conn.execute(
            "UPDATE heads SET hash = ?1, body = ?2 WHERE vault_id = ?3 AND seq = ?4",
            rusqlite::params![&hash.0[..], head.to_bytes().unwrap(), vault, seq],
        )
        .unwrap();
        conn.execute(
            "UPDATE vaults SET head_hash = ?1 WHERE vault_id = ?2",
            rusqlite::params![&hash.0[..], vault],
        )
        .unwrap();
    }

    /// The vault key envelope the server holds for `account` now.
    fn envelope(&self, vault: &str, account: Id) -> ShareEnvelope {
        let body: Vec<u8> = self
            .conn()
            .query_row(
                "SELECT e.body FROM share_envelopes e JOIN shared_vaults s ON s.vault_id = e.vault_id AND s.key_ref = e.key_ref WHERE e.vault_id = ?1 AND e.account_id = ?2",
                [vault, &account.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        ShareEnvelope::from_bytes(&body).unwrap()
    }

    fn events(&self, vault: &str) -> Vec<MembershipEvent> {
        let conn = self.conn();
        let mut st = conn
            .prepare("SELECT body FROM membership_events WHERE vault_id = ?1 ORDER BY seq")
            .unwrap();
        st.query_map([vault], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|b| MembershipEvent::from_bytes(&b.unwrap()).unwrap())
            .collect()
    }
}

fn device_key(app: &App) -> lockdown_crypto::VerifyingKey {
    with_identity(app, |identity, _| {
        lockdown_vault::identity::device_keys(identity)
            .into_iter()
            .find(|(id, _)| *id == identity.device_id())
            .unwrap()
            .1
    })
}

fn fingerprint(app: &App, login: &str) -> String {
    app.sharing_overview()
        .unwrap()
        .contacts
        .into_iter()
        .find(|c| c.login == login)
        .unwrap()
        .fingerprint
}

fn quiet_lines(app: &App) -> Vec<String> {
    let out = app.sync().unwrap();
    if let Ok(st) = app.status() {
        eprintln!("DEBUG {:?} {:?}", out.lines, st.held_back);
    }
    out.lines
        .into_iter()
        .filter(|l| l.contains("couldn't be verified"))
        .collect()
}

fn vault_line(app: &App, vault: &str) -> Option<String> {
    app.vaults()
        .unwrap()
        .into_iter()
        .find(|v| v.id == vault)
        .and_then(|v| v.unverified)
}

/// Bob writes a note with his app, and the server makes it a head his
/// new keys signed before anyone else syncs it.
fn write_as(t: &Three, keys: &Keys, title: &str) {
    t.bob
        .add_item_in(&t.vault, "note", Some(title), false, Vec::new())
        .unwrap();
    t.bob.sync().unwrap();
    t.hand.sign_newest_as(&t.vault, keys);
}

struct Three {
    _dir: tempfile::TempDir,
    _server: RunningServer,
    hand: Hand,
    alice: App,
    bob: App,
    carol: App,
    vault: String,
    bob_account: Id,
}

/// Alice owns a household; Bob may edit it and Carol may `carol_right`.
/// Bob wrote to it, and everyone synced.
fn three(carol_right: &str) -> Three {
    let dir = tempfile::tempdir().unwrap();
    let server = server_on_disk(dir.path());
    let url = server.url();
    let hand = Hand(dir.path().join("server.sqlite"));
    let (alice, bob, carol) = (
        app(dir.path(), "alice"),
        app(dir.path(), "bob"),
        app(dir.path(), "carol"),
    );
    register(&alice, &url, "alice");
    register(&bob, &url, "bob");
    register(&carol, &url, "carol");
    let vault = alice.create_household("Family").unwrap();
    alice
        .add_item_in(&vault, "note", Some("Door code"), false, Vec::new())
        .unwrap();
    alice.sync().unwrap();
    alice.lookup_contact(BOB).unwrap();
    alice.lookup_contact("carol@example.com").unwrap();
    alice.share_vault(&vault, BOB, "edit", pw()).unwrap();
    alice
        .share_vault(&vault, "carol@example.com", carol_right, pw())
        .unwrap();
    bob.sync().unwrap();
    bob.add_item_in(&vault, "note", Some("Bob's"), false, Vec::new())
        .unwrap();
    bob.sync().unwrap();
    carol.sync().unwrap();
    alice.sync().unwrap();
    let bob_account = hand.account(BOB);
    Three {
        _dir: dir,
        _server: server,
        hand,
        alice,
        bob,
        carol,
        vault,
        bob_account,
    }
}

#[test]
fn a_hand_over_moves_every_pin_silently_and_what_was_held_back_clears() {
    let t = three("edit");
    let before = fingerprint(&t.carol, BOB);
    let bob2 = Keys::new(t._dir.path(), "bob2");
    write_as(&t, &bob2, "New keys' note");

    // Bob's new keys appear before their hand-over does: nothing vouches
    // for them. Carol keeps her pin and holds back the head they signed,
    // with the quiet line; Alice, the owner, is asked.
    t.hand.publish(t.bob_account, &bob2);
    t.alice.sync().unwrap();
    assert_eq!(
        t.alice
            .key_resets()
            .unwrap()
            .iter()
            .map(|r| r.words.clone())
            .collect::<Vec<_>>(),
        [QUESTION]
    );
    assert_eq!(quiet_lines(&t.carol), [format!("In Family: {HELD}")]);
    assert_eq!(vault_line(&t.carol, &t.vault).as_deref(), Some(HELD));
    assert_eq!(fingerprint(&t.carol, BOB), before);
    assert!(
        t.carol.key_resets().unwrap().is_empty(),
        "only the owner is asked"
    );

    // The hand-over Bob's old keys signed arrives on his new profile: every
    // pin moves at the next sync, with nothing shown, the held-back head is
    // taken and the line goes by itself; Alice's question goes too.
    let mut bob2 = bob2;
    let h = handover(&t.bob, &bob2);
    publish_handover(&mut bob2.identity, bob2.keystore.as_ref(), &h).unwrap();
    t.hand.publish(t.bob_account, &bob2);
    assert!(quiet_lines(&t.carol).is_empty());
    assert_eq!(vault_line(&t.carol, &t.vault), None);
    let after = fingerprint(&t.carol, BOB);
    assert_ne!(after, before);
    t.alice.sync().unwrap();
    assert!(t.alice.key_resets().unwrap().is_empty());
    assert_eq!(fingerprint(&t.alice, BOB), after);
    assert_eq!(vault_line(&t.alice, &t.vault), None);
    // What Bob wrote with his old keys still opens everywhere, and what
    // the new ones wrote was taken.
    for who in [&t.alice, &t.carol] {
        let titles: Vec<String> = who.list().unwrap().into_iter().map(|r| r.title).collect();
        assert!(titles.iter().any(|t| t == "Bob's"), "{titles:?}");
        assert!(titles.iter().any(|t| t == "New keys' note"), "{titles:?}");
    }
    // Accepted once and never backwards: a server that shows Bob's old
    // keys again is refused, even to a deliberate lookup, and nothing moves.
    with_identity(&t.bob, |identity, keystore| {
        t.hand.publish_identity(t.bob_account, identity, keystore)
    });
    let err = t.carol.lookup_contact(BOB).unwrap_err();
    assert!(
        err.message.contains("already moved on from"),
        "{}",
        err.message
    );
    t.carol.sync().unwrap();
    assert_eq!(fingerprint(&t.carol, BOB), after);
}

#[test]
fn a_reset_is_let_back_in_by_the_owner_alone_and_everyone_else_follows() {
    let t = three("manage");
    let carol_before = fingerprint(&t.carol, BOB);
    let bob2 = Keys::new(t._dir.path(), "bob2");
    write_as(&t, &bob2, "New keys' note");
    t.hand.publish(t.bob_account, &bob2);

    // Alice, the owner, is asked; Carol manages the vault but isn't asked:
    // only the owner's signature roots everyone else's trust in it.
    t.alice.sync().unwrap();
    let asked = t.alice.key_resets().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].words, QUESTION);
    assert_eq!(asked[0].login, BOB);
    assert_eq!(t.alice.sharing_overview().unwrap().resets, asked);
    // A head the new keys signed: held back on Carol's side, quietly.
    assert_eq!(quiet_lines(&t.carol), [format!("In Family: {HELD}")]);
    assert!(t.carol.key_resets().unwrap().is_empty());
    assert_eq!(fingerprint(&t.carol, BOB), carol_before);
    // Not now: the question stays, across a lock, until it is answered.
    t.alice.lock().unwrap();
    t.alice.unlock().unwrap();
    assert_eq!(t.alice.key_resets().unwrap(), asked);
    // Nothing is shared with the new keys meanwhile.
    let err = t
        .alice
        .share_vault(&t.vault, BOB, "edit", pw())
        .unwrap_err();
    assert!(
        err.message.contains("reset their account"),
        "{}",
        err.message
    );
    let alice_key = device_key(&t.alice);
    assert!(!bob2.opens(&t.hand.envelope(&t.vault, t.bob_account), &alice_key));

    // Alice says yes: one membership change she signed names the new keys,
    // the vault key is wrapped to them, and her question is answered.
    assert_eq!(
        t.alice
            .let_back_in(&t.bob_account.to_string(), pw())
            .unwrap(),
        BOB
    );
    assert!(t.alice.key_resets().unwrap().is_empty());
    let last = t.hand.events(&t.vault).pop().unwrap();
    let Change::Add {
        member,
        keys: Some(keys),
        ..
    } = &last.change
    else {
        panic!("the yes isn't a re-admission: {:?}", last.change)
    };
    assert_eq!(member.profile_id, bob2.identity.profile().profile_id);
    assert_eq!(keys.isk_pub, bob2.identity.profile().isk_pub);
    assert!(bob2.opens(&t.hand.envelope(&t.vault, t.bob_account), &alice_key));
    let bob2_fp = fingerprint(&t.alice, BOB);
    assert_ne!(bob2_fp, carol_before);

    // Carol's device follows Alice's change: her pin moves, and the held
    // head is taken; the line goes by itself.
    assert!(quiet_lines(&t.carol).is_empty());
    assert_eq!(fingerprint(&t.carol, BOB), bob2_fp);
    assert_eq!(vault_line(&t.carol, &t.vault), None);
    t.alice.sync().unwrap();
    assert_eq!(vault_line(&t.alice, &t.vault), None);
    for who in [&t.alice, &t.carol] {
        assert!(
            who.list()
                .unwrap()
                .iter()
                .any(|r| r.title == "New keys' note")
        );
    }
}

#[test]
fn a_server_that_fakes_a_reset_or_forges_a_hand_over_never_gets_the_vault_key() {
    let t = three("edit");
    let carol_before = fingerprint(&t.carol, BOB);
    // The server makes up keys for Bob, with a hand-over it forged: its own
    // key signed both sides.
    let mut mallory = Keys::new(t._dir.path(), "mallory");
    let real = handover(&t.bob, &mallory);
    let mut forged = real.clone();
    forged.from_sig = forged.to_sig.clone();
    forged.from_pq_sig = forged.to_pq_sig.clone();
    lockdown_vault::handover::set_profile_extra(
        &mut mallory.identity,
        mallory.keystore.as_ref(),
        &[(
            lockdown_vault::key_change::HANDOVER_FIELD,
            Some(ciborium::Value::Array(vec![ciborium::Value::Bytes(
                forged.to_bytes().unwrap(),
            )])),
        )],
    )
    .unwrap();
    t.hand.publish(t.bob_account, &mallory);

    // Nobody moves a pin; only the owner is asked.
    t.carol.sync().unwrap();
    assert_eq!(fingerprint(&t.carol, BOB), carol_before);
    assert!(t.carol.key_resets().unwrap().is_empty());
    t.alice.sync().unwrap();
    assert_eq!(fingerprint(&t.alice, BOB), carol_before);
    assert_eq!(t.alice.key_resets().unwrap().len(), 1);

    // Alice doesn't answer, and goes on: she removes Carol, which moves the
    // vault to a new key. Bob's new envelope goes to the keys pinned, never
    // the server's: Mallory's keys can't open it, Bob's old ones can.
    let alice_key = device_key(&t.alice);
    let carol_account = t.hand.account("carol@example.com");
    t.alice
        .remove_member(&t.vault, &carol_account.to_string())
        .unwrap();
    let env = t.hand.envelope(&t.vault, t.bob_account);
    assert!(!mallory.opens(&env, &alice_key));
    let bob_opens = with_identity(&t.bob, |identity, _| {
        let persona = identity.personas().next().unwrap().persona_id;
        let kem = identity.persona_kem_secret(persona).unwrap();
        env.open::<VaultKey>(&kem, &env.place, &alice_key).is_ok()
    });
    assert!(bob_opens);
    // No membership change names the server's keys.
    assert!(t.hand.events(&t.vault).iter().all(|e| !matches!(
        &e.change,
        Change::Add { keys: Some(k), .. } if k.isk_pub == mallory.identity.profile().isk_pub
    )));
    // Sharing with Bob deliberately is refused in words, not sealed to
    // the new keys.
    let item = t
        .alice
        .add_item_in(
            &t.alice
                .vaults()
                .unwrap()
                .into_iter()
                .find(|v| v.primary)
                .unwrap()
                .id,
            "note",
            Some("Wifi"),
            false,
            Vec::new(),
        )
        .unwrap();
    t.alice.sync().unwrap();
    let err = t.alice.share_item(&item, BOB, "view", pw()).unwrap_err();
    assert!(
        err.message.contains("reset their account"),
        "{}",
        err.message
    );
}
