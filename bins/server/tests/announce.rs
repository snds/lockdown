//! `--announce` end to end: the server's answer is found by the same
//! browse the app runs (docs/plan/39-first-run.md §1.7 source 4).
//!
//! The test asks a responder on a loopback port, which is exactly the
//! path a real answer takes back to the app (the app asks from an
//! ephemeral port, so every responder answers it directly, RFC 6762 §6.7);
//! only the multicast hop is left out, because whether a CI runner loops
//! multicast back depends on the runner, not on Lockdown. Set
//! `LOCKDOWN_MULTICAST_TEST=1` to also run it over real multicast DNS on
//! port 5353.

use std::net::{Ipv4Addr, UdpSocket};
use std::time::Duration;

#[test]
fn a_browse_finds_an_answering_server() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let at = socket.local_addr().unwrap();
    lockdown_server::announce::serve(
        socket,
        8443,
        "https",
        "lockdown-test".into(),
        Ipv4Addr::new(192, 168, 1, 20),
    )
    .unwrap();
    let found = lockdown_sync::dnssd::browse_at(at, Duration::from_secs(2), 16);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].host, "lockdown-test.local");
    assert_eq!(found[0].base(), "https://lockdown-test.local:8443");
    assert_eq!(
        found[0].addr,
        Some(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)))
    );
}

#[test]
fn a_question_for_anything_else_gets_no_answer() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let at = socket.local_addr().unwrap();
    lockdown_server::announce::serve(socket, 1, "http", "x".into(), Ipv4Addr::LOCALHOST).unwrap();
    let asker = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    asker
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    // Not DNS at all, then a well-formed answer (not a question).
    asker.send_to(b"hello", at).unwrap();
    let answer =
        lockdown_sync::dnssd::announcement(0, false, "y", "y", 1, Ipv4Addr::LOCALHOST, "http")
            .unwrap();
    asker.send_to(&answer, at).unwrap();
    let mut buf = [0u8; 512];
    assert!(asker.recv_from(&mut buf).is_err());
}

#[test]
fn over_real_multicast_when_asked() {
    if std::env::var_os("LOCKDOWN_MULTICAST_TEST").is_none() {
        return;
    }
    let port = 18_787;
    lockdown_server::announce::start(port, "https", "lockdown-test".into(), Ipv4Addr::LOCALHOST)
        .unwrap();
    let found = lockdown_sync::dnssd::browse(Duration::from_secs(2), 16);
    let ours = found
        .iter()
        .find(|s| s.port == port)
        .unwrap_or_else(|| panic!("the announcing server wasn't found: {found:?}"));
    assert_eq!(ours.host, "lockdown-test.local");
}
