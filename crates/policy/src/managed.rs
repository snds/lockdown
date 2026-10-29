//! A server an administrator presets on this computer (docs/plan/39-first-run.md
//! §1.7 source 1, D472).
//!
//! The file is JSON at a fixed place: `/Library/Application Support/Lockdown/policy.json`
//! on macOS, `/etc/lockdown/policy.json` on Linux. It names a server, may pin
//! that server's log key, and may lock the choice so the person can't pick
//! another:
//!
//! ```json
//! {"v": 1, "server": "https://vault.example.org", "server_key": "<64 hex>", "lock": false}
//! ```
//!
//! It is read only when root owns it and neither its group nor anyone else
//! may write it, so a program running as the person can't plant one. Windows
//! has no policy file in this slice: trusting one needs an ACL check, and
//! the registry path is a later piece.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The policy file's format version this build reads.
pub const MANAGED_V: u16 = 1;
/// The largest policy file read.
const MAX_FILE: u64 = 16 * 1024;

/// What an administrator set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedServer {
    /// The server's address.
    pub server: String,
    /// The server's log key, when the policy pins it (32 bytes).
    pub server_key: Option<[u8; 32]>,
    /// Whether the person may choose another server.
    pub lock: bool,
}

/// Why a policy file was not used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ManagedError {
    /// The file is there but anyone but root could change it.
    #[error("{0} isn't owned by the administrator alone, so Lockdown ignores it")]
    NotTrusted(String),
    /// The file didn't read as a policy.
    #[error("{0} isn't a Lockdown policy Lockdown can read: {1}")]
    Unreadable(String, String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    v: u16,
    server: String,
    #[serde(default)]
    server_key: Option<String>,
    #[serde(default)]
    lock: bool,
}

/// Where this platform's policy file lives, if it has one.
pub fn default_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(PathBuf::from(
            "/Library/Application Support/Lockdown/policy.json",
        ))
    } else if cfg!(target_os = "linux") {
        Some(PathBuf::from("/etc/lockdown/policy.json"))
    } else {
        None
    }
}

/// Reads a policy from its bytes. `name` names it in errors.
pub fn parse(bytes: &[u8], name: &str) -> Result<ManagedServer, ManagedError> {
    let bad = |why: &str| ManagedError::Unreadable(name.to_string(), why.to_string());
    let file: File = serde_json::from_slice(bytes).map_err(|e| bad(&e.to_string()))?;
    if file.v != MANAGED_V {
        return Err(bad(&format!(
            "version {} isn't one this build reads",
            file.v
        )));
    }
    let server = file.server.trim().trim_end_matches('/').to_string();
    if server.is_empty() || server.len() > 512 || server.chars().any(char::is_control) {
        return Err(bad("the server isn't an address"));
    }
    let server_key = match file.server_key {
        None => None,
        Some(hex) => {
            let hex = hex.trim();
            if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(bad("the server key isn't 64 hex digits"));
            }
            let mut key = [0u8; 32];
            for (i, byte) in key.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                    .map_err(|_| bad("the server key isn't 64 hex digits"))?;
            }
            Some(key)
        }
    };
    Ok(ManagedServer {
        server,
        server_key,
        lock: file.lock,
    })
}

/// Reads the policy at `path`: `Ok(None)` when there is none, an error when
/// one is there but untrusted or unreadable.
pub fn load(path: &Path) -> Result<Option<ManagedServer>, ManagedError> {
    let name = path.display().to_string();
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ManagedError::Unreadable(name, e.to_string())),
    };
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(ManagedError::Unreadable(name, "not a small file".into()));
    }
    trusted(&meta, &name)?;
    let bytes =
        std::fs::read(path).map_err(|e| ManagedError::Unreadable(name.clone(), e.to_string()))?;
    parse(&bytes, &name).map(Some)
}

#[cfg(unix)]
fn trusted(meta: &std::fs::Metadata, name: &str) -> Result<(), ManagedError> {
    use std::os::unix::fs::MetadataExt;
    if meta.uid() == 0 && meta.mode() & 0o022 == 0 {
        Ok(())
    } else {
        Err(ManagedError::NotTrusted(name.to_string()))
    }
}

#[cfg(not(unix))]
fn trusted(_meta: &std::fs::Metadata, name: &str) -> Result<(), ManagedError> {
    Err(ManagedError::NotTrusted(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_names_a_server_and_may_pin_its_key() {
        let p = parse(
            br#"{"v":1,"server":"https://vault.example.org/","server_key":"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff","lock":true}"#,
            "p",
        )
        .unwrap();
        assert_eq!(p.server, "https://vault.example.org");
        assert_eq!(p.server_key.unwrap()[1], 0x11);
        assert!(p.lock);
        let p = parse(br#"{"v":1,"server":"https://a.example"}"#, "p").unwrap();
        assert_eq!((p.server_key, p.lock), (None, false));
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            &br#"{"v":2,"server":"https://a.example"}"#[..],
            br#"{"v":1,"server":""}"#,
            br#"{"v":1,"server":"https://a.example","server_key":"xyz"}"#,
            br#"{"v":1,"server":"https://a.example","extra":1}"#,
            br#"not json"#,
        ] {
            assert!(parse(bad, "p").is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn a_file_anyone_but_root_owns_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.json");
        assert_eq!(load(&path).unwrap(), None);
        std::fs::write(&path, br#"{"v":1,"server":"https://a.example"}"#).unwrap();
        // The test runs as the person (or as root in a container): either
        // way a file the group or others can write is refused.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
            assert!(matches!(load(&path), Err(ManagedError::NotTrusted(_))));
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let owned_by_root = {
                use std::os::unix::fs::MetadataExt;
                std::fs::metadata(&path).unwrap().uid() == 0
            };
            assert_eq!(load(&path).is_ok(), owned_by_root);
        }
    }
}
