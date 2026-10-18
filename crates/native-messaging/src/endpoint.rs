//! Where the host and the desktop app meet. The desktop listens on a Unix
//! socket in Lockdown's data directory (D47) and writes the endpoint file,
//! `native-host.json`, beside it: the socket's path and which executable,
//! by path and SHA-256, the host may accept on the other end. The host
//! reads the file, connects, and checks the process it reached against it
//! before it relays a byte (D4: both sides verify).
//!
//! The file says nothing secret. Anyone who can rewrite it can also rewrite
//! the browser's host manifest, so it adds no trust the data directory's
//! own permissions (0700, the file 0600) do not already give.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The endpoint file's name inside the data directory.
pub const ENDPOINT_FILE: &str = "native-host.json";

/// The socket's name inside the data directory.
pub const SOCKET_NAME: &str = "browser.sock";

/// The endpoint file's version.
pub const ENDPOINT_V: u32 = 1;

/// The longest socket path tried in place (`sun_path` is 104 bytes on
/// macOS, 108 on Linux; 100 leaves room on both).
const MAX_SOCKET_PATH: usize = 100;

/// The endpoint file's contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointFile {
    pub v: u32,
    /// The socket the desktop listens on.
    pub socket: PathBuf,
    /// The desktop app's executable.
    pub app_exe: PathBuf,
    /// SHA-256 of that executable, lowercase hex.
    pub app_sha256: String,
}

impl EndpointFile {
    /// Reads the endpoint file from `data`.
    pub fn read(data: &Path) -> Result<Self> {
        let bytes = std::fs::read(data.join(ENDPOINT_FILE))?;
        let file: Self =
            serde_json::from_slice(&bytes).map_err(|e| Error::BadMessage(e.to_string()))?;
        if file.v != ENDPOINT_V {
            return Err(Error::BadMessage(format!(
                "the endpoint file is version {}, this host reads {ENDPOINT_V}",
                file.v
            )));
        }
        Ok(file)
    }

    /// Writes the endpoint file into `data`, user-only, through a temporary
    /// file and a rename so the host never reads half of it.
    pub fn write(&self, data: &Path) -> Result<()> {
        let body = serde_json::to_vec_pretty(self).map_err(|e| Error::BadMessage(e.to_string()))?;
        write_private(&data.join(ENDPOINT_FILE), &body)
    }

    /// The SHA-256 as 32 bytes.
    pub fn app_hash(&self) -> Result<[u8; 32]> {
        let bytes = data_encoding::HEXLOWER_PERMISSIVE
            .decode(self.app_sha256.as_bytes())
            .map_err(|e| Error::BadMessage(e.to_string()))?;
        bytes
            .try_into()
            .map_err(|_| Error::BadMessage("the app's hash isn't 32 bytes".into()))
    }
}

/// Writes `bytes` to `path` readable by this user only, through a
/// temporary file and a rename.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Lockdown's data directory as the app resolves it (D47): `LOCKDOWN_DATA`,
/// else the `lockdown` directory beside `LOCKDOWN_ROOT` or the platform's
/// identity root (D15). The browser starts the host with the browser's own
/// environment, so a second profile set by those variables reaches it.
pub fn data_dir_from_env() -> Result<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    if let Some(data) = env("LOCKDOWN_DATA") {
        return Ok(PathBuf::from(data));
    }
    let root = match env("LOCKDOWN_ROOT") {
        Some(root) => PathBuf::from(root),
        None => ecosystem_identity::default_root().map_err(|e| Error::NoDataDir(e.to_string()))?,
    };
    Ok(beside(&root))
}

/// The `lockdown` directory beside an identity root (the app's rule).
pub fn beside(root: &Path) -> PathBuf {
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("lockdown"),
        _ => root.join("lockdown"),
    }
}

/// Where the socket for the data directory `data` lives: inside it when
/// the path fits the platform's limit, else a short path named by a hash
/// of `data` under the user's runtime directory (or the temporary
/// directory), in a folder only this user may enter.
pub fn socket_path(data: &Path) -> PathBuf {
    let inside = data.join(SOCKET_NAME);
    if inside.as_os_str().len() <= MAX_SOCKET_PATH {
        return inside;
    }
    // FNV-1a: a stable short name, not a security property.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.as_os_str().as_encoded_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("snds-lockdown-{h:016x}"))
        .join(SOCKET_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_file_round_trips_user_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = EndpointFile {
            v: ENDPOINT_V,
            socket: dir.path().join(SOCKET_NAME),
            app_exe: PathBuf::from("/opt/lockdown/lockdown-desktop"),
            app_sha256: "ab".repeat(32),
        };
        file.write(dir.path()).unwrap();
        assert_eq!(EndpointFile::read(dir.path()).unwrap(), file);
        assert_eq!(file.app_hash().unwrap(), [0xab; 32]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(ENDPOINT_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let mut future = file.clone();
        future.v = 9;
        future.write(dir.path()).unwrap();
        assert!(EndpointFile::read(dir.path()).is_err());
    }

    #[test]
    fn a_short_data_dir_keeps_the_socket_inside_and_a_deep_one_gets_a_short_path() {
        let short = Path::new("/home/me/.local/share/snds/lockdown");
        assert_eq!(socket_path(short), short.join(SOCKET_NAME));
        let deep = Path::new("/").join("d".repeat(120));
        let a = socket_path(&deep);
        assert_eq!(a, socket_path(&deep));
        assert!(a.as_os_str().len() < deep.as_os_str().len());
        assert_ne!(a, socket_path(&Path::new("/").join("e".repeat(120))));
    }

    #[test]
    fn the_data_dir_sits_beside_the_identity_root() {
        assert_eq!(
            beside(Path::new("/home/me/.local/share/snds/identity")),
            PathBuf::from("/home/me/.local/share/snds/lockdown")
        );
    }
}
