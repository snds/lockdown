//! The security center (docs/plan/13-security-center.md): one report over
//! the open vault and the account, decided here so every shell shows the
//! same findings in the same words.
//!
//! | Finding | From | Severity |
//! |---|---|---|
//! | `breached` | the k-anonymity range check ([`lockdown_security::breach`]): only five hex characters of a password's SHA-1 leave the device | urgent |
//! | `reused` | a keyed BLAKE3 of each password, with a key made for this one report and never kept, grouped here | important |
//! | `weak` | zxcvbn's score ([`crate::strength`]) of each stored password, in the core | important (0–1), suggested (2) |
//! | `missing_2fa` | the bundled 2fa.directory snapshot against each login's website | suggested |
//! | `lookalike` | the matcher's lookalike rules: a login's website against your other logins and the directory's well-known sites | important |
//! | `new_sign_in` | the server's account alerts: a device the account hadn't seen | important |
//! | `exposed_email` | the server's breach monitor, for the account's address | urgent (a breach in the last year), important |
//!
//! **What crosses to a shell:** findings in words, item ids and titles,
//! counts, a breach's count of sightings, a website's address (never a
//! crown jewel's). Never a password, a score's input, a hash or a reuse
//! group's key. Scoring and hashing happen on the worker thread inside
//! [`crate::session::security`]'s callback.
//!
//! **The network:** [`App::security_check`] collects the five-character
//! prefixes on the worker, makes the range requests on the caller's
//! thread (so a slow network never holds the vault), and records only the
//! verdicts: `security.cbor` beside `settings.cbor` holds, per item and
//! field, the item version checked, how often the password was seen, and
//! when; no hash and no password. A changed password is a new version, so
//! it is checked again; every verdict is re-checked after a week.
//!
//! **Dismiss and snooze** are a setting on the item (the reserved field,
//! so they sync); an account alert's dismissal is kept by the server for
//! every device.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use lockdown_matcher::{Confusables, EquivalentDomains, Origin, lookalike};
use lockdown_security::breach::{self, Candidate, Checked};
use lockdown_security::twofa::TwoFaDirectory;
use lockdown_security::{DEFAULT_RANGE_ENDPOINT, HttpFetch, PLAYBOOKS, change_password};
use lockdown_sync::wire::{AccountAlert, AlertKind, AlertsSince};
use lockdown_vault::ItemKind;
use serde::{Deserialize, Serialize};

pub use crate::session::security::Dismissal;
use crate::session::security::ItemFacts;
use crate::types::DeviceInfo;
use crate::{App, AppError, AppResult, Core, Settings, session, strength};
pub use lockdown_security::{ChangeLink, LinkKind, Playbook};

/// The cache file, beside `settings.cbor`.
pub const CACHE_FILE: &str = "security.cbor";

/// A verdict older than this is checked again.
pub const RECHECK_SECS: i64 = 7 * 24 * 3600;

/// How long one range request or probe may take.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);

const CACHE_V: u16 = 1;

// --- what a shell gets -------------------------------------------------------

/// Another item a finding names (a reused password's other places, the
/// login a lookalike imitates).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RelatedItem {
    pub id: String,
    pub title: String,
}

/// One finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable across reports: `<kind>:<item>[:<field>]` or `alert:<seq>`.
    pub id: String,
    /// `breached`, `reused`, `weak`, `missing_2fa`, `lookalike`,
    /// `new_sign_in` or `exposed_email`.
    pub kind: String,
    /// `urgent`, `important` or `suggested`.
    pub severity: String,
    /// What it is, as a heading.
    pub title: String,
    /// The particulars, in a sentence or two.
    pub detail: String,
    /// The key a dismissal is filed under on the item (`weak:pw`).
    pub key: String,
    pub item_id: Option<String>,
    pub item_title: Option<String>,
    pub item_kind: Option<String>,
    pub field_id: Option<String>,
    /// The item's website, as typed; never a crown jewel's.
    pub site: Option<String>,
    pub related: Vec<RelatedItem>,
    /// The site's own page on turning on two-step sign-in.
    pub setup_url: Option<String>,
    /// Whether "Change password" applies (a login with a website).
    pub change_password: bool,
    /// The playbook's id ([`Playbook`]).
    pub playbook: String,
    /// The account alert's number, for an alert.
    pub alert: Option<u64>,
    /// When it happened, for an alert.
    pub at: Option<i64>,
    /// Set when dismissed or snoozed (then it is in `dismissed`).
    pub dismissed: Option<Dismissal>,
}

/// How many findings of each severity are open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SeverityCounts {
    pub urgent: u32,
    pub important: u32,
    pub suggested: u32,
}

/// Where the breached-password check stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BreachCheckView {
    /// The setting.
    pub enabled: bool,
    /// The source in a few words.
    pub source: String,
    /// When a check last finished.
    pub checked_at: Option<i64>,
    /// Passwords with a current verdict.
    pub checked: u32,
    /// Passwords not checked yet (new, changed, or the check failed).
    pub unchecked: u32,
    /// The last check's problem, in words.
    pub message: Option<String>,
}

/// Where the account alerts stand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AlertsView {
    /// Whether this device has an account to ask.
    pub account: bool,
    /// When they were last fetched in this session.
    pub checked_at: Option<i64>,
    pub message: Option<String>,
}

/// The bundled 2FA directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DirectoryView {
    pub source: String,
    pub licence: String,
    pub date: String,
    pub sites: u32,
}

/// The whole report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecurityReport {
    /// Open findings, most severe first.
    pub findings: Vec<Finding>,
    /// Dismissed and snoozed ones.
    pub dismissed: Vec<Finding>,
    pub counts: SeverityCounts,
    /// How many stored passwords were looked at.
    pub passwords: u32,
    pub breach: BreachCheckView,
    pub alerts: AlertsView,
    pub directory: DirectoryView,
    pub generated_at: i64,
}

// --- what the core keeps -----------------------------------------------------

/// Kept on the worker while the vault is open; dropped on lock.
#[derive(Default)]
pub(crate) struct SecurityState {
    /// zxcvbn scores by (item, field, version): a password is scored once
    /// per version.
    scores: HashMap<(String, String, u64), Option<u8>>,
    alerts: Option<AlertCache>,
}

struct AlertCache {
    alerts: Vec<AccountAlert>,
    at: i64,
    message: Option<String>,
}

/// One verdict: no hash, no password.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Verdict {
    version: u64,
    count: u64,
    at: i64,
}

/// `security.cbor`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct BreachCache {
    v: u16,
    source: String,
    checked_at: Option<i64>,
    message: Option<String>,
    /// By `<item>/<field>`.
    entries: BTreeMap<String, Verdict>,
}

impl BreachCache {
    fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| lockdown_crypto::cbor::from_slice::<Self>(&b).ok())
            .filter(|c| c.v <= CACHE_V)
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let mut on_disk = self.clone();
        on_disk.v = CACHE_V;
        let bytes = lockdown_crypto::cbor::to_canonical(&on_disk).map_err(|e| e.to_string())?;
        lockdown_store::write_private(path, &bytes)
            .map_err(|e| format!("couldn't write {}: {e}", path.display()))
    }
}

fn entry_key(item: &str, field: &str) -> String {
    format!("{item}/{field}")
}

/// The breach source the settings name, in a few words (what the cache is
/// filed under, so changing it checks everything again).
fn source_words(settings: &Settings) -> String {
    let endpoint = settings
        .breach_endpoint
        .as_deref()
        .unwrap_or(DEFAULT_RANGE_ENDPOINT);
    if let Some(path) = endpoint.strip_prefix("file://") {
        format!("the breach list in {path}")
    } else {
        endpoint
            .split("://")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .unwrap_or(endpoint)
            .to_string()
    }
}

// --- the report --------------------------------------------------------------

/// One password, as facts.
struct PasswordFacts {
    item: usize,
    field_id: String,
    label: String,
    score: Option<u8>,
    reuse: lockdown_crypto::Hash32,
}

/// A login's website as the lookalike check compares it.
#[derive(Clone)]
struct Site {
    host: String,
    domain: String,
    label: String,
    skeleton: String,
}

fn site_of(url: &str) -> Option<Site> {
    let origin = Origin::parse(url).ok()?;
    let domain = origin.registrable_domain()?;
    let label = origin.registrable_label()?;
    Some(Site {
        skeleton: Confusables::builtin().skeleton(&label),
        host: origin.host,
        domain,
        label,
    })
}

/// The directory's well-known sites, prepared once per process.
fn well_known() -> &'static Vec<Site> {
    static SITES: OnceLock<Vec<Site>> = OnceLock::new();
    SITES.get_or_init(|| {
        let mut seen = std::collections::BTreeSet::new();
        TwoFaDirectory::builtin()
            .domains()
            .filter_map(site_of)
            .filter(|s| seen.insert(s.domain.clone()))
            .collect()
    })
}

/// Whether `candidate` imitates `trusted` under the matcher's rules
/// (`lookalike::host_imitates_with`), with a cheap test first so a few
/// thousand well-known sites cost little.
fn imitates(candidate: &Site, trusted: &Site, equivalent: &EquivalentDomains) -> bool {
    if candidate.domain == trusted.domain
        || equivalent.equivalent(&candidate.domain, &trusted.domain)
    {
        return false;
    }
    if candidate.host.starts_with(&format!("{}.", trusted.domain))
        || candidate.host.contains(&format!(".{}.", trusted.domain))
    {
        return true;
    }
    let maybe = candidate.skeleton == trusted.skeleton
        || candidate
            .label
            .chars()
            .count()
            .abs_diff(trusted.label.chars().count())
            <= 1
        || candidate.label.contains(&trusted.label);
    maybe
        && lookalike::label_imitates_with(Confusables::builtin(), &candidate.label, &trusted.label)
}

/// The site a page imitates, by the rule the security center applies to
/// saved logins (the browser extension's hard stop reuses it,
/// docs/plan/16-browser-extension.md §7): the closest of `saved` (a confusable
/// skeleton first, then by name), else a well-known site from the
/// directory. `None` for a page that is itself well known, is one of
/// `saved`'s sites, or imitates nothing.
pub fn imitated_site(page: &str, saved: &[String]) -> Option<String> {
    let site = site_of(page)?;
    if TwoFaDirectory::builtin().lookup(&site.domain).is_some() {
        return None;
    }
    let equivalent = EquivalentDomains::curated();
    let rank = |t: &Site| (t.skeleton != site.skeleton, t.domain.clone());
    let mine: Vec<Site> = saved.iter().filter_map(|u| site_of(u)).collect();
    if mine.iter().any(|t| t.domain == site.domain) {
        return None;
    }
    mine.iter()
        .filter(|t| imitates(&site, t, &equivalent))
        .min_by_key(|t| rank(t))
        .or_else(|| {
            well_known()
                .iter()
                .filter(|t| imitates(&site, t, &equivalent))
                .min_by_key(|t| rank(t))
        })
        .map(|t| t.domain.clone())
}

fn active(d: Option<&Dismissal>, now: i64) -> Option<Dismissal> {
    d.copied().filter(|d| d.until.is_none_or(|u| u > now))
}

fn count_words(n: u64) -> String {
    match n {
        1 => "once".into(),
        2 => "twice".into(),
        n => {
            let digits = n.to_string();
            let mut out = String::new();
            for (i, c) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(c);
            }
            format!("{out} times")
        }
    }
}

fn list_words(titles: &[String]) -> String {
    let shown: Vec<String> = titles
        .iter()
        .take(3)
        .map(|t| format!("\u{201c}{t}\u{201d}"))
        .collect();
    let more = titles.len().saturating_sub(3);
    match (shown.len(), more) {
        (0, _) => "another item".into(),
        (1, 0) => shown[0].clone(),
        (_, 0) => {
            let (last, rest) = shown.split_last().expect("two or more");
            format!("{} and {last}", rest.join(", "))
        }
        _ => format!("{} and {more} more", shown.join(", ")),
    }
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` date.
fn days_of(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "urgent" => 0,
        "important" => 1,
        _ => 2,
    }
}

struct Inputs<'a> {
    items: &'a [ItemFacts],
    passwords: &'a [PasswordFacts],
    cache: &'a BreachCache,
    source: String,
    breach_enabled: bool,
    alerts: Option<&'a AlertCache>,
    has_account: bool,
    devices: &'a [DeviceInfo],
    now: i64,
}

fn finding(kind: &str, severity: &str, title: &str, playbook: &str) -> Finding {
    Finding {
        id: String::new(),
        kind: kind.into(),
        severity: severity.into(),
        title: title.into(),
        detail: String::new(),
        key: kind.into(),
        item_id: None,
        item_title: None,
        item_kind: None,
        field_id: None,
        site: None,
        related: Vec::new(),
        setup_url: None,
        change_password: false,
        playbook: playbook.into(),
        alert: None,
        at: None,
        dismissed: None,
    }
}

fn on_item(mut f: Finding, item: &ItemFacts, field: Option<&str>) -> Finding {
    f.id = match field {
        Some(field) => format!("{}:{}:{field}", f.kind, item.item_id),
        None => format!("{}:{}", f.kind, item.item_id),
    };
    if let Some(field) = field {
        f.key = format!("{}:{field}", f.kind);
    }
    f.item_id = Some(item.item_id.clone());
    f.item_title = Some(item.title.clone());
    f.item_kind = Some(item.kind.as_str().to_string());
    f.field_id = field.map(str::to_string);
    if !item.crown_jewel {
        f.site = item.urls.first().cloned();
    }
    f.change_password = item.kind == ItemKind::Login
        && !item.urls.is_empty()
        && matches!(f.kind.as_str(), "breached" | "reused" | "weak");
    f
}

fn assemble(inp: &Inputs<'_>) -> SecurityReport {
    let mut all: Vec<Finding> = Vec::new();
    let equivalent = EquivalentDomains::curated();
    let directory = TwoFaDirectory::builtin();

    // Passwords: breached, weak, reused.
    let mut groups: HashMap<lockdown_crypto::Hash32, Vec<&PasswordFacts>> = HashMap::new();
    let mut checked = 0u32;
    for p in inp.passwords {
        let item = &inp.items[p.item];
        let label_note = if p.label == "Password" {
            String::new()
        } else {
            format!(" ({})", p.label)
        };
        groups.entry(p.reuse).or_default().push(p);
        let verdict = inp
            .cache
            .entries
            .get(&entry_key(&item.item_id, &p.field_id))
            .filter(|v| v.version == item.version && inp.cache.source == inp.source);
        if let Some(v) = verdict {
            checked += 1;
            if v.count > 0 {
                let mut f = on_item(
                    finding(
                        "breached",
                        "urgent",
                        "Password found in a data breach",
                        "breached-password",
                    ),
                    item,
                    Some(&p.field_id),
                );
                f.detail = format!(
                    "This password{label_note} has been seen {} in data breaches. Change it here and anywhere else you use it.",
                    count_words(v.count)
                );
                all.push(f);
            }
        }
        if let Some(score) = p.score.filter(|s| *s <= 2) {
            let (severity, words) = if score <= 1 {
                ("important", "would be guessed quickly")
            } else {
                ("suggested", "could be guessed with some effort")
            };
            let mut f = on_item(
                finding("weak", severity, "Easy-to-guess password", "weak-password"),
                item,
                Some(&p.field_id),
            );
            f.detail = format!("This password{label_note} {words}.");
            all.push(f);
        }
    }
    for members in groups.values() {
        let mut items: Vec<usize> = members.iter().map(|p| p.item).collect();
        items.sort_unstable();
        items.dedup();
        if items.len() < 2 {
            continue;
        }
        for p in members {
            let item = &inp.items[p.item];
            let others: Vec<&ItemFacts> = items
                .iter()
                .filter(|i| **i != p.item)
                .map(|i| &inp.items[*i])
                .collect();
            let mut f = on_item(
                finding(
                    "reused",
                    "important",
                    "Password used in more than one place",
                    "reused-password",
                ),
                item,
                Some(&p.field_id),
            );
            let titles: Vec<String> = others.iter().map(|o| o.title.clone()).collect();
            f.detail = format!(
                "The same password is also saved in {}.",
                list_words(&titles)
            );
            f.related = others
                .iter()
                .map(|o| RelatedItem {
                    id: o.item_id.clone(),
                    title: o.title.clone(),
                })
                .collect();
            all.push(f);
        }
    }

    // Logins' websites: missing two-step sign-in, lookalikes.
    let saved: Vec<(usize, Site)> = inp
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.kind == ItemKind::Login)
        .flat_map(|(n, i)| {
            i.urls
                .iter()
                .filter_map(move |u| site_of(u).map(|s| (n, s)))
        })
        .collect();
    for (n, item) in inp.items.iter().enumerate() {
        if item.kind != ItemKind::Login {
            continue;
        }
        let first = item.urls.first().and_then(|u| Origin::parse(u).ok());
        if let Some(origin) = &first
            && let Some(entry) = directory.lookup(&origin.host)
            && !item.has_code
            && !item.has_passkey
        {
            let mut f = on_item(
                finding(
                    "missing_2fa",
                    "suggested",
                    "Two-step sign-in is available",
                    "missing-2fa",
                ),
                item,
                None,
            );
            let methods = entry.method_words();
            let how = match methods.as_slice() {
                [] => String::new(),
                [one] => format!(" with {one}"),
                [rest @ .., last] => format!(" with {} or {last}", rest.join(", ")),
            };
            f.detail = format!(
                "{} lets you add a second step when you sign in{how}.{}",
                entry.name,
                if entry.takes_app_codes() {
                    " Lockdown can keep the code with this login."
                } else {
                    ""
                }
            );
            f.setup_url = entry.documentation.clone();
            all.push(f);
        }
        let mut flagged = false;
        for url in &item.urls {
            if flagged {
                break;
            }
            let Some(site) = site_of(url) else { continue };
            if directory.lookup(&site.domain).is_some() {
                continue; // a well-known site is itself, not a copy
            }
            // The closest site wins, not the first found: one whose confusable
            // skeleton is identical (paypa1 → paypal) before a one-letter
            // neighbour (paypay), then by name, so every machine agrees.
            let rank = |t: &Site| (t.skeleton != site.skeleton, t.domain.clone());
            let mine = saved
                .iter()
                .filter(|(m, other)| *m != n && imitates(&site, other, &equivalent))
                .min_by_key(|(_, other)| rank(other));
            let target = match mine {
                Some((m, other)) => Some((other.domain.clone(), Some(&inp.items[*m]))),
                None => well_known()
                    .iter()
                    .filter(|t| imitates(&site, t, &equivalent))
                    .min_by_key(|t| rank(t))
                    .map(|t| (t.domain.clone(), None)),
            };
            if let Some((domain, other)) = target {
                let mut f = on_item(
                    finding(
                        "lookalike",
                        "important",
                        "Website looks like another site",
                        "lookalike-site",
                    ),
                    item,
                    None,
                );
                f.detail = match other {
                    Some(o) => format!(
                        "{} looks a lot like {domain}, where your login \u{201c}{}\u{201d} is. Fake sites copy real addresses to collect passwords.",
                        site.host, o.title
                    ),
                    None => format!(
                        "{} looks a lot like {domain}. Fake sites copy real addresses to collect passwords.",
                        site.host
                    ),
                };
                if let Some(o) = other {
                    f.related.push(RelatedItem {
                        id: o.item_id.clone(),
                        title: o.title.clone(),
                    });
                }
                if item.crown_jewel {
                    f.detail = format!("This item's website looks a lot like {domain}.");
                }
                all.push(f);
                flagged = true;
            }
        }
    }

    // The account's alerts.
    if let Some(cache) = inp.alerts {
        for a in &cache.alerts {
            let mut f = match a.kind {
                AlertKind::NewDevice => {
                    let device = a
                        .device_id
                        .and_then(|id| inp.devices.iter().find(|d| d.id == id.to_string()));
                    if device.is_some_and(|d| d.revoked_at.is_some()) {
                        continue; // already dealt with
                    }
                    let mut f = finding(
                        "new_sign_in",
                        "important",
                        "New device signed in",
                        "new-sign-in",
                    );
                    f.detail = match device {
                        Some(d) => format!(
                            "\u{201c}{}\u{201d} signed in to your Lockdown account for the first time. If that wasn't you, revoke it in Devices.",
                            d.name
                        ),
                        None => "A device this one hasn't seen before signed in to your Lockdown account. If that wasn't you, revoke it in Devices.".into(),
                    };
                    f
                }
                AlertKind::EmailBreach => {
                    let Some(b) = &a.breach else { continue };
                    let recent = b
                        .breach_date
                        .as_deref()
                        .and_then(days_of)
                        .is_none_or(|d| inp.now / 86_400 - d <= 365);
                    let mut f = finding(
                        "exposed_email",
                        if recent { "urgent" } else { "important" },
                        "Email address in a data breach",
                        "exposed-email",
                    );
                    let when = b
                        .breach_date
                        .as_deref()
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    let what = if b.data.is_empty() {
                        String::new()
                    } else {
                        format!(" What was taken: {}.", b.data.join(", ").to_lowercase())
                    };
                    let site = b
                        .domain
                        .as_deref()
                        .map(|d| format!(" of {d}"))
                        .unwrap_or_default();
                    f.detail = format!(
                        "{} was in the {}{site} breach{when}.{what}",
                        b.address, b.name
                    );
                    f.site = b.domain.as_ref().map(|d| format!("https://{d}"));
                    f
                }
                AlertKind::SecurityKeyAdded | AlertKind::SecurityKeyRemoved => {
                    let added = a.kind == AlertKind::SecurityKeyAdded;
                    let mut f = finding(
                        "security_key_change",
                        "important",
                        if added {
                            "Security key added to your account"
                        } else {
                            "Security key removed from your account"
                        },
                        "new-sign-in",
                    );
                    let name = a
                        .key_name
                        .as_deref()
                        .map(|n| format!("\u{201c}{n}\u{201d}"))
                        .unwrap_or_else(|| "A security key".into());
                    f.detail = format!(
                        "{name} was {} your Lockdown account's sign-in. If that wasn't you, someone is signed in: end the sessions you don't know in Devices and change your master password.",
                        if added { "added to" } else { "removed from" }
                    );
                    f
                }
                // A kind a newer server sends that this build doesn't know.
                AlertKind::Other => continue,
            };
            f.id = format!("alert:{}", a.seq);
            f.key = f.id.clone();
            f.alert = Some(a.seq);
            f.at = Some(a.at);
            if a.dismissed {
                f.dismissed = Some(Dismissal {
                    at: a.at,
                    until: None,
                });
            }
            all.push(f);
        }
    }

    // Split open from dismissed, with snoozes that ran out open again.
    for f in all.iter_mut().filter(|f| f.alert.is_none()) {
        let item = inp
            .items
            .iter()
            .find(|i| Some(&i.item_id) == f.item_id.as_ref());
        f.dismissed = item.and_then(|i| active(i.security.dismissed.get(&f.key), inp.now));
    }
    all.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(a.kind.cmp(&b.kind))
            .then(a.item_title.cmp(&b.item_title))
            .then(a.id.cmp(&b.id))
    });
    let (dismissed, findings): (Vec<Finding>, Vec<Finding>) =
        all.into_iter().partition(|f| f.dismissed.is_some());
    let mut counts = SeverityCounts::default();
    for f in &findings {
        match f.severity.as_str() {
            "urgent" => counts.urgent += 1,
            "important" => counts.important += 1,
            _ => counts.suggested += 1,
        }
    }
    let passwords = u32::try_from(inp.passwords.len()).unwrap_or(u32::MAX);
    SecurityReport {
        findings,
        dismissed,
        counts,
        passwords,
        breach: BreachCheckView {
            enabled: inp.breach_enabled,
            source: inp.source.clone(),
            checked_at: inp
                .cache
                .checked_at
                .filter(|_| inp.cache.source == inp.source),
            checked,
            unchecked: passwords - checked.min(passwords),
            message: inp.cache.message.clone().filter(|_| inp.breach_enabled),
        },
        alerts: AlertsView {
            account: inp.has_account,
            checked_at: inp.alerts.map(|a| a.at),
            message: inp.alerts.and_then(|a| a.message.clone()),
        },
        directory: DirectoryView {
            source: directory.source.clone(),
            licence: directory.licence.clone(),
            date: directory.date.clone(),
            sites: u32::try_from(directory.len()).unwrap_or(u32::MAX),
        },
        generated_at: inp.now,
    }
}

// --- on the worker -------------------------------------------------------------

impl Core {
    /// The report over the open vault, with what this session already
    /// knows of the breach check and the account's alerts.
    fn security_report(&mut self, now: i64) -> AppResult<SecurityReport> {
        let settings = self.settings.clone();
        let mut scores = std::mem::take(&mut self.security.scores);
        let key =
            lockdown_crypto::rand::random_32().map_err(|e| AppError::refused(e.to_string()))?;
        let mut passwords = Vec::new();
        let session = self.session()?;
        let scanned = session.security_scan(|facts, field_id, label, bytes| {
            let score = *scores
                .entry((facts.item_id.clone(), field_id.to_string(), facts.version))
                .or_insert_with(|| {
                    let host = facts
                        .urls
                        .first()
                        .and_then(|u| Origin::parse(u).ok())
                        .map(|o| o.host)
                        .unwrap_or_default();
                    strength::score(bytes, &[&facts.title, &facts.username, &host])
                        .ok()
                        .flatten()
                });
            passwords.push((
                facts.item_id.clone(),
                PasswordFacts {
                    item: 0,
                    field_id: field_id.to_string(),
                    label: label.to_string(),
                    score,
                    reuse: lockdown_crypto::hash::keyed_hash(&key, bytes),
                },
            ));
        });
        let items = match scanned {
            Ok(items) => items,
            Err(e) => {
                self.security.scores = scores;
                return Err(AppError::refused(e));
            }
        };
        // Each password points at its item by position in the scan.
        let positions: HashMap<&str, usize> = items
            .iter()
            .enumerate()
            .map(|(n, i)| (i.item_id.as_str(), n))
            .collect();
        let passwords: Vec<PasswordFacts> = passwords
            .into_iter()
            .filter_map(|(id, mut p)| {
                p.item = *positions.get(id.as_str())?;
                Some(p)
            })
            .collect();
        let devices = session.device_list();
        let has_account = session.has_account();
        let cache = BreachCache::load(&session.data_dir().join(CACHE_FILE));
        self.security.scores = scores;
        Ok(assemble(&Inputs {
            items: &items,
            passwords: &passwords,
            cache: &cache,
            source: source_words(&settings),
            breach_enabled: settings.breach_check,
            alerts: self.security.alerts.as_ref(),
            has_account,
            devices: &devices,
            now,
        }))
    }
}

/// What a check needs from the worker before going to the network.
struct Plan {
    candidates: Vec<Candidate>,
    versions: HashMap<String, u64>,
    endpoint: Option<String>,
    source: String,
    account: Option<Result<(String, String), String>>,
}

impl Core {
    fn security_plan(&mut self, now: i64) -> AppResult<Plan> {
        let settings = self.settings.clone();
        let source = source_words(&settings);
        let session = self.session()?;
        let cache = BreachCache::load(&session.data_dir().join(CACHE_FILE));
        let mut candidates = Vec::new();
        let mut versions = HashMap::new();
        if settings.breach_check {
            session
                .security_scan(|facts, field_id, _, bytes| {
                    let key = entry_key(&facts.item_id, field_id);
                    let fresh = cache.source == source
                        && cache.entries.get(&key).is_some_and(|v| {
                            v.version == facts.version && now - v.at < RECHECK_SECS
                        });
                    if !fresh {
                        versions.insert(key.clone(), facts.version);
                        candidates.push(Candidate::new(key, bytes));
                    }
                })
                .map_err(AppError::refused)?;
        }
        Ok(Plan {
            candidates,
            versions,
            endpoint: settings.breach_check.then(|| {
                settings
                    .breach_endpoint
                    .clone()
                    .unwrap_or_else(|| DEFAULT_RANGE_ENDPOINT.to_string())
            }),
            source,
            account: session.account_session(),
        })
    }

    fn security_apply(
        &mut self,
        plan_versions: HashMap<String, u64>,
        source: String,
        breach: Option<Result<Checked, String>>,
        alerts: Option<Result<AlertsSince, String>>,
        now: i64,
    ) -> AppResult<()> {
        let path = self.session()?.data_dir().join(CACHE_FILE);
        if let Some(result) = breach {
            let mut cache = BreachCache::load(&path);
            if cache.source != source {
                cache = BreachCache {
                    source: source.clone(),
                    ..BreachCache::default()
                };
            }
            match result {
                Ok(checked) => {
                    for (key, count) in checked.found {
                        if let Some(v) = plan_versions.get(&key) {
                            cache.entries.insert(
                                key,
                                Verdict {
                                    version: *v,
                                    count,
                                    at: now,
                                },
                            );
                        }
                    }
                    for key in checked.clear {
                        if let Some(v) = plan_versions.get(&key) {
                            cache.entries.insert(
                                key,
                                Verdict {
                                    version: *v,
                                    count: 0,
                                    at: now,
                                },
                            );
                        }
                    }
                    cache.message = checked.error.map(|e| e.to_string());
                    cache.checked_at = Some(now);
                }
                Err(e) => cache.message = Some(e),
            }
            // Forget verdicts for items and fields no longer in the vault.
            let live: std::collections::BTreeSet<String> = {
                let mut keys = std::collections::BTreeSet::new();
                self.session()?
                    .security_scan(|facts, field_id, _, _| {
                        keys.insert(entry_key(&facts.item_id, field_id));
                    })
                    .map_err(AppError::refused)?;
                keys
            };
            cache.entries.retain(|k, _| live.contains(k));
            cache.save(&path).map_err(AppError::refused)?;
        }
        if let Some(result) = alerts {
            self.security.alerts = Some(match result {
                Ok(a) => AlertCache {
                    alerts: a.alerts,
                    at: now,
                    message: None,
                },
                Err(message) => AlertCache {
                    alerts: self
                        .security
                        .alerts
                        .take()
                        .map(|c| c.alerts)
                        .unwrap_or_default(),
                    at: now,
                    message: Some(message),
                },
            });
        }
        Ok(())
    }

    /// After a dismissal wrote version `new` of `item`: the password did
    /// not change, so its verdicts and scores carry over.
    fn security_carry(&mut self, item: &str, old: u64, new: u64) -> AppResult<()> {
        if old == new {
            return Ok(());
        }
        let moved: Vec<((String, String, u64), Option<u8>)> = self
            .security
            .scores
            .iter()
            .filter(|((i, _, v), _)| i == item && *v == old)
            .map(|((i, f, _), s)| ((i.clone(), f.clone(), new), *s))
            .collect();
        self.security.scores.extend(moved);
        let path = self.session()?.data_dir().join(CACHE_FILE);
        let mut cache = BreachCache::load(&path);
        let prefix = format!("{item}/");
        let mut changed = false;
        for (k, v) in cache.entries.iter_mut() {
            if k.starts_with(&prefix) && v.version == old {
                v.version = new;
                changed = true;
            }
        }
        if changed {
            cache.save(&path).map_err(AppError::refused)?;
        }
        Ok(())
    }

    pub(crate) fn security_forget(&mut self) {
        self.security = SecurityState::default();
    }
}

fn alerts_error(e: lockdown_sync::TransportError) -> String {
    match e {
        lockdown_sync::TransportError::Refused { status: 404, .. } => {
            "this server doesn't send account alerts yet".into()
        }
        lockdown_sync::TransportError::Refused { status: 401, .. } => {
            "the session with the server has run out; sync to sign in again".into()
        }
        other => other.to_string(),
    }
}

// --- the commands --------------------------------------------------------------

impl App {
    /// The report from what this device knows now: no network.
    pub fn security_report(&self) -> AppResult<SecurityReport> {
        self.call(|core| core.security_report(session::now_secs()))
    }

    /// Checks again: passwords without a current verdict go to the breach
    /// source (five hex characters each), and the account's alerts are
    /// fetched; then the report. The network calls run on this thread,
    /// not the vault's.
    pub fn security_check(&self) -> AppResult<SecurityReport> {
        let now = session::now_secs();
        let plan = self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            core.security_plan(now)
        })?;
        let breach = plan.endpoint.as_deref().map(|endpoint| {
            HttpFetch::new(NETWORK_TIMEOUT)
                .map_err(|e| e.to_string())
                .and_then(|fetch| {
                    breach::source_for(endpoint, Box::new(fetch)).map_err(|e| e.to_string())
                })
                .map(|source| breach::check(source.as_ref(), &plan.candidates))
        });
        drop(plan.candidates);
        let alerts = plan.account.map(|account| {
            let (server, token) = account?;
            let transport =
                lockdown_sync::HttpTransport::new(&server).map_err(|e| e.to_string())?;
            transport.account_alerts(&token, 0).map_err(alerts_error)
        });
        let (versions, source) = (plan.versions, plan.source);
        self.call(move |core| {
            core.security_apply(versions, source, breach, alerts, now)?;
            core.security_report(now)
        })
    }

    /// Dismisses (`until: None`) or snoozes a finding on an item: a setting
    /// on the item, so it syncs. `key` is the finding's `key`.
    pub fn security_dismiss(
        &self,
        item_id: &str,
        key: &str,
        until: Option<i64>,
    ) -> AppResult<SecurityReport> {
        let (item_id, key) = (item_id.to_string(), key.to_string());
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            let now = session::now_secs();
            if until.is_some_and(|u| u <= now) {
                return Err(AppError::refused("a snooze has to end in the future"));
            }
            let old = core
                .session()?
                .item_anywhere(
                    lockdown_vault::Id::parse(&item_id)
                        .map_err(|_| AppError::refused("that isn't an item id"))?,
                )
                .and_then(|i| i.latest().map(|v| v.n))
                .map_err(|e| AppError::refused(e.to_string()))?;
            let new = core
                .session()?
                .security_set(&item_id, &key, Some(Dismissal { at: now, until }))
                .map_err(AppError::refused)?;
            core.security_carry(&item_id, old, new)?;
            core.security_report(now)
        })
    }

    /// Brings a dismissed or snoozed finding back.
    pub fn security_restore(&self, item_id: &str, key: &str) -> AppResult<SecurityReport> {
        let (item_id, key) = (item_id.to_string(), key.to_string());
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            let old = core
                .session()?
                .item_anywhere(
                    lockdown_vault::Id::parse(&item_id)
                        .map_err(|_| AppError::refused("that isn't an item id"))?,
                )
                .and_then(|i| i.latest().map(|v| v.n))
                .map_err(|e| AppError::refused(e.to_string()))?;
            let new = core
                .session()?
                .security_set(&item_id, &key, None)
                .map_err(AppError::refused)?;
            core.security_carry(&item_id, old, new)?;
            core.security_report(session::now_secs())
        })
    }

    /// Dismisses an account alert for every device of the account.
    pub fn security_dismiss_alert(&self, seq: u64) -> AppResult<SecurityReport> {
        let account = self.call(|core| {
            core.clock.touch(std::time::Instant::now());
            core.session()?
                .account_session()
                .ok_or_else(|| AppError::refused("no account on this device"))?
                .map_err(AppError::refused)
        })?;
        let (server, token) = account;
        let transport = lockdown_sync::HttpTransport::new(&server)
            .map_err(|e| AppError::refused(e.to_string()))?;
        transport
            .dismiss_alert(&token, seq)
            .map_err(|e| AppError::refused(alerts_error(e)))?;
        self.call(move |core| {
            if let Some(cache) = core.security.alerts.as_mut() {
                for a in cache.alerts.iter_mut().filter(|a| a.seq == seq) {
                    a.dismissed = true;
                }
            }
            core.security_report(session::now_secs())
        })
    }

    /// The page to change an item's password on: the site's
    /// `/.well-known/change-password` when it supports it (checked first
    /// unless the setting is off), else its front page.
    pub fn security_change_link(&self, item_id: &str) -> AppResult<ChangeLink> {
        let item_id = item_id.to_string();
        let (site, check) = self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            let check = core.settings.change_password_check;
            let site = core
                .session()?
                .security_site(&item_id)
                .map_err(AppError::refused)?
                .ok_or_else(|| AppError::refused("this item has no website saved"))?;
            Ok((site, check))
        })?;
        let fetch = if check {
            Some(HttpFetch::new(NETWORK_TIMEOUT).map_err(|e| AppError::refused(e.to_string()))?)
        } else {
            None
        };
        change_password::resolve(
            &site,
            fetch.as_ref().map(|f| f as &dyn lockdown_security::Fetch),
        )
        .ok_or_else(|| {
            AppError::refused("this item's website isn't a web address Lockdown can open")
        })
    }

    /// What to do about each kind of finding.
    pub fn security_playbooks(&self) -> Vec<Playbook> {
        PLAYBOOKS.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_words_read_plainly() {
        assert_eq!(count_words(1), "once");
        assert_eq!(count_words(2), "twice");
        assert_eq!(count_words(10_437_277), "10,437,277 times");
        assert_eq!(count_words(999), "999 times");
        let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(list_words(&t(&["A"])), "\u{201c}A\u{201d}");
        assert_eq!(
            list_words(&t(&["A", "B"])),
            "\u{201c}A\u{201d} and \u{201c}B\u{201d}"
        );
        assert_eq!(
            list_words(&t(&["A", "B", "C", "D", "E"])),
            "\u{201c}A\u{201d}, \u{201c}B\u{201d}, \u{201c}C\u{201d} and 2 more"
        );
        assert_eq!(days_of("1970-01-01"), Some(0));
        assert_eq!(days_of("2026-10-18"), Some(20_744));
        assert_eq!(days_of("2026-13-01"), None);
        assert_eq!(days_of("soon"), None);
    }

    #[test]
    fn the_lookalike_test_follows_the_matchers_rules() {
        let eq = EquivalentDomains::curated();
        let s = |u| site_of(u).unwrap();
        assert!(imitates(
            &s("https://paypa1.com/login"),
            &s("paypal.com"),
            &eq
        ));
        assert!(imitates(
            &s("https://paypal.com.evil.net"),
            &s("paypal.com"),
            &eq
        ));
        assert!(imitates(
            &s("https://xn--pypal-4ve.com"),
            &s("paypal.com"),
            &eq
        ));
        assert!(!imitates(
            &s("https://www.paypal.com"),
            &s("paypal.com"),
            &eq
        ));
        assert!(!imitates(&s("https://amazon.co.uk"), &s("amazon.com"), &eq));
        assert!(!imitates(&s("https://example.org"), &s("paypal.com"), &eq));
        // The well-known list is the directory's, prepared once.
        assert!(well_known().len() > 1000);
        assert!(
            well_known()
                .iter()
                .any(|t| imitates(&s("https://githuh.com"), t, &eq))
        );
    }
}
