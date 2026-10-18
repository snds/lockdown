//! Email from the server (`account-alerts`; docs/plan/25-self-host-2.md §5):
//! an SMTP setting, an outbox that sends on its own thread, and the plain
//! templates the server sends: a new device to approve, a new device that
//! signed in, a security key added or removed.
//!
//! **What is in a message.** The account's login name (the address it goes
//! to), the device's own name for itself, the address it came from, the
//! time, and links to this server's own pages. Never a token, a key, a code
//! other than the one-time approval link's, or anything from a vault (the
//! server has none of it). The log names a message's kind and whether it
//! went, never its address.
//!
//! **SMTP.** `smtps://host:465` is TLS from the first byte; `smtp://host:587`
//! must upgrade with `STARTTLS` (only a relay on this machine may stay
//! plain). TLS is `lockdown-tls`'s client (TLS 1.3, the hybrid post-quantum
//! group first), trusting the system's roots or `--smtp-ca`. `AUTH PLAIN`
//! is sent only inside TLS (or to this machine). Every header value is
//! checked for line breaks; the body travels base64-encoded, so no line of
//! it can end the message early.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::Duration;

use zeroize::Zeroizing;

/// One message, as the outbox queues it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    /// What it is about, for the log (`verify_device`, `new_device`, …).
    pub kind: &'static str,
    /// The address it goes to.
    pub to: String,
    /// The subject.
    pub subject: String,
    /// The body, plain text.
    pub body: String,
}

/// Something that delivers mail.
pub trait Mailer: Send + Sync {
    /// Sends one message; an error says why in words.
    fn send(&self, mail: &Mail) -> Result<(), String>;
}

/// How to reach the SMTP relay.
#[derive(Clone)]
pub struct SmtpConfig {
    /// `smtps://host[:465]` or `smtp://host[:587]` (STARTTLS).
    pub url: String,
    /// The account on the relay, when it asks for one.
    pub user: Option<String>,
    /// Its password.
    pub password: Option<Zeroizing<String>>,
    /// `From:`, as `Name <address>` or a bare address.
    pub from: String,
    /// Certificates to trust instead of the system's (a private relay).
    pub ca: Option<Vec<rustls::pki_types::CertificateDer<'static>>>,
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("url", &self.url)
            .field("from", &self.from)
            .field("user", &self.user.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Security {
    /// TLS from the start.
    Implicit,
    /// Plain, then `STARTTLS` (required unless the relay is this machine).
    StartTls,
}

/// The SMTP client.
pub struct SmtpMailer {
    host: String,
    port: u16,
    security: Security,
    local: bool,
    user: Option<String>,
    password: Option<Zeroizing<String>>,
    from_header: String,
    from_address: String,
    tls: Arc<rustls::ClientConfig>,
}

impl std::fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SmtpMailer({}:{})", self.host, self.port)
    }
}

/// Whether text may go in a header: no line breaks, no other controls.
fn header_safe(text: &str) -> bool {
    !text.chars().any(|c| c.is_control())
}

/// The address inside `Name <address>`, or the bare address.
fn address_of(from: &str) -> Option<String> {
    let addr = match (from.rfind('<'), from.rfind('>')) {
        (Some(a), Some(b)) if a < b => &from[a + 1..b],
        _ => from,
    };
    let addr = addr.trim();
    let ok = crate::breach_monitor::looks_like_email(addr)
        && header_safe(addr)
        && !addr.contains(['<', '>', '"', ',', ';', '(', ')', '\\']);
    ok.then(|| addr.to_string())
}

impl SmtpMailer {
    /// A client for `config`; refuses settings that would send in the clear
    /// across a network or let a header be forged.
    pub fn new(config: SmtpConfig) -> Result<Self, String> {
        let url = url::Url::parse(config.url.trim()).map_err(|_| {
            format!(
                "'{}' isn't an SMTP address (smtps://host or smtp://host)",
                config.url
            )
        })?;
        let (security, default_port) = match url.scheme() {
            "smtps" => (Security::Implicit, 465),
            "smtp" => (Security::StartTls, 587),
            other => {
                return Err(format!(
                    "an SMTP relay can't be {other}://; use smtps:// or smtp://"
                ));
            }
        };
        let host = url
            .host_str()
            .ok_or("the SMTP address names no host")?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let local = host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false);
        if !header_safe(&config.from) {
            return Err("the From address holds a line break".into());
        }
        let from_address = address_of(&config.from)
            .ok_or_else(|| format!("'{}' isn't a From address", config.from))?;
        if config.user.is_some() != config.password.is_some() {
            return Err("give both the SMTP user and its password, or neither".into());
        }
        let trust = match &config.ca {
            Some(certs) => lockdown_tls::Trust::Only(certs.clone()),
            None => lockdown_tls::Trust::Platform,
        };
        let tls = lockdown_tls::client_config(&trust).map_err(|e| e.to_string())?;
        Ok(Self {
            host,
            port: url.port().unwrap_or(default_port),
            security,
            local,
            user: config.user,
            password: config.password,
            from_header: config.from.trim().to_string(),
            from_address,
            tls: Arc::new(tls),
        })
    }

    /// Where it sends, for the operator.
    pub fn describe(&self) -> String {
        let how = match self.security {
            Security::Implicit => "TLS",
            Security::StartTls if self.local => "STARTTLS when offered (this machine)",
            Security::StartTls => "STARTTLS",
        };
        format!(
            "{}:{} ({how}) from {}",
            self.host, self.port, self.from_address
        )
    }
}

/// A line-oriented connection, plain or TLS.
trait Wire: Read + Write {}
impl<T: Read + Write> Wire for T {}

struct Conn {
    reader: BufReader<Box<dyn Wire>>,
}

impl Conn {
    fn write(&mut self, line: &str) -> Result<(), String> {
        let w = self.reader.get_mut();
        w.write_all(line.as_bytes())
            .and_then(|()| w.write_all(b"\r\n"))
            .and_then(|()| w.flush())
            .map_err(|_| "the relay closed the connection".to_string())
    }

    /// One reply (every line of a multi-line one): its code and its text.
    fn reply(&mut self) -> Result<(u16, String), String> {
        let mut text = String::new();
        loop {
            let mut line = String::new();
            let n = (&mut self.reader)
                .take(4096)
                .read_line(&mut line)
                .map_err(|_| "the relay didn't answer".to_string())?;
            if n == 0 || line.len() < 4 {
                return Err("the relay closed the connection".into());
            }
            let code: u16 = line[..3]
                .parse()
                .map_err(|_| "the relay's answer didn't read".to_string())?;
            text.push_str(line[4..].trim_end());
            text.push('\n');
            if line.as_bytes()[3] != b'-' {
                return Ok((code, text));
            }
            if text.len() > 64 << 10 {
                return Err("the relay's answer is too long".into());
            }
        }
    }

    fn expect(&mut self, codes: &[u16], step: &str) -> Result<String, String> {
        let (code, text) = self.reply()?;
        if codes.contains(&code) {
            Ok(text)
        } else {
            Err(format!("the relay refused {step} ({code})"))
        }
    }

    fn command(&mut self, line: &str, codes: &[u16], step: &str) -> Result<String, String> {
        self.write(line)?;
        self.expect(codes, step)
    }
}

/// `Sun, 18 Oct 2026 09:30:00 +0000`.
fn rfc5322_date(secs: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = lockdown_sync::engine::civil_date(secs);
    let (y, rest) = date.split_at(4);
    let m: usize = rest[1..3].parse().unwrap_or(1);
    let d: u32 = rest[4..6].parse().unwrap_or(1);
    let weekday = DAYS[usize::try_from(secs.div_euclid(86_400).rem_euclid(7)).unwrap_or(0)];
    let rem = secs.rem_euclid(86_400);
    format!(
        "{weekday}, {d:02} {} {y} {:02}:{:02}:{:02} +0000",
        MONTHS[m.clamp(1, 12) - 1],
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// A header value: plain when it is printable ASCII, else RFC 2047 base64.
fn encode_header(text: &str) -> String {
    if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        text.to_string()
    } else {
        format!(
            "=?UTF-8?B?{}?=",
            data_encoding::BASE64.encode(text.as_bytes())
        )
    }
}

/// The whole message, headers and a base64 body wrapped at 76 columns.
pub fn message(from: &str, mail: &Mail, now: i64, id: &str) -> Result<String, String> {
    if !header_safe(&mail.to) || !header_safe(&mail.subject) || !header_safe(from) {
        return Err("a header holds a line break".into());
    }
    let domain = address_of(from)
        .and_then(|a| a.split_once('@').map(|(_, d)| d.to_string()))
        .unwrap_or_else(|| "localhost".into());
    let body = data_encoding::BASE64.encode(mail.body.replace('\n', "\r\n").as_bytes());
    let mut out = String::new();
    out.push_str(&format!("From: {from}\r\n"));
    out.push_str(&format!("To: <{}>\r\n", mail.to));
    out.push_str(&format!("Subject: {}\r\n", encode_header(&mail.subject)));
    out.push_str(&format!("Date: {}\r\n", rfc5322_date(now)));
    out.push_str(&format!("Message-ID: <{id}@{domain}>\r\n"));
    out.push_str("Auto-Submitted: auto-generated\r\n");
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    out.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
    for chunk in body.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push_str("\r\n");
    }
    Ok(out)
}

impl SmtpMailer {
    fn tls_wrap(&self, tcp: TcpStream) -> Result<Box<dyn Wire>, String> {
        let name = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|_| "the relay's name isn't one TLS can check".to_string())?;
        let conn = rustls::ClientConnection::new(self.tls.clone(), name)
            .map_err(|e| format!("TLS to the relay: {e}"))?;
        Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
    }

    fn connect(&self) -> Result<TcpStream, String> {
        let addrs = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|_| format!("the relay {} wasn't found", self.host))?;
        let mut last = String::from("the relay has no address");
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, Duration::from_secs(15)) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_secs(60)))
                        .and_then(|()| s.set_write_timeout(Some(Duration::from_secs(60))))
                        .map_err(|e| e.to_string())?;
                    return Ok(s);
                }
                Err(_) => {
                    last = format!("the relay {}:{} couldn't be reached", self.host, self.port)
                }
            }
        }
        Err(last)
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, mail: &Mail) -> Result<(), String> {
        if address_of(&mail.to).as_deref() != Some(mail.to.as_str()) {
            return Err("that isn't an address mail can go to".into());
        }
        let now = lockdown_vault::Timestamp::now().0;
        let id = data_encoding::HEXLOWER
            .encode(&lockdown_crypto::rand::random_16().map_err(|e| e.to_string())?);
        let text = message(&self.from_header, mail, now, &id)?;
        let tcp = self.connect()?;
        let mut tls_on = self.security == Security::Implicit;
        let wire: Box<dyn Wire> = if tls_on {
            self.tls_wrap(tcp.try_clone().map_err(|e| e.to_string())?)?
        } else {
            Box::new(tcp.try_clone().map_err(|e| e.to_string())?)
        };
        let mut c = Conn {
            reader: BufReader::new(wire),
        };
        c.expect(&[220], "the connection")?;
        let ehlo = format!(
            "EHLO {}",
            self.from_address.split('@').nth(1).unwrap_or("localhost")
        );
        let caps = c.command(&ehlo, &[250], "EHLO")?;
        if !tls_on {
            let offered = caps
                .lines()
                .any(|l| l.trim().eq_ignore_ascii_case("STARTTLS"));
            if offered {
                c.command("STARTTLS", &[220], "STARTTLS")?;
                drop(c);
                c = Conn {
                    reader: BufReader::new(self.tls_wrap(tcp)?),
                };
                tls_on = true;
                c.command(&ehlo, &[250], "EHLO")?;
            } else if !self.local {
                return Err(
                    "the relay doesn't offer STARTTLS; mail isn't sent in the clear".into(),
                );
            }
        }
        if let (Some(user), Some(password)) = (&self.user, &self.password) {
            if !tls_on && !self.local {
                return Err("the relay's password is only sent inside TLS".into());
            }
            let token = Zeroizing::new(format!("\0{user}\0{}", password.as_str()));
            let line = Zeroizing::new(format!(
                "AUTH PLAIN {}",
                data_encoding::BASE64.encode(token.as_bytes())
            ));
            c.command(&line, &[235], "the sign-in")?;
        }
        c.command(
            &format!("MAIL FROM:<{}>", self.from_address),
            &[250],
            "the sender",
        )?;
        c.command(
            &format!("RCPT TO:<{}>", mail.to),
            &[250, 251],
            "the recipient",
        )?;
        c.command("DATA", &[354], "the message")?;
        // The body is base64 and the headers checked, so no line is a lone
        // "."; the terminator ends it.
        let w = c.reader.get_mut();
        w.write_all(text.as_bytes())
            .and_then(|()| w.write_all(b".\r\n"))
            .and_then(|()| w.flush())
            .map_err(|_| "the relay closed the connection".to_string())?;
        c.expect(&[250], "the message")?;
        let _ = c.command("QUIT", &[221], "QUIT");
        Ok(())
    }
}

// --- the outbox ----------------------------------------------------------------------

/// Messages wait here and go out on a thread of their own, so a slow relay
/// never holds a request up.
pub struct Outbox {
    tx: SyncSender<Mail>,
}

impl std::fmt::Debug for Outbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Outbox")
    }
}

/// How many messages may wait at once.
pub const OUTBOX_MAX: usize = 1_000;

impl Outbox {
    /// Starts the sending thread over `mailer`.
    pub fn start(mailer: Box<dyn Mailer>) -> std::io::Result<Self> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Mail>(OUTBOX_MAX);
        std::thread::Builder::new()
            .name("lockdown-mail".into())
            .spawn(move || {
                for mail in rx {
                    let mut tries = 0;
                    loop {
                        tries += 1;
                        match mailer.send(&mail) {
                            Ok(()) => {
                                eprintln!("lockdown-server: sent a {} email", mail.kind);
                                break;
                            }
                            Err(why) if tries < 3 => {
                                eprintln!(
                                    "lockdown-server: a {} email didn't go ({why}); trying again",
                                    mail.kind
                                );
                                std::thread::sleep(Duration::from_secs(5 * tries));
                            }
                            Err(why) => {
                                eprintln!(
                                    "lockdown-server: a {} email didn't go and was dropped ({why})",
                                    mail.kind
                                );
                                break;
                            }
                        }
                    }
                }
            })?;
        Ok(Self { tx })
    }

    /// Queues a message; `false` when the outbox is full (the message is
    /// dropped and the log says so).
    pub fn queue(&self, mail: Mail) -> bool {
        match self.tx.try_send(mail) {
            Ok(()) => true,
            Err(TrySendError::Full(m)) | Err(TrySendError::Disconnected(m)) => {
                eprintln!(
                    "lockdown-server: a {} email was dropped (the outbox is full)",
                    m.kind
                );
                false
            }
        }
    }
}

// --- the server's mail settings ------------------------------------------------------------

/// How many approval emails one account may be sent an hour (a stolen
/// password must not turn into a flood of mail).
pub const VERIFY_PER_HOUR: usize = 5;

/// What the server does with email.
pub struct MailSettings {
    /// Where messages wait.
    pub outbox: Outbox,
    /// The server's public address, for links (`https://vault.example`):
    /// set by the operator, never taken from a request's `Host`.
    pub public_url: String,
    /// Whether a device new to an account waits for the account's email to
    /// approve it (on unless the operator turned it off).
    pub verify_new_devices: bool,
    /// Where mail goes, in words, for the admin page.
    pub describe: String,
    sent: std::sync::Mutex<std::collections::BTreeMap<lockdown_vault::Id, Vec<i64>>>,
}

impl std::fmt::Debug for MailSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailSettings")
            .field("public_url", &self.public_url)
            .field("verify_new_devices", &self.verify_new_devices)
            .finish_non_exhaustive()
    }
}

impl MailSettings {
    /// Settings over `outbox`. The public URL must be `https://` (or
    /// `http://` to this machine), with no path.
    pub fn new(
        outbox: Outbox,
        public_url: &str,
        verify_new_devices: bool,
        describe: String,
    ) -> Result<Self, String> {
        let url = url::Url::parse(public_url.trim())
            .map_err(|_| format!("'{public_url}' isn't the server's public address"))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            return Err("the public address must be https:// (links in email go there)".into());
        }
        if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
            return Err("give the public address as https://host[:port], with no path".into());
        }
        Ok(Self {
            outbox,
            public_url: url.as_str().trim_end_matches('/').to_string(),
            verify_new_devices,
            describe,
            sent: std::sync::Mutex::default(),
        })
    }

    /// Takes one of the account's approval emails for this hour; `false`
    /// when it has had its share.
    pub fn take_verification(&self, account_id: lockdown_vault::Id, now: i64) -> bool {
        let mut sent = self
            .sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sent.retain(|_, times| {
            times.retain(|t| now - t < 3600);
            !times.is_empty()
        });
        let times = sent.entry(account_id).or_default();
        if times.len() >= VERIFY_PER_HOUR {
            return false;
        }
        times.push(now);
        true
    }
}

// --- the templates ---------------------------------------------------------------------

/// What the templates say about where a sign-in came from.
#[derive(Clone, Debug, Default)]
pub struct SignInFacts {
    /// The device's own name for itself (`lockdown/0.1.0 (macos)`).
    pub device: String,
    /// The address it came from.
    pub address: String,
    /// When (seconds since the Unix epoch).
    pub at: i64,
}

/// `2026-10-18 09:30 UTC`.
pub fn when(secs: i64) -> String {
    let rem = secs.rem_euclid(86_400);
    format!(
        "{} {:02}:{:02} UTC",
        lockdown_sync::engine::civil_date(secs),
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// The account's session list, where any sign-in can be ended.
fn sessions_line(public_url: &str) -> String {
    format!(
        "See where your account is signed in, and end any of it: in the Lockdown app, Devices, then Signed in; or in the web vault at {}/vault/#/devices",
        public_url.trim_end_matches('/')
    )
}

/// The message asking the account's owner to approve a new device.
pub fn verify_device(
    to: &str,
    facts: &SignInFacts,
    link: &str,
    public_url: &str,
    minutes: i64,
) -> Mail {
    Mail {
        kind: "verify_device",
        to: to.to_string(),
        subject: "Approve a new device for your Lockdown account".into(),
        body: format!(
            "A device Lockdown hasn't seen before is signing in to your account ({to}). The password was right; the device waits for you.\n\n\
             Device: {}\nFrom: {}\nWhen: {}\n\n\
             If this is you, open this link and choose Approve (it works once, for {minutes} minutes):\n{link}\n\n\
             If it isn't you, don't open the link. Someone has your account password: change it from one of your devices (Settings, then Account), and keep your Secret Key and recovery kit to yourself; the password alone doesn't open your vault.\n\n\
             {}\n\n\
             This message was sent by your Lockdown server. It never contains your password, your Secret Key or anything from your vault.\n",
            facts.device,
            facts.address,
            when(facts.at),
            sessions_line(public_url),
        ),
    }
}

/// The message telling the account a new device signed in.
pub fn new_device(to: &str, facts: &SignInFacts, public_url: &str) -> Mail {
    Mail {
        kind: "new_device",
        to: to.to_string(),
        subject: "A new device signed in to your Lockdown account".into(),
        body: format!(
            "A device that hadn't signed in to your Lockdown account ({to}) before just did.\n\n\
             Device: {}\nFrom: {}\nWhen: {}\n\n\
             If this was you, there's nothing to do.\n\n\
             If it wasn't: end that sign-in now, then change your account password from one of your devices (Settings, then Account), and revoke the device in Devices.\n\
             {}\n\n\
             This message was sent by your Lockdown server. It never contains your password, your Secret Key or anything from your vault.\n",
            facts.device,
            facts.address,
            when(facts.at),
            sessions_line(public_url),
        ),
    }
}

/// The message telling the account a security key was added or removed.
pub fn security_key_changed(
    to: &str,
    added: bool,
    key_name: &str,
    at: i64,
    public_url: &str,
) -> Mail {
    let (subject, what) = if added {
        (
            "A security key was added to your Lockdown account",
            "was added to",
        )
    } else {
        (
            "A security key was removed from your Lockdown account",
            "was removed from",
        )
    };
    let name: String = key_name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    Mail {
        kind: "security_key",
        to: to.to_string(),
        subject: subject.into(),
        body: format!(
            "The security key \"{name}\" {what} your Lockdown account ({to}) at {}.\n\n\
             Security keys are what a new device must show after the password. If you didn't make this change, someone is signed in to your account: end the sessions you don't know, change your account password, and look at your security keys in Settings, then Account.\n\
             {}\n\n\
             This message was sent by your Lockdown server. It never contains your password, your Secret Key or anything from your vault.\n",
            when(at),
            sessions_line(public_url),
        ),
    }
}

/// `sean@example.com` as `s•••@example.com`: enough to recognise it.
pub fn masked(address: &str) -> String {
    match address.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}•••@{domain}")
        }
        None => "your address".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_headers_are_written_safely() {
        assert_eq!(rfc5322_date(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        assert_eq!(
            rfc5322_date(1_792_316_980),
            "Sun, 18 Oct 2026 09:49:40 +0000"
        );
        assert_eq!(encode_header("Hi"), "Hi");
        assert_eq!(encode_header("Hé"), "=?UTF-8?B?SMOp?=");
        let mail = Mail {
            kind: "t",
            to: "me@example.com".into(),
            subject: "x\r\nBcc: evil@example.com".into(),
            body: "b".into(),
        };
        assert!(message("Lockdown <l@example.com>", &mail, 0, "1").is_err());
        assert_eq!(
            address_of("Lockdown <l@example.com>").as_deref(),
            Some("l@example.com")
        );
        assert!(address_of("a@b.com>, evil@x.com").is_none());
        assert_eq!(masked("sean@example.com"), "s•••@example.com");
    }

    #[test]
    fn plain_relays_are_for_this_machine_only() {
        let config = |url: &str| SmtpConfig {
            url: url.into(),
            user: None,
            password: None,
            from: "Lockdown <lockdown@example.com>".into(),
            ca: None,
        };
        assert!(SmtpMailer::new(config("smtp://127.0.0.1:2525")).is_ok());
        assert!(SmtpMailer::new(config("smtps://mail.example.com")).is_ok());
        assert!(SmtpMailer::new(config("http://mail.example.com")).is_err());
        let mut no_from = config("smtps://mail.example.com");
        no_from.from = "Lockdown".into();
        assert!(SmtpMailer::new(no_from).is_err());
        let mut half = config("smtps://mail.example.com");
        half.user = Some("u".into());
        assert!(SmtpMailer::new(half).is_err());
    }

    #[test]
    fn the_templates_hold_no_secrets_and_link_the_session_list() {
        let facts = SignInFacts {
            device: "lockdown/0.1.0 (linux)".into(),
            address: "192.0.2.7".into(),
            at: 1_792_316_980,
        };
        let m = verify_device(
            "sean@example.com",
            &facts,
            "https://vault.example/approve/x?code=y",
            "https://vault.example",
            15,
        );
        assert!(m.body.contains("https://vault.example/approve/x?code=y"));
        assert!(m.body.contains("https://vault.example/vault/#/devices"));
        assert!(m.body.contains("2026-10-18 09:49 UTC"));
        let n = new_device("sean@example.com", &facts, "https://vault.example/");
        assert!(n.body.contains("https://vault.example/vault/#/devices"));
        let k = security_key_changed(
            "sean@example.com",
            true,
            "Blue\nkey",
            0,
            "https://v.example",
        );
        assert!(k.body.contains("\"Bluekey\" was added"));
        assert!(k.subject.contains("added"));
    }
}
