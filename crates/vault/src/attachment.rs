//! Attachments (02-model §6): a file sealed in 1 MiB chunks under its own
//! Attachment Key, stored as one blob named by the BLAKE3 of its
//! ciphertext.
//!
//! What binds a chunk to its place: every chunk is sealed with associated
//! data naming the profile, persona, vault, item and attachment, **its
//! index and the total count**, so a chunk moved, reordered, duplicated or
//! borrowed from another file fails its tag, and a blob with a chunk
//! missing fails before the last chunk is read (the count is in every
//! chunk's AAD and the blob's length must fit it exactly).
//!
//! What the server and the store see: a blob of `chunks` frames, each
//! `nonce (24) ‖ ciphertext ‖ tag (16)`, and its hash. The file's name,
//! type and real size travel sealed in [`AttachmentRef::meta`] under the
//! same key. The plaintext is padded to a size bucket before chunking
//! ([`padded_size`]), so a blob's length tells the server which bucket a
//! file is in, never its size:
//!
//! | real size | padded to |
//! |---|---|
//! | up to 4 KiB | 4 KiB |
//! | up to 16 KiB | 16 KiB |
//! | up to 64 KiB | 64 KiB |
//! | up to 256 KiB | 256 KiB |
//! | up to 1 MiB | 1 MiB |
//! | larger | the next whole MiB |
//!
//! So every chunk but the last is full, and the last is full too once a
//! file is over 1 MiB. The Attachment Key is wrapped under the Item Key at
//! a [`KeyPlace`](crate::keys::KeyPlace) with purpose `attachment`, and the
//! signed attachment list sits inside the item version, so the manifest
//! covers which files an item has (the key envelopes are outside the
//! signature like every other envelope; see `ItemVersion::without_keys`).

use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_crypto::cbor::{from_slice, to_canonical};
use lockdown_crypto::hash::hash;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::keys::AttachmentKey;
use crate::{Error, Hash32, Id, Result};

/// The attachment format version, part of every chunk's associated data.
pub const ATTACHMENT_V: u16 = 1;

/// One chunk of plaintext.
pub const CHUNK_SIZE: usize = 1 << 20;

/// Bytes each sealed chunk adds: the 24-byte nonce and the 16-byte tag.
pub const CHUNK_OVERHEAD: usize = 24 + 16;

/// The size buckets below one chunk. Above them, whole MiB.
pub const SIZE_BUCKETS: [u64; 5] = [4 << 10, 16 << 10, 64 << 10, 256 << 10, 1 << 20];

/// The largest file this crate seals: 1 GiB. Servers cap lower
/// (`lockdown-server --max-blob`).
pub const MAX_ATTACHMENT_BYTES: u64 = 1 << 30;

/// The padded plaintext length for a file of `len` bytes.
pub fn padded_size(len: u64) -> u64 {
    for bucket in SIZE_BUCKETS {
        if len <= bucket {
            return bucket;
        }
    }
    let mib = 1u64 << 20;
    len.div_ceil(mib) * mib
}

/// How many chunks a padded length takes.
pub fn chunk_count(padded: u64) -> u32 {
    u32::try_from(padded.div_ceil(CHUNK_SIZE as u64).max(1)).unwrap_or(u32::MAX)
}

/// The blob length for a padded plaintext length.
pub fn blob_size(padded: u64) -> u64 {
    padded + u64::from(chunk_count(padded)) * CHUNK_OVERHEAD as u64
}

/// What a file is, sealed beside its chunks. Wiped when dropped.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct AttachmentMeta {
    /// The file name as attached.
    pub name: String,
    /// The media type, as told or guessed by the caller.
    pub mime: String,
    /// The real size in bytes, so the padding comes off on open.
    pub size: u64,
}

/// Where an attachment belongs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttachmentPlace {
    /// The profile.
    pub profile_id: Id,
    /// The persona.
    pub persona_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The item.
    pub item_id: Id,
    /// The attachment.
    pub att_id: Id,
}

/// Which sealed part of an attachment.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Part {
    /// The sealed name, type and size.
    Meta,
    /// One chunk.
    Chunk,
}

/// The associated data of one sealed part: the place, the part, and for a
/// chunk its index and the count. Canonical CBOR of this goes to the AEAD.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct AttachmentAad {
    profile_id: Id,
    persona_id: Id,
    vault_id: Id,
    item_id: Id,
    att_id: Id,
    part: Part,
    index: u32,
    count: u32,
    v: u16,
}

fn aad(place: &AttachmentPlace, part: Part, index: u32, count: u32) -> Result<Vec<u8>> {
    Ok(to_canonical(&AttachmentAad {
        profile_id: place.profile_id,
        persona_id: place.persona_id,
        vault_id: place.vault_id,
        item_id: place.item_id,
        att_id: place.att_id,
        part,
        index,
        count,
        v: ATTACHMENT_V,
    })?)
}

/// A file sealed and ready to store: the blob, its name, and what the
/// item's attachment list records.
#[derive(Clone, Debug)]
pub struct SealedAttachment {
    /// The blob: every sealed chunk, back to back.
    pub blob: Vec<u8>,
    /// BLAKE3 of the blob: its name everywhere.
    pub blob_hash: Hash32,
    /// How many chunks.
    pub chunks: u32,
    /// The sealed name, type and size.
    pub meta: Sealed,
}

/// Seals `plaintext` under `key` at `place`.
pub fn seal_attachment(
    key: &AttachmentKey,
    place: &AttachmentPlace,
    meta: &AttachmentMeta,
    plaintext: &[u8],
) -> Result<SealedAttachment> {
    let len = plaintext.len() as u64;
    if len > MAX_ATTACHMENT_BYTES {
        return Err(Error::Invalid(format!(
            "a file is at most {} bytes; this one is {len}",
            MAX_ATTACHMENT_BYTES
        )));
    }
    if meta.size != len {
        return Err(Error::Invalid(
            "the attachment's size must be the plaintext's length".into(),
        ));
    }
    let padded = padded_size(len);
    let count = chunk_count(padded);
    let padded_len = usize::try_from(padded).map_err(|_| Error::Invalid("too large".into()))?;
    let mut padded_plain = Zeroizing::new(vec![0u8; padded_len]);
    padded_plain[..plaintext.len()].copy_from_slice(plaintext);
    let mut blob = Vec::with_capacity(padded_len + count as usize * CHUNK_OVERHEAD);
    for (index, chunk) in padded_plain.chunks(CHUNK_SIZE).enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::Invalid("too many chunks".into()))?;
        let sealed = seal(key, &aad(place, Part::Chunk, index, count)?, chunk)?;
        blob.extend_from_slice(&sealed.nonce);
        blob.extend_from_slice(&sealed.ct);
    }
    let meta_bytes = Zeroizing::new(to_canonical(meta)?);
    let meta_sealed = seal(key, &aad(place, Part::Meta, 0, count)?, &meta_bytes)?;
    Ok(SealedAttachment {
        blob_hash: hash(&blob),
        blob,
        chunks: count,
        meta: meta_sealed,
    })
}

/// Opens the sealed name, type and size.
pub fn open_meta(
    key: &AttachmentKey,
    place: &AttachmentPlace,
    chunks: u32,
    meta: &Sealed,
) -> Result<AttachmentMeta> {
    let plain = open(key, &aad(place, Part::Meta, 0, chunks)?, meta)?;
    Ok(from_slice(&plain)?)
}

/// Where each chunk's frame sits in a blob of `blob_len` bytes holding
/// `chunks` chunks: every frame but the last is full; the last takes the
/// rest, which must be a whole frame of at least one byte of ciphertext.
pub fn chunk_frames(blob_len: usize, chunks: u32) -> Result<Vec<std::ops::Range<usize>>> {
    let full = CHUNK_SIZE + CHUNK_OVERHEAD;
    let count = chunks as usize;
    if count == 0 {
        return Err(Error::Invalid(
            "an attachment has at least one chunk".into(),
        ));
    }
    let before_last = (count - 1) * full;
    let Some(last) = blob_len.checked_sub(before_last) else {
        return Err(Error::Crypto(lockdown_crypto::Error::Tampered));
    };
    if last <= CHUNK_OVERHEAD || last > full {
        return Err(Error::Crypto(lockdown_crypto::Error::Tampered));
    }
    let mut frames = Vec::with_capacity(count);
    for i in 0..count - 1 {
        frames.push(i * full..(i + 1) * full);
    }
    frames.push(before_last..blob_len);
    Ok(frames)
}

/// Opens one chunk frame (`nonce ‖ ciphertext ‖ tag`) as chunk `index` of
/// `count`. A frame from another position, file or count fails its tag.
pub fn open_chunk(
    key: &AttachmentKey,
    place: &AttachmentPlace,
    index: u32,
    count: u32,
    frame: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    if frame.len() <= CHUNK_OVERHEAD {
        return Err(Error::Crypto(lockdown_crypto::Error::Tampered));
    }
    let sealed = Sealed {
        nonce: frame[..24].to_vec(),
        ct: frame[24..].to_vec(),
    };
    Ok(open(key, &aad(place, Part::Chunk, index, count)?, &sealed)?)
}

/// Opens a whole blob: checks its hash against `expected_hash` first (the
/// name the signed attachment list carries), then every chunk in order,
/// and strips the padding to the sealed size.
pub fn open_attachment(
    key: &AttachmentKey,
    place: &AttachmentPlace,
    chunks: u32,
    expected_hash: Hash32,
    meta: &Sealed,
    blob: &[u8],
) -> Result<(AttachmentMeta, Zeroizing<Vec<u8>>)> {
    if hash(blob) != expected_hash {
        return Err(Error::BlobMismatch {
            expected: expected_hash,
            found: hash(blob),
        });
    }
    let meta = open_meta(key, place, chunks, meta)?;
    let frames = chunk_frames(blob.len(), chunks)?;
    let mut plain = Zeroizing::new(Vec::with_capacity(blob.len()));
    for (index, frame) in frames.into_iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::Invalid("too many chunks".into()))?;
        let chunk = open_chunk(key, place, index, chunks, &blob[frame])?;
        plain.extend_from_slice(&chunk);
    }
    let size = usize::try_from(meta.size).map_err(|_| Error::Invalid("too large".into()))?;
    if size > plain.len() {
        return Err(Error::Crypto(lockdown_crypto::Error::Tampered));
    }
    plain.truncate(size);
    Ok((meta, plain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::SymmetricKey;

    fn place() -> AttachmentPlace {
        AttachmentPlace {
            profile_id: Id::new(),
            persona_id: Id::new(),
            vault_id: Id::new(),
            item_id: Id::new(),
            att_id: Id::new(),
        }
    }

    fn key() -> AttachmentKey {
        AttachmentKey::from_bytes(lockdown_crypto::rand::random_32().unwrap())
    }

    fn meta(len: usize) -> AttachmentMeta {
        AttachmentMeta {
            name: "scan.pdf".into(),
            mime: "application/pdf".into(),
            size: len as u64,
        }
    }

    #[test]
    fn padding_lands_in_the_stated_buckets() {
        assert_eq!(padded_size(0), 4 << 10);
        assert_eq!(padded_size(1), 4 << 10);
        assert_eq!(padded_size(4 << 10), 4 << 10);
        assert_eq!(padded_size((4 << 10) + 1), 16 << 10);
        assert_eq!(padded_size(100_000), 256 << 10);
        assert_eq!(padded_size(1 << 20), 1 << 20);
        assert_eq!(padded_size((1 << 20) + 1), 2 << 20);
        assert_eq!(padded_size(5 << 20), 5 << 20);
        assert_eq!(chunk_count(4 << 10), 1);
        assert_eq!(chunk_count(1 << 20), 1);
        assert_eq!(chunk_count(2 << 20), 2);
        assert_eq!(blob_size(5 << 20), (5 << 20) + 5 * 40);
    }

    #[test]
    fn a_five_mib_file_round_trips_in_five_chunks() {
        let k = key();
        let p = place();
        let data: Vec<u8> = (0..(5 << 20) - 123).map(|i| (i % 251) as u8).collect();
        let sealed = seal_attachment(&k, &p, &meta(data.len()), &data).unwrap();
        assert_eq!(sealed.chunks, 5);
        assert_eq!(sealed.blob.len() as u64, blob_size(5 << 20));
        assert_eq!(sealed.blob_hash, hash(&sealed.blob));
        let (m, plain) =
            open_attachment(&k, &p, 5, sealed.blob_hash, &sealed.meta, &sealed.blob).unwrap();
        assert_eq!(m.size, data.len() as u64);
        assert_eq!(m.name, "scan.pdf");
        assert_eq!(plain.as_slice(), data.as_slice());
        // Small and empty files: one chunk, one bucket.
        for len in [0usize, 1, 4096, 4097, 1 << 20] {
            let data = vec![7u8; len];
            let s = seal_attachment(&k, &p, &meta(len), &data).unwrap();
            assert_eq!(s.chunks, 1);
            let (_, back) = open_attachment(&k, &p, 1, s.blob_hash, &s.meta, &s.blob).unwrap();
            assert_eq!(back.as_slice(), data.as_slice());
        }
    }

    #[test]
    fn a_reordered_chunk_fails_its_tag() {
        let k = key();
        let p = place();
        let data = vec![1u8; 3 << 20];
        let sealed = seal_attachment(&k, &p, &meta(data.len()), &data).unwrap();
        let frames = chunk_frames(sealed.blob.len(), 3).unwrap();
        let (f0, f1) = (
            sealed.blob[frames[0].clone()].to_vec(),
            sealed.blob[frames[1].clone()].to_vec(),
        );
        // In place, each opens.
        open_chunk(&k, &p, 0, 3, &f0).unwrap();
        open_chunk(&k, &p, 1, 3, &f1).unwrap();
        // Swapped: both fail their tags.
        assert!(matches!(
            open_chunk(&k, &p, 0, 3, &f1),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        assert!(matches!(
            open_chunk(&k, &p, 1, 3, &f0),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        // A chunk claimed for a file with another count: fails.
        assert!(open_chunk(&k, &p, 0, 4, &f0).is_err());
        // A chunk from another attachment id: fails.
        let other = AttachmentPlace {
            att_id: Id::new(),
            ..p
        };
        assert!(open_chunk(&k, &other, 0, 3, &f0).is_err());
        // The whole blob with two frames swapped: refused by name (the hash
        // no longer matches), and by tag when the name is "fixed" too.
        let mut swapped = sealed.blob.clone();
        swapped[frames[0].clone()].copy_from_slice(&f1);
        swapped[frames[1].clone()].copy_from_slice(&f0);
        assert!(matches!(
            open_attachment(&k, &p, 3, sealed.blob_hash, &sealed.meta, &swapped),
            Err(Error::BlobMismatch { .. })
        ));
        assert!(matches!(
            open_attachment(&k, &p, 3, hash(&swapped), &sealed.meta, &swapped),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
    }

    #[test]
    fn a_truncated_last_chunk_fails() {
        let k = key();
        let p = place();
        let data = vec![9u8; (2 << 20) + 5];
        let sealed = seal_attachment(&k, &p, &meta(data.len()), &data).unwrap();
        assert_eq!(sealed.chunks, 3);
        // One byte off the end: the hash disagrees, and with the hash
        // "fixed" the last frame's tag fails.
        let mut short = sealed.blob.clone();
        short.pop();
        assert!(open_attachment(&k, &p, 3, sealed.blob_hash, &sealed.meta, &short).is_err());
        assert!(matches!(
            open_attachment(&k, &p, 3, hash(&short), &sealed.meta, &short),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        // A whole chunk off the end: the count in every AAD says three.
        let frames = chunk_frames(sealed.blob.len(), 3).unwrap();
        let two = sealed.blob[..frames[1].end].to_vec();
        assert!(matches!(
            open_attachment(&k, &p, 3, hash(&two), &sealed.meta, &two),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        // And told it has two: the AADs still say three.
        assert!(matches!(
            open_attachment(&k, &p, 2, hash(&two), &sealed.meta, &two),
            Err(Error::Crypto(lockdown_crypto::Error::Tampered))
        ));
        // Another key opens nothing.
        assert!(
            open_attachment(&key(), &p, 3, sealed.blob_hash, &sealed.meta, &sealed.blob).is_err()
        );
    }
}
