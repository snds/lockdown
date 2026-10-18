//! What the post-quantum signatures cost (docs/plan/28-pq-signatures.md
//! §6): bytes per head and per item, and the time to sign and check a head
//! of a 10,000-item vault with Ed25519 alone and with ML-DSA-65 beside it.
//!
//! The sizes are asserted; the timings are printed. Run with
//! `cargo test -p lockdown-vault --test pq_cost -- --nocapture`.

use std::time::{Duration, Instant};

use lockdown_crypto::SigningKey;
use lockdown_crypto::pq_sign::{PUBLIC_KEY_LEN, PqSigningKey, SIGNATURE_LEN};
use lockdown_vault::test_support::{sample_item, sample_vault};
use lockdown_vault::{Hash32, Id, ItemState, ManifestEntry, ManifestHead, verify_chain};

const ITEMS: usize = 10_000;
const ROUNDS: u32 = 20;

/// A head listing `n` items, signed with Ed25519 by `device`.
fn head_with(n: usize, device: &SigningKey) -> ManifestHead {
    let vault = sample_vault();
    let mut head = vault.head().unwrap().clone();
    head.entries = (0..n)
        .map(|i| ManifestEntry {
            item_id: Id::new(),
            rev: i as u64 + 1,
            version_hash: Hash32([(i % 251) as u8; 32]),
            state: ItemState::Live,
            shredded: false,
        })
        .collect();
    head.entries.sort_by_key(|e| e.item_id);
    head.sign(device).unwrap();
    head
}

fn time(mut f: impl FnMut()) -> Duration {
    f();
    let start = Instant::now();
    for _ in 0..ROUNDS {
        f();
    }
    start.elapsed() / ROUNDS
}

#[test]
fn what_post_quantum_signatures_cost_a_ten_thousand_item_vault() {
    let device = SigningKey::from_bytes([1; 32]);
    let pq = PqSigningKey::from_seed([2; 32]);
    let pq_pub = pq.verifying_key();
    let ed_pub = device.verifying_key();

    // Sizes: the signature and the key are fixed by FIPS 204.
    assert_eq!(SIGNATURE_LEN, 3309);
    assert_eq!(PUBLIC_KEY_LEN, 1952);
    let small = head_with(1, &device);
    let mut small_pq = small.clone();
    small_pq.sign_post_quantum(&pq).unwrap();
    let small_extra = small_pq.to_bytes().unwrap().len() - small.to_bytes().unwrap().len();
    assert!(
        (SIGNATURE_LEN..SIGNATURE_LEN + 16).contains(&small_extra),
        "{small_extra}"
    );

    let big = head_with(ITEMS, &device);
    let mut big_pq = big.clone();
    big_pq.sign_post_quantum(&pq).unwrap();
    let big_bytes = big.to_bytes().unwrap().len();
    let big_pq_bytes = big_pq.to_bytes().unwrap().len();
    assert_eq!(big_pq_bytes - big_bytes, small_extra);
    let item_bytes = sample_item().to_bytes().unwrap().len();

    // Times: one head, as a commit signs it and a load or pull checks it.
    let signable = big.signable().unwrap();
    let t_signable = time(|| {
        big.signable().unwrap();
    });
    let t_ed_sign = time(|| {
        device.sign(&signable);
    });
    let t_ed_verify = time(|| {
        ed_pub.verify(&signable, &big.sig, "head").unwrap();
    });
    let t_pq_sign = time(|| {
        pq.sign(&signable, b"lockdown vault head").unwrap();
    });
    let t_pq_verify = time(|| {
        big_pq.verify_post_quantum(&pq_pub).unwrap();
    });
    let t_pq_verify_bare = time(|| {
        pq_pub
            .verify(
                &signable,
                b"lockdown vault head",
                big_pq.pq_sig.as_ref().unwrap(),
                "head",
            )
            .unwrap();
    });
    let small_signable = small.signable().unwrap();
    let small_sig = small_pq.pq_sig.clone().unwrap();
    let t_small_ed_sign = time(|| {
        device.sign(&small_signable);
    });
    let t_small_ed_verify = time(|| {
        ed_pub.verify(&small_signable, &small.sig, "head").unwrap();
    });
    let t_small_pq_sign = time(|| {
        pq.sign(&small_signable, b"lockdown vault head").unwrap();
    });
    let t_small_pq_verify = time(|| {
        pq_pub
            .verify(&small_signable, b"lockdown vault head", &small_sig, "head")
            .unwrap();
    });
    let t_chain_ed = time(|| {
        verify_chain(std::slice::from_ref(&big), |_| Some(ed_pub.clone()), 0).unwrap();
    });

    println!("post-quantum cost, {ITEMS} items, mean of {ROUNDS}:");
    println!(
        "  ML-DSA-65 signature     {SIGNATURE_LEN} B (Ed25519 64 B), public key {PUBLIC_KEY_LEN} B"
    );
    println!(
        "  head, {ITEMS} entries      {big_bytes} B; with pq_sig {big_pq_bytes} B (+{small_extra} B, +{:.2}%)",
        100.0 * small_extra as f64 / big_bytes as f64
    );
    println!(
        "  one sample login item   {item_bytes} B (an ML-DSA signature per version would add {:.0}%)",
        100.0 * SIGNATURE_LEN as f64 / item_bytes as f64
    );
    println!(
        "  per item, signed per version instead: {} MB more for {ITEMS} items of one version",
        (SIGNATURE_LEN * ITEMS) as f64 / 1e6
    );
    println!(
        "  one-entry head          Ed25519 sign {t_small_ed_sign:?} verify {t_small_ed_verify:?}; ML-DSA-65 sign {t_small_pq_sign:?} verify {t_small_pq_verify:?}"
    );
    println!("  canonical head bytes    {t_signable:?}");
    println!("  Ed25519 sign            {t_ed_sign:?}   verify {t_ed_verify:?}");
    println!("  ML-DSA-65 sign          {t_pq_sign:?}   verify {t_pq_verify_bare:?}");
    println!(
        "  head check, Ed25519 chain step {t_chain_ed:?}; ML-DSA step with encoding {t_pq_verify:?}"
    );
}
