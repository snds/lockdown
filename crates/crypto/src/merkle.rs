//! An RFC 6962-style Merkle tree over BLAKE3: the key-transparency log's
//! shape (04-security §6, 02-model §8). Leaves and inner nodes hash under
//! different one-byte prefixes, so a leaf can never be passed off as a
//! node or the other way round, and the proofs are the Certificate
//! Transparency ones (RFC 9162 §2.1): an **inclusion proof** shows one
//! leaf is in a tree of a given size and root, a **consistency proof**
//! shows a tree of one size and root is a prefix of a later tree, so the
//! log only ever grew. BLAKE3 rather than SHA-256 because every other hash
//! in Lockdown is BLAKE3 and one hash function is one less thing to audit;
//! the construction is unchanged.
//!
//! ```text
//! leaf(d)     = BLAKE3(0x00 ‖ d)
//! node(l, r)  = BLAKE3(0x01 ‖ l ‖ r)
//! MTH({})     = BLAKE3("")
//! MTH(D[n])   = node(MTH(D[0:k]), MTH(D[k:n])), k the largest power of two below n
//! ```
//!
//! Everything here takes the list of leaf hashes; the log keeps those and
//! recomputes what it serves. Fine for a log of tens of thousands of
//! entries; a cached tree is a later optimisation with no format change.

use crate::hash::{Hash32, hash};

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

/// The hash of a leaf's bytes.
pub fn leaf_hash(data: &[u8]) -> Hash32 {
    let mut buf = Vec::with_capacity(data.len() + 1);
    buf.push(LEAF_PREFIX);
    buf.extend_from_slice(data);
    hash(&buf)
}

/// The hash of an inner node over its two children.
pub fn node_hash(left: &Hash32, right: &Hash32) -> Hash32 {
    let mut buf = [0u8; 65];
    buf[0] = NODE_PREFIX;
    buf[1..33].copy_from_slice(left.as_bytes());
    buf[33..].copy_from_slice(right.as_bytes());
    hash(&buf)
}

/// The root of the empty tree.
pub fn empty_root() -> Hash32 {
    hash(b"")
}

/// The largest power of two strictly below `n` (`n >= 2`).
fn split_point(n: usize) -> usize {
    debug_assert!(n >= 2);
    1usize << (usize::BITS - 1 - (n - 1).leading_zeros())
}

/// The root over `leaves` (already leaf-hashed), oldest first.
pub fn root(leaves: &[Hash32]) -> Hash32 {
    match leaves.len() {
        0 => empty_root(),
        1 => leaves[0],
        n => {
            let k = split_point(n);
            node_hash(&root(&leaves[..k]), &root(&leaves[k..]))
        }
    }
}

/// The inclusion proof for leaf `index` in the tree over `leaves`
/// (RFC 9162 §2.1.3.1), or `None` when the index is out of range.
pub fn inclusion_proof(leaves: &[Hash32], index: u64) -> Option<Vec<Hash32>> {
    let index = usize::try_from(index).ok()?;
    if index >= leaves.len() {
        return None;
    }
    let mut path = Vec::new();
    path_into(leaves, index, &mut path);
    Some(path)
}

fn path_into(leaves: &[Hash32], m: usize, out: &mut Vec<Hash32>) {
    let n = leaves.len();
    if n <= 1 {
        return;
    }
    let k = split_point(n);
    if m < k {
        path_into(&leaves[..k], m, out);
        out.push(root(&leaves[k..]));
    } else {
        path_into(&leaves[k..], m - k, out);
        out.push(root(&leaves[..k]));
    }
}

/// Checks that `leaf` sits at `index` in a tree of `size` leaves with
/// `root_hash` (RFC 9162 §2.1.3.2).
pub fn verify_inclusion(
    leaf: Hash32,
    index: u64,
    size: u64,
    proof: &[Hash32],
    root_hash: Hash32,
) -> bool {
    if index >= size {
        return false;
    }
    let mut fnode = index;
    let mut snode = size - 1;
    let mut r = leaf;
    for p in proof {
        if snode == 0 {
            return false;
        }
        if fnode & 1 == 1 || fnode == snode {
            r = node_hash(p, &r);
            while fnode & 1 == 0 && fnode != 0 {
                fnode >>= 1;
                snode >>= 1;
            }
        } else {
            r = node_hash(&r, p);
        }
        fnode >>= 1;
        snode >>= 1;
    }
    snode == 0 && r == root_hash
}

/// The consistency proof from the tree over the first `from` leaves to the
/// tree over all of `leaves` (RFC 9162 §2.1.4.1), or `None` when `from` is
/// out of range. Empty when `from` is zero or equals the size.
pub fn consistency_proof(leaves: &[Hash32], from: u64) -> Option<Vec<Hash32>> {
    let from = usize::try_from(from).ok()?;
    if from > leaves.len() {
        return None;
    }
    if from == 0 || from == leaves.len() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    subproof(leaves, from, true, &mut out);
    Some(out)
}

fn subproof(leaves: &[Hash32], m: usize, complete: bool, out: &mut Vec<Hash32>) {
    let n = leaves.len();
    if m == n {
        if !complete {
            out.push(root(leaves));
        }
        return;
    }
    let k = split_point(n);
    if m <= k {
        subproof(&leaves[..k], m, complete, out);
        out.push(root(&leaves[k..]));
    } else {
        subproof(&leaves[k..], m - k, false, out);
        out.push(root(&leaves[..k]));
    }
}

/// Checks that the tree of `from` leaves with `old_root` is a prefix of the
/// tree of `to` leaves with `new_root` (RFC 9162 §2.1.4.2). The empty tree
/// is a prefix of every tree; a tree is a prefix of itself.
pub fn verify_consistency(
    from: u64,
    to: u64,
    old_root: Hash32,
    new_root: Hash32,
    proof: &[Hash32],
) -> bool {
    if from > to {
        return false;
    }
    if from == 0 {
        return proof.is_empty() && old_root == empty_root();
    }
    if from == to {
        return proof.is_empty() && old_root == new_root;
    }
    if proof.is_empty() {
        return false;
    }
    let mut path: Vec<Hash32> = Vec::with_capacity(proof.len() + 1);
    if from.is_power_of_two() {
        path.push(old_root);
    }
    path.extend_from_slice(proof);
    let mut fnode = from - 1;
    let mut snode = to - 1;
    while fnode & 1 == 1 {
        fnode >>= 1;
        snode >>= 1;
    }
    let mut fr = path[0];
    let mut sr = path[0];
    for c in &path[1..] {
        if snode == 0 {
            return false;
        }
        if fnode & 1 == 1 || fnode == snode {
            fr = node_hash(c, &fr);
            sr = node_hash(c, &sr);
            while fnode & 1 == 0 && fnode != 0 {
                fnode >>= 1;
                snode >>= 1;
            }
        } else {
            sr = node_hash(&sr, c);
        }
        fnode >>= 1;
        snode >>= 1;
    }
    snode == 0 && fr == old_root && sr == new_root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(n: usize) -> Vec<Hash32> {
        (0..n)
            .map(|i| leaf_hash(format!("entry {i}").as_bytes()))
            .collect()
    }

    #[test]
    fn prefixes_separate_leaves_from_nodes() {
        let a = leaf_hash(b"x");
        let b = leaf_hash(b"y");
        assert_ne!(a, hash(b"x"));
        assert_ne!(node_hash(&a, &b), node_hash(&b, &a));
        assert_eq!(root(&[]), empty_root());
        assert_eq!(root(&[a]), a);
        assert_eq!(root(&[a, b]), node_hash(&a, &b));
        // Three leaves: ((a,b),c), the RFC shape.
        let c = leaf_hash(b"z");
        assert_eq!(root(&[a, b, c]), node_hash(&node_hash(&a, &b), &c));
        // A leaf's bytes hashed as a node (or the other way) never match.
        let mut fake = Vec::new();
        fake.push(0x01);
        fake.extend_from_slice(b"x");
        assert_ne!(leaf_hash(b"x"), hash(&fake));
    }

    #[test]
    fn inclusion_proofs_hold_over_one_two_three_and_a_hundred_entries() {
        for n in [1usize, 2, 3, 100] {
            let l = leaves(n);
            let r = root(&l);
            for i in 0..n {
                let proof = inclusion_proof(&l, i as u64).unwrap();
                assert!(
                    verify_inclusion(l[i], i as u64, n as u64, &proof, r),
                    "n={n} i={i}"
                );
                // Another leaf, another index, another size, a bent proof: all fail.
                assert!(!verify_inclusion(
                    leaf_hash(b"other"),
                    i as u64,
                    n as u64,
                    &proof,
                    r
                ));
                if n > 1 {
                    let j = (i + 1) % n;
                    assert!(
                        !verify_inclusion(l[i], j as u64, n as u64, &proof, r),
                        "n={n} i={i}"
                    );
                }
                if let Some(first) = proof.first() {
                    let mut bent = proof.clone();
                    bent[0] = node_hash(first, first);
                    assert!(!verify_inclusion(l[i], i as u64, n as u64, &bent, r));
                    let short = &proof[..proof.len() - 1];
                    assert!(!verify_inclusion(l[i], i as u64, n as u64, short, r));
                }
            }
            assert!(inclusion_proof(&l, n as u64).is_none());
        }
    }

    #[test]
    fn consistency_proofs_hold_between_every_pair_of_sizes() {
        for n in [1usize, 2, 3, 100] {
            let l = leaves(n);
            let new_root = root(&l);
            for m in 0..=n {
                let old_root = root(&l[..m]);
                let proof = consistency_proof(&l, m as u64).unwrap();
                assert!(
                    verify_consistency(m as u64, n as u64, old_root, new_root, &proof),
                    "m={m} n={n}"
                );
                if m > 0 && m < n {
                    // A different old tree of the same size: not a prefix.
                    let mut forked = l[..m].to_vec();
                    forked[m - 1] = leaf_hash(b"swapped");
                    assert!(
                        !verify_consistency(m as u64, n as u64, root(&forked), new_root, &proof),
                        "m={m} n={n}"
                    );
                    // The wrong new root.
                    assert!(!verify_consistency(
                        m as u64,
                        n as u64,
                        old_root,
                        node_hash(&new_root, &new_root),
                        &proof
                    ));
                    // A bent or shortened proof.
                    let mut bent = proof.clone();
                    bent[0] = node_hash(&bent[0], &bent[0]);
                    assert!(!verify_consistency(
                        m as u64, n as u64, old_root, new_root, &bent
                    ));
                    assert!(!verify_consistency(
                        m as u64,
                        n as u64,
                        old_root,
                        new_root,
                        &proof[..proof.len() - 1]
                    ));
                }
            }
            assert!(consistency_proof(&l, n as u64 + 1).is_none());
        }
        // Shrinking is never consistent; the empty tree needs the empty root.
        assert!(!verify_consistency(3, 2, empty_root(), empty_root(), &[]));
        assert!(!verify_consistency(
            0,
            2,
            leaf_hash(b"x"),
            empty_root(),
            &[]
        ));
        assert!(verify_consistency(0, 0, empty_root(), empty_root(), &[]));
    }

    #[test]
    fn a_forked_log_fails_consistency_at_every_later_size() {
        // Two logs that agree on the first ten entries and part at the
        // eleventh: no later size of one is consistent with the other.
        let shared = leaves(10);
        let mut a = shared.clone();
        let mut b = shared.clone();
        a.push(leaf_hash(b"a's eleventh"));
        b.push(leaf_hash(b"b's eleventh"));
        for extra in 0..5 {
            a.push(leaf_hash(format!("a {extra}").as_bytes()));
            b.push(leaf_hash(format!("b {extra}").as_bytes()));
        }
        let proof = consistency_proof(&b, 11).unwrap();
        assert!(!verify_consistency(
            11,
            b.len() as u64,
            root(&a[..11]),
            root(&b),
            &proof
        ));
        assert!(verify_consistency(
            10,
            b.len() as u64,
            root(&shared),
            root(&b),
            &consistency_proof(&b, 10).unwrap()
        ));
        assert_ne!(root(&a[..11]), root(&b[..11]));
    }
}
