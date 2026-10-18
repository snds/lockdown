//! The server's own secrets, in one file made on first run: the seed of
//! the Ed25519 key that signs session tokens, the OPAQUE server setup
//! (OPRF seed and key pair), and the seed of the Ed25519 key that signs
//! the key-transparency log's tree heads, and the seed of the ML-DSA-65
//! key that signs them beside it (docs/plan/28-pq-signatures.md). Losing
//! the file logs every session out, makes every password file unusable and
//! changes the log keys every client pinned, so back it up with the
//! database. The file is readable only by the user. A file from before a
//! log key existed gains it on the next start and is rewritten.

use std::path::Path;

use lockdown_crypto::auth::OpaqueServer;
use lockdown_crypto::pq_sign::PqSigningKey;
use lockdown_crypto::{SigningKey, cbor};
use serde::{Deserialize, Serialize};

const KEYS_V: u16 = 1;

#[derive(Serialize, Deserialize)]
struct KeyFile {
    v: u16,
    #[serde(with = "serde_bytes")]
    token_seed: [u8; 32],
    #[serde(with = "serde_bytes")]
    opaque_setup: Vec<u8>,
    /// The log key's seed; absent in a file written before the log existed.
    #[serde(default, with = "serde_bytes", skip_serializing_if = "Option::is_none")]
    log_seed: Option<Vec<u8>>,
    /// The post-quantum log key's seed (FIPS 204 `ξ`); absent in a file
    /// written before it existed.
    #[serde(default, with = "serde_bytes", skip_serializing_if = "Option::is_none")]
    pq_log_seed: Option<Vec<u8>>,
}

/// What the key file holds, open.
pub struct ServerKeys {
    /// Signs session tokens.
    pub token_key: SigningKey,
    /// Runs the OPAQUE server half.
    pub opaque: OpaqueServer,
    /// Signs the key-transparency log's tree heads.
    pub log_key: SigningKey,
    /// Signs them beside it with ML-DSA-65.
    pub pq_log_key: PqSigningKey,
}

impl std::fmt::Debug for ServerKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServerKeys(…)")
    }
}

/// What can go wrong with the key file.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// Reading or writing failed.
    #[error("couldn't read or write the server key file: {0}")]
    Io(#[from] std::io::Error),
    /// The file did not decode or is from a newer build.
    #[error("the server key file didn't decode ({0}); it is not this build's or was changed")]
    Decode(String),
    /// A key step failed.
    #[error(transparent)]
    Crypto(#[from] lockdown_crypto::Error),
}

impl ServerKeys {
    /// Fresh keys, kept nowhere. For tests and for a server with no disk.
    pub fn generate() -> Result<Self, KeyError> {
        Ok(Self {
            token_key: SigningKey::generate()?,
            opaque: OpaqueServer::generate()?,
            log_key: SigningKey::generate()?,
            pq_log_key: PqSigningKey::from_seed(lockdown_crypto::rand::random_32()?),
        })
    }

    /// Opens the key file at `path`, or makes one if there is none. A
    /// file without one of the log keys gains it once and is rewritten.
    pub fn load_or_create(path: &Path) -> Result<Self, KeyError> {
        let mut file = match std::fs::read(path) {
            Ok(bytes) => {
                let file: KeyFile =
                    cbor::from_slice(&bytes).map_err(|e| KeyError::Decode(e.to_string()))?;
                if file.v != KEYS_V {
                    return Err(KeyError::Decode(format!("version {}", file.v)));
                }
                file
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let keys = Self::generate()?;
                KeyFile {
                    v: KEYS_V,
                    token_seed: keys.token_key.inner().to_bytes(),
                    opaque_setup: keys.opaque.to_bytes().expose().to_vec(),
                    log_seed: None,
                    pq_log_seed: None,
                }
            }
            Err(e) => return Err(e.into()),
        };
        let mut changed = false;
        if file.log_seed.is_none() {
            file.log_seed = Some(lockdown_crypto::rand::random_32()?.to_vec());
            changed = true;
        }
        if file.pq_log_seed.is_none() {
            file.pq_log_seed = Some(lockdown_crypto::rand::random_32()?.to_vec());
            changed = true;
        }
        let seed = |s: &Option<Vec<u8>>, what: &str| -> Result<[u8; 32], KeyError> {
            s.as_deref()
                .unwrap_or_default()
                .try_into()
                .map_err(|_| KeyError::Decode(format!("the {what} seed isn't 32 bytes")))
        };
        let keys = Self {
            token_key: SigningKey::from_bytes(file.token_seed),
            opaque: OpaqueServer::from_bytes(&file.opaque_setup)?,
            log_key: SigningKey::from_bytes(seed(&file.log_seed, "log")?),
            pq_log_key: PqSigningKey::from_seed(seed(&file.pq_log_seed, "post-quantum log")?),
        };
        if changed {
            let bytes = cbor::to_canonical(&file).map_err(|e| KeyError::Decode(e.to_string()))?;
            write_private(path, &bytes)?;
        }
        Ok(keys)
    }
}

/// Writes a file readable only by the user, through a temporary file and
/// a rename.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        create_private_dir(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Creates a directory only the user can enter.
pub(crate) fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_file_is_made_once_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys").join("server-key.cbor");
        let first = ServerKeys::load_or_create(&path).unwrap();
        let again = ServerKeys::load_or_create(&path).unwrap();
        assert_eq!(
            first.token_key.verifying_key().to_bytes(),
            again.token_key.verifying_key().to_bytes()
        );
        assert_eq!(
            first.opaque.to_bytes().expose(),
            again.opaque.to_bytes().expose()
        );
        assert_eq!(
            first.log_key.verifying_key().to_bytes(),
            again.log_key.verifying_key().to_bytes()
        );
        assert_eq!(
            first.pq_log_key.verifying_key(),
            again.pq_log_key.verifying_key()
        );
        // A file from before the log key: one is added and kept.
        let old = KeyFile {
            v: KEYS_V,
            token_seed: [1; 32],
            opaque_setup: first.opaque.to_bytes().expose().to_vec(),
            log_seed: None,
            pq_log_seed: None,
        };
        std::fs::write(&path, cbor::to_canonical(&old).unwrap()).unwrap();
        let upgraded = ServerKeys::load_or_create(&path).unwrap();
        let kept = ServerKeys::load_or_create(&path).unwrap();
        assert_eq!(
            upgraded.log_key.verifying_key().to_bytes(),
            kept.log_key.verifying_key().to_bytes()
        );
        assert_eq!(
            upgraded.pq_log_key.verifying_key(),
            kept.pq_log_key.verifying_key()
        );
        // A file from after the log key and before the post-quantum one
        // keeps its log key and gains the other.
        let log_seed = [4; 32];
        let middle = KeyFile {
            v: KEYS_V,
            token_seed: [1; 32],
            opaque_setup: first.opaque.to_bytes().expose().to_vec(),
            log_seed: Some(log_seed.to_vec()),
            pq_log_seed: None,
        };
        std::fs::write(&path, cbor::to_canonical(&middle).unwrap()).unwrap();
        let gained = ServerKeys::load_or_create(&path).unwrap();
        assert_eq!(
            gained.log_key.verifying_key().to_bytes(),
            SigningKey::from_bytes(log_seed).verifying_key().to_bytes()
        );
        assert_eq!(
            ServerKeys::load_or_create(&path)
                .unwrap()
                .pq_log_key
                .verifying_key(),
            gained.pq_log_key.verifying_key()
        );
        assert_eq!(kept.token_key.inner().to_bytes(), [1; 32]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        std::fs::write(&path, b"nope").unwrap();
        assert!(matches!(
            ServerKeys::load_or_create(&path),
            Err(KeyError::Decode(_))
        ));
    }
}
