//! Finding a Lockdown server without typing it (docs/plan/39-first-run.md
//! §1.7, D468–D470).
//!
//! Discovery **finds and describes**; it never signs in, never sends a
//! token or a byte of the vault, and moves nothing. The sources, in rank
//! order:
//!
//! 1. an administrator's policy file (`lockdown_policy::managed`);
//! 2. this computer's config: the account `sync.cbor` names, and the
//!    server Settings suggests;
//! 3. a server on this computer (`127.0.0.1:8787` and `[::1]:8787`);
//! 4. the local network (DNS-SD `_lockdown._tcp`, `lockdown_sync::dnssd`);
//! 5. an email's domain, only after one is typed on the "I already use
//!    Lockdown" path: `https://<domain>/.well-known/lockdown`, then the SRV
//!    record `_lockdown._tcp.<domain>`.
//!
//! Then whatever the person types. Each candidate is checked by fetching its
//! `/.well-known/lockdown` through a client that follows no redirect, sends
//! no cookie or credential, times out, and reads at most [`MAX_ANSWER`]
//! bytes; `https://` is required except on this computer. Every answer is
//! hostile: versions and names are kept only when short and printable, at
//! most [`MAX_PER_SOURCE`] candidates come from one source, and nothing is
//! written to disk until the person confirms.
//!
//! **The confirm** shows the server key's fingerprint, the log key through
//! `derive_key("lockdown/server-fingerprint/v1", …)` as six groups of five
//! digits (the form people's fingerprints take, D190). Confirming pins that
//! key for the address (`confirmed-servers.cbor`); a sync refuses a server
//! whose log key differs from the one confirmed.

use std::collections::BTreeMap;
use std::path::Path;

use lockdown_security::http::Fetch;
use serde::{Deserialize, Serialize};

/// The server binary's default port.
pub const DEFAULT_PORT: u16 = 8787;
/// The most bytes read from any answer.
pub const MAX_ANSWER: usize = 16 * 1024;
/// The most candidates one source may add.
pub const MAX_PER_SOURCE: usize = 16;
const FINGERPRINT_CONTEXT: &str = "lockdown/server-fingerprint/v1";
const CONFIRMED_FILE: &str = "confirmed-servers.cbor";
const CONFIRMED_V: u16 = 1;

/// Where a candidate was found, best first.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Found {
    /// An administrator's policy file.
    Policy,
    /// This computer's config (an account in use, or Settings).
    Settings,
    /// A server on this computer.
    ThisComputer,
    /// The local network.
    Network,
    /// An email's domain.
    Domain {
        /// The domain.
        domain: String,
    },
    /// Typed by the person.
    Typed,
}

impl Found {
    fn rank(&self) -> u8 {
        match self {
            Self::Policy => 0,
            Self::Settings => 1,
            Self::ThisComputer => 2,
            Self::Network => 3,
            Self::Domain { .. } => 4,
            Self::Typed => 5,
        }
    }
}

/// What a check of a candidate found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// A Lockdown server this build speaks to.
    Ready,
    /// No answer.
    Unreachable,
    /// Plain http to another computer: never used.
    NeedsHttps,
    /// It answered, but not as a Lockdown server.
    NotLockdown,
    /// It answered as a Vaultwarden or Bitwarden server.
    Vaultwarden,
    /// Its key isn't the one an administrator or an earlier confirm set.
    WrongKey,
}

/// One server the person may pick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// The base address, as it will be used.
    pub address: String,
    /// Every source that named it, best first.
    pub sources: Vec<Found>,
    /// What the check found.
    pub state: State,
    /// Why it isn't ready, in plain words.
    pub reason: Option<String>,
    /// The server key's fingerprint, six groups of five digits.
    pub fingerprint: Option<String>,
    /// The server's release, when it said one.
    pub version: Option<String>,
    /// Whether an administrator's policy pinned this server's key and it matches.
    pub policy_key: bool,
    /// Whether an administrator's policy locks the choice to this server.
    pub locked: bool,
    /// Whether this is the account's server now.
    pub in_use: bool,
    /// The log key itself (for the confirm), never shown.
    #[serde(skip)]
    pub log_key: Option<[u8; 32]>,
}

/// The fingerprint of a server's log key.
pub fn fingerprint(log_key: &[u8; 32]) -> String {
    lockdown_vault::contact::digits(&lockdown_crypto::hash::derive_key(
        FINGERPRINT_CONTEXT,
        log_key,
    ))
}

/// An address as typed, made into a base address: trimmed, no trailing
/// slash, `https://` when no scheme was given. `None` for anything that
/// isn't a plausible address.
pub fn normalize(address: &str) -> Option<String> {
    let a = address.trim().trim_end_matches('/');
    if a.is_empty() || a.len() > 255 || a.chars().any(|c| c.is_control() || c == ' ') {
        return None;
    }
    let lower = a.to_ascii_lowercase();
    let full = if lower.starts_with("https://") || lower.starts_with("http://") {
        a.to_string()
    } else if lower.contains("://") {
        return None;
    } else {
        format!("https://{a}")
    };
    let rest = full.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (name, port) = match host.strip_prefix('[') {
        Some(v6) => {
            let (addr, rest) = v6.split_once(']')?;
            addr.parse::<std::net::Ipv6Addr>().ok()?;
            (addr, rest.strip_prefix(':'))
        }
        None => match host.rsplit_once(':') {
            Some((n, p)) => (n, Some(p)),
            None => (host, None),
        },
    };
    let name_ok =
        !name.is_empty() && (host.starts_with('[') || lockdown_sync::dnssd::plausible_host(name));
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.parse::<u16>().is_ok());
    (name_ok && port_ok).then_some(full)
}

fn printable(s: &str, max: usize) -> Option<String> {
    let s = s.trim();
    (!s.is_empty() && s.len() <= max && s.chars().all(|c| !c.is_control())).then(|| s.to_string())
}

fn host_of(address: &str) -> String {
    address
        .split_once("://")
        .map_or(address, |(_, r)| r)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(address)
        .to_string()
}

#[derive(Deserialize)]
struct WellKnown {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    api: Vec<u16>,
    features: WellKnownFeatures,
}

#[derive(Deserialize)]
struct WellKnownFeatures {
    kt_log_key: String,
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn get(fetch: &dyn Fetch, url: &str) -> Result<(u16, Vec<u8>), String> {
    let got = fetch.get(url, &[]).map_err(|e| e.to_string())?;
    if got.body.len() > MAX_ANSWER {
        return Err("it sent more than a Lockdown server would".into());
    }
    Ok((got.status, got.body))
}

/// Whether an answer to `/api/config` looks like Vaultwarden's or
/// Bitwarden's: a `version` and a `server.name`, or Vaultwarden's own
/// `vaultwarden` marker.
fn looks_like_bitwarden(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let version = v.get("version").and_then(|x| x.as_str()).is_some();
    let named = v
        .get("server")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .is_some();
    version && named
}

/// Checks one address: fetches its well-known document and says what it is.
/// `pin` is a key the address must have (a policy's, or one confirmed).
pub fn check(fetch: &dyn Fetch, address: &str, found: Found, pin: Option<[u8; 32]>) -> Candidate {
    let mut c = Candidate {
        address: normalize(address).unwrap_or_else(|| address.trim().to_string()),
        sources: vec![found],
        state: State::Unreachable,
        reason: None,
        fingerprint: None,
        version: None,
        policy_key: false,
        locked: false,
        in_use: false,
        log_key: None,
    };
    let host = host_of(&c.address);
    if normalize(address).is_none() {
        c.state = State::NotLockdown;
        c.reason = Some("That isn't a server address.".into());
        return c;
    }
    if let Err(e) = lockdown_sync::transport::check_base(&c.address) {
        let _ = e;
        c.state = State::NeedsHttps;
        c.reason = Some("Lockdown only talks to a server on another computer over https.".into());
        return c;
    }
    let (status, body) = match get(fetch, &format!("{}/.well-known/lockdown", c.address)) {
        Ok(answer) => answer,
        Err(why) => {
            c.reason = Some(format!("Lockdown can't reach {host}: {why}"));
            return c;
        }
    };
    let doc = (status == 200)
        .then(|| serde_json::from_slice::<WellKnown>(&body).ok())
        .flatten();
    let Some(doc) = doc else {
        // Not a Lockdown server; one more question tells Vaultwarden apart.
        if let Ok((200, body)) = get(fetch, &format!("{}/api/config", c.address))
            && looks_like_bitwarden(&body)
        {
            c.state = State::Vaultwarden;
            c.reason = Some(format!(
                "{host} is a Vaultwarden or Bitwarden server, not a Lockdown server. You can bring its passwords in instead."
            ));
        } else {
            c.state = State::NotLockdown;
            c.reason = Some(format!("{host} isn't a Lockdown server."));
        }
        return c;
    };
    c.version = doc.version.as_deref().and_then(|v| printable(v, 64));
    let Some(key) = hex32(&doc.features.kt_log_key) else {
        c.state = State::NotLockdown;
        c.reason = Some(format!(
            "{host} didn't give a server key Lockdown can check."
        ));
        return c;
    };
    if !doc.api.contains(&lockdown_sync::wire::API_V) {
        c.state = State::NotLockdown;
        c.reason = Some(format!(
            "{host} runs a Lockdown this version can't talk to. Update one of them."
        ));
        return c;
    }
    c.fingerprint = Some(fingerprint(&key));
    c.log_key = Some(key);
    if let Some(pin) = pin
        && pin != key
    {
        c.state = State::WrongKey;
        c.reason = Some(format!(
            "{host} has a different key from the one you confirmed before, so Lockdown won't connect. If whoever runs it changed it on purpose, ask them, then remove the server in Settings → Server and add it again."
        ));
        return c;
    }
    c.state = State::Ready;
    c
}

/// What to look at.
#[derive(Default)]
pub struct Sources {
    /// The policy file's server, or why it was refused.
    pub policy: Option<Result<lockdown_policy::managed::ManagedServer, String>>,
    /// Addresses this computer's config names, with where.
    pub config: Vec<String>,
    /// Whether to ask this computer's default port.
    pub this_computer: bool,
    /// What the local network answered (base addresses).
    pub network: Vec<String>,
    /// An email's domain, with the SRV targets found for it.
    pub domain: Option<(String, Vec<(String, u16)>)>,
    /// Typed addresses.
    pub typed: Vec<String>,
    /// The account's server now.
    pub in_use: Option<String>,
    /// Keys pinned by earlier confirms.
    pub confirmed: BTreeMap<String, [u8; 32]>,
}

/// Checks every source and ranks the candidates: by best source, a ready
/// one before one that isn't, then by address. The same address from
/// several sources is one candidate listing each.
pub fn discover(fetch: &dyn Fetch, sources: &Sources) -> Vec<Candidate> {
    let mut wanted: Vec<(String, Found)> = Vec::new();
    let mut policy_lock: Option<(String, Option<[u8; 32]>, bool)> = None;
    if let Some(Ok(p)) = &sources.policy
        && let Some(a) = normalize(&p.server)
    {
        policy_lock = Some((a.clone(), p.server_key, p.lock));
        wanted.push((a, Found::Policy));
    }
    let add = |list: &[String], found: Found, wanted: &mut Vec<(String, Found)>| {
        for a in list
            .iter()
            .filter_map(|a| normalize(a))
            .take(MAX_PER_SOURCE)
        {
            wanted.push((a, found.clone()));
        }
    };
    add(&sources.config, Found::Settings, &mut wanted);
    if sources.this_computer {
        add(
            &[
                format!("http://127.0.0.1:{DEFAULT_PORT}"),
                format!("http://[::1]:{DEFAULT_PORT}"),
            ],
            Found::ThisComputer,
            &mut wanted,
        );
    }
    add(&sources.network, Found::Network, &mut wanted);
    if let Some((domain, srv)) = &sources.domain {
        let found = Found::Domain {
            domain: domain.clone(),
        };
        let mut list = vec![format!("https://{domain}")];
        list.extend(srv.iter().map(|(host, port)| {
            if *port == 443 {
                format!("https://{host}")
            } else {
                format!("https://{host}:{port}")
            }
        }));
        add(&list, found, &mut wanted);
    }
    add(&sources.typed, Found::Typed, &mut wanted);

    // One check per address, every source it came from kept.
    let mut by_address: BTreeMap<String, Vec<Found>> = BTreeMap::new();
    for (a, f) in wanted {
        let key = a.to_ascii_lowercase();
        let entry = by_address.entry(key).or_default();
        if !entry.contains(&f) {
            entry.push(f);
        }
    }
    let mut out = Vec::new();
    for (address, mut found) in by_address {
        found.sort();
        let policy_pin = policy_lock
            .as_ref()
            .filter(|(a, ..)| a.eq_ignore_ascii_case(&address))
            .and_then(|(_, k, _)| *k);
        let pin = policy_pin.or_else(|| sources.confirmed.get(&address).copied());
        let mut c = check(fetch, &address, found[0].clone(), pin);
        c.sources = found;
        c.policy_key = policy_pin.is_some() && c.state == State::Ready;
        c.locked = policy_lock
            .as_ref()
            .is_some_and(|(a, _, lock)| *lock && a.eq_ignore_ascii_case(&address));
        c.in_use = sources
            .in_use
            .as_deref()
            .and_then(normalize)
            .is_some_and(|u| u.eq_ignore_ascii_case(&address));
        out.push(c);
    }
    // A policy that locks the choice hides every other server.
    if out.iter().any(|c| c.locked) {
        out.retain(|c| c.locked);
    }
    out.sort_by(|a, b| {
        let rank = |c: &Candidate| c.sources.first().map_or(9, Found::rank);
        (!a.in_use, rank(a), a.state != State::Ready, &a.address).cmp(&(
            !b.in_use,
            rank(b),
            b.state != State::Ready,
            &b.address,
        ))
    });
    out
}

/// The domain of an email-shaped login (`me@example.com` → `example.com`).
pub fn email_domain(login: &str) -> Option<String> {
    let (local, domain) = login.trim().rsplit_once('@')?;
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    (!local.is_empty()
        && domain.contains('.')
        && lockdown_sync::dnssd::plausible_host(&domain)
        && !domain.starts_with('.')
        && !domain.starts_with('-'))
    .then_some(domain)
}

#[derive(Serialize, Deserialize, Default)]
struct Confirmed {
    v: u16,
    #[serde(default)]
    servers: BTreeMap<String, serde_bytes::ByteBuf>,
}

/// The server keys the person confirmed, by address.
pub fn confirmed(data: &Path) -> BTreeMap<String, [u8; 32]> {
    let Ok(bytes) = std::fs::read(data.join(CONFIRMED_FILE)) else {
        return BTreeMap::new();
    };
    let Ok(file) = lockdown_crypto::cbor::from_slice::<Confirmed>(&bytes) else {
        return BTreeMap::new();
    };
    file.servers
        .into_iter()
        .filter_map(|(a, k)| Some((a, <[u8; 32]>::try_from(k.as_slice()).ok()?)))
        .collect()
}

/// Pins `log_key` for `address` (a confirm). A different key already pinned
/// for it is refused: removing the server comes first.
pub fn confirm(data: &Path, address: &str, log_key: [u8; 32]) -> Result<(), String> {
    let address = normalize(address).ok_or("That isn't a server address.")?;
    let mut all = confirmed(data);
    if let Some(old) = all.get(&address.to_ascii_lowercase())
        && *old != log_key
    {
        return Err(format!(
            "{} has a different key from the one you confirmed before, so Lockdown won't connect.",
            host_of(&address)
        ));
    }
    all.insert(address.to_ascii_lowercase(), log_key);
    write_confirmed(data, &all)
}

/// Forgets a confirmed server (Settings → Server → Remove).
pub fn forget(data: &Path, address: &str) -> Result<(), String> {
    let mut all = confirmed(data);
    if let Some(a) = normalize(address) {
        all.remove(&a.to_ascii_lowercase());
    }
    write_confirmed(data, &all)
}

fn write_confirmed(data: &Path, all: &BTreeMap<String, [u8; 32]>) -> Result<(), String> {
    let file = Confirmed {
        v: CONFIRMED_V,
        servers: all
            .iter()
            .map(|(a, k)| (a.clone(), serde_bytes::ByteBuf::from(k.to_vec())))
            .collect(),
    };
    let bytes = lockdown_crypto::cbor::to_canonical(&file).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    lockdown_store::write_private(&data.join(CONFIRMED_FILE), &bytes).map_err(|e| e.to_string())
}

/// Refuses a server whose log key isn't the one the person confirmed for it.
pub fn check_pinned(data: &Path, server: &str, log_key: &[u8; 32]) -> Result<(), String> {
    let Some(address) = normalize(server) else {
        return Ok(());
    };
    match confirmed(data).get(&address.to_ascii_lowercase()) {
        Some(pinned) if pinned != log_key => Err(format!(
            "{} has a different key from the one you confirmed before, so Lockdown won't connect. If whoever runs it changed it on purpose, ask them, then remove the server in Settings → Server and add it again.",
            host_of(&address)
        )),
        _ => Ok(()),
    }
}

/// Discovery from a shell: each call gathers what it needs from the core
/// in one quick call, then does its network work off the core's thread, so
/// a slow network never holds up an unlock.
#[cfg(feature = "native")]
impl crate::App {
    fn discovery_home(
        &self,
    ) -> crate::AppResult<(std::path::PathBuf, Vec<String>, Option<String>)> {
        self.call(|core| {
            let mut config = Vec::new();
            let in_use = crate::session::account_server_on_disk(&core.paths)
                .filter(|s| !lockdown_sync::is_local(s));
            if let Some(s) = &in_use {
                config.push(s.clone());
            }
            if let Some(s) = core
                .settings
                .server
                .clone()
                .filter(|s| !s.trim().is_empty())
            {
                config.push(s);
            }
            Ok((core.paths.data.clone(), config, in_use))
        })
    }

    fn fetcher() -> crate::AppResult<lockdown_security::http::HttpFetch> {
        lockdown_security::http::HttpFetch::new(std::time::Duration::from_secs(3))
            .map_err(|e| crate::AppError::refused(e.to_string()))
    }

    /// Every server found: the policy, this computer's config, this
    /// computer's default port and, with `network`, the local network
    /// (about three seconds), ranked.
    pub fn discovery_candidates(&self, network: bool) -> crate::AppResult<Vec<Candidate>> {
        let (data, config, in_use) = self.discovery_home()?;
        let policy = lockdown_policy::managed::default_path().and_then(|p| {
            match lockdown_policy::managed::load(&p) {
                Ok(found) => found.map(Ok),
                Err(e) => Some(Err(e.to_string())),
            }
        });
        let network = if network {
            lockdown_sync::dnssd::browse(std::time::Duration::from_secs(3), MAX_PER_SOURCE)
                .iter()
                .map(lockdown_sync::dnssd::Service::base)
                .collect()
        } else {
            Vec::new()
        };
        let sources = Sources {
            policy,
            config,
            this_computer: true,
            network,
            domain: None,
            typed: Vec::new(),
            in_use,
            confirmed: confirmed(&data),
        };
        Ok(discover(&Self::fetcher()?, &sources))
    }

    /// Checks an address the person typed.
    pub fn discovery_check(&self, address: &str) -> crate::AppResult<Candidate> {
        let (data, _, in_use) = self.discovery_home()?;
        let pin =
            normalize(address).and_then(|a| confirmed(&data).get(&a.to_ascii_lowercase()).copied());
        let mut c = check(&Self::fetcher()?, address, Found::Typed, pin);
        c.in_use = in_use
            .as_deref()
            .and_then(normalize)
            .is_some_and(|u| u.eq_ignore_ascii_case(&c.address));
        Ok(c)
    }

    /// What an email-shaped login's domain says: its well-known document
    /// and its SRV record. Empty for a login that isn't email-shaped.
    pub fn discovery_domain(&self, login: &str) -> crate::AppResult<Vec<Candidate>> {
        let Some(domain) = email_domain(login) else {
            return Ok(Vec::new());
        };
        let (data, _, _) = self.discovery_home()?;
        let srv = lockdown_sync::dnssd::srv_lookup(
            &domain,
            std::time::Duration::from_secs(2),
            MAX_PER_SOURCE,
        );
        let sources = Sources {
            domain: Some((domain, srv)),
            confirmed: confirmed(&data),
            ..Sources::default()
        };
        Ok(discover(&Self::fetcher()?, &sources))
    }

    /// The person confirmed `address` after seeing its key: checked again
    /// now, and its key pinned before anything else is sent there.
    pub fn discovery_confirm(&self, address: &str) -> crate::AppResult<Candidate> {
        let c = self.discovery_check(address)?;
        let (Some(key), State::Ready) = (c.log_key, c.state) else {
            return Err(crate::AppError::refused(
                c.reason
                    .clone()
                    .unwrap_or_else(|| "That server isn't ready.".into()),
            ));
        };
        let (data, _, _) = self.discovery_home()?;
        confirm(&data, &c.address, key).map_err(crate::AppError::refused)?;
        Ok(c)
    }

    /// Forgets a confirmed server's key.
    pub fn discovery_forget(&self, address: &str) -> crate::AppResult<()> {
        let (data, _, _) = self.discovery_home()?;
        forget(&data, address).map_err(crate::AppError::refused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_security::http::{FetchError, Fetched};
    use std::sync::Mutex;

    /// Answers by URL; records every URL asked, and every header sent.
    #[derive(Default)]
    struct Fake {
        answers: BTreeMap<String, Result<Fetched, FetchError>>,
        asked: Mutex<Vec<String>>,
        headers: Mutex<usize>,
    }

    impl Fake {
        fn answer(mut self, url: &str, status: u16, body: &str) -> Self {
            self.answers.insert(
                url.into(),
                Ok(Fetched {
                    status,
                    body: body.as_bytes().to_vec(),
                }),
            );
            self
        }
    }

    impl Fetch for Fake {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Fetched, FetchError> {
            self.asked.lock().unwrap().push(url.to_string());
            *self.headers.lock().unwrap() += headers.len();
            self.answers
                .get(url)
                .cloned()
                .unwrap_or(Err(FetchError::Network {
                    host: host_of(url),
                    why: "the connection failed".into(),
                }))
        }
    }

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn doc(key: &str) -> String {
        format!(
            r#"{{"v":1,"version":"0.1.0","api":[1],"features":{{"opaque":true,"signups":true,"key_transparency":true,"checkpoints":true,"blobs":true,"push":false,"kt_log_key":"{key}","live":true,"sessions":true,"storage":true}}}}"#
        )
    }

    #[test]
    fn a_lockdown_server_is_ready_with_its_fingerprint() {
        let f =
            Fake::default().answer("https://vault.example/.well-known/lockdown", 200, &doc(KEY));
        let c = check(&f, "vault.example/", Found::Typed, None);
        assert_eq!(c.state, State::Ready, "{c:?}");
        assert_eq!(c.address, "https://vault.example");
        let fp = c.fingerprint.unwrap();
        assert_eq!(fp.split(' ').count(), 6);
        assert!(
            fp.split(' ')
                .all(|g| g.len() == 5 && g.chars().all(|d| d.is_ascii_digit()))
        );
        assert_eq!(c.version.as_deref(), Some("0.1.0"));
        // Nothing but the document was asked, with no header at all.
        assert_eq!(*f.headers.lock().unwrap(), 0);
        assert_eq!(f.asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn hostile_and_odd_answers_never_read_as_ready() {
        let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_ANSWER));
        let f = Fake::default()
            .answer("https://big.example/.well-known/lockdown", 200, &big)
            .answer(
                "https://short.example/.well-known/lockdown",
                200,
                &doc("abcd"),
            )
            .answer(
                "https://old.example/.well-known/lockdown",
                200,
                &doc(KEY).replace("[1]", "[9]"),
            )
            .answer("https://html.example/.well-known/lockdown", 200, "<html>")
            .answer("https://vw.example/.well-known/lockdown", 404, "")
            .answer(
                "https://vw.example/api/config",
                200,
                r#"{"version":"2025.1.0","server":{"name":"Vaultwarden","url":"x"}}"#,
            )
            .answer(
                "https://ctl.example/.well-known/lockdown",
                200,
                &doc(KEY).replace("0.1.0", "evil\\u0007"),
            );
        assert_eq!(
            check(&f, "big.example", Found::Typed, None).state,
            State::Unreachable
        );
        assert_eq!(
            check(&f, "short.example", Found::Typed, None).state,
            State::NotLockdown
        );
        assert_eq!(
            check(&f, "old.example", Found::Typed, None).state,
            State::NotLockdown
        );
        assert_eq!(
            check(&f, "html.example", Found::Typed, None).state,
            State::NotLockdown
        );
        let vw = check(&f, "vw.example", Found::Typed, None);
        assert_eq!(vw.state, State::Vaultwarden);
        assert!(vw.reason.unwrap().contains("bring its passwords in"));
        let ctl = check(&f, "ctl.example", Found::Typed, None);
        assert_eq!((ctl.state, ctl.version), (State::Ready, None));
        // Plain http to another computer is never even asked.
        let lan = check(&f, "http://nas.local:8787", Found::Network, None);
        assert_eq!(lan.state, State::NeedsHttps);
        assert!(
            !f.asked
                .lock()
                .unwrap()
                .iter()
                .any(|u| u.contains("nas.local"))
        );
        // Not an address at all.
        for bad in ["", "ftp://x", "a b", "https://user@host", "https:///"] {
            assert_eq!(
                check(&f, bad, Found::Typed, None).state,
                State::NotLockdown,
                "{bad}"
            );
        }
        // A key that differs from the one pinned.
        let f2 = Fake::default().answer("https://v.example/.well-known/lockdown", 200, &doc(KEY));
        let wrong = check(&f2, "v.example", Found::Typed, Some([9; 32]));
        assert_eq!(wrong.state, State::WrongKey);
    }

    #[test]
    fn candidates_rank_by_source_then_readiness_then_address() {
        let f = Fake::default()
            .answer(
                "https://policy.example/.well-known/lockdown",
                200,
                &doc(KEY),
            )
            .answer("https://b.lan/.well-known/lockdown", 200, &doc(KEY))
            .answer("https://a.lan/.well-known/lockdown", 200, &doc(KEY))
            .answer("http://127.0.0.1:8787/.well-known/lockdown", 200, &doc(KEY))
            .answer("https://example.com/.well-known/lockdown", 404, "")
            .answer(
                "https://vault.example.com/.well-known/lockdown",
                200,
                &doc(KEY),
            );
        let sources = Sources {
            policy: Some(Ok(lockdown_policy::managed::ManagedServer {
                server: "https://policy.example".into(),
                server_key: hex32(KEY),
                lock: false,
            })),
            config: vec!["https://b.lan".into()],
            this_computer: true,
            network: vec![
                "https://zz.lan".into(),
                "https://b.lan".into(),
                "https://a.lan".into(),
            ],
            domain: Some((
                "example.com".into(),
                vec![("vault.example.com".into(), 443)],
            )),
            typed: vec![],
            in_use: None,
            confirmed: BTreeMap::new(),
        };
        let found = discover(&f, &sources);
        let order: Vec<&str> = found.iter().map(|c| c.address.as_str()).collect();
        assert_eq!(
            order,
            [
                "https://policy.example",
                "https://b.lan",
                "http://127.0.0.1:8787",
                "http://[::1]:8787",
                "https://a.lan",
                "https://zz.lan",
                "https://vault.example.com",
                "https://example.com",
            ]
        );
        assert!(found[0].policy_key);
        assert_eq!(found[1].sources, vec![Found::Settings, Found::Network]);
        assert_eq!(found[3].state, State::Unreachable);
        // The same answer every time: no hash order anywhere.
        assert_eq!(discover(&f, &sources), found);

        // A policy that locks the choice hides the rest; a policy key that
        // doesn't match is refused.
        let mut locked = sources;
        locked.policy = Some(Ok(lockdown_policy::managed::ManagedServer {
            server: "https://policy.example".into(),
            server_key: Some([1; 32]),
            lock: true,
        }));
        let only = discover(&f, &locked);
        assert_eq!(only.len(), 1);
        assert_eq!((only[0].state, only[0].locked), (State::WrongKey, true));
    }

    #[test]
    fn email_domains_and_confirmed_keys() {
        assert_eq!(
            email_domain("me@Example.COM").as_deref(),
            Some("example.com")
        );
        for bad in [
            "me",
            "@example.com",
            "me@localhost",
            "me@exa mple.com",
            "me@-x.com",
        ] {
            assert_eq!(email_domain(bad), None, "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        confirm(dir.path(), "https://vault.example/", [7; 32]).unwrap();
        assert_eq!(
            confirmed(dir.path()).get("https://vault.example"),
            Some(&[7; 32])
        );
        check_pinned(dir.path(), "https://vault.example", &[7; 32]).unwrap();
        assert!(check_pinned(dir.path(), "https://vault.example", &[8; 32]).is_err());
        assert!(confirm(dir.path(), "vault.example", [8; 32]).is_err());
        check_pinned(dir.path(), "https://other.example", &[8; 32]).unwrap();
        forget(dir.path(), "vault.example").unwrap();
        confirm(dir.path(), "vault.example", [8; 32]).unwrap();
    }
}
