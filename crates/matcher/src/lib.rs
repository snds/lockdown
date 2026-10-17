//! `lockdown-matcher`: which saved logins belong on which page
//! (03-architecture §2 `matcher` row; 02-model §10; brief §4.9 and §5.7
//! on clickjacking).
//!
//! The rules, per saved URL and synced end to end:
//!
//! | Rule | Matches | Default |
//! |---|---|---|
//! | `exact_host` | host equal, port equal if the saved URL names one, scheme ignored | yes |
//! | `base_domain` | same registrable domain by the Public Suffix List | opt-in |
//! | `equivalent` | a curated or user-defined group (`amazon.com` / `amazon.co.uk`) | curated list on by default |
//! | `starts_with` | the saved URL is a prefix of the current one | opt-in |
//! | `never` | the item is not offered here | |
//!
//! Lookalike hosts (homoglyphs, typo-squats, `paypal.com.evil.net`) get a
//! hard stop, never a fill. Cross-origin frames never get a fill without a
//! confirmation (the DEF CON 33 clickjacking lesson). Related origins from
//! `/.well-known/webauthn` count for passkeys only, capped at five labels.

pub mod lookalike;
pub mod related_origins;
pub mod wire;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use related_origins::RelatedOriginsCache;

/// What can go wrong when parsing a URL.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MatchError {
    /// The text is not a URL with a host.
    #[error("'{0}' isn't a web address with a host")]
    NotAUrl(String),
    /// The equivalent-domain data did not parse.
    #[error("the equivalent-domain list didn't read correctly: {0}")]
    BadData(String),
}

/// The per-URL rule (02-model §10).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchRule {
    /// Host must equal; port must equal if the saved URL names one. The default.
    #[default]
    ExactHost,
    /// Same registrable domain by the Public Suffix List.
    BaseDomain,
    /// Same curated or user-defined group (also checked under every other
    /// rule but `never`, because the curated list is on by default).
    Equivalent,
    /// The saved URL is a prefix of the current one (host exact, then path).
    StartsWith,
    /// Never offer this item here.
    Never,
}

/// How a saved URL relates to the page. On the wire: `exact`,
/// `base_domain`, `equivalent`, `lookalike`, `no_match`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchDecision {
    /// Same host (and port, if the saved URL names one).
    Exact,
    /// Same registrable domain, and the rule allows it.
    BaseDomain,
    /// Same equivalent-domain group.
    Equivalent,
    /// The page's host imitates the saved host. Hard stop.
    Lookalike,
    /// Unrelated.
    NoMatch,
}

/// A parsed page or saved address: scheme, lowercase ASCII host, port, path.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Origin {
    /// `https`, `http`, or an app scheme.
    pub scheme: String,
    /// The host, lowercased and in ASCII (punycode for internationalised names).
    pub host: String,
    /// The port, when the address names one explicitly.
    pub port: Option<u16>,
    /// The path, `/` when none.
    pub path: String,
}

impl Origin {
    /// Parses a URL. Text without a scheme is read as `https://`.
    pub fn parse(text: &str) -> Result<Self, MatchError> {
        let trimmed = text.trim();
        let with_scheme = if trimmed.contains("://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };
        let url =
            url::Url::parse(&with_scheme).map_err(|_| MatchError::NotAUrl(text.to_string()))?;
        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| MatchError::NotAUrl(text.to_string()))?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        Ok(Self {
            scheme: url.scheme().to_string(),
            host,
            port: url.port(),
            path: if url.path().is_empty() {
                "/".to_string()
            } else {
                url.path().to_string()
            },
        })
    }

    /// The registrable domain (eTLD+1) by the Public Suffix List, such as
    /// `example.co.uk` for `www.example.co.uk`. `None` for IP addresses,
    /// bare suffixes and single labels.
    pub fn registrable_domain(&self) -> Option<String> {
        if self.host.parse::<std::net::IpAddr>().is_ok() {
            return None;
        }
        let domain = psl::domain(self.host.as_bytes())?;
        let text = std::str::from_utf8(domain.as_bytes()).ok()?;
        if !text.contains('.') {
            return None;
        }
        Some(text.to_string())
    }

    /// The registrable domain's left-most label: `example` for
    /// `www.example.co.uk`. What lookalike checks compare.
    pub fn registrable_label(&self) -> Option<String> {
        let d = self.registrable_domain()?;
        d.split('.').next().map(str::to_string)
    }

    /// Whether two addresses are the same web origin (scheme, host, port).
    pub fn same_origin(&self, other: &Origin) -> bool {
        self.scheme == other.scheme && self.host == other.host && self.port == other.port
    }
}

/// A URL saved on an item, with its match rule.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SavedUrl {
    /// The parsed address.
    pub origin: Origin,
    /// The rule for this URL.
    pub rule: MatchRule,
}

impl SavedUrl {
    /// Parses `text` and attaches `rule`.
    pub fn parse(text: &str, rule: MatchRule) -> Result<Self, MatchError> {
        Ok(Self {
            origin: Origin::parse(text)?,
            rule,
        })
    }
}

/// One named group of domains that share logins.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DomainGroup {
    /// A short name, such as `amazon`.
    pub name: String,
    /// Registrable domains in the group.
    pub domains: Vec<String>,
}

#[derive(Deserialize)]
struct GroupFile {
    groups: Vec<DomainGroup>,
}

/// The equivalent-domain groups in force: the curated list plus the
/// person's own.
#[derive(Clone, Debug, Default)]
pub struct EquivalentDomains {
    groups: Vec<DomainGroup>,
    index: BTreeMap<String, usize>,
}

/// The curated seed list shipped with the crate.
pub const CURATED_JSON: &str = include_str!("../data/equivalent-domains.json");

impl EquivalentDomains {
    /// An empty set: no equivalence at all.
    pub fn none() -> Self {
        Self::default()
    }

    /// The curated groups. The seed cannot fail to parse; the crate's tests
    /// check it, so this falls back to an empty set rather than panicking.
    pub fn curated() -> Self {
        Self::from_json(CURATED_JSON).unwrap_or_default()
    }

    /// Groups from a JSON document of the seed's shape.
    pub fn from_json(json: &str) -> Result<Self, MatchError> {
        let file: GroupFile =
            serde_json::from_str(json).map_err(|e| MatchError::BadData(e.to_string()))?;
        let mut set = Self::default();
        for g in file.groups {
            set.add_group(g);
        }
        Ok(set)
    }

    /// Adds a group (a user-defined one, or a curated one).
    pub fn add_group(&mut self, group: DomainGroup) {
        let idx = self.groups.len();
        for d in &group.domains {
            self.index.insert(d.to_ascii_lowercase(), idx);
        }
        self.groups.push(group);
    }

    /// The group a registrable domain belongs to, if any.
    pub fn group_of(&self, registrable_domain: &str) -> Option<&DomainGroup> {
        self.index
            .get(&registrable_domain.to_ascii_lowercase())
            .map(|i| &self.groups[*i])
    }

    /// Whether two registrable domains are in one group.
    pub fn equivalent(&self, a: &str, b: &str) -> bool {
        match (
            self.index.get(&a.to_ascii_lowercase()),
            self.index.get(&b.to_ascii_lowercase()),
        ) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// How many groups.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// Decides how `saved` relates to `current`, under `equiv`.
pub fn decide(saved: &SavedUrl, current: &Origin, equiv: &EquivalentDomains) -> MatchDecision {
    if saved.rule == MatchRule::Never {
        return MatchDecision::NoMatch;
    }
    let s = &saved.origin;
    let host_equal = s.host == current.host && (s.port.is_none() || s.port == current.port);
    if host_equal {
        return match saved.rule {
            MatchRule::StartsWith if !current.path.starts_with(&s.path) => MatchDecision::NoMatch,
            _ => MatchDecision::Exact,
        };
    }
    let saved_domain = s.registrable_domain();
    let current_domain = current.registrable_domain();
    if let (Some(a), Some(b)) = (&saved_domain, &current_domain) {
        if a == b {
            if saved.rule == MatchRule::BaseDomain {
                return MatchDecision::BaseDomain;
            }
            // Same registrable domain, different host, rule says exact: not a
            // match, but not a lookalike either (it is the same site owner).
            return MatchDecision::NoMatch;
        }
        if equiv.equivalent(a, b) {
            return MatchDecision::Equivalent;
        }
    }
    if lookalike::host_imitates(&current.host, &s.host) {
        return MatchDecision::Lookalike;
    }
    MatchDecision::NoMatch
}

/// How sensitive the thing being filled is. Codes, cards and passkeys need a
/// second deliberate action by default (brief §4.9).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillKind {
    /// Username and password.
    Login,
    /// A one-time code.
    Code,
    /// Payment card details.
    Card,
    /// A passkey assertion.
    Passkey,
    /// Identity or address fields.
    Identity,
}

/// What the extension knows about the page at fill time.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FillContext {
    /// The top-level page.
    pub top: Origin,
    /// The frame holding the field (same as `top` when there is no frame).
    pub frame: Origin,
    /// The field and every ancestor are visible, on screen, and not covered.
    pub field_visible: bool,
    /// The person already took a deliberate action for this fill.
    pub user_confirmed: bool,
    /// What is being filled.
    pub kind: FillKind,
}

/// What the extension may do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FillVerdict {
    /// Fill now.
    Fill,
    /// Ask first, with this reason shown.
    Confirm(String),
    /// Do not fill; show this reason.
    Refuse(String),
}

/// The fill gate. Order: the match decision, visibility, cross-origin
/// frames, then the second-action rule for sensitive kinds.
pub fn fill_check(ctx: &FillContext, decision: MatchDecision) -> FillVerdict {
    match decision {
        MatchDecision::Lookalike => {
            return FillVerdict::Refuse(format!(
                "{} looks like a different site you have a login for; Lockdown won't fill here",
                ctx.frame.host
            ));
        }
        MatchDecision::NoMatch => {
            return FillVerdict::Refuse("no saved login matches this page".into());
        }
        MatchDecision::Exact | MatchDecision::BaseDomain | MatchDecision::Equivalent => {}
    }
    if !ctx.field_visible {
        return FillVerdict::Refuse(
            "the field isn't fully visible; hidden fields are how clickjacking steals a fill"
                .into(),
        );
    }
    if !ctx.frame.same_origin(&ctx.top) && !ctx.user_confirmed {
        return FillVerdict::Confirm(format!(
            "this field belongs to {} inside a page from {}; confirm before filling",
            ctx.frame.host, ctx.top.host
        ));
    }
    match ctx.kind {
        FillKind::Code | FillKind::Card | FillKind::Passkey if !ctx.user_confirmed => {
            FillVerdict::Confirm(
                "codes, cards and passkeys need a deliberate tap before filling".into(),
            )
        }
        _ => FillVerdict::Fill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(url: &str, rule: MatchRule) -> SavedUrl {
        SavedUrl::parse(url, rule).unwrap()
    }
    fn origin(url: &str) -> Origin {
        Origin::parse(url).unwrap()
    }

    #[test]
    fn the_curated_list_parses_and_groups() {
        let eq = EquivalentDomains::curated();
        assert!(eq.len() >= 15, "{} groups", eq.len());
        assert!(eq.equivalent("google.com", "youtube.com"));
        assert!(eq.equivalent("amazon.com", "amazon.co.uk"));
        assert!(eq.equivalent("microsoft.com", "outlook.com"));
        assert!(eq.equivalent("apple.com", "icloud.com"));
        assert!(!eq.equivalent("apple.com", "google.com"));
        assert_eq!(eq.group_of("live.com").unwrap().name, "microsoft");
    }

    #[test]
    fn registrable_domain_uses_the_public_suffix_list() {
        assert_eq!(
            origin("https://www.example.co.uk/a")
                .registrable_domain()
                .as_deref(),
            Some("example.co.uk")
        );
        assert_eq!(
            origin("https://foo.github.io")
                .registrable_domain()
                .as_deref(),
            Some("foo.github.io")
        );
        assert_eq!(
            origin("https://www.example.co.uk")
                .registrable_label()
                .as_deref(),
            Some("example")
        );
        assert_eq!(origin("https://127.0.0.1:8080").registrable_domain(), None);
        assert_eq!(origin("https://localhost").registrable_domain(), None);
        assert_eq!(origin("https://co.uk").registrable_domain(), None);
    }

    #[test]
    fn exact_host_is_the_default_and_ports_count_when_named() {
        let eq = EquivalentDomains::curated();
        let s = saved("https://accounts.example.com/login", MatchRule::ExactHost);
        assert_eq!(
            decide(&s, &origin("http://accounts.example.com/other"), &eq),
            MatchDecision::Exact
        );
        assert_eq!(
            decide(&s, &origin("https://accounts.example.com:8443"), &eq),
            MatchDecision::Exact
        );
        assert_eq!(
            decide(&s, &origin("https://www.example.com"), &eq),
            MatchDecision::NoMatch
        );
        let with_port = saved("https://example.com:8443", MatchRule::ExactHost);
        assert_eq!(
            decide(&with_port, &origin("https://example.com"), &eq),
            MatchDecision::NoMatch
        );
        assert_eq!(
            decide(&with_port, &origin("https://example.com:8443"), &eq),
            MatchDecision::Exact
        );
    }

    #[test]
    fn base_domain_is_opt_in() {
        let eq = EquivalentDomains::none();
        let s = saved("https://accounts.example.co.uk", MatchRule::BaseDomain);
        assert_eq!(
            decide(&s, &origin("https://shop.example.co.uk"), &eq),
            MatchDecision::BaseDomain
        );
        assert_eq!(
            decide(&s, &origin("https://example.co.uk"), &eq),
            MatchDecision::BaseDomain
        );
        assert_eq!(
            decide(&s, &origin("https://example.com"), &eq),
            MatchDecision::NoMatch
        );
        let exact = saved("https://accounts.example.co.uk", MatchRule::ExactHost);
        assert_eq!(
            decide(&exact, &origin("https://shop.example.co.uk"), &eq),
            MatchDecision::NoMatch
        );
    }

    #[test]
    fn equivalent_groups_apply_by_default_and_user_groups_are_explicit() {
        let eq = EquivalentDomains::curated();
        let s = saved("https://www.amazon.com", MatchRule::ExactHost);
        assert_eq!(
            decide(&s, &origin("https://www.amazon.co.uk"), &eq),
            MatchDecision::Equivalent
        );
        assert_eq!(
            decide(
                &s,
                &origin("https://www.amazon.co.uk"),
                &EquivalentDomains::none()
            ),
            MatchDecision::NoMatch
        );
        let mut mine = EquivalentDomains::none();
        mine.add_group(DomainGroup {
            name: "work".into(),
            domains: vec!["example.com".into(), "example.net".into()],
        });
        let w = saved("https://sso.example.com", MatchRule::ExactHost);
        assert_eq!(
            decide(&w, &origin("https://mail.example.net"), &mine),
            MatchDecision::Equivalent
        );
    }

    #[test]
    fn starts_with_and_never() {
        let eq = EquivalentDomains::none();
        let s = saved("https://example.com/app/", MatchRule::StartsWith);
        assert_eq!(
            decide(&s, &origin("https://example.com/app/login"), &eq),
            MatchDecision::Exact
        );
        assert_eq!(
            decide(&s, &origin("https://example.com/other"), &eq),
            MatchDecision::NoMatch
        );
        let n = saved("https://example.com", MatchRule::Never);
        assert_eq!(
            decide(&n, &origin("https://example.com"), &eq),
            MatchDecision::NoMatch
        );
    }

    #[test]
    fn lookalikes_get_a_hard_stop() {
        let eq = EquivalentDomains::curated();
        let s = saved("https://www.paypal.com", MatchRule::ExactHost);
        for host in [
            "https://www.paypa1.com",
            "https://www.xn--pypal-4ve.com", // Cyrillic а
            "https://paypal.com.evil.net",
            "https://paypal-secure.com",
            "https://www.paypai.com",
        ] {
            assert_eq!(
                decide(&s, &origin(host), &eq),
                MatchDecision::Lookalike,
                "{host}"
            );
        }
        assert_eq!(
            decide(&s, &origin("https://www.example.com"), &eq),
            MatchDecision::NoMatch
        );
    }

    #[test]
    fn never_fill_cross_origin_iframes_without_confirmation() {
        let top = origin("https://news.example");
        let frame = origin("https://login.bank.example");
        let mut ctx = FillContext {
            top: top.clone(),
            frame: frame.clone(),
            field_visible: true,
            user_confirmed: false,
            kind: FillKind::Login,
        };
        assert!(
            matches!(fill_check(&ctx, MatchDecision::Exact), FillVerdict::Confirm(r) if r.contains("inside a page from news.example"))
        );
        ctx.user_confirmed = true;
        assert_eq!(fill_check(&ctx, MatchDecision::Exact), FillVerdict::Fill);
        // Same origin, no confirmation needed for a login.
        ctx.user_confirmed = false;
        ctx.frame = top.clone();
        assert_eq!(fill_check(&ctx, MatchDecision::Exact), FillVerdict::Fill);
        // A hidden field is refused outright.
        ctx.field_visible = false;
        assert!(
            matches!(fill_check(&ctx, MatchDecision::Exact), FillVerdict::Refuse(r) if r.contains("clickjacking"))
        );
        ctx.field_visible = true;
        // Codes, cards and passkeys need a second action by default.
        for kind in [FillKind::Code, FillKind::Card, FillKind::Passkey] {
            ctx.kind = kind;
            assert!(matches!(
                fill_check(&ctx, MatchDecision::Exact),
                FillVerdict::Confirm(_)
            ));
        }
        // A lookalike is refused whatever else is true.
        ctx.user_confirmed = true;
        assert!(matches!(
            fill_check(&ctx, MatchDecision::Lookalike),
            FillVerdict::Refuse(_)
        ));
    }
}
