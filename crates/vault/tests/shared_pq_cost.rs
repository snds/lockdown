//! What post-quantum signatures add to a shared vault
//! (docs/plan/47-shared-vault-pq.md §7): the bytes on each sharing record,
//! and the time to check a 10,000-item shared vault's chain when another
//! member's heads are held to their own key as well as this identity's.
//!
//! The sizes are asserted; the timings are printed. Run with
//! `cargo test -p lockdown-vault --test shared_pq_cost -- --nocapture`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use ecosystem_identity::SymmetricKey as _;
use lockdown_crypto::SigningKey;
use lockdown_crypto::pq_sign::{PqSigningKey, SIGNATURE_LEN};
use lockdown_vault::pq::{MemberPq, check_events, check_heads, check_member_heads};
use lockdown_vault::sharing::{
    Change, EmergencyGrant, Party, Right, Roster, ShareEnvelope, SharePlace, SharePurpose,
    VaultKind,
};
use lockdown_vault::test_support::sample_vault;
use lockdown_vault::{Hash32, Id, ItemState, ManifestEntry, ManifestHead};

const ITEMS: usize = 10_000;
/// Heads in the chain, alternately Alice's and Bob's.
const HEADS: u64 = 6;
const ROUNDS: u32 = 3;

fn time(mut f: impl FnMut()) -> Duration {
    f();
    let start = Instant::now();
    for _ in 0..ROUNDS {
        f();
    }
    start.elapsed() / ROUNDS
}

fn party() -> Party {
    Party {
        account_id: Id::new(),
        profile_id: Id::new(),
        persona_id: Id::new(),
    }
}

#[test]
fn what_post_quantum_signatures_add_to_a_ten_thousand_item_shared_vault() {
    let (alice_dev, bob_dev) = (Id::new(), Id::new());
    let (alice_ed, bob_ed) = (
        SigningKey::from_bytes([1; 32]),
        SigningKey::from_bytes([2; 32]),
    );
    let (alice_pq, bob_pq) = (
        PqSigningKey::from_seed([3; 32]),
        PqSigningKey::from_seed([4; 32]),
    );

    // A chain of 10,000-item heads, Alice's and Bob's in turn, each signed
    // with both keys of its maker.
    let mut base = sample_vault().head().unwrap().clone();
    base.entries = (0..ITEMS)
        .map(|i| ManifestEntry {
            item_id: Id::new(),
            rev: i as u64 + 1,
            version_hash: Hash32([(i % 251) as u8; 32]),
            state: ItemState::Live,
            shredded: false,
        })
        .collect();
    base.entries.sort_by_key(|e| e.item_id);
    let heads: Vec<ManifestHead> = (1..=HEADS)
        .map(|seq| {
            let mut h = base.clone();
            h.seq = seq;
            let (dev, ed, pq) = if seq % 2 == 1 {
                (alice_dev, &alice_ed, &alice_pq)
            } else {
                (bob_dev, &bob_ed, &bob_pq)
            };
            h.signer = dev;
            h.sign(ed).unwrap();
            h.sign_post_quantum(pq).unwrap();
            h
        })
        .collect();
    let alice_key = alice_pq.verifying_key();
    let own = |id: Id| id == alice_dev;
    let members = BTreeMap::from([(
        Id::new(),
        MemberPq {
            devices: BTreeSet::from([bob_dev]),
            key: Some(bob_pq.verifying_key()),
            since: Some(2),
        },
    )]);
    let t_own = time(|| {
        check_heads(&heads, Some(&alice_key), &own, Some(1)).unwrap();
    });
    let t_members = time(|| {
        check_member_heads(&heads, &members).unwrap();
    });

    // Sharing records: each grows by one ML-DSA-65 signature.
    let (alice, bob) = (party(), party());
    let vault = Id::new();
    let mut genesis = Roster::genesis(vault, alice, VaultKind::Shared, Id::new(), alice_dev);
    genesis.sign(&alice_ed).unwrap();
    let plain = genesis.to_bytes().unwrap().len();
    genesis.sign_post_quantum(&alice_pq).unwrap();
    let event_extra = genesis.to_bytes().unwrap().len() - plain;
    let mut events = vec![genesis];
    for _ in 0..49 {
        let roster = Roster::replay_trusted(&events).unwrap();
        let mut e = roster.propose(
            Change::Add {
                member: party(),
                right: Right::View,
            },
            None,
            alice.account_id,
            alice_dev,
        );
        e.sign(&alice_ed).unwrap();
        e.sign_post_quantum(&alice_pq).unwrap();
        events.push(e);
    }
    let key_of = |_: Id| Some(alice_pq.verifying_key());
    let t_events = time(|| {
        check_events(&events, &key_of).unwrap();
    });

    let (_, bob_kem) = ecosystem_identity::kem::kem(ecosystem_identity::kem::DEFAULT_KEM)
        .keypair([7; 32])
        .unwrap();
    let place = SharePlace {
        purpose: SharePurpose::Vault,
        vault_id: vault,
        item_id: None,
        key_id: Id::new(),
        to: bob,
        from_account: alice.account_id,
        grant_id: None,
    };
    let key = lockdown_vault::VaultKey::from_bytes([9; 32]);
    let mut env = ShareEnvelope::seal(&key, &bob_kem, place, alice_dev, &alice_ed).unwrap();
    let env_plain = env.to_bytes().unwrap().len();
    env.sign_post_quantum(&alice_pq).unwrap();
    let env_extra = env.to_bytes().unwrap().len() - env_plain;
    let mut grant = EmergencyGrant::new(alice, bob, 7, vec![vault], alice_dev).unwrap();
    grant.sign(&alice_ed).unwrap();
    let grant_plain = grant.to_bytes().unwrap().len();
    grant.sign_post_quantum(&alice_pq).unwrap();
    let grant_extra = grant.to_bytes().unwrap().len() - grant_plain;
    for (what, extra) in [
        ("membership event", event_extra),
        ("share envelope", env_extra),
        ("emergency grant", grant_extra),
    ] {
        assert!(
            (SIGNATURE_LEN..SIGNATURE_LEN + 16).contains(&extra),
            "{what}: {extra}"
        );
    }

    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!("shared vault, {ITEMS} items, {HEADS} heads (Alice's and Bob's in turn):");
    println!(
        "  this identity's heads checked (3): {:.1} ms; the other member's heads too (3 more): +{:.1} ms ({:.1} ms a head)",
        ms(t_own),
        ms(t_members),
        ms(t_members) / (HEADS as f64 / 2.0)
    );
    println!(
        "  a membership event +{event_extra} B ({plain} B before), a share envelope +{env_extra} B ({env_plain} B before), an emergency grant +{grant_extra} B ({grant_plain} B before)"
    );
    println!(
        "  50 post-quantum membership events checked: {:.1} ms ({:.2} ms an event)",
        ms(t_events),
        ms(t_events) / 50.0
    );
}
