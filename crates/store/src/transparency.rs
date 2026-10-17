//! What this device remembers about each server's key-transparency log
//! (04-security §6 "fetch the latest signed checkpoint and a consistency
//! proof from the last one seen; store it"), in a file of its own beside
//! `remembered.cbor`: `transparency.cbor`.
//!
//! Per server: the log key pinned the first time it was seen (a change is
//! refused, never silently taken), the newest signed tree head verified
//! here and when, how far the account's own log entries were audited, and
//! the newest identity head and this device's own key as the log showed
//! them.
//! The memory only moves forward: an STH smaller than the remembered one,
//! or the same size with another root, is refused here as well as in the
//! sync step, so a rolled-back log is caught even by a caller that forgot
//! to check. Per device, never synced; a person carries a view to another
//! device with `lockdown checkpoint`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lockdown_vault::Hash32;
use lockdown_vault::transparency::SignedTreeHead;
use serde::{Deserialize, Serialize};

use crate::remembered::write_private;
use crate::{StoreError, StoreResult};

const TRANSPARENCY_V: u16 = 1;

/// One server's log as this device last verified it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ServerView {
    /// The server's log key, pinned on first sight.
    #[serde(with = "serde_bytes")]
    pub log_key: [u8; 32],
    /// The newest STH verified here.
    pub sth: SignedTreeHead,
    /// The log index up to which this account's own entries were audited
    /// (every entry below it was checked; the next audit starts here).
    #[serde(default)]
    pub audited: u64,
    /// The newest identity manifest the log showed for the account.
    #[serde(default)]
    pub identity_logged: Option<(u64, Hash32)>,
    /// Whether this device's own key was seen in the log.
    #[serde(default)]
    pub own_device_logged: bool,
    /// When this device last verified the tree head (seconds since the
    /// Unix epoch); `None` in a file written before the field existed.
    #[serde(default)]
    pub checked_at: Option<i64>,
}

#[derive(Default, Serialize, Deserialize)]
struct TransparencyFile {
    v: u16,
    #[serde(default)]
    servers: BTreeMap<String, ServerView>,
}

/// The device's memory of every server's log, backed by one file.
pub struct TransparencyMemory {
    path: PathBuf,
    file: TransparencyFile,
}

impl std::fmt::Debug for TransparencyMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransparencyMemory")
            .field("path", &self.path)
            .field("servers", &self.file.servers.len())
            .finish()
    }
}

impl TransparencyMemory {
    /// Opens the memory at `path`, or starts an empty one if there is none.
    pub fn open(path: &Path) -> StoreResult<Self> {
        let file = match std::fs::read(path) {
            Ok(bytes) => {
                let file: TransparencyFile = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| StoreError::Tampered("the transparency memory".into()))?;
                if file.v > TRANSPARENCY_V {
                    return Err(StoreError::SchemaTooNew {
                        found: u32::from(file.v),
                        supported: u32::from(TRANSPARENCY_V),
                    });
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => TransparencyFile {
                v: TRANSPARENCY_V,
                ..Default::default()
            },
            Err(e) => return Err(StoreError::Backend(e.to_string())),
        };
        Ok(Self {
            path: path.to_path_buf(),
            file,
        })
    }

    /// Where the file lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What is remembered for a server, by its base URL.
    pub fn view(&self, server: &str) -> Option<&ServerView> {
        self.file.servers.get(server.trim_end_matches('/'))
    }

    /// Records a verified view of a server's log and writes the file. The
    /// log key may not change; the STH may not shrink, nor keep its size
    /// with another root; the audited index may not go back.
    pub fn remember(&mut self, server: &str, view: ServerView) -> StoreResult<()> {
        let server = server.trim_end_matches('/').to_string();
        if let Some(seen) = self.file.servers.get(&server) {
            if seen.log_key != view.log_key {
                return Err(StoreError::Backend(format!(
                    "refusing to remember a different log key for {server}: the pinned one stays"
                )));
            }
            if view.sth.size < seen.sth.size
                || (view.sth.size == seen.sth.size && view.sth.root != seen.sth.root)
            {
                return Err(StoreError::Backend(format!(
                    "refusing to remember tree head {} for {server}: {} is already remembered",
                    view.sth.size, seen.sth.size
                )));
            }
            if view.audited < seen.audited {
                return Err(StoreError::Backend(format!(
                    "refusing to move the audited index back for {server}"
                )));
            }
        }
        self.file.servers.insert(server, view);
        self.write()
    }

    fn write(&self) -> StoreResult<()> {
        let bytes = lockdown_crypto::cbor::to_canonical(&self.file)
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        write_private(&self.path, &bytes).map_err(|e| StoreError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_crypto::SigningKey;
    use lockdown_vault::transparency::TreeHead;

    fn sth(key: &SigningKey, size: u64, root: u8) -> SignedTreeHead {
        SignedTreeHead::sign(
            TreeHead {
                size,
                root: Hash32([root; 32]),
                time: 1,
            },
            key,
        )
        .unwrap()
    }

    #[test]
    fn the_memory_pins_the_key_and_only_moves_forward() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transparency.cbor");
        let key = SigningKey::generate().unwrap();
        let log_key = key.verifying_key().to_bytes();
        let mut memory = TransparencyMemory::open(&path).unwrap();
        assert!(memory.view("https://s.example").is_none());
        let view = |size, root, audited| ServerView {
            log_key,
            sth: sth(&key, size, root),
            audited,
            identity_logged: None,
            own_device_logged: false,
            checked_at: None,
        };
        memory
            .remember("https://s.example/", view(2, 2, 1))
            .unwrap();
        drop(memory);
        let mut memory = TransparencyMemory::open(&path).unwrap();
        assert_eq!(memory.view("https://s.example").unwrap().sth.size, 2);
        // An older STH: refused. Same size, other root: refused. Audited
        // index back: refused. Another key: refused. Forward: fine.
        assert!(memory.remember("https://s.example", view(1, 1, 1)).is_err());
        assert!(memory.remember("https://s.example", view(2, 9, 1)).is_err());
        assert!(memory.remember("https://s.example", view(3, 3, 0)).is_err());
        let other = SigningKey::generate().unwrap();
        let mut foreign = view(3, 3, 1);
        foreign.log_key = other.verifying_key().to_bytes();
        assert!(memory.remember("https://s.example", foreign).is_err());
        memory.remember("https://s.example", view(2, 2, 1)).unwrap();
        memory.remember("https://s.example", view(5, 5, 3)).unwrap();
        // Another server is remembered on its own.
        memory.remember("https://t.example", view(1, 1, 0)).unwrap();
        assert_eq!(memory.view("https://t.example").unwrap().sth.size, 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
