//! `--announce` end to end on this computer: the server's multicast DNS
//! answer is found by the same browse the app runs (docs/plan/39-first-run.md
//! §1.7 source 4). A machine whose network refuses multicast (some CI
//! sandboxes) can't open the socket; the test says so and the codec tests
//! cover the rest.

use std::net::Ipv4Addr;
use std::time::Duration;

#[test]
fn a_browse_finds_an_announcing_server() {
    let port = 18_787;
    match lockdown_server::announce::start(
        port,
        "https",
        "lockdown-test".into(),
        Ipv4Addr::LOCALHOST,
    ) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("multicast DNS isn't available here ({e}); the codec tests cover the rest");
            return;
        }
    }
    let mut found = Vec::new();
    for _ in 0..3 {
        found = lockdown_sync::dnssd::browse(Duration::from_millis(800), 16);
        if found.iter().any(|s| s.port == port) {
            break;
        }
    }
    let ours = found
        .iter()
        .find(|s| s.port == port)
        .unwrap_or_else(|| panic!("the announcing server wasn't found: {found:?}"));
    assert_eq!(ours.host, "lockdown-test.local");
    assert_eq!(ours.base(), format!("https://lockdown-test.local:{port}"));
}
