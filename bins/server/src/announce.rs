//! The server tells devices on the local network that it is here
//! (`--announce`, docs/plan/39-first-run.md §1.7 source 4, D471): it answers
//! multicast DNS questions for `_lockdown._tcp.local` with its host name,
//! port and scheme. Nothing else: no account, no name of a person, no
//! version. It is on by default when the server listens on a network
//! address and off on loopback, where no other device could reach it.
//!
//! The answers are built and questions read by `lockdown_sync::dnssd`,
//! which bounds everything it reads. A question from a port other than
//! 5353 is answered to that port alone, with its id and question echoed
//! (RFC 6762 §6.7), which is how Lockdown's own browse asks.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use lockdown_sync::dnssd;

/// Whether to announce, as the flag says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announce {
    /// On for a network address, off on loopback.
    Auto,
    /// Always.
    On,
    /// Never.
    Off,
}

impl Announce {
    /// Whether a server listening on `listen` announces.
    pub fn applies(self, listen: IpAddr) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Auto => !listen.is_loopback(),
        }
    }
}

/// The address other devices on the network reach this one at: the one a
/// packet to the multicast group would leave from.
pub fn outward_address() -> Option<Ipv4Addr> {
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    probe.connect((dnssd::MDNS_GROUP, dnssd::MDNS_PORT)).ok()?;
    match probe.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// This computer's name as a host label (`nas`), from the environment or
/// `/etc/hostname`, else `lockdown`.
pub fn host_label() -> String {
    let raw = ["HOSTNAME", "COMPUTERNAME"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    let label: String = raw
        .trim()
        .split('.')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect();
    if label.is_empty() {
        "lockdown".into()
    } else {
        label.to_ascii_lowercase()
    }
}

fn socket() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    s.set_reuse_port(true)?;
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, dnssd::MDNS_PORT)).into())?;
    let s: UdpSocket = s.into();
    s.join_multicast_v4(&dnssd::MDNS_GROUP, &Ipv4Addr::UNSPECIFIED)?;
    s.set_multicast_ttl_v4(255)?;
    Ok(s)
}

/// Starts answering on port 5353 on a thread of its own. Returns why it
/// couldn't.
pub fn start(port: u16, scheme: &'static str, host: String, addr: Ipv4Addr) -> Result<(), String> {
    let socket = socket().map_err(|e| format!("couldn't listen for multicast DNS: {e}"))?;
    let instance = format!("Lockdown on {host}");
    // Say so once at start, as responders do.
    if let Ok(msg) = dnssd::announcement(0, false, &instance, &host, port, addr, scheme) {
        let _ = socket.send_to(&msg, (dnssd::MDNS_GROUP, dnssd::MDNS_PORT));
    }
    serve(socket, port, scheme, host, addr)
}

/// Answers questions arriving on `socket` on a thread of its own: the
/// multicast socket [`start`] opens, or, in a test, a loopback one.
pub fn serve(
    socket: UdpSocket,
    port: u16,
    scheme: &'static str,
    host: String,
    addr: Ipv4Addr,
) -> Result<(), String> {
    let instance = format!("Lockdown on {host}");
    std::thread::Builder::new()
        .name("lockdown-announce".into())
        .spawn(move || {
            let mut buf = vec![0u8; dnssd::MAX_MESSAGE];
            loop {
                let Ok((n, from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((answer, to)) =
                    dnssd::respond(&buf[..n], from.port(), &instance, &host, port, addr, scheme)
                else {
                    continue;
                };
                let _ = match to {
                    dnssd::ReplyTo::Asker => socket.send_to(&answer, from),
                    dnssd::ReplyTo::Group => {
                        socket.send_to(&answer, (dnssd::MDNS_GROUP, dnssd::MDNS_PORT))
                    }
                };
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_announces_on_a_network_address_only() {
        assert!(!Announce::Auto.applies("127.0.0.1".parse().unwrap()));
        assert!(!Announce::Auto.applies("::1".parse().unwrap()));
        assert!(Announce::Auto.applies("0.0.0.0".parse().unwrap()));
        assert!(Announce::Auto.applies("192.168.1.5".parse().unwrap()));
        assert!(Announce::On.applies("127.0.0.1".parse().unwrap()));
        assert!(!Announce::Off.applies("192.168.1.5".parse().unwrap()));
    }

    #[test]
    fn the_host_label_is_one_safe_label() {
        let label = host_label();
        assert!(!label.is_empty() && label.len() <= 63);
        assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
