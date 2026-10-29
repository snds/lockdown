//! Finding Lockdown servers by name (docs/plan/39-first-run.md §1.7, D468,
//! D471): DNS-SD for `_lockdown._tcp` on the local network (multicast DNS,
//! RFC 6762 and 6763), and the SRV record `_lockdown._tcp.<domain>` for an
//! email's domain (RFC 2782).
//!
//! The codec here builds the few messages Lockdown sends (a PTR question,
//! an SRV question, a server's answer about itself) and reads answers.
//! Every answer is hostile: a message is at most [`MAX_MESSAGE`] bytes, a
//! name at most 255 bytes, compression pointers may only point backwards
//! and at most [`MAX_JUMPS`] times, counts are bounded by what the message
//! can hold, and anything that doesn't fit is dropped rather than guessed.
//! Nothing read here is trusted beyond "a candidate to show": the client
//! then fetches the server's own document and the person confirms its key.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The service type, under the local domain.
pub const SERVICE: &str = "_lockdown._tcp.local";
/// The service type label pair, for a unicast domain (`_lockdown._tcp.<domain>`).
pub const SERVICE_LABELS: &str = "_lockdown._tcp";
/// Multicast DNS's group and port.
pub const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// Multicast DNS's port.
pub const MDNS_PORT: u16 = 5353;
/// The largest message read or written.
pub const MAX_MESSAGE: usize = 9000;
/// The most compression jumps followed in one name.
pub const MAX_JUMPS: usize = 16;
/// The most records read from one message.
const MAX_RECORDS: usize = 64;

const T_A: u16 = 1;
const T_PTR: u16 = 12;
const T_TXT: u16 = 16;
const T_AAAA: u16 = 28;
const T_SRV: u16 = 33;
const CLASS_IN: u16 = 1;
/// The top bit of a class: "unicast response wanted" in a question, "cache
/// flush" in an answer.
const CLASS_TOP: u16 = 0x8000;

/// What a record says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Data {
    /// A pointer to an instance name.
    Ptr(String),
    /// Where an instance is served.
    Srv {
        /// Its port.
        port: u16,
        /// The host name.
        target: String,
    },
    /// The instance's key=value strings.
    Txt(Vec<String>),
    /// An IPv4 address.
    A(Ipv4Addr),
    /// An IPv6 address.
    Aaaa(Ipv6Addr),
}

/// One record of an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The owner name, lower case, no trailing dot.
    pub name: String,
    /// What it says.
    pub data: Data,
}

/// One question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// The name asked about, lower case.
    pub name: String,
    /// The type asked for.
    pub kind: u16,
}

/// A message, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    /// Its id.
    pub id: u16,
    /// Whether it is a response.
    pub response: bool,
    /// The questions.
    pub questions: Vec<Question>,
    /// Every answer, authority and additional record this codec reads.
    pub records: Vec<Record>,
}

/// A Lockdown server an answer described.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    /// The instance name (`Lockdown on nas._lockdown._tcp.local`).
    pub instance: String,
    /// The host name (`nas.local`), no trailing dot.
    pub host: String,
    /// The port.
    pub port: u16,
    /// An address for the host, when the answer carried one.
    pub addr: Option<IpAddr>,
    /// `https` or `http`, from the TXT record's `scheme=` (https when absent).
    pub scheme: String,
}

impl Service {
    /// The base address a client fetches from.
    pub fn base(&self) -> String {
        let host = if self.host.is_empty() {
            match self.addr {
                Some(IpAddr::V6(a)) => format!("[{a}]"),
                Some(IpAddr::V4(a)) => a.to_string(),
                None => String::new(),
            }
        } else {
            self.host.clone()
        };
        if (self.scheme == "https" && self.port == 443)
            || (self.scheme == "http" && self.port == 80)
        {
            format!("{}://{host}", self.scheme)
        } else {
            format!("{}://{host}:{}", self.scheme, self.port)
        }
    }
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// Writes a name as labels. Refuses a label over 63 bytes or a name over 255.
fn put_name(out: &mut Vec<u8>, name: &str) -> Result<(), String> {
    let name = name.trim_end_matches('.');
    if name.len() > 253 {
        return Err("a name is too long".into());
    }
    for label in name.split('.').filter(|l| !l.is_empty()) {
        if label.len() > 63 {
            return Err("a label is too long".into());
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    Ok(())
}

fn header(out: &mut Vec<u8>, id: u16, flags: u16, qd: u16, an: u16, ar: u16) {
    put_u16(out, id);
    put_u16(out, flags);
    put_u16(out, qd);
    put_u16(out, an);
    put_u16(out, 0);
    put_u16(out, ar);
}

/// A question for `name` of type `kind`. `unicast` asks a multicast
/// responder to answer the asker directly.
fn question(
    id: u16,
    name: &str,
    kind: u16,
    unicast: bool,
    recursion: bool,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(64);
    header(&mut out, id, if recursion { 0x0100 } else { 0 }, 1, 0, 0);
    put_name(&mut out, name)?;
    put_u16(&mut out, kind);
    put_u16(&mut out, CLASS_IN | if unicast { CLASS_TOP } else { 0 });
    Ok(out)
}

/// The multicast question for every Lockdown server on the network.
pub fn browse_query() -> Vec<u8> {
    question(0, SERVICE, T_PTR, true, false).unwrap_or_default()
}

/// The unicast question for `_lockdown._tcp.<domain>`'s SRV record.
pub fn srv_query(id: u16, domain: &str) -> Result<Vec<u8>, String> {
    question(
        id,
        &format!("{SERVICE_LABELS}.{domain}"),
        T_SRV,
        false,
        true,
    )
}

/// Reads a name at `at`, following compression pointers backwards only.
/// Returns the name and where the name's own bytes end.
fn read_name(msg: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut end = None;
    let mut jumps = 0;
    loop {
        let len = *msg.get(at)? as usize;
        match len & 0xC0 {
            0x00 => {
                if len == 0 {
                    return Some((labels.join("."), end.unwrap_or(at + 1)));
                }
                let label = msg.get(at + 1..at + 1 + len)?;
                total += len + 1;
                if total > 255 {
                    return None;
                }
                labels.push(String::from_utf8_lossy(label).to_ascii_lowercase());
                at += 1 + len;
            }
            0xC0 => {
                let ptr = ((len & 0x3F) << 8) | *msg.get(at + 1)? as usize;
                if ptr >= at || jumps >= MAX_JUMPS {
                    return None;
                }
                jumps += 1;
                end.get_or_insert(at + 2);
                at = ptr;
            }
            _ => return None,
        }
    }
}

fn u16_at(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*msg.get(at)?, *msg.get(at + 1)?]))
}

/// Reads a message. `None` for anything malformed.
pub fn parse(msg: &[u8]) -> Option<Message> {
    if msg.len() < 12 || msg.len() > MAX_MESSAGE {
        return None;
    }
    let id = u16_at(msg, 0)?;
    let flags = u16_at(msg, 2)?;
    let qd = u16_at(msg, 4)? as usize;
    let rr = u16_at(msg, 6)? as usize + u16_at(msg, 8)? as usize + u16_at(msg, 10)? as usize;
    // Each question takes at least 5 bytes and each record 11.
    if qd * 5 + rr * 11 > msg.len() - 12 {
        return None;
    }
    let mut at = 12;
    let mut questions = Vec::new();
    for _ in 0..qd {
        let (name, next) = read_name(msg, at)?;
        let kind = u16_at(msg, next)?;
        questions.push(Question { name, kind });
        at = next + 4;
    }
    let mut records = Vec::new();
    for _ in 0..rr {
        let (name, next) = read_name(msg, at)?;
        let kind = u16_at(msg, next)?;
        let class = u16_at(msg, next + 2)? & !CLASS_TOP;
        let len = u16_at(msg, next + 8)? as usize;
        let start = next + 10;
        let rdata = msg.get(start..start + len)?;
        at = start + len;
        if class != CLASS_IN || records.len() >= MAX_RECORDS {
            continue;
        }
        let data = match kind {
            T_PTR => Data::Ptr(read_name(msg, start)?.0),
            T_SRV if len >= 7 => Data::Srv {
                port: u16_at(rdata, 4)?,
                target: read_name(msg, start + 6)?.0,
            },
            T_TXT => {
                let mut strings = Vec::new();
                let mut i = 0;
                while i < rdata.len() {
                    let n = rdata[i] as usize;
                    let s = rdata.get(i + 1..i + 1 + n)?;
                    strings.push(String::from_utf8_lossy(s).into_owned());
                    i += 1 + n;
                }
                Data::Txt(strings)
            }
            T_A if len == 4 => Data::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
            T_AAAA if len == 16 => {
                let mut b = [0u8; 16];
                b.copy_from_slice(rdata);
                Data::Aaaa(Ipv6Addr::from(b))
            }
            _ => continue,
        };
        records.push(Record { name, data });
    }
    Some(Message {
        id,
        response: flags & 0x8000 != 0,
        questions,
        records,
    })
}

/// Whether a host name is one Lockdown shows and asks: printable, short,
/// letters, digits, hyphens and dots.
pub fn plausible_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// The servers `records` describe under `service` (the PTR's owner):
/// instance → SRV → address, with each piece checked. At most `max`.
pub fn services(records: &[Record], service: &str, max: usize) -> Vec<Service> {
    let service = service.trim_end_matches('.').to_ascii_lowercase();
    let mut out: Vec<Service> = Vec::new();
    for r in records {
        let Data::Ptr(instance) = &r.data else {
            continue;
        };
        if r.name != service || out.iter().any(|s| &s.instance == instance) {
            continue;
        }
        let Some((port, target)) = records.iter().find_map(|s| match &s.data {
            Data::Srv { port, target } if &s.name == instance => Some((*port, target.clone())),
            _ => None,
        }) else {
            continue;
        };
        if port == 0 || !plausible_host(&target) {
            continue;
        }
        let addr = records.iter().find_map(|a| match a.data {
            Data::A(ip) if a.name == target => Some(IpAddr::V4(ip)),
            Data::Aaaa(ip) if a.name == target => Some(IpAddr::V6(ip)),
            _ => None,
        });
        let scheme = records
            .iter()
            .find_map(|t| match &t.data {
                Data::Txt(strings) if &t.name == instance => strings
                    .iter()
                    .find_map(|s| s.strip_prefix("scheme=").map(str::to_string)),
                _ => None,
            })
            .filter(|s| s == "http" || s == "https")
            .unwrap_or_else(|| "https".into());
        out.push(Service {
            instance: instance.clone(),
            host: target,
            port,
            addr,
            scheme,
        });
        if out.len() >= max {
            break;
        }
    }
    out
}

/// The SRV targets of an answer to [`srv_query`], lowest priority first,
/// then by host (no other order is trusted).
pub fn srv_targets(msg: &Message, name: &str, max: usize) -> Vec<(String, u16)> {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    let mut out: Vec<(String, u16)> = msg
        .records
        .iter()
        .filter_map(|r| match &r.data {
            Data::Srv { port, target }
                if r.name == name && *port != 0 && plausible_host(target) =>
            {
                Some((target.clone(), *port))
            }
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out.truncate(max);
    out
}

/// Whether a message asks for Lockdown servers (a PTR, or any type, for
/// [`SERVICE`]).
pub fn asks_for_lockdown(msg: &Message) -> bool {
    !msg.response
        && msg
            .questions
            .iter()
            .any(|q| q.name == SERVICE && (q.kind == T_PTR || q.kind == 255))
}

/// What a server says about itself: a PTR to its instance, the SRV with
/// its host and port, a TXT with its scheme, and its address. `id` and
/// `question` are echoed for a legacy unicast asker (RFC 6762 §6.7).
pub fn announcement(
    id: u16,
    echo_question: bool,
    instance_label: &str,
    host: &str,
    port: u16,
    addr: Ipv4Addr,
    scheme: &str,
) -> Result<Vec<u8>, String> {
    let instance = format!("{instance_label}.{SERVICE}");
    let host = format!("{}.local", host.trim_end_matches(".local"));
    let mut out = Vec::with_capacity(256);
    header(&mut out, id, 0x8400, u16::from(echo_question), 1, 3);
    if echo_question {
        put_name(&mut out, SERVICE)?;
        put_u16(&mut out, T_PTR);
        put_u16(&mut out, CLASS_IN);
    }
    let record = |out: &mut Vec<u8>,
                  name: &str,
                  kind: u16,
                  flush: bool,
                  data: &[u8]|
     -> Result<(), String> {
        put_name(out, name)?;
        put_u16(out, kind);
        put_u16(out, CLASS_IN | if flush { CLASS_TOP } else { 0 });
        out.extend_from_slice(&120u32.to_be_bytes());
        put_u16(out, data.len() as u16);
        out.extend_from_slice(data);
        Ok(())
    };
    let mut ptr = Vec::new();
    put_name(&mut ptr, &instance)?;
    record(&mut out, SERVICE, T_PTR, false, &ptr)?;
    let mut srv = Vec::new();
    put_u16(&mut srv, 0);
    put_u16(&mut srv, 0);
    put_u16(&mut srv, port);
    put_name(&mut srv, &host)?;
    record(&mut out, &instance, T_SRV, true, &srv)?;
    let txt_line = format!("scheme={scheme}");
    let mut txt = vec![txt_line.len() as u8];
    txt.extend_from_slice(txt_line.as_bytes());
    record(&mut out, &instance, T_TXT, true, &txt)?;
    record(&mut out, &host, T_A, true, &addr.octets())?;
    Ok(out)
}

/// The nameservers this computer uses, from `/etc/resolv.conf`.
pub fn system_nameservers(conf: &str) -> Vec<IpAddr> {
    conf.lines()
        .filter_map(|l| l.trim().strip_prefix("nameserver"))
        .filter_map(|rest| rest.trim().split(['%', ' ']).next()?.parse().ok())
        .take(3)
        .collect()
}

/// Asks the network for Lockdown servers for `wait`, from an ephemeral
/// port, so responders answer this socket directly (RFC 6762 §6.7). Only
/// answers from the local network are read.
#[cfg(feature = "http")]
pub fn browse(wait: std::time::Duration, max: usize) -> Vec<Service> {
    use std::net::UdpSocket;
    let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else {
        return Vec::new();
    };
    let _ = socket.set_multicast_ttl_v4(255);
    if socket
        .send_to(&browse_query(), (MDNS_GROUP, MDNS_PORT))
        .is_err()
    {
        return Vec::new();
    }
    let own = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|probe| {
            probe.connect((MDNS_GROUP, MDNS_PORT))?;
            probe.local_addr()
        })
        .ok()
        .and_then(|a| match a.ip() {
            IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
            _ => None,
        });
    let until = std::time::Instant::now() + wait;
    let mut records = Vec::new();
    let mut buf = vec![0u8; MAX_MESSAGE];
    while let Some(left) = until.checked_duration_since(std::time::Instant::now()) {
        if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
            break;
        }
        let Ok((n, from)) = socket.recv_from(&mut buf) else {
            break;
        };
        // Multicast DNS answers come from this network only: a private,
        // link-local or loopback address, or one in this computer's own /24.
        let local = match from.ip() {
            IpAddr::V4(ip) => {
                ip.is_private()
                    || ip.is_link_local()
                    || ip.is_loopback()
                    || own.is_some_and(|o: Ipv4Addr| o.octets()[..3] == ip.octets()[..3])
            }
            IpAddr::V6(ip) => ip.is_loopback() || ip.is_unicast_link_local(),
        };
        if !local {
            continue;
        }
        if let Some(msg) = parse(&buf[..n]).filter(|m| m.response) {
            records.extend(msg.records);
            if records.len() > MAX_RECORDS * 4 {
                break;
            }
        }
    }
    services(&records, SERVICE, max)
}

/// Looks up `_lockdown._tcp.<domain>` with this computer's resolver,
/// waiting at most `wait`. Empty when there is no answer or no resolver.
#[cfg(feature = "http")]
pub fn srv_lookup(domain: &str, wait: std::time::Duration, max: usize) -> Vec<(String, u16)> {
    use std::net::UdpSocket;
    if !plausible_host(domain) {
        return Vec::new();
    }
    let conf = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let Some(server) = system_nameservers(&conf).into_iter().next() else {
        return Vec::new();
    };
    let bind: IpAddr = match server {
        IpAddr::V4(_) => Ipv4Addr::UNSPECIFIED.into(),
        IpAddr::V6(_) => Ipv6Addr::UNSPECIFIED.into(),
    };
    let Ok(socket) = UdpSocket::bind((bind, 0)) else {
        return Vec::new();
    };
    let id = lockdown_crypto::rand::random_16()
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .unwrap_or(0x4c44);
    let name = format!("{SERVICE_LABELS}.{domain}");
    let Ok(query) = srv_query(id, domain) else {
        return Vec::new();
    };
    if socket.connect((server, 53)).is_err()
        || socket.send(&query).is_err()
        || socket.set_read_timeout(Some(wait)).is_err()
    {
        return Vec::new();
    }
    let mut buf = vec![0u8; MAX_MESSAGE];
    let Ok(n) = socket.recv(&mut buf) else {
        return Vec::new();
    };
    match parse(&buf[..n]) {
        Some(msg)
            if msg.id == id
                && msg.response
                && msg
                    .questions
                    .iter()
                    .any(|q| q.name == name.to_ascii_lowercase()) =>
        {
            srv_targets(&msg, &name, max)
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_announcement_reads_back_as_one_service() {
        let msg = announcement(
            0,
            false,
            "Lockdown on nas",
            "nas",
            8443,
            Ipv4Addr::new(192, 168, 1, 20),
            "https",
        )
        .unwrap();
        let read = parse(&msg).unwrap();
        assert!(read.response);
        let found = services(&read.records, SERVICE, 16);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].host, "nas.local");
        assert_eq!(found[0].port, 8443);
        assert_eq!(
            found[0].addr,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)))
        );
        assert_eq!(found[0].base(), "https://nas.local:8443");
        // Echoed for a legacy unicast asker.
        let echoed =
            parse(&announcement(7, true, "x", "h", 1, Ipv4Addr::LOCALHOST, "http").unwrap())
                .unwrap();
        assert_eq!(echoed.id, 7);
        assert_eq!(echoed.questions[0].name, SERVICE);
        assert_eq!(
            services(&echoed.records, SERVICE, 16)[0].base(),
            "http://h.local:1"
        );
    }

    #[test]
    fn the_browse_question_is_recognised() {
        let q = parse(&browse_query()).unwrap();
        assert!(asks_for_lockdown(&q));
        let other = parse(&question(0, "_http._tcp.local", T_PTR, false, false).unwrap()).unwrap();
        assert!(!asks_for_lockdown(&other));
    }

    #[test]
    fn hostile_messages_are_refused_or_trimmed() {
        // Too short, a forward pointer, a pointer loop, counts that can't fit.
        assert!(parse(&[0; 11]).is_none());
        let mut fwd = vec![0, 0, 0x84, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        fwd.extend_from_slice(&[0xC0, 20, 0, 12, 0, 1]);
        assert!(parse(&fwd).is_none());
        let mut lp = vec![0, 0, 0x84, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        lp.extend_from_slice(&[0xC0, 12, 0, 12, 0, 1]);
        assert!(parse(&lp).is_none());
        let huge = [0u8, 0, 0x84, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0];
        assert!(parse(&huge).is_none());
        assert!(parse(&vec![0u8; MAX_MESSAGE + 1]).is_none());
        // A label longer than the message.
        let mut cut = vec![0, 0, 0x84, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        cut.extend_from_slice(&[60, b'a']);
        assert!(parse(&cut).is_none());
        // An SRV naming a host with odd characters, or port 0, is dropped.
        let mut records = vec![Record {
            name: SERVICE.into(),
            data: Data::Ptr("x._lockdown._tcp.local".into()),
        }];
        records.push(Record {
            name: "x._lockdown._tcp.local".into(),
            data: Data::Srv {
                port: 443,
                target: "evil host/<script>".into(),
            },
        });
        assert!(services(&records, SERVICE, 16).is_empty());
        // A flood is capped.
        let mut many = Vec::new();
        for i in 0..40 {
            let inst = format!("s{i}._lockdown._tcp.local");
            many.push(Record {
                name: SERVICE.into(),
                data: Data::Ptr(inst.clone()),
            });
            many.push(Record {
                name: inst,
                data: Data::Srv {
                    port: 1,
                    target: format!("h{i}.local"),
                },
            });
        }
        assert_eq!(services(&many, SERVICE, 16).len(), 16);
    }

    #[test]
    fn srv_answers_are_read_in_a_fixed_order() {
        let name = "_lockdown._tcp.example.com";
        let msg = Message {
            id: 1,
            response: true,
            questions: vec![],
            records: vec![
                Record {
                    name: name.into(),
                    data: Data::Srv {
                        port: 443,
                        target: "b.example.com".into(),
                    },
                },
                Record {
                    name: name.into(),
                    data: Data::Srv {
                        port: 443,
                        target: "a.example.com".into(),
                    },
                },
                Record {
                    name: "other".into(),
                    data: Data::Srv {
                        port: 443,
                        target: "c.example.com".into(),
                    },
                },
            ],
        };
        assert_eq!(
            srv_targets(&msg, name, 16),
            vec![
                ("a.example.com".to_string(), 443),
                ("b.example.com".to_string(), 443)
            ]
        );
        let q = parse(&srv_query(9, "example.com").unwrap()).unwrap();
        assert_eq!(q.questions[0].name, name);
        assert_eq!(q.questions[0].kind, T_SRV);
    }

    #[test]
    fn nameservers_come_from_resolv_conf() {
        let conf = "# x\nnameserver 192.168.1.1\nnameserver fe80::1%en0\nsearch lan\n";
        assert_eq!(
            system_nameservers(conf),
            vec![
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
                "fe80::1".parse::<IpAddr>().unwrap()
            ]
        );
    }
}
