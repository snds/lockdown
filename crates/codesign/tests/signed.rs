//! Signed builds checking each other over the real channel, both ways
//! (docs/plan/29-release-polish.md §2). This test binary plays the desktop
//! app; `examples/peer.rs` plays the browser host.
//!
//! It runs only when CI has signed things for it
//! (`LOCKDOWN_CODESIGN_TEST=require`, the `codesign` jobs in
//! `.github/workflows/ci.yml`): this test binary and one peer with a
//! throwaway certificate A, another peer with a throwaway certificate B, one
//! peer left unsigned, and on macOS one peer signed with A under another
//! identifier. The certificates are made in the job and thrown away with it;
//! none is a real identity. Elsewhere, and on a developer's machine, it
//! says so and passes, because there is nothing signed to check.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use ecosystem_identity::handshake::{PeerChannel, PeerTransport};
use lockdown_codesign::{HOST_IDENTIFIER, PeerCode, Policy, SignatureVerifier, own_policy};

fn peer(name: &str) -> Option<PathBuf> {
    std::env::var_os(format!("LOCKDOWN_CODESIGN_PEER_{name}"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// What happened when one peer connected.
struct Outcome {
    /// This end's verdict on the peer.
    ours: Result<(), String>,
    /// The peer's verdict on this end, as it sent it.
    theirs: String,
    /// The peer's exit code (0 trusted us, 3 refused us).
    code: Option<i32>,
}

#[cfg(unix)]
fn listen(dir: &std::path::Path) -> (lockdown_codesign::SocketTransport, String) {
    let path = dir.join("signed.sock");
    let t = lockdown_codesign::SocketTransport::at(&path);
    t.listen().unwrap();
    (t, path.display().to_string())
}

#[cfg(unix)]
fn code_of(ch: &lockdown_codesign::SocketChannel) -> PeerCode {
    ch.peer_code()
}

#[cfg(windows)]
fn listen(dir: &std::path::Path) -> (lockdown_pipe::PipeTransport, String) {
    let name = lockdown_pipe::pipe_name(dir);
    let t = lockdown_pipe::PipeTransport::at(&name);
    t.listen().unwrap();
    (t, name)
}

#[cfg(windows)]
fn code_of(ch: &lockdown_pipe::PipeChannel) -> PeerCode {
    ch.peer_process()
        .and_then(|p| p.exe.clone())
        .map_or(PeerCode::Unknown, PeerCode::Image)
}

fn run(policy: &Arc<Policy>, peer: &PathBuf) -> Outcome {
    let dir = tempfile::tempdir().unwrap();
    let (transport, address) = listen(dir.path());
    let child = Command::new(peer)
        .arg(&address)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("couldn't start {}: {e}", peer.display()));
    let mut ch = transport
        .accept(Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("{} didn't connect: {e}", peer.display()));
    let ours = SignatureVerifier::new(Arc::clone(policy), code_of(&ch)).check();
    let theirs = String::from_utf8_lossy(&ch.recv(Duration::from_secs(60)).unwrap()).into_owned();
    let reply = match &ours {
        Ok(()) => "trusted".to_string(),
        Err(why) => format!("refused: {why}"),
    };
    ch.send(reply.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    println!(
        "{}: ours {reply}; theirs {theirs}; it heard {}",
        peer.display(),
        String::from_utf8_lossy(&out.stdout).trim()
    );
    Outcome {
        ours,
        theirs,
        code: out.status.code(),
    }
}

#[test]
fn signed_builds_accept_the_same_signer_and_refuse_another_both_ways() {
    if std::env::var("LOCKDOWN_CODESIGN_TEST").as_deref() != Ok("require") {
        println!("not run: no signed binaries (CI's codesign jobs sign them)");
        return;
    }
    let policy = Arc::new(own_policy(HOST_IDENTIFIER));
    assert_eq!(
        policy.kind(),
        "signature",
        "this test binary must be signed for the check to run: {}",
        policy.describe()
    );
    println!("policy: {}", policy.describe());

    let same = peer("SAME").expect("LOCKDOWN_CODESIGN_PEER_SAME");
    let o = run(&policy, &same);
    assert!(o.ours.is_ok(), "the same signer was refused: {:?}", o.ours);
    assert_eq!(o.theirs, "trusted", "the same-signer peer refused this end");
    assert_eq!(o.code, Some(0));

    let other = peer("OTHER").expect("LOCKDOWN_CODESIGN_PEER_OTHER");
    let o = run(&policy, &other);
    let why = o.ours.expect_err("another signer was trusted");
    assert!(
        why.contains("not") || why.contains("refused"),
        "unexpected reason: {why}"
    );
    assert!(
        o.theirs.starts_with("refused"),
        "the other signer trusted this end: {}",
        o.theirs
    );
    assert_eq!(o.code, Some(3));

    let unsigned = peer("UNSIGNED").expect("LOCKDOWN_CODESIGN_PEER_UNSIGNED");
    let o = run(&policy, &unsigned);
    assert!(
        o.ours.is_err(),
        "an unsigned peer was trusted by a signed build"
    );
    // The unsigned peer itself has no signer to compare with, so it leaves
    // the decision to the path and hash check (which this test leaves out).
    assert_eq!(o.theirs, "trusted");

    if let Some(wrong_id) = peer("WRONG_ID") {
        let o = run(&policy, &wrong_id);
        assert!(
            o.ours.is_err(),
            "a peer signed by the same certificate under another identifier was trusted"
        );
        assert_eq!(o.theirs, "trusted");
    }
}
