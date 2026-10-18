//! The local channel on Windows (docs/plan/24-release.md §3,
//! 04-security §13): what the Unix socket in the identity crate is on
//! Linux and macOS, as a named pipe.
//!
//! ```text
//! lockdown-native-host ──\\.\pipe\snds-lockdown-browser-<hash>──▶ desktop app
//!   (client: CreateFileW,           user-only DACL,              (server: CreateNamedPipeW,
//!    identification only)           local clients only)           first instance only)
//! ```
//!
//! - [`pipe_name`]: where the two meet, derived from Lockdown's data
//!   directory so two profiles never share a pipe.
//! - [`check`]: who is on the other end, by the Unix socket's rules: the
//!   same user, an allowed executable by path, and that file's SHA-256.
//!   Pure, so it is tested on every platform.
//! - `windows` (Windows only): the transport itself, implementing the
//!   identity crate's [`PeerTransport`] and [`PeerChannel`] with the same
//!   4-byte big-endian length framing and 64 KiB frame limit as the socket.
//! - `registry` (Windows only): the per-user registry value each browser
//!   finds the host manifest by.
//!
//! The identity crate's `PeerIdentity` has no Windows variant (it is
//! `non_exhaustive` and only the identity crate can add one), so a pipe
//! channel reports `PeerIdentity::Unknown`, which every identity-crate
//! verifier refuses, and carries what it learned about its peer beside it
//! ([`check::PeerProcess`]). [`check::PipeVerifier::bind`] turns that into
//! a [`PeerVerifier`] for that one channel.
//!
//! [`PeerTransport`]: ecosystem_identity::handshake::PeerTransport
//! [`PeerChannel`]: ecosystem_identity::handshake::PeerChannel
//! [`PeerVerifier`]: ecosystem_identity::handshake::PeerVerifier

#![cfg_attr(not(windows), forbid(unsafe_code))]

use std::path::Path;

use sha2::{Digest, Sha256};

pub mod check;
#[cfg(windows)]
pub mod registry;
#[cfg(windows)]
pub mod windows;

pub use check::{BoundVerifier, PeerProcess, PipeVerifier};
#[cfg(windows)]
pub use windows::{PipeChannel, PipeTransport};

/// The prefix every Lockdown browser pipe name starts with.
pub const PIPE_PREFIX: &str = r"\\.\pipe\snds-lockdown-browser-";

/// The browser channel's pipe for the data directory `data` (D47): the
/// prefix and the first 16 hex digits of SHA-256 over the directory's
/// path. Deterministic, so the app can name it in the endpoint file, and
/// distinct per data directory, so a second profile gets its own.
pub fn pipe_name(data: &Path) -> String {
    let digest = Sha256::digest(data.as_os_str().as_encoded_bytes());
    let short = data_encoding::HEXLOWER.encode(&digest[..8]);
    format!("{PIPE_PREFIX}{short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_data_directory_gets_its_own_short_pipe_name() {
        let a = pipe_name(Path::new(r"C:\Users\me\AppData\Roaming\snds\lockdown"));
        let b = pipe_name(Path::new(r"C:\Users\you\AppData\Roaming\snds\lockdown"));
        assert!(a.starts_with(PIPE_PREFIX));
        assert_eq!(a.len(), PIPE_PREFIX.len() + 16);
        assert_ne!(a, b);
        assert_eq!(
            a,
            pipe_name(Path::new(r"C:\Users\me\AppData\Roaming\snds\lockdown"))
        );
        // Pipe names are at most 256 characters; this one is far below.
        assert!(a.len() < 64);
    }
}
