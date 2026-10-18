//! Post-quantum vault heads through a real identity and a real store
//! (docs/plan/28-pq-signatures.md): once the rollout rule publishes the
//! identity's ML-DSA-65 key and every device reads post-quantum heads, a
//! commit signs with both keys; reopening checks both; a store whose head
//! has either signature bent, or has lost the post-quantum one this device
//! remembers, is refused.

use std::rc::Rc;

use ecosystem_identity::FileKeystore;
use lockdown_store::{HeadMemory, SqliteStore};
use lockdown_vault::identity::{default_persona, open_or_create_identity, open_or_create_vault};
use lockdown_vault::test_support::sample_login_fields;
use lockdown_vault::{ItemClass, ItemKind, Store};

/// The words of a refusal; an open that succeeds is a failure here.
fn refusal<T>(opened: Result<T, lockdown_vault::Error>) -> String {
    match opened {
        Ok(_) => panic!("the store should have been refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_rolled_out_identity_signs_heads_with_both_keys_and_checks_both_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("identity");
    let data = dir.path().join("lockdown");
    let keystore = Rc::new(FileKeystore::open_or_create(&data.join("device-key.cbor")).unwrap());
    let mut identity =
        open_or_create_identity(&root, "Test laptop", keystore.as_ref(), None).unwrap();
    let mut store = SqliteStore::open(&data.join("vault.sqlite")).unwrap();
    let persona = default_persona(&identity).unwrap();

    // Before the rollout: no key published, heads are Ed25519 only.
    let opened = open_or_create_vault(
        &mut identity,
        persona,
        &mut store,
        Box::new(keystore.clone()),
        &|_| 0,
    )
    .unwrap();
    assert!(opened.vault.post_quantum().is_none());
    assert!(!opened.vault.signs_post_quantum());
    let vault_id = opened.vault.context().vault_id;
    drop(opened);

    // The only device marks itself; the key goes out; heads go hybrid.
    assert!(lockdown_vault::pq::roll_out(&mut identity, keystore.as_ref()).unwrap());
    assert!(lockdown_vault::pq::heads_ready(&identity).unwrap());
    let mut opened = lockdown_vault::identity::open_vault(
        &identity,
        vault_id,
        &store,
        Box::new(keystore.clone()),
        0,
    )
    .unwrap();
    assert!(opened.vault.signs_post_quantum());
    opened
        .vault
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    opened.save(&mut store).unwrap();
    let seq = opened.vault.seq();
    assert!(opened.vault.head().unwrap().pq_sig.is_some());
    assert_eq!(opened.vault.post_quantum_since(seq), Some(seq));
    drop(opened);

    let open = |store: &SqliteStore| {
        lockdown_vault::identity::open_vault(
            &identity,
            vault_id,
            store,
            Box::new(keystore.clone()),
            0,
        )
    };
    let reopened = open(&store).unwrap();
    assert_eq!(
        reopened
            .vault
            .head()
            .unwrap()
            .pq_sig
            .as_ref()
            .unwrap()
            .len(),
        3309
    );
    let good = reopened.vault.head().unwrap().clone();
    drop(reopened);

    // The ML-DSA signature bent in the store, Ed25519 untouched: refused.
    let mut bent = good.clone();
    bent.pq_sig.as_mut().unwrap()[7] ^= 1;
    store.put_head(bent).unwrap();
    let err = refusal(open(&store));
    assert!(
        err.contains("post-quantum signature didn't check out"),
        "{err}"
    );

    // The Ed25519 signature bent, ML-DSA untouched: refused.
    let mut bent = good.clone();
    bent.sig[7] ^= 1;
    store.put_head(bent).unwrap();
    let err = refusal(open(&store));
    assert!(err.contains("device signature didn't check out"), "{err}");

    // The post-quantum signature stripped: this head was the chain's first,
    // so a device with no memory can't tell; one that remembers it can.
    let mut stripped = good.clone();
    stripped.pq_sig = None;
    store.put_head(stripped).unwrap();
    let mut fresh = open(&store).unwrap();
    let mut memory = HeadMemory::open(&data.join("remembered.cbor")).unwrap();
    memory.remember_pq_since(vault_id, seq).unwrap();
    let err = fresh
        .vault
        .require_post_quantum_from(memory.pq_since(vault_id))
        .unwrap_err()
        .to_string();
    assert!(err.contains("lost its post-quantum signature"), "{err}");

    // Put back as signed: opens, and holds to the memory.
    store.put_head(good).unwrap();
    let mut back = open(&store).unwrap();
    back.vault
        .require_post_quantum_from(memory.pq_since(vault_id))
        .unwrap();
}
