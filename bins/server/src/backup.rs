//! The server's own backup (05-self-host §4; docs/plan/20-self-host-server.md §8):
//! an encrypted snapshot of everything a dead server needs to come back,
//! under a **Backup Key** the person holds and the server never keeps.
//!
//! - `lockdown-server backup keygen` prints a new Backup Key once. Keep it
//!   with the recovery kit; without it the snapshot is noise.
//! - `lockdown-server backup create --out FILE` takes a consistent copy of
//!   the database (`VACUUM INTO` for SQLite; for Postgres every row, in one
//!   repeatable-read transaction, into the same SQLite file, so one format
//!   serves both and a backup restores into either), the server key file
//!   and every attachment body kept outside the database (files with
//!   `--blob-dir`, objects in an S3 bucket), and writes them as one sealed
//!   file. Safe while the server runs.
//! - `lockdown-server backup verify --in FILE` opens it, checks every
//!   file's hash and opens the database copy (a restore drill without
//!   touching anything).
//! - `lockdown-server backup restore --in FILE --data DIR` writes it back
//!   into an empty data directory (or, with `--database-url`, an empty
//!   Postgres database; attachment bodies to `--blob-dir` or the bucket).
//!
//! The format (`lockdown-server-backup` v1):
//!
//! ```text
//! "LDSRVBK1"                       8 bytes
//! header length                    u32, big-endian
//! header                           canonical CBOR {v, snapshot_id, created, server_version, schema}
//! chunks, each:                    u32 length ‖ 24-byte nonce ‖ ciphertext and tag
//! ```
//!
//! The plaintext stream is `u32 manifest length ‖ manifest ‖ every file's
//! bytes in manifest order`, cut into 1 MiB chunks. Each chunk is
//! XChaCha20-Poly1305 under `HKDF-SHA-512(Backup Key, salt = snapshot_id,
//! info = "lockdown/server-backup/v1")` with associated data `BLAKE3(header)
//! ‖ chunk index ‖ last-chunk flag`, so the header can't be swapped, chunks
//! can't be reordered, and a cut-off file is refused. The manifest (names,
//! sizes and BLAKE3 hashes) is inside the encryption; only the header is
//! readable.
//!
//! Everything inside is already ciphertext or signed metadata (05-self-host
//! §2) except the server key file, which is why the whole snapshot is
//! sealed again.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use lockdown_crypto::identity::SymmetricKey;
use lockdown_crypto::{Key32, Sealed};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use std::sync::Arc;

use crate::storage::{BlobBodies, BlobDir, SCHEMA_VERSION, SqlStorage};

/// The file's first bytes.
pub const MAGIC: &[u8; 8] = b"LDSRVBK1";
/// The format version.
pub const BACKUP_V: u16 = 1;
/// Plaintext bytes per chunk.
pub const CHUNK: usize = 1 << 20;
/// The database's name inside a snapshot.
pub const DB_NAME: &str = "lockdown-server.sqlite";
/// The key file's name inside a snapshot.
pub const KEY_NAME: &str = "server-key.cbor";
/// The prefix of attachment files inside a snapshot.
pub const BLOB_PREFIX: &str = "blobs/";

const INFO: &[u8] = b"lockdown/server-backup/v1";
const MAX_MANIFEST: usize = 64 << 20;
const MAX_HEADER: usize = 4096;

/// A Backup Key: 32 random bytes, written as Base32 in groups of four.
pub struct BackupKey(Key32);

impl std::fmt::Debug for BackupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BackupKey(…)")
    }
}

impl BackupKey {
    /// A new random key.
    pub fn generate() -> Result<Self, String> {
        Key32::generate().map(Self).map_err(|e| e.to_string())
    }

    /// The key as the person keeps it: `LDBK-XXXX-XXXX-…`.
    pub fn display(&self) -> Zeroizing<String> {
        let b32 = Zeroizing::new(data_encoding::BASE32_NOPAD.encode(self.0.expose()));
        let groups: Vec<&str> = b32
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();
        Zeroizing::new(format!("LDBK-{}", groups.join("-")))
    }

    /// Reads a key as [`BackupKey::display`] writes it; case, spaces and
    /// dashes don't matter.
    pub fn parse(text: &str) -> Result<Self, String> {
        let cleaned: Zeroizing<String> = Zeroizing::new(
            text.trim()
                .trim_start_matches("LDBK")
                .trim_start_matches("ldbk")
                .chars()
                .filter(|c| !matches!(c, '-' | ' ' | '\n' | '\r' | '\t'))
                .map(|c| c.to_ascii_uppercase())
                .collect(),
        );
        let bytes = Zeroizing::new(
            data_encoding::BASE32_NOPAD
                .decode(cleaned.as_bytes())
                .map_err(|_| "that isn't a Backup Key (LDBK-…)".to_string())?,
        );
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| "that isn't a Backup Key (LDBK-…): wrong length".to_string())?;
        Ok(Self(Key32::from_bytes(arr)))
    }

    fn file_key(&self, snapshot_id: &[u8; 16]) -> Result<Key32, String> {
        let mut out = Zeroizing::new([0u8; 32]);
        lockdown_crypto::kdf::derive(self.0.expose(), Some(snapshot_id), INFO, out.as_mut())
            .map_err(|e| e.to_string())?;
        Ok(Key32::from_bytes(*out))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Header {
    v: u16,
    #[serde(with = "serde_bytes")]
    snapshot_id: [u8; 16],
    created: i64,
    server_version: String,
    schema: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    size: u64,
    #[serde(with = "serde_bytes")]
    hash: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Manifest {
    files: Vec<Entry>,
}

/// What a backup holds, for the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The snapshot's id, hex.
    pub snapshot_id: String,
    /// When it was made (seconds since the Unix epoch).
    pub created: i64,
    /// The server release that made it.
    pub server_version: String,
    /// How many files.
    pub files: usize,
    /// How many attachment files among them.
    pub blobs: usize,
    /// Their bytes in all.
    pub bytes: u64,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "snapshot {} made at {} (Unix time) by lockdown-server {}: {} files ({} attachments), {} bytes",
            self.snapshot_id, self.created, self.server_version, self.files, self.blobs, self.bytes
        )
    }
}

/// What a snapshot takes: the open store (its database, and attachment
/// bodies wherever it keeps them) and the server key file.
#[derive(Clone, Copy, Debug)]
pub struct Sources<'a> {
    /// The store.
    pub storage: &'a SqlStorage,
    /// The server key file.
    pub server_key: &'a Path,
}

fn io(what: &str, e: std::io::Error) -> String {
    format!("{what}: {e}")
}

/// Chunks plaintext into sealed records.
struct Sealer<W: Write> {
    out: W,
    key: Key32,
    header_hash: [u8; 32],
    buf: Zeroizing<Vec<u8>>,
    index: u64,
}

fn chunk_aad(header_hash: &[u8; 32], index: u64, last: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(41);
    aad.extend_from_slice(header_hash);
    aad.extend_from_slice(&index.to_be_bytes());
    aad.push(u8::from(last));
    aad
}

impl<W: Write> Sealer<W> {
    fn push(&mut self, mut bytes: &[u8]) -> Result<(), String> {
        while !bytes.is_empty() {
            let room = CHUNK - self.buf.len();
            let take = room.min(bytes.len());
            self.buf.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.buf.len() == CHUNK {
                // Not the last: the last chunk is written by `finish`,
                // possibly empty, so a stream cut at a chunk edge is caught.
                self.emit(false)?;
            }
        }
        Ok(())
    }

    fn emit(&mut self, last: bool) -> Result<(), String> {
        let aad = chunk_aad(&self.header_hash, self.index, last);
        let sealed =
            lockdown_crypto::seal(&self.key, &aad, &self.buf).map_err(|e| e.to_string())?;
        let len = u32::try_from(sealed.nonce.len() + sealed.ct.len())
            .map_err(|_| "a chunk is too large".to_string())?;
        self.out
            .write_all(&len.to_be_bytes())
            .and_then(|()| self.out.write_all(&sealed.nonce))
            .and_then(|()| self.out.write_all(&sealed.ct))
            .map_err(|e| io("couldn't write the backup", e))?;
        self.buf.clear();
        self.index += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<W, String> {
        self.emit(true)?;
        self.out
            .flush()
            .map_err(|e| io("couldn't write the backup", e))?;
        Ok(self.out)
    }
}

/// Reads sealed records back as plaintext.
struct Opener<R: Read> {
    input: R,
    key: Key32,
    header_hash: [u8; 32],
    buf: Zeroizing<Vec<u8>>,
    pos: usize,
    index: u64,
    ended: bool,
}

impl<R: Read> Opener<R> {
    fn next_chunk(&mut self) -> Result<(), String> {
        if self.ended {
            return Err("the backup ends before its files do".into());
        }
        let mut len = [0u8; 4];
        self.input
            .read_exact(&mut len)
            .map_err(|_| "the backup is cut off".to_string())?;
        let len = u32::from_be_bytes(len) as usize;
        if !(24 + 16..=24 + 16 + CHUNK).contains(&len) {
            return Err("the backup is damaged (a chunk of an impossible size)".into());
        }
        let mut raw = vec![0u8; len];
        self.input
            .read_exact(&mut raw)
            .map_err(|_| "the backup is cut off".to_string())?;
        let sealed = Sealed {
            nonce: raw[..24].to_vec(),
            ct: raw[24..].to_vec(),
        };
        // Try as a middle chunk, then as the last one.
        let (plain, last) = match lockdown_crypto::open(
            &self.key,
            &chunk_aad(&self.header_hash, self.index, false),
            &sealed,
        ) {
            Ok(p) => (p, false),
            Err(_) => (
                lockdown_crypto::open(
                    &self.key,
                    &chunk_aad(&self.header_hash, self.index, true),
                    &sealed,
                )
                .map_err(|_| {
                    "the backup didn't open: a wrong Backup Key, or the file was changed"
                        .to_string()
                })?,
                true,
            ),
        };
        self.buf = Zeroizing::new(plain.to_vec());
        self.pos = 0;
        self.index += 1;
        self.ended = last;
        Ok(())
    }

    fn read_exact(&mut self, mut out: &mut [u8]) -> Result<(), String> {
        while !out.is_empty() {
            if self.pos == self.buf.len() {
                self.next_chunk()?;
                continue;
            }
            let take = (self.buf.len() - self.pos).min(out.len());
            out[..take].copy_from_slice(&self.buf[self.pos..self.pos + take]);
            self.pos += take;
            out = &mut out[take..];
        }
        Ok(())
    }

    /// After the last file: nothing may follow.
    fn finish(mut self) -> Result<(), String> {
        while !self.ended {
            if self.pos != self.buf.len() {
                return Err("the backup holds more than its manifest names".into());
            }
            self.next_chunk()?;
        }
        if self.pos != self.buf.len() {
            return Err("the backup holds more than its manifest names".into());
        }
        let mut extra = [0u8; 1];
        match self.input.read(&mut extra) {
            Ok(0) => Ok(()),
            _ => Err("the backup has bytes after its last chunk".into()),
        }
    }
}

fn blob_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    let Ok(vaults) = std::fs::read_dir(dir) else {
        return Ok(out);
    };
    for vault in vaults {
        let vault = vault.map_err(|e| io("couldn't list the blob directory", e))?;
        if !vault.path().is_dir() {
            continue;
        }
        let vault_name = vault.file_name().to_string_lossy().to_string();
        for file in
            std::fs::read_dir(vault.path()).map_err(|e| io("couldn't list a blob folder", e))?
        {
            let file = file.map_err(|e| io("couldn't list a blob folder", e))?;
            let name = file.file_name().to_string_lossy().to_string();
            if file.path().is_file() && !name.ends_with(".tmp") {
                out.push((format!("{BLOB_PREFIX}{vault_name}/{name}"), file.path()));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Whether `name` is one a snapshot may hold (nothing that could climb out
/// of the directory it is restored into).
fn allowed_name(name: &str) -> bool {
    if name == DB_NAME || name == KEY_NAME {
        return true;
    }
    let Some(rest) = name.strip_prefix(BLOB_PREFIX) else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 128
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn hash_file(path: &Path) -> Result<(u64, [u8; 32]), String> {
    let mut f = std::fs::File::open(path)
        .map_err(|e| io(&format!("couldn't read {}", path.display()), e))?;
    let mut h = blake3::Hasher::new();
    let mut size = 0u64;
    let mut buf = vec![0u8; 64 << 10];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| io(&format!("couldn't read {}", path.display()), e))?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((size, *h.finalize().as_bytes()))
}

/// Writes a sealed snapshot of `sources` to `out` (never over an existing
/// file; user-only).
pub fn create(sources: &Sources, key: &BackupKey, out: &Path, now: i64) -> Result<Summary, String> {
    if out.exists() {
        return Err(format!(
            "{} is already there; pick another name",
            out.display()
        ));
    }
    let work = tempfile::Builder::new()
        .prefix(".lockdown-backup-")
        .tempdir_in(
            out.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .map_err(|e| io("couldn't make a working folder beside the backup", e))?;
    let db_copy = work.path().join(DB_NAME);
    let storage = sources.storage;
    crate::storage::Storage::snapshot_to(storage, &db_copy).map_err(|e| e.to_string())?;

    let mut files: Vec<(String, PathBuf)> = vec![
        (DB_NAME.to_string(), db_copy),
        (KEY_NAME.to_string(), sources.server_key.to_path_buf()),
    ];
    // Every body kept outside the database: files are read where they are
    // (every file in the directory, as before), objects in a bucket are
    // fetched by their rows into the working folder.
    let outside = storage.outside_blobs().map_err(|e| e.to_string())?;
    match storage.bodies() {
        Some(bodies) => match bodies.local_dir() {
            Some(dir) => files.extend(blob_files(dir)?),
            None => {
                for (vault, hash, _) in outside {
                    let name = format!("{BLOB_PREFIX}{vault}/{}", hash.to_hex());
                    let bytes = bodies
                        .get(vault, hash)
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("{name} is missing from where it is kept"))?;
                    let path = work.path().join(&name);
                    if let Some(dir) = path.parent() {
                        crate::keys::create_private_dir(dir)
                            .map_err(|e| io("couldn't make a working folder", e))?;
                    }
                    crate::keys::write_private(&path, &bytes)
                        .map_err(|e| io("couldn't stage an attachment", e))?;
                    files.push((name, path));
                }
            }
        },
        None if !outside.is_empty() => {
            return Err(
                "the database names attachment bodies kept outside it, but no place for them is set (--blob-dir or --s3-*)"
                    .into(),
            );
        }
        None => {}
    }
    let mut manifest = Manifest { files: Vec::new() };
    for (name, path) in &files {
        let (size, hash) = hash_file(path)?;
        manifest.files.push(Entry {
            name: name.clone(),
            size,
            hash,
        });
    }
    let snapshot_id = lockdown_crypto::rand::random_16().map_err(|e| e.to_string())?;
    let header = Header {
        v: BACKUP_V,
        snapshot_id,
        created: now,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        schema: SCHEMA_VERSION,
    };
    let header_bytes = lockdown_crypto::cbor::to_canonical(&header).map_err(|e| e.to_string())?;
    let manifest_bytes =
        lockdown_crypto::cbor::to_canonical(&manifest).map_err(|e| e.to_string())?;

    let partial = out.with_extension("partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&partial)
        .map_err(|e| io(&format!("couldn't write {}", partial.display()), e))?;
    let mut writer = std::io::BufWriter::new(file);
    writer
        .write_all(MAGIC)
        .and_then(|()| writer.write_all(&(header_bytes.len() as u32).to_be_bytes()))
        .and_then(|()| writer.write_all(&header_bytes))
        .map_err(|e| io("couldn't write the backup", e))?;
    let mut sealer = Sealer {
        out: writer,
        key: key.file_key(&snapshot_id)?,
        header_hash: *blake3::hash(&header_bytes).as_bytes(),
        buf: Zeroizing::new(Vec::with_capacity(CHUNK)),
        index: 0,
    };
    let result = (|| {
        sealer.push(&(manifest_bytes.len() as u32).to_be_bytes())?;
        sealer.push(&manifest_bytes)?;
        let mut buf = Zeroizing::new(vec![0u8; 64 << 10]);
        for ((_, path), entry) in files.iter().zip(&manifest.files) {
            let mut f = std::fs::File::open(path)
                .map_err(|e| io(&format!("couldn't read {}", path.display()), e))?;
            let mut copied = 0u64;
            loop {
                let n = f
                    .read(&mut buf)
                    .map_err(|e| io(&format!("couldn't read {}", path.display()), e))?;
                if n == 0 {
                    break;
                }
                copied += n as u64;
                sealer.push(&buf[..n])?;
            }
            if copied != entry.size {
                return Err(format!(
                    "{} changed while it was copied; run the backup again",
                    path.display()
                ));
            }
        }
        let writer = sealer.finish()?;
        let file = writer
            .into_inner()
            .map_err(|e| format!("couldn't write the backup: {e}"))?;
        file.sync_all()
            .map_err(|e| io("couldn't write the backup", e))
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    std::fs::rename(&partial, out).map_err(|e| io("couldn't put the backup in place", e))?;
    Ok(summary(&header, &manifest))
}

fn summary(header: &Header, manifest: &Manifest) -> Summary {
    Summary {
        snapshot_id: data_encoding::HEXLOWER.encode(&header.snapshot_id),
        created: header.created,
        server_version: header.server_version.clone(),
        files: manifest.files.len(),
        blobs: manifest
            .files
            .iter()
            .filter(|f| f.name.starts_with(BLOB_PREFIX))
            .count(),
        bytes: manifest.files.iter().map(|f| f.size).sum(),
    }
}

/// Opens a backup and reads every file through `sink(name, bytes)` after
/// its hash checks.
fn read_backup(
    input: &Path,
    key: &BackupKey,
    mut sink: impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<Summary, String> {
    let file = std::fs::File::open(input)
        .map_err(|e| io(&format!("couldn't open {}", input.display()), e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| "that isn't a Lockdown server backup".to_string())?;
    if &magic != MAGIC {
        return Err("that isn't a Lockdown server backup".into());
    }
    let mut len = [0u8; 4];
    reader
        .read_exact(&mut len)
        .map_err(|_| "the backup is cut off".to_string())?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_HEADER {
        return Err("the backup's header is too large".into());
    }
    let mut header_bytes = vec![0u8; len];
    reader
        .read_exact(&mut header_bytes)
        .map_err(|_| "the backup is cut off".to_string())?;
    let header: Header = lockdown_crypto::cbor::from_slice(&header_bytes)
        .map_err(|_| "the backup's header didn't decode".to_string())?;
    if header.v != BACKUP_V {
        return Err(format!(
            "this backup is format {}; this build reads format {BACKUP_V}",
            header.v
        ));
    }
    if header.schema > SCHEMA_VERSION {
        return Err(format!(
            "this backup's database is schema {}, newer than this build reads ({SCHEMA_VERSION}); restore with lockdown-server {} or later",
            header.schema, header.server_version
        ));
    }
    let mut opener = Opener {
        input: reader,
        key: key.file_key(&header.snapshot_id)?,
        header_hash: *blake3::hash(&header_bytes).as_bytes(),
        buf: Zeroizing::new(Vec::new()),
        pos: 0,
        index: 0,
        ended: false,
    };
    let mut mlen = [0u8; 4];
    opener.read_exact(&mut mlen)?;
    let mlen = u32::from_be_bytes(mlen) as usize;
    if mlen > MAX_MANIFEST {
        return Err("the backup's manifest is too large".into());
    }
    let mut mbytes = vec![0u8; mlen];
    opener.read_exact(&mut mbytes)?;
    let manifest: Manifest = lockdown_crypto::cbor::from_slice(&mbytes)
        .map_err(|_| "the backup's manifest didn't decode".to_string())?;
    let mut names = std::collections::BTreeSet::new();
    for entry in &manifest.files {
        if !allowed_name(&entry.name) || !names.insert(entry.name.as_str()) {
            return Err(format!(
                "the backup names a file it may not hold: {:?}",
                entry.name
            ));
        }
    }
    if !names.contains(DB_NAME) || !names.contains(KEY_NAME) {
        return Err("the backup lacks the database or the server key file".into());
    }
    for entry in &manifest.files {
        let size = usize::try_from(entry.size)
            .map_err(|_| "a file in the backup is too large".to_string())?;
        let mut bytes = Zeroizing::new(vec![0u8; size]);
        opener.read_exact(&mut bytes)?;
        if blake3::hash(&bytes).as_bytes() != &entry.hash {
            return Err(format!(
                "{} in the backup doesn't match its hash",
                entry.name
            ));
        }
        sink(&entry.name, &bytes)?;
    }
    opener.finish()?;
    Ok(summary(&header, &manifest))
}

/// Opens and checks a backup, and opens its database copy, changing
/// nothing.
pub fn verify(input: &Path, key: &BackupKey) -> Result<(Summary, DbFacts), String> {
    let work = tempfile::tempdir().map_err(|e| io("couldn't make a working folder", e))?;
    let mut facts = None;
    let summary = read_backup(input, key, |name, bytes| {
        if name == DB_NAME {
            let path = work.path().join(DB_NAME);
            crate::keys::write_private(&path, bytes)
                .map_err(|e| io("couldn't write the database copy", e))?;
            facts = Some(db_facts(&path)?);
        }
        if name == KEY_NAME {
            let path = work.path().join(KEY_NAME);
            crate::keys::write_private(&path, bytes)
                .map_err(|e| io("couldn't write the key copy", e))?;
            crate::keys::ServerKeys::load_or_create(&path)
                .map_err(|e| format!("the server key file in the backup doesn't open: {e}"))?;
        }
        Ok(())
    })?;
    Ok((summary, facts.ok_or("the backup lacks the database")?))
}

/// What a database copy holds, in counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DbFacts {
    /// Accounts.
    pub accounts: u64,
    /// Vaults.
    pub vaults: u64,
    /// Entries in the key-transparency log.
    pub log_size: u64,
}

fn db_facts(path: &Path) -> Result<DbFacts, String> {
    let storage =
        SqlStorage::open(path).map_err(|e| format!("the database copy doesn't open: {e}"))?;
    storage.counts().map_err(|e| e.to_string())
}

/// Where a restore writes.
#[derive(Clone)]
pub struct RestoreTarget {
    /// The (empty) data directory: the key file, and the database unless
    /// `database` names a Postgres one.
    pub data: PathBuf,
    /// An empty Postgres database to restore the rows into, instead of a
    /// SQLite file in `data`.
    pub database: Option<crate::db::PgOptions>,
    /// Where attachment bodies go; `None` is files in `<data>/blobs`.
    pub bodies: Option<Arc<dyn BlobBodies>>,
}

impl std::fmt::Debug for RestoreTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestoreTarget")
            .field("data", &self.data)
            .field("postgres", &self.database.is_some())
            .finish_non_exhaustive()
    }
}

/// Writes a backup into an empty data directory (or an empty Postgres
/// database), with its attachment bodies where `target` says.
pub fn restore(input: &Path, key: &BackupKey, target: &RestoreTarget) -> Result<Summary, String> {
    let data = target.data.as_path();
    for name in [DB_NAME, KEY_NAME] {
        if data.join(name).exists() {
            return Err(format!(
                "{} already holds a server ({name}); restore into an empty directory",
                data.display()
            ));
        }
    }
    crate::keys::create_private_dir(data).map_err(|e| io("couldn't make the data directory", e))?;
    let pg = match &target.database {
        Some(options) => {
            let store = SqlStorage::open_postgres(options.clone()).map_err(|e| e.to_string())?;
            if store.counts().map_err(|e| e.to_string())?.accounts > 0 {
                return Err(
                    "that database already holds a server's accounts; restore into an empty one"
                        .into(),
                );
            }
            Some(store)
        }
        None => None,
    };
    let bodies: Arc<dyn BlobBodies> = match &target.bodies {
        Some(b) => b.clone(),
        None => Arc::new(BlobDir::new(&data.join("blobs")).map_err(|e| e.to_string())?),
    };
    // Into a staging folder first, so a backup that fails half-way leaves
    // the data directory as it was.
    let staging = tempfile::Builder::new()
        .prefix(".lockdown-restore-")
        .tempdir_in(data)
        .map_err(|e| io("couldn't make a staging folder", e))?;
    let summary = read_backup(input, key, |name, bytes| {
        crate::keys::write_private(&staging.path().join(name), bytes)
            .map_err(|e| io(&format!("couldn't write {name}"), e))
    })?;
    let staged = staging.path().join(DB_NAME);
    db_facts(&staged)?;
    for (name, path) in blob_files(&staging.path().join("blobs"))? {
        let Some((vault, hash)) = blob_name(&name) else {
            return Err(format!("the backup holds an attachment named {name:?}"));
        };
        let bytes = std::fs::read(&path).map_err(|e| io("couldn't read a staged blob", e))?;
        bodies
            .put(vault, hash, &bytes)
            .map_err(|e| format!("couldn't put an attachment in place: {e}"))?;
    }
    if let Some(store) = &pg {
        let copy = SqlStorage::open(&staged).map_err(|e| e.to_string())?;
        copy.copy_rows_to(store).map_err(|e| e.to_string())?;
    }
    std::fs::rename(staging.path().join(KEY_NAME), data.join(KEY_NAME))
        .map_err(|e| io(&format!("couldn't put {KEY_NAME} in place"), e))?;
    if pg.is_none() {
        std::fs::rename(&staged, data.join(DB_NAME))
            .map_err(|e| io(&format!("couldn't put {DB_NAME} in place"), e))?;
    }
    Ok(summary)
}

/// `blobs/<vault>/<hash>` as the vault and the hash.
fn blob_name(name: &str) -> Option<(lockdown_vault::Id, lockdown_vault::Hash32)> {
    let rest = name.strip_prefix(BLOB_PREFIX)?;
    let (vault, hash) = rest.split_once('/')?;
    let vault = lockdown_vault::Id::parse(vault).ok()?;
    let bytes: [u8; 32] = data_encoding::HEXLOWER_PERMISSIVE
        .decode(hash.as_bytes())
        .ok()?
        .try_into()
        .ok()?;
    Some((vault, lockdown_vault::Hash32(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_print_and_parse_back_and_junk_is_refused() {
        let key = BackupKey::generate().unwrap();
        let text = key.display();
        assert!(text.starts_with("LDBK-"));
        let back = BackupKey::parse(&text.to_lowercase()).unwrap();
        assert_eq!(back.0.expose(), key.0.expose());
        let spaced = text.replace('-', " ");
        assert!(BackupKey::parse(&spaced).is_ok());
        assert!(BackupKey::parse("LDBK-AAAA").is_err());
        assert!(BackupKey::parse("not a key at all!").is_err());
    }

    #[test]
    fn only_the_known_names_may_be_in_a_snapshot() {
        assert!(allowed_name(DB_NAME) && allowed_name(KEY_NAME));
        assert!(allowed_name("blobs/0b8f-aa/abcdef0123"));
        for bad in [
            "blobs/../x",
            "blobs/a/../../b",
            "../server-key.cbor",
            "blobs/a",
            "blobs/a/b/c",
            "/etc/passwd",
            "blobs/a/b.tmp",
        ] {
            assert!(!allowed_name(bad), "{bad}");
        }
    }
}
