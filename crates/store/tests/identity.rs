//! The adoption path end to end (D10, D11): one run creates the identity,
//! the vault and an item; a second run, with nothing in common but the
//! identity root, the device keystore file and the store file, opens the
//! vault through the identity and reads the item. No kit, no password.

use std::path::Path;
use std::rc::Rc;
use std::thread;

use ecosystem_identity::FileKeystore;
use lockdown_store::{HeadMemory, RememberedHead, SqliteStore};
use lockdown_vault::identity::{default_persona, open_or_create_identity, open_or_create_vault};
use lockdown_vault::test_support::sample_login_fields;
use lockdown_vault::{Error, Id, ItemClass, ItemKind, PlainField};

/// What one run of the app holds, built from paths alone.
struct Run {
    keystore: Rc<FileKeystore>,
    identity: ecosystem_identity::Identity,
    store: SqliteStore,
    memory: HeadMemory,
}

impl Run {
    fn start(root: &Path, data: &Path) -> Self {
        let keystore =
            Rc::new(FileKeystore::open_or_create(&data.join("device-key.cbor")).unwrap());
        let mut memory = HeadMemory::open(&data.join("remembered.cbor")).unwrap();
        let identity =
            open_or_create_identity(root, "Test laptop", keystore.as_ref(), memory.identity())
                .unwrap();
        memory.remember_identity(identity.head()).unwrap();
        let store = SqliteStore::open(&data.join("vault.sqlite")).unwrap();
        Self {
            keystore,
            identity,
            store,
            memory,
        }
    }

    fn open_vault(&mut self) -> Result<lockdown_vault::identity::OpenVault, Error> {
        let persona = default_persona(&self.identity)?;
        let memory = &self.memory;
        let last = |vault_id: Id| memory.last_accepted_seq(vault_id);
        let opened = open_or_create_vault(
            &mut self.identity,
            persona,
            &mut self.store,
            Box::new(self.keystore.clone()),
            &last,
        )?;
        self.remember(&opened.vault)?;
        Ok(opened)
    }

    fn remember(&mut self, vault: &lockdown_vault::Vault) -> Result<(), Error> {
        let head = vault.head()?;
        self.memory.remember_vault(
            vault.context().vault_id,
            RememberedHead {
                seq: head.seq,
                hash: head.hash()?,
            },
        )?;
        Ok(())
    }
}

#[test]
fn a_vault_made_in_one_run_opens_in_another_through_the_identity_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("identity");
    let data = dir.path().join("lockdown");

    // Run A: nothing exists. Create everything and one item.
    let (item_id, vault_id, profile_id, seq) = {
        let mut a = Run::start(&root, &data);
        let mut opened = a.open_vault().unwrap();
        assert!(opened.created);
        let item_id = opened
            .vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        opened.save(&mut a.store).unwrap();
        a.remember(&opened.vault).unwrap();
        (
            item_id,
            opened.vault.context().vault_id,
            a.identity.profile().profile_id,
            opened.vault.seq(),
        )
    };

    // Run B: a separate thread with fresh state, from the same paths.
    let (root_b, data_b) = (root.clone(), data.clone());
    let read_back = thread::spawn(move || {
        let mut b = Run::start(&root_b, &data_b);
        assert_eq!(b.identity.profile().profile_id, profile_id);
        assert_eq!(b.identity.profile().manager.as_deref(), Some("lockdown"));
        let mut opened = b.open_vault().unwrap();
        assert!(!opened.created);
        assert_eq!(opened.vault.context().vault_id, vault_id);
        assert_eq!(opened.vault.seq(), seq);
        let fields = opened.vault.open_item(item_id).unwrap();
        // And keep working: a second item, saved and remembered.
        opened
            .vault
            .create_item(
                ItemKind::Note,
                ItemClass::Standard,
                vec![PlainField::text("body", "note", "from run B")],
            )
            .unwrap();
        opened.save(&mut b.store).unwrap();
        b.remember(&opened.vault).unwrap();
        (fields, opened.vault.seq())
    })
    .join()
    .unwrap();
    assert_eq!(read_back.0, sample_login_fields());
    assert_eq!(read_back.1, seq + 1);

    // Run C: the store file put back to run A's state is a rollback, and
    // the memory, which lives elsewhere, catches it.
    let mut c = Run::start(&root, &data);
    let opened = c.open_vault().unwrap();
    assert_eq!(opened.vault.seq(), seq + 1);
    drop(opened);
    drop(c);
    let memory = HeadMemory::open(&data.join("remembered.cbor")).unwrap();
    assert_eq!(memory.last_accepted_seq(vault_id), seq + 1);
    // Rewind the store by one head through its own API, as an old copy would.
    {
        let conn = rusqlite::Connection::open(data.join("vault.sqlite")).unwrap();
        conn.execute(
            "DELETE FROM heads WHERE seq > ?1",
            [i64::try_from(seq).unwrap()],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM items WHERE item_id != ?1",
            [item_id.to_string()],
        )
        .unwrap();
    }
    let mut c = Run::start(&root, &data);
    let err = c.open_vault().unwrap_err();
    assert!(
        err.to_string().starts_with("Rollback attempt blocked"),
        "{err}"
    );

    // A device that was never enrolled cannot get in, and no second
    // identity appears.
    let stranger = dir.path().join("stranger");
    std::fs::create_dir_all(&stranger).unwrap();
    let other = FileKeystore::open_or_create(&stranger.join("device-key.cbor")).unwrap();
    let err = open_or_create_identity(&root, "Stranger", &other, None).unwrap_err();
    assert!(err.to_string().contains("isn't enrolled"), "{err}");
}
