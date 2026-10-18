//! The named pipe on a real Windows kernel (the Windows CI runner): both
//! ends in this test process, so the peer each side sees is this test
//! binary, by process id, user, path and hash.

#![cfg(windows)]

use std::time::{Duration, Instant};

use ecosystem_identity::handshake::verify::AllowedBinary;
use ecosystem_identity::handshake::{PeerChannel, PeerIdentity, PeerTransport, PeerVerifier};
use ecosystem_identity::{Error, handshake::wire::MAX_FRAME};
use lockdown_pipe::check::hash_file;
use lockdown_pipe::{PipeTransport, PipeVerifier, pipe_name};

fn fresh_name() -> String {
    let dir = tempfile::tempdir().unwrap();
    pipe_name(dir.path())
}

fn me() -> AllowedBinary {
    let exe = std::env::current_exe().unwrap();
    AllowedBinary {
        app: "lockdown".into(),
        sha256: Some(hash_file(&exe).unwrap()),
        path: exe,
    }
}

#[test]
fn frames_go_both_ways_and_each_side_sees_this_process_as_its_peer() {
    let name = fresh_name();
    let server = PipeTransport::at(&name);
    server.listen().unwrap();
    let client_name = name.clone();
    let client = std::thread::spawn(move || {
        let mut ch = PipeTransport::at(&client_name)
            .connect(Duration::from_secs(5))
            .unwrap();
        let peer = ch.peer_process().cloned().expect("the server's process");
        assert_eq!(peer.pid, std::process::id());
        assert!(peer.same_user);
        ch.send(b"hello").unwrap();
        let answer = ch.recv(Duration::from_secs(5)).unwrap();
        ch.send(&vec![7u8; MAX_FRAME]).unwrap();
        assert_eq!(ch.recv(Duration::from_secs(5)).unwrap(), b"got it");
        (answer, peer)
    });
    let mut ch = server.accept(Duration::from_secs(5)).unwrap();
    assert_eq!(ch.peer(), PeerIdentity::Unknown);
    let peer = ch.peer_process().cloned().expect("the client's process");
    assert_eq!(peer.pid, std::process::id());
    assert!(peer.same_user, "the client runs as this user");
    let verifier = PipeVerifier::new(vec![me()]);
    assert_eq!(
        verifier
            .bind(ch.peer_process())
            .verify(&ch.peer())
            .unwrap()
            .app,
        "lockdown"
    );
    assert_eq!(ch.recv(Duration::from_secs(5)).unwrap(), b"hello");
    ch.send(b"ready").unwrap();
    // The largest frame the socket allows arrives whole.
    assert_eq!(ch.recv(Duration::from_secs(5)).unwrap().len(), MAX_FRAME);
    ch.send(b"got it").unwrap();
    let (answer, client_peer) = client.join().unwrap();
    assert_eq!(answer, b"ready");
    assert!(
        verifier
            .bind(Some(&client_peer))
            .verify(&PeerIdentity::Unknown)
            .is_ok()
    );
}

#[test]
fn another_binary_or_another_hash_is_refused_on_a_real_pipe() {
    let name = fresh_name();
    let server = PipeTransport::at(&name);
    server.listen().unwrap();
    let n = name.clone();
    let client = std::thread::spawn(move || {
        let ch = PipeTransport::at(&n)
            .connect(Duration::from_secs(5))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        drop(ch);
    });
    let ch = server.accept(Duration::from_secs(5)).unwrap();
    let mut wrong_hash = me();
    wrong_hash.sha256 = Some([0; 32]);
    let mut wrong_path = me();
    wrong_path.path = wrong_path.path.with_file_name("lockdown-native-host.exe");
    for allowed in [wrong_hash, wrong_path] {
        assert!(matches!(
            PipeVerifier::new(vec![allowed])
                .bind(ch.peer_process())
                .verify(&ch.peer()),
            Err(Error::PeerUntrusted(_))
        ));
    }
    client.join().unwrap();
}

#[test]
fn a_second_server_on_a_held_name_is_refused() {
    let name = fresh_name();
    let first = PipeTransport::at(&name);
    first.listen().unwrap();
    let second = PipeTransport::at(&name);
    let err = second.listen().unwrap_err();
    assert!(err.to_string().contains("already holds"), "{err}");
}

#[test]
fn accept_times_out_and_a_later_client_still_gets_through() {
    let name = fresh_name();
    let server = PipeTransport::at(&name);
    let started = Instant::now();
    assert!(matches!(
        server.accept(Duration::from_millis(100)),
        Err(Error::Timeout)
    ));
    assert!(started.elapsed() < Duration::from_secs(5));
    let n = name.clone();
    let client = std::thread::spawn(move || {
        let mut ch = PipeTransport::at(&n)
            .connect(Duration::from_secs(5))
            .unwrap();
        ch.send(b"late").unwrap();
        // Stays until the server has read it.
        let _ = ch.recv(Duration::from_secs(5));
    });
    let mut got = None;
    for _ in 0..50 {
        match server.accept(Duration::from_millis(100)) {
            Ok(mut ch) => {
                got = Some(ch.recv(Duration::from_secs(5)).unwrap());
                ch.send(b"ok").unwrap();
                break;
            }
            Err(Error::Timeout) => {}
            Err(e) => panic!("{e}"),
        }
    }
    client.join().unwrap();
    assert_eq!(got.as_deref(), Some(&b"late"[..]));
}

#[test]
fn two_clients_one_after_another_are_both_served() {
    let name = fresh_name();
    let server = PipeTransport::at(&name);
    server.listen().unwrap();
    let n = name.clone();
    let clients = std::thread::spawn(move || {
        for i in 0u8..2 {
            let mut ch = PipeTransport::at(&n)
                .connect(Duration::from_secs(5))
                .unwrap();
            ch.send(&[i]).unwrap();
            assert_eq!(ch.recv(Duration::from_secs(5)).unwrap(), [i + 10]);
        }
    });
    for _ in 0..2 {
        let mut ch = server.accept(Duration::from_secs(5)).unwrap();
        let got = ch.recv(Duration::from_secs(5)).unwrap();
        ch.send(&[got[0] + 10]).unwrap();
    }
    clients.join().unwrap();
}

#[test]
fn no_server_fails_fast_and_a_gone_peer_or_silence_is_reported() {
    let name = fresh_name();
    let started = Instant::now();
    assert!(
        PipeTransport::at(&name)
            .connect(Duration::from_secs(10))
            .is_err()
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "a missing app is reported at once, not after the timeout"
    );

    let server = PipeTransport::at(&name);
    server.listen().unwrap();
    let n = name.clone();
    let client = std::thread::spawn(move || {
        let mut ch = PipeTransport::at(&n)
            .connect(Duration::from_secs(5))
            .unwrap();
        // Silence first: the server's recv times out.
        std::thread::sleep(Duration::from_millis(400));
        // A frame over the limit cannot be sent.
        assert!(ch.send(&vec![0u8; MAX_FRAME + 1]).is_err());
        drop(ch);
    });
    let mut ch = server.accept(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        ch.recv(Duration::from_millis(100)),
        Err(Error::Timeout)
    ));
    client.join().unwrap();
    assert!(matches!(
        ch.recv(Duration::from_secs(5)),
        Err(Error::Handshake(_))
    ));
}
