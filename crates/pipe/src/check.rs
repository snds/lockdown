//! Who is on the other end of a pipe, by the rules the identity crate's
//! `ProcessIdentityVerifier` applies on a Unix socket (03-architecture §7,
//! D4, D33): the peer runs as this user, its executable is one of the
//! allowed binaries by path, and when the allow-list carries a SHA-256, the
//! file at that path hashes to it. Either side runs the check before it
//! relays or answers a byte.
//!
//! The transport fills a [`PeerProcess`] from the pipe
//! (`GetNamedPipeClientProcessId` or `GetNamedPipeServerProcessId`, then
//! the process's token and image name) and keeps the process open for as
//! long as the channel lives, so the id cannot be reused by another
//! process during the check. The decision itself is here, without any
//! Win32, so it runs in tests on every platform.

use std::path::{Path, PathBuf};

use ecosystem_identity::handshake::verify::AllowedBinary;
use ecosystem_identity::handshake::{
    PeerIdentity, PeerVerifier, ProcessIdentityVerifier, TrustedPeer,
};
use ecosystem_identity::{Error, Result};

/// What the transport learned about the process on the other end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerProcess {
    /// Its process id.
    pub pid: u32,
    /// Whether its token's user is this process's user. `false` also when
    /// the token could not be read.
    pub same_user: bool,
    /// Its executable, when the image name could be read.
    pub exe: Option<PathBuf>,
}

/// Checks `peer` against `allowed` by user, path and hash; `hash` reads a
/// file's SHA-256 (the real check passes [`hash_file`]).
pub fn check(
    allowed: &[AllowedBinary],
    peer: &PeerProcess,
    hash: &dyn Fn(&Path) -> Result<[u8; 32]>,
) -> Result<TrustedPeer> {
    if !peer.same_user {
        return Err(Error::PeerUntrusted(
            "the app on the other end runs as another user".into(),
        ));
    }
    let Some(exe) = &peer.exe else {
        return Err(Error::PeerUntrusted(
            "the other app's executable couldn't be read".into(),
        ));
    };
    let exe = exe.canonicalize().unwrap_or_else(|_| exe.clone());
    let Some(found) = allowed.iter().find(|a| {
        let want = a.path.canonicalize().unwrap_or_else(|_| a.path.clone());
        same_path(&want, &exe)
    }) else {
        return Err(Error::PeerUntrusted(format!(
            "{} isn't an installed ecosystem app",
            exe.display()
        )));
    };
    if let Some(want) = found.sha256 {
        let have = hash(&exe).map_err(|_| {
            Error::PeerUntrusted("the other app's executable couldn't be hashed".into())
        })?;
        if have != want {
            return Err(Error::PeerUntrusted(format!(
                "{} isn't the binary the ecosystem installed",
                exe.display()
            )));
        }
    }
    Ok(TrustedPeer {
        app: found.app.clone(),
    })
}

/// SHA-256 of a file on disk: the identity crate's own reader.
pub fn hash_file(path: &Path) -> Result<[u8; 32]> {
    ProcessIdentityVerifier::hash_of(path)
}

/// Whether two canonical paths name the same file. Windows paths compare
/// without regard to case (NTFS's default); elsewhere byte for byte.
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// The allow-list a side checks its pipe peer against.
#[derive(Clone, Debug, Default)]
pub struct PipeVerifier {
    allowed: Vec<AllowedBinary>,
}

impl PipeVerifier {
    /// A verifier that accepts these binaries.
    pub fn new(allowed: Vec<AllowedBinary>) -> Self {
        Self { allowed }
    }

    /// The binaries it accepts.
    pub fn allowed(&self) -> &[AllowedBinary] {
        &self.allowed
    }

    /// A verifier for one channel, over what that channel's transport
    /// learned about its peer (`None` when it learned nothing, which is
    /// refused).
    pub fn bind(&self, peer: Option<&PeerProcess>) -> BoundVerifier {
        BoundVerifier {
            allowed: self.allowed.clone(),
            peer: peer.cloned(),
        }
    }
}

/// A [`PeerVerifier`] for one pipe channel. The identity crate passes the
/// channel's `PeerIdentity`, which on a pipe is always `Unknown`; this
/// verifier checks the process the pipe itself reported instead, so it must
/// be made from the channel it is used for and from no other.
#[derive(Clone, Debug)]
pub struct BoundVerifier {
    allowed: Vec<AllowedBinary>,
    peer: Option<PeerProcess>,
}

impl PeerVerifier for BoundVerifier {
    fn verify(&self, identity: &PeerIdentity) -> Result<TrustedPeer> {
        // A channel that does know its peer the identity crate's way is not
        // a pipe channel; this verifier was bound to the wrong one.
        if !matches!(identity, PeerIdentity::Unknown) {
            return Err(Error::PeerUntrusted(
                "this check was made for a different connection".into(),
            ));
        }
        let Some(peer) = &self.peer else {
            return Err(Error::PeerUntrusted(
                "the pipe didn't say which process is on the other end".into(),
            ));
        };
        check(&self.allowed, peer, &hash_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> (PathBuf, [u8; 32]) {
        let exe = std::env::current_exe().unwrap();
        let hash = hash_file(&exe).unwrap();
        (exe, hash)
    }

    fn allowed(path: PathBuf, sha256: Option<[u8; 32]>) -> Vec<AllowedBinary> {
        vec![AllowedBinary {
            app: "native-host".into(),
            path,
            sha256,
        }]
    }

    fn peer(exe: Option<PathBuf>, same_user: bool) -> PeerProcess {
        PeerProcess {
            pid: std::process::id(),
            same_user,
            exe,
        }
    }

    #[test]
    fn the_allowed_binary_by_path_and_hash_is_trusted() {
        let (exe, hash) = me();
        let ok = check(
            &allowed(exe.clone(), Some(hash)),
            &peer(Some(exe.clone()), true),
            &hash_file,
        )
        .unwrap();
        assert_eq!(ok.app, "native-host");
        // A path-only entry (a development build) trusts the path alone.
        assert!(
            check(
                &allowed(exe.clone(), None),
                &peer(Some(exe), true),
                &hash_file
            )
            .is_ok()
        );
    }

    #[test]
    fn another_user_an_unknown_image_another_path_or_another_hash_is_refused() {
        let (exe, hash) = me();
        let list = allowed(exe.clone(), Some(hash));
        for (who, why) in [
            (peer(Some(exe.clone()), false), "another user"),
            (peer(None, true), "couldn't be read"),
            (
                peer(Some(exe.with_file_name("not-the-host.exe")), true),
                "isn't an installed",
            ),
        ] {
            match check(&list, &who, &hash_file) {
                Err(Error::PeerUntrusted(m)) => assert!(m.contains(why), "{m}"),
                other => panic!("{who:?} was not refused: {other:?}"),
            }
        }
        match check(
            &allowed(exe.clone(), Some([0; 32])),
            &peer(Some(exe.clone()), true),
            &hash_file,
        ) {
            Err(Error::PeerUntrusted(m)) => assert!(m.contains("isn't the binary"), "{m}"),
            other => panic!("a wrong hash was not refused: {other:?}"),
        }
        let unreadable = |_: &Path| -> Result<[u8; 32]> { Err(Error::Timeout) };
        assert!(matches!(
            check(&list, &peer(Some(exe), true), &unreadable),
            Err(Error::PeerUntrusted(_))
        ));
    }

    #[test]
    fn a_symlinked_or_relative_spelling_of_the_same_file_matches() {
        let (exe, hash) = me();
        let dir = exe.parent().unwrap();
        let dotted = dir.join(".").join(exe.file_name().unwrap());
        assert!(
            check(
                &allowed(dotted, Some(hash)),
                &peer(Some(exe), true),
                &hash_file
            )
            .is_ok()
        );
    }

    #[test]
    fn a_bound_verifier_checks_its_own_peer_and_refuses_without_one() {
        let (exe, hash) = me();
        let v = PipeVerifier::new(allowed(exe.clone(), Some(hash)));
        assert_eq!(v.allowed().len(), 1);
        let bound = v.bind(Some(&peer(Some(exe), true)));
        assert_eq!(
            bound.verify(&PeerIdentity::Unknown).unwrap().app,
            "native-host"
        );
        assert!(bound.verify(&PeerIdentity::InProcess).is_err());
        assert!(v.bind(None).verify(&PeerIdentity::Unknown).is_err());
    }

    #[test]
    fn windows_paths_compare_without_case_and_others_with_it() {
        let a = Path::new("C:/Program Files/Lockdown/lockdown-native-host.exe");
        let b = Path::new("c:/program files/lockdown/LOCKDOWN-NATIVE-HOST.EXE");
        assert_eq!(same_path(a, b), cfg!(windows));
        assert!(same_path(a, a));
    }
}
