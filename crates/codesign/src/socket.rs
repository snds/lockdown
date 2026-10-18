//! The browser channel's Unix socket on macOS, with the connecting
//! process's **audit token** (docs/plan/29-release-polish.md §2).
//!
//! The identity crate's `UnixSocketTransport` reports a peer by process id
//! only. A process id can name another program by the time it is checked:
//! a process connects, hands the socket to a helper, and replaces itself
//! with a signed binary under the same id. The audit token the kernel
//! records at `connect` (`LOCAL_PEERTOKEN`) carries the id's version as
//! well, and the Security framework refuses a token whose process has since
//! changed, so the code-signature check reaches the program that really
//! connected.
//!
//! Everything else is the identity crate's socket, unchanged: the socket
//! file is `0600` in a `0700` directory, a stale socket is removed, frames
//! are the same 4-byte big-endian length and 64 KiB limit
//! (`ecosystem_identity::handshake::wire`), and the channel reports the
//! same `PeerIdentity::Unix` (user, process id, executable) so the path and
//! hash check runs as before. It builds on every Unix, so it is tested on
//! Linux too; only macOS reports a token.

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ecosystem_identity::handshake::wire::{read_frame, write_frame};
use ecosystem_identity::handshake::{PeerChannel, PeerIdentity, PeerTransport};
use ecosystem_identity::{Error, Result};

/// A macOS audit token (`audit_token_t`, eight 32-bit words).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditToken(pub [u32; 8]);

impl AuditToken {
    /// The process id inside the token.
    pub fn pid(&self) -> u32 {
        self.0[5]
    }

    /// The token as the bytes `kSecGuestAttributeAudit` takes.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.0) {
            chunk.copy_from_slice(&word.to_ne_bytes());
        }
        out
    }
}

fn at(call: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |e| Error::Io(std::io::Error::new(e.kind(), format!("{call}: {e}")))
}

/// A Unix socket at one path; the app listens and accepts, the host
/// connects. The socket file is removed when the listening side is dropped.
pub struct SocketTransport {
    path: PathBuf,
    listener: Mutex<Option<UnixListener>>,
}

impl std::fmt::Debug for SocketTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SocketTransport({})", self.path.display())
    }
}

impl SocketTransport {
    /// The transport at `path`.
    pub fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            listener: Mutex::new(None),
        }
    }

    /// The socket's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Binds the socket now (user-only), so a host that connects before the
    /// first `accept` is queued, not refused.
    pub fn listen(&self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut guard = self
            .listener
            .lock()
            .map_err(|_| Error::Handshake("the socket listener is poisoned".into()))?;
        if guard.is_some() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(at("create socket directory"))?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(at("chmod socket directory"))?;
        }
        // A socket left by a crashed app: nobody listens if connect fails.
        if self.path.exists() && UnixStream::connect(&self.path).is_err() {
            std::fs::remove_file(&self.path).map_err(at("remove stale socket"))?;
        }
        let listener = UnixListener::bind(&self.path).map_err(at("bind"))?;
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))
            .map_err(at("chmod socket"))?;
        listener
            .set_nonblocking(true)
            .map_err(at("set listener non-blocking"))?;
        *guard = Some(listener);
        Ok(())
    }
}

impl Drop for SocketTransport {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.listener.lock()
            && guard.take().is_some()
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// One connection, with what the kernel said about the other end when it
/// was made.
pub struct SocketChannel {
    stream: UnixStream,
    peer: PeerIdentity,
    token: Option<AuditToken>,
    read_timeout: Duration,
}

impl std::fmt::Debug for SocketChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SocketChannel({:?}, token: {})",
            self.peer,
            self.token.is_some()
        )
    }
}

impl SocketChannel {
    fn new(stream: UnixStream, timeout: Duration) -> Result<Self> {
        stream
            .set_read_timeout(Some(timeout))
            .map_err(at("set read timeout"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(at("set write timeout"))?;
        let (peer, token) = peer_of(&stream);
        Ok(Self {
            stream,
            peer,
            token,
            read_timeout: timeout,
        })
    }

    /// The other end's audit token (macOS); `None` elsewhere, or when the
    /// kernel didn't give one.
    pub fn audit_token(&self) -> Option<AuditToken> {
        self.token
    }

    /// What the signature check needs of the other end: its audit token.
    pub fn peer_code(&self) -> crate::PeerCode {
        self.token
            .map_or(crate::PeerCode::Unknown, crate::PeerCode::AuditToken)
    }
}

impl PeerTransport for SocketTransport {
    type Channel = SocketChannel;

    fn connect(&self, timeout: Duration) -> Result<SocketChannel> {
        let stream = UnixStream::connect(&self.path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                Error::Handshake("nobody is listening on the socket".into())
            } else {
                Error::Io(e)
            }
        })?;
        SocketChannel::new(stream, timeout)
    }

    fn accept(&self, timeout: Duration) -> Result<SocketChannel> {
        self.listen()?;
        let deadline = Instant::now() + timeout;
        loop {
            let accepted = {
                let guard = self
                    .listener
                    .lock()
                    .map_err(|_| Error::Handshake("the socket listener is poisoned".into()))?;
                let listener = guard
                    .as_ref()
                    .ok_or_else(|| Error::Handshake("the socket isn't listening".into()))?;
                listener.accept()
            };
            match accepted {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .map_err(at("set stream blocking"))?;
                    return SocketChannel::new(stream, timeout);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(Error::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(at("accept")(e)),
            }
        }
    }
}

impl PeerChannel for SocketChannel {
    fn send(&mut self, payload: &[u8]) -> Result<()> {
        write_frame(&mut self.stream, payload)
    }

    fn recv(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        // macOS refuses socket options once the peer has hung up; the
        // timeout set at open still bounds the read (as in the identity
        // crate's socket).
        if timeout != self.read_timeout && self.stream.set_read_timeout(Some(timeout)).is_ok() {
            self.read_timeout = timeout;
        }
        read_frame(&mut self.stream)
    }

    fn peer(&self) -> PeerIdentity {
        self.peer.clone()
    }
}

/// What the kernel says about the other end: `SO_PEERCRED` on Linux.
#[cfg(target_os = "linux")]
fn peer_of(stream: &UnixStream) -> (PeerIdentity, Option<AuditToken>) {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` is a valid, writable `ucred` and `len` is its size;
    // getsockopt writes at most `len` bytes into it and updates `len`.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if rc != 0 {
        return (PeerIdentity::Unknown, None);
    }
    (
        PeerIdentity::Unix {
            pid: Some(cred.pid),
            uid: cred.uid,
            gid: cred.gid,
            exe: std::fs::read_link(format!("/proc/{}/exe", cred.pid)).ok(),
        },
        None,
    )
}

/// What the kernel says about the other end on macOS: user and group from
/// `getpeereid`, the audit token from `LOCAL_PEERTOKEN`, the process id
/// from `LOCAL_PEERPID` and its image from `proc_pidpath`.
#[cfg(target_os = "macos")]
fn peer_of(stream: &UnixStream) -> (PeerIdentity, Option<AuditToken>) {
    use std::os::fd::AsRawFd;
    let fd = stream.as_raw_fd();
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    // SAFETY: both out-pointers are valid for the duration of the call.
    if unsafe { libc::getpeereid(fd, &raw mut uid, &raw mut gid) } != 0 {
        return (PeerIdentity::Unknown, None);
    }
    let mut token = [0u32; 8];
    let mut len = std::mem::size_of_val(&token) as libc::socklen_t;
    // SAFETY: `token` is 32 writable bytes and `len` says so; the kernel
    // writes an `audit_token_t` (eight u32) and updates `len`.
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_LOCAL,
            libc::LOCAL_PEERTOKEN,
            token.as_mut_ptr().cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    let token =
        (rc == 0 && len as usize == std::mem::size_of_val(&token)).then_some(AuditToken(token));
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: `pid` is a valid, writable `pid_t` and `len` is its size.
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    let pid = (rc == 0 && pid > 0).then_some(pid);
    (
        PeerIdentity::Unix {
            pid,
            uid,
            gid,
            exe: pid.and_then(image_of),
        },
        token,
    )
}

/// The running image of `pid`.
#[cfg(target_os = "macos")]
fn image_of(pid: i32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buf` is writable for its full length, which is the size we
    // pass; proc_pidpath writes at most that many bytes and returns the
    // length written.
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf)))
}

/// Other Unixes: user and group only.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn peer_of(stream: &UnixStream) -> (PeerIdentity, Option<AuditToken>) {
    use std::os::fd::AsRawFd;
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    // SAFETY: both out-pointers are valid for the duration of the call.
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &raw mut uid, &raw mut gid) } != 0 {
        return (PeerIdentity::Unknown, None);
    }
    (
        PeerIdentity::Unix {
            pid: None,
            uid,
            gid,
            exe: None,
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::handshake::wire::MAX_FRAME;

    #[test]
    fn frames_go_both_ways_and_each_end_sees_this_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sock/browser.sock");
        let server = SocketTransport::at(&path);
        server.listen().unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        let client = std::thread::spawn({
            let path = path.clone();
            move || {
                let mut ch = SocketTransport::at(&path)
                    .connect(Duration::from_secs(5))
                    .unwrap();
                ch.send(b"hello").unwrap();
                let big = ch.recv(Duration::from_secs(5)).unwrap();
                assert_eq!(big.len(), MAX_FRAME);
                (ch.peer(), ch.audit_token())
            }
        });
        let mut ch = server.accept(Duration::from_secs(5)).unwrap();
        assert_eq!(ch.recv(Duration::from_secs(5)).unwrap(), b"hello");
        ch.send(&vec![7u8; MAX_FRAME]).unwrap();
        assert!(ch.send(&vec![7u8; MAX_FRAME + 1]).is_err());
        let (client_saw, client_token) = client.join().unwrap();
        for seen in [ch.peer(), client_saw] {
            match seen {
                PeerIdentity::Unix { pid, uid, exe, .. } => {
                    // SAFETY: getuid has no preconditions.
                    assert_eq!(uid, unsafe { libc::getuid() });
                    if cfg!(any(target_os = "linux", target_os = "macos")) {
                        assert_eq!(pid, Some(std::process::id() as i32));
                        let exe = exe.unwrap().canonicalize().unwrap();
                        assert_eq!(
                            exe,
                            std::env::current_exe().unwrap().canonicalize().unwrap()
                        );
                    }
                }
                other => panic!("no peer credentials: {other:?}"),
            }
        }
        if cfg!(target_os = "macos") {
            assert_eq!(ch.audit_token().unwrap().pid(), std::process::id());
            assert_eq!(client_token.unwrap().pid(), std::process::id());
        } else {
            assert!(ch.audit_token().is_none() && client_token.is_none());
        }
        drop(ch);
        drop(server);
        assert!(!path.exists(), "the socket goes with the listener");
    }

    #[test]
    fn a_stale_socket_is_replaced_and_nobody_listening_fails_fast() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("browser.sock");
        assert!(
            SocketTransport::at(&path)
                .connect(Duration::from_secs(1))
                .is_err()
        );
        // A socket file nobody listens on, as a crashed app leaves it.
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        let server = SocketTransport::at(&path);
        server.listen().unwrap();
        assert!(matches!(
            server.accept(Duration::from_millis(50)),
            Err(Error::Timeout)
        ));
        assert!(
            SocketTransport::at(&path)
                .connect(Duration::from_secs(1))
                .is_ok()
        );
    }

    #[test]
    fn a_token_is_eight_native_words_with_the_pid_sixth() {
        let t = AuditToken([1, 2, 3, 4, 5, 4242, 7, 8]);
        assert_eq!(t.pid(), 4242);
        let b = t.to_bytes();
        assert_eq!(&b[20..24], &4242u32.to_ne_bytes());
    }
}
