//! Who is calling: the client's address behind trusted reverse proxies.
//!
//! The login brake (D57) counts per client address and the session list
//! shows where each session was last used. Behind a reverse proxy every
//! connection comes from the proxy, so the server takes the client's
//! address from `X-Forwarded-For`, but only when the connection itself
//! comes from an address the operator listed (`--trusted-proxy`, an IP or
//! a CIDR block). Anyone else's header is ignored: it is just text from
//! the network. The header is read from the right, skipping the trusted
//! hops, so a client cannot put a fake address in front of the real one.

use std::net::IpAddr;

/// An address or a block of them (`10.0.0.0/8`, `fd00::/8`, `127.0.0.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    net: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parses `addr` or `addr/prefix`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (addr, prefix) = match text.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (text, None),
        };
        let net: IpAddr = addr
            .parse()
            .map_err(|_| format!("{text} isn't an IP address or a CIDR block"))?;
        let max = if net.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("{text}: the prefix is 0 to {max}"))?,
            None => max,
        };
        Ok(Self { net, prefix })
    }

    /// Whether `ip` is in the block. An IPv4 address seen as IPv6
    /// (`::ffff:a.b.c.d`) counts as the IPv4 address.
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical(ip);
        match (self.net, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                prefix_eq(&net.octets(), &ip.octets(), self.prefix)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                prefix_eq(&net.octets(), &ip.octets(), self.prefix)
            }
            _ => false,
        }
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

fn prefix_eq(a: &[u8], b: &[u8], prefix: u8) -> bool {
    let full = usize::from(prefix / 8);
    let rest = prefix % 8;
    if a[..full] != b[..full] {
        return false;
    }
    if rest == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rest);
    (a[full] & mask) == (b[full] & mask)
}

/// The client's address: `peer` itself, unless `peer` is a trusted proxy
/// and `forwarded_for` names someone further out.
pub fn client_ip(peer: IpAddr, forwarded_for: Option<&str>, trusted: &[Cidr]) -> IpAddr {
    let peer = canonical(peer);
    let is_trusted = |ip: IpAddr| trusted.iter().any(|c| c.contains(ip));
    if !is_trusted(peer) {
        return peer;
    }
    let Some(header) = forwarded_for else {
        return peer;
    };
    let mut client = peer;
    for hop in header.rsplit(',') {
        let Ok(ip) = hop.trim().parse::<IpAddr>() else {
            // A hop that isn't an address: stop at the last good one.
            break;
        };
        client = canonical(ip);
        if !is_trusted(client) {
            break;
        }
    }
    client
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn blocks_parse_and_match() {
        let b = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(b.contains(ip("10.200.3.4")));
        assert!(!b.contains(ip("11.0.0.1")));
        assert!(b.contains(ip("::ffff:10.1.2.3")));
        let odd = Cidr::parse("172.16.0.0/12").unwrap();
        assert!(odd.contains(ip("172.31.255.255")));
        assert!(!odd.contains(ip("172.32.0.0")));
        let one = Cidr::parse("127.0.0.1").unwrap();
        assert!(one.contains(ip("127.0.0.1")) && !one.contains(ip("127.0.0.2")));
        let v6 = Cidr::parse("fd00::/8").unwrap();
        assert!(v6.contains(ip("fd12::1")) && !v6.contains(ip("fe80::1")));
        assert!(Cidr::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
        for bad in ["", "10.0.0.0/33", "fd00::/129", "nope", "10.0.0.0/x"] {
            assert!(Cidr::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_header_counts_only_from_a_trusted_proxy_and_from_the_right() {
        let trusted = [Cidr::parse("10.0.0.0/8").unwrap()];
        // No proxy: the header is ignored.
        assert_eq!(
            client_ip(ip("198.51.100.9"), Some("1.2.3.4"), &trusted),
            ip("198.51.100.9")
        );
        // Through the proxy: the client it saw.
        assert_eq!(
            client_ip(ip("10.0.0.5"), Some("203.0.113.7"), &trusted),
            ip("203.0.113.7")
        );
        // A client that sends its own header gets it ignored: the proxy
        // appends the real address, which is read first.
        assert_eq!(
            client_ip(ip("10.0.0.5"), Some("6.6.6.6, 203.0.113.7"), &trusted),
            ip("203.0.113.7")
        );
        // Two trusted hops.
        assert_eq!(
            client_ip(ip("10.0.0.5"), Some("203.0.113.7, 10.9.9.9"), &trusted),
            ip("203.0.113.7")
        );
        // No header, or junk at the end: the proxy itself, or the last
        // address that parsed.
        assert_eq!(client_ip(ip("10.0.0.5"), None, &trusted), ip("10.0.0.5"));
        assert_eq!(
            client_ip(ip("10.0.0.5"), Some("203.0.113.7, junk"), &trusted),
            ip("10.0.0.5")
        );
    }
}
