//! Generates the matching set (06-roadmap workstream 10; 09-extension-boundary §3).
//!
//! Every row is **constructed** with its expected decision, never computed
//! by calling the matcher, so the set is an independent statement of what
//! the rules mean. `tests/sites.rs` regenerates the set and compares it to
//! the committed `sites.json`, then runs every row through `decide`.
//!
//! Families (the `group` field):
//!
//! | group | what it covers |
//! |---|---|
//! | `exact` | same host under exact_host: case, trailing dot, scheme, path, query, userinfo, ports |
//! | `subdomain` | depth under exact_host (no match) and base_domain (match), multi-label suffixes |
//! | `starts_with` | the prefix rule |
//! | `equivalent` | every curated group, first domain ↔ every other, both ways; `never` on a group |
//! | `lookalike` | digit swaps, `rn`, hyphen decoys, leading-subdomain squats, Cyrillic homoglyphs, one-character typos |
//! | `unrelated` | two real sites that must not match |
//! | `host-kinds` | IP literals, localhost, IDN test names, extension origins, unparsable schemes |
//!
//! The site list is real, public, top-traffic registrable domains only.

use std::collections::BTreeSet;

use lockdown_matcher::MatcherData;
use serde::{Deserialize, Serialize};

/// Where the committed set lives, relative to the crate root.
pub const SITES_PATH: &str = "tests/sites/sites.json";

/// The schema version of `sites.json`.
pub const SET_VERSION: u32 = 1;

/// One match row.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Row {
    /// `<group>-<n>`.
    pub id: String,
    /// The family.
    pub group: String,
    /// The saved address, as the person typed it.
    pub saved: String,
    /// The rule on the saved address.
    pub rule: String,
    /// The page address.
    pub current: String,
    /// `exact`, `base_domain`, `equivalent`, `lookalike`, `no_match`, or
    /// `error` when the page address must not parse.
    pub expected: String,
    /// Why.
    pub note: String,
}

/// One related-origins row.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RelatedRow {
    /// `related-<n>`.
    pub id: String,
    /// The relying-party id.
    pub rp_id: String,
    /// The document's `origins`.
    pub origins: Vec<String>,
    /// When the document was fetched.
    pub fetched_at: u64,
    /// Now.
    pub now: u64,
    /// Freshness window.
    pub max_age_secs: u64,
    /// The origin asking.
    pub origin: String,
    /// Whether it may use the relying party's passkeys.
    pub expected: bool,
    /// Why.
    pub note: String,
}

/// The whole file.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SiteSet {
    /// [`SET_VERSION`].
    pub set_version: u32,
    /// Where this came from.
    pub generated_by: String,
    /// How many match rows.
    pub rows: usize,
    /// How many related-origins rows.
    pub related_rows: usize,
    /// The match rows.
    pub matches: Vec<Row>,
    /// The related-origins rows.
    pub related_origins: Vec<RelatedRow>,
}

/// Real, public, top-traffic registrable domains. Multi-label public
/// suffixes (`co.uk`, `co.jp`, `com.ar`, `com.au`) are in on purpose.
pub const SITES: &[&str] = &[
    "google.com",
    "youtube.com",
    "facebook.com",
    "instagram.com",
    "amazon.com",
    "wikipedia.org",
    "reddit.com",
    "netflix.com",
    "microsoft.com",
    "live.com",
    "linkedin.com",
    "apple.com",
    "github.com",
    "paypal.com",
    "ebay.com",
    "twitch.tv",
    "spotify.com",
    "dropbox.com",
    "adobe.com",
    "zoom.us",
    "slack.com",
    "discord.com",
    "pinterest.com",
    "wordpress.com",
    "shopify.com",
    "stripe.com",
    "cloudflare.com",
    "salesforce.com",
    "atlassian.com",
    "gitlab.com",
    "stackoverflow.com",
    "medium.com",
    "nytimes.com",
    "bbc.co.uk",
    "theguardian.com",
    "amazon.co.uk",
    "google.co.uk",
    "yahoo.co.jp",
    "rakuten.co.jp",
    "mercadolibre.com.ar",
    "telstra.com.au",
    "booking.com",
    "airbnb.com",
    "uber.com",
    "coinbase.com",
    "chase.com",
    "bankofamerica.com",
    "wellsfargo.com",
    "hsbc.co.uk",
    "barclays.co.uk",
    "steampowered.com",
    "epicgames.com",
    "roblox.com",
    "duckduckgo.com",
    "icloud.com",
    "outlook.com",
    "canva.com",
    "figma.com",
    "notion.so",
    "trello.com",
];

fn label_of(site: &str) -> &str {
    site.split('.').next().unwrap_or(site)
}

fn suffix_of(site: &str) -> &str {
    site.split_once('.').map(|(_, s)| s).unwrap_or("")
}

struct Builder {
    rows: Vec<Row>,
    counters: std::collections::BTreeMap<String, usize>,
}

impl Builder {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            counters: Default::default(),
        }
    }

    fn row(
        &mut self,
        group: &str,
        saved: &str,
        rule: &str,
        current: &str,
        expected: &str,
        note: &str,
    ) {
        let n = self.counters.entry(group.to_string()).or_insert(0);
        *n += 1;
        self.rows.push(Row {
            id: format!("{group}-{n:03}"),
            group: group.into(),
            saved: saved.into(),
            rule: rule.into(),
            current: current.into(),
            expected: expected.into(),
            note: note.into(),
        });
    }
}

/// Builds the set from the data in force (the curated groups come from it).
pub fn generate(data: &MatcherData) -> SiteSet {
    let mut b = Builder::new();

    // ---- exact: same host, exact_host rule -------------------------------
    for site in SITES {
        let saved = format!("https://www.{site}/login");
        let host = format!("www.{site}");
        b.row(
            "exact",
            &saved,
            "exact_host",
            &format!("https://{host}/login"),
            "exact",
            "same address",
        );
        b.row(
            "exact",
            &saved,
            "exact_host",
            &format!("HTTPS://{}./Account?next=%2Fhome#top", host.to_uppercase()),
            "exact",
            "host case, trailing dot, scheme case, query and fragment are not identity",
        );
        b.row(
            "exact",
            &saved,
            "exact_host",
            &format!("http://{host}/other/path"),
            "exact",
            "scheme and path are ignored under exact_host",
        );
        b.row(
            "exact",
            &saved,
            "exact_host",
            &format!("https://{host}:8443/"),
            "exact",
            "the saved address names no port, so any port matches",
        );
        b.row(
            "exact",
            &saved,
            "exact_host",
            &format!("https://user:secret@{host}/"),
            "exact",
            "userinfo does not change the host",
        );
        b.row(
            "exact",
            &format!("https://{host}:8443"),
            "exact_host",
            &format!("https://{host}/"),
            "no_match",
            "the saved address names a port the page does not have",
        );
        b.row(
            "exact",
            &host,
            "exact_host",
            &format!("https://{host}/"),
            "exact",
            "a saved address without a scheme reads as https",
        );
    }

    // ---- subdomain: depth, base_domain, multi-label suffixes -------------
    for site in SITES {
        let www = format!("https://www.{site}");
        b.row(
            "subdomain",
            &www,
            "exact_host",
            &format!("https://accounts.{site}/"),
            "no_match",
            "another host of the same site is not exact",
        );
        b.row(
            "subdomain",
            &www,
            "exact_host",
            &format!("https://{site}/"),
            "no_match",
            "the bare registrable domain is another host",
        );
        b.row(
            "subdomain",
            &www,
            "base_domain",
            &format!("https://accounts.{site}/"),
            "base_domain",
            "same registrable domain by the PSL",
        );
        b.row(
            "subdomain",
            &www,
            "base_domain",
            &format!("https://a.b.c.{site}/"),
            "base_domain",
            "depth does not matter under base_domain",
        );
        b.row(
            "subdomain",
            &format!("https://{site}"),
            "base_domain",
            &format!("https://www.{site}/"),
            "base_domain",
            "bare saved, www page",
        );
        b.row(
            "subdomain",
            &www,
            "base_domain",
            &format!("https://www.{site}/"),
            "exact",
            "the same host is exact even under base_domain",
        );
    }
    // A multi-label suffix is not a registrable domain of its own.
    for (site, suffix) in [
        ("bbc.co.uk", "co.uk"),
        ("yahoo.co.jp", "co.jp"),
        ("mercadolibre.com.ar", "com.ar"),
        ("telstra.com.au", "com.au"),
    ] {
        b.row(
            "subdomain",
            &format!("https://www.{site}"),
            "base_domain",
            &format!("https://other.{suffix}/"),
            "no_match",
            "sharing a public suffix is not sharing a site",
        );
    }

    // ---- starts_with -----------------------------------------------------
    for site in ["github.com", "notion.so", "wordpress.com", "medium.com"] {
        let saved = format!("https://{site}/team-a/");
        b.row(
            "starts_with",
            &saved,
            "starts_with",
            &format!("https://{site}/team-a/login"),
            "exact",
            "the saved path is a prefix",
        );
        b.row(
            "starts_with",
            &saved,
            "starts_with",
            &format!("https://{site}/team-a/"),
            "exact",
            "the same path",
        );
        b.row(
            "starts_with",
            &saved,
            "starts_with",
            &format!("https://{site}/team-b/"),
            "no_match",
            "another path",
        );
        b.row(
            "starts_with",
            &saved,
            "starts_with",
            &format!("https://{site}/team-a"),
            "no_match",
            "without the trailing slash the saved path is not a prefix",
        );
        b.row(
            "starts_with",
            &saved,
            "starts_with",
            &format!("https://www.{site}/team-a/"),
            "no_match",
            "another host, whatever the path",
        );
    }

    // ---- equivalent: every curated group, first ↔ each, both ways --------
    let mut seen_groups: BTreeSet<String> = BTreeSet::new();
    let mut group_domains: Vec<(String, Vec<String>)> = Vec::new();
    for site in SITES.iter().chain(
        [
            "google.de",
            "amazon.co.jp",
            "outlook.com",
            "hulu.com",
            "proton.me",
            "wikidata.org",
            "bitbucket.org",
            "steamcommunity.com",
            "nflxext.com",
            "messenger.com",
            "aol.com",
            "zoom.com",
            "paypal.me",
            "githubusercontent.com",
        ]
        .iter(),
    ) {
        if let Some(g) = data.equivalent.group_of(site)
            && seen_groups.insert(g.name.clone())
        {
            group_domains.push((g.name.clone(), g.domains.clone()));
        }
    }
    for (name, domains) in &group_domains {
        let first = &domains[0];
        for other in &domains[1..] {
            b.row(
                "equivalent",
                &format!("https://www.{first}"),
                "exact_host",
                &format!("https://www.{other}/"),
                "equivalent",
                &format!("curated group `{name}`"),
            );
            b.row(
                "equivalent",
                &format!("https://www.{other}"),
                "exact_host",
                &format!("https://login.{first}/"),
                "equivalent",
                &format!("curated group `{name}`, the other way and another host"),
            );
        }
        if domains.len() > 1 {
            b.row(
                "equivalent",
                &format!("https://www.{first}"),
                "never",
                &format!("https://www.{}/", domains[1]),
                "no_match",
                "`never` beats the group",
            );
            b.row(
                "equivalent",
                &format!("https://www.{first}"),
                "never",
                &format!("https://www.{first}/"),
                "no_match",
                "`never` beats the same host too",
            );
        }
    }

    // ---- lookalike -------------------------------------------------------
    for site in SITES {
        let label = label_of(site);
        let suffix = suffix_of(site);
        let saved = format!("https://www.{site}");
        // Digit swap with a confusable digit.
        if let Some(pos) = label.find(['o', 'l']) {
            let c = &label[pos..pos + 1];
            let swapped = format!(
                "{}{}{}",
                &label[..pos],
                if c == "o" { "0" } else { "1" },
                &label[pos + 1..]
            );
            b.row(
                "lookalike",
                &saved,
                "exact_host",
                &format!("https://www.{swapped}.{suffix}/"),
                "lookalike",
                "a digit that renders like the letter",
            );
        }
        // rn for m.
        if let Some(pos) = label.find('m') {
            let swapped = format!("{}rn{}", &label[..pos], &label[pos + 1..]);
            b.row(
                "lookalike",
                &saved,
                "exact_host",
                &format!("https://www.{swapped}.{suffix}/"),
                "lookalike",
                "`rn` reads as `m`",
            );
        }
        // Hyphen decoy.
        b.row(
            "lookalike",
            &saved,
            "exact_host",
            &format!("https://{label}-login.com/"),
            "lookalike",
            "the trusted label as a hyphenated part",
        );
        b.row(
            "lookalike",
            &saved,
            "exact_host",
            &format!("https://secure-{label}.net/signin"),
            "lookalike",
            "the trusted label as a hyphenated part, other side",
        );
        // Leading-subdomain squat.
        b.row(
            "lookalike",
            &saved,
            "exact_host",
            &format!("https://{site}.example.net/"),
            "lookalike",
            "the trusted registrable domain as a leading sub-domain",
        );
        b.row(
            "lookalike",
            &saved,
            "exact_host",
            &format!("https://login.{site}.example.org/"),
            "lookalike",
            "the trusted registrable domain in the middle of the host",
        );
        // Cyrillic homoglyph: first of a, e, o, p, c.
        let cyr = [('a', 'а'), ('e', 'е'), ('o', 'о'), ('p', 'р'), ('c', 'с')];
        if let Some((pos, (_, to))) = label.char_indices().find_map(|(i, ch)| {
            cyr.iter()
                .find(|(from, _)| *from == ch)
                .map(|pair| (i, *pair))
        }) {
            let mut swapped = String::new();
            swapped.push_str(&label[..pos]);
            swapped.push(to);
            swapped.push_str(&label[pos + 1..]);
            b.row(
                "lookalike",
                &saved,
                "exact_host",
                &format!("https://www.{swapped}.{suffix}/"),
                "lookalike",
                "a Cyrillic letter in place of the Latin one (punycode on the wire)",
            );
        }
        // One-character typos on labels of six or more characters.
        if label.chars().count() >= 6 {
            let dropped = &label[..label.len() - 1];
            b.row(
                "lookalike",
                &saved,
                "exact_host",
                &format!("https://www.{dropped}.{suffix}/"),
                "lookalike",
                "last character dropped: edit distance one",
            );
            let doubled = format!("{}{}{}", &label[..2], &label[1..2], &label[2..]);
            b.row(
                "lookalike",
                &saved,
                "exact_host",
                &format!("https://{doubled}.{suffix}/"),
                "lookalike",
                "a doubled letter: edit distance one",
            );
        }
        // Same lookalike under `never`: the rule wins, the item is simply not offered.
        b.row(
            "lookalike",
            &saved,
            "never",
            &format!("https://{label}-login.com/"),
            "no_match",
            "`never` means not offered, not a warning",
        );
    }
    // Short labels get no typo guess.
    b.row(
        "lookalike",
        "https://www.uber.com",
        "exact_host",
        "https://www.ubar.com/",
        "no_match",
        "labels under six characters: no edit-distance guess",
    );
    b.row(
        "lookalike",
        "https://www.chase.com",
        "exact_host",
        "https://www.chasm.com/",
        "no_match",
        "labels under six characters: no edit-distance guess",
    );
    // Same registrable domain is never a lookalike.
    b.row(
        "lookalike",
        "https://www.paypal.com",
        "exact_host",
        "https://paypa1.paypal.com/",
        "no_match",
        "a suspicious label under the real domain is the real site's own host",
    );

    // ---- unrelated: two real sites ---------------------------------------
    let pairs: &[(&str, &str)] = &[
        ("google.com", "wikipedia.org"),
        ("facebook.com", "netflix.com"),
        ("amazon.com", "reddit.com"),
        ("github.com", "spotify.com"),
        ("paypal.com", "dropbox.com"),
        ("apple.com", "adobe.com"),
        ("microsoft.com", "slack.com"),
        ("ebay.com", "zoom.us"),
        ("linkedin.com", "discord.com"),
        ("twitch.tv", "pinterest.com"),
        ("stripe.com", "shopify.com"),
        ("cloudflare.com", "salesforce.com"),
        ("atlassian.com", "gitlab.com"),
        ("stackoverflow.com", "nytimes.com"),
        ("bbc.co.uk", "theguardian.com"),
        ("yahoo.co.jp", "rakuten.co.jp"),
        ("booking.com", "airbnb.com"),
        ("coinbase.com", "chase.com"),
        ("bankofamerica.com", "wellsfargo.com"),
        ("hsbc.co.uk", "barclays.co.uk"),
        ("steampowered.com", "epicgames.com"),
        ("roblox.com", "duckduckgo.com"),
        ("canva.com", "figma.com"),
        ("notion.so", "trello.com"),
        ("medium.com", "wordpress.com"),
        ("uber.com", "instagram.com"),
        ("youtube.com", "live.com"),
        ("outlook.com", "icloud.com"),
        ("mercadolibre.com.ar", "telstra.com.au"),
        ("amazon.co.uk", "google.co.uk"),
    ];
    for (a, c) in pairs {
        b.row(
            "unrelated",
            &format!("https://www.{a}"),
            "exact_host",
            &format!("https://www.{c}/"),
            "no_match",
            "different sites",
        );
        b.row(
            "unrelated",
            &format!("https://www.{c}"),
            "base_domain",
            &format!("https://www.{a}/"),
            "no_match",
            "different sites, base_domain changes nothing",
        );
    }

    // ---- host-kinds ------------------------------------------------------
    b.row(
        "host-kinds",
        "http://192.168.1.1",
        "exact_host",
        "http://192.168.1.1/admin",
        "exact",
        "an IPv4 literal is a host",
    );
    b.row(
        "host-kinds",
        "http://192.168.1.1",
        "exact_host",
        "http://192.168.1.2/",
        "no_match",
        "another address",
    );
    b.row(
        "host-kinds",
        "http://192.168.1.1:8080",
        "exact_host",
        "http://192.168.1.1/",
        "no_match",
        "the saved address names a port",
    );
    b.row(
        "host-kinds",
        "http://192.168.1.1:8080",
        "exact_host",
        "http://192.168.1.1:8080/",
        "exact",
        "same address and port",
    );
    b.row(
        "host-kinds",
        "http://192.168.1.1",
        "base_domain",
        "http://192.168.1.2/",
        "no_match",
        "IP literals have no registrable domain",
    );
    b.row(
        "host-kinds",
        "http://[::1]:3000",
        "exact_host",
        "http://[::1]:3000/",
        "exact",
        "an IPv6 literal is a host",
    );
    b.row(
        "host-kinds",
        "http://[2001:db8::1]",
        "exact_host",
        "http://[2001:db8::2]/",
        "no_match",
        "another IPv6 address",
    );
    b.row(
        "host-kinds",
        "http://localhost:3000",
        "exact_host",
        "http://localhost:3000/app",
        "exact",
        "localhost with a port",
    );
    b.row(
        "host-kinds",
        "http://localhost:3000",
        "exact_host",
        "http://localhost:4000/",
        "no_match",
        "another port",
    );
    b.row(
        "host-kinds",
        "http://localhost",
        "exact_host",
        "http://localhost:3000/",
        "exact",
        "the saved address names no port",
    );
    b.row(
        "host-kinds",
        "http://localhost",
        "base_domain",
        "http://app.localhost/",
        "no_match",
        "localhost has no registrable domain",
    );
    b.row(
        "host-kinds",
        "http://127.0.0.1",
        "exact_host",
        "http://localhost/",
        "no_match",
        "a name and an address are different hosts",
    );
    b.row(
        "host-kinds",
        "https://www.paypal.com",
        "exact_host",
        "http://127.0.0.1:8080/",
        "no_match",
        "a local address never imitates a site",
    );
    // IANA IDN test names.
    b.row(
        "host-kinds",
        "https://例え.テスト",
        "exact_host",
        "https://xn--r8jz45g.xn--zckzah/",
        "exact",
        "an internationalised name and its punycode are one host",
    );
    b.row(
        "host-kinds",
        "https://xn--r8jz45g.xn--zckzah",
        "exact_host",
        "https://例え.テスト/",
        "exact",
        "the other way round",
    );
    b.row(
        "host-kinds",
        "https://例え.テスト",
        "exact_host",
        "https://www.xn--r8jz45g.xn--zckzah/",
        "no_match",
        "another host under exact_host",
    );
    b.row(
        "host-kinds",
        "https://例え.テスト",
        "base_domain",
        "https://www.xn--r8jz45g.xn--zckzah/",
        "base_domain",
        "same registrable domain under base_domain",
    );
    b.row(
        "host-kinds",
        "https://müller.example",
        "exact_host",
        "https://xn--mller-kva.example/",
        "exact",
        "a Latin IDN label and its punycode",
    );
    // Extension and other origins.
    b.row(
        "host-kinds",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/popup.html",
        "exact_host",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/options.html",
        "exact",
        "an extension id is a host",
    );
    b.row(
        "host-kinds",
        "https://www.github.com",
        "exact_host",
        "chrome-extension://abcdefghijklmnopabcdefghijklmnop/popup.html",
        "no_match",
        "an extension page is not a site",
    );
    b.row(
        "host-kinds",
        "moz-extension://0f7f2d9e-3c1b-4e62-9b1a-5c3f1e2d4a6b/",
        "exact_host",
        "moz-extension://0f7f2d9e-3c1b-4e62-9b1a-5c3f1e2d4a6b/index.html",
        "exact",
        "a Firefox extension origin",
    );
    for current in [
        "file:///C:/Users/me/login.html",
        "file:///home/me/login.html",
        "data:text/html,<form>",
        "about:blank",
        "javascript:alert(1)",
        "blob:https://www.github.com/0f7f2d9e",
        "mailto:someone@example.com",
        "",
        "not a url",
    ] {
        b.row(
            "host-kinds",
            "https://www.github.com",
            "exact_host",
            current,
            "error",
            "no host: the page address does not parse, so nothing is offered",
        );
    }
    b.row(
        "host-kinds",
        "https://www.github.com",
        "exact_host",
        "https://www.github.com:65536/",
        "error",
        "an impossible port does not parse",
    );
    b.row(
        "host-kinds",
        "https://www.github.com",
        "exact_host",
        "https://www.github.com@evil.example/",
        "no_match",
        "the saved host as userinfo: the real host is after the @",
    );
    b.row(
        "host-kinds",
        "https://www.paypal.com",
        "exact_host",
        "https://www.paypal.com@paypal.com.example.net/",
        "lookalike",
        "userinfo decoy over a leading-subdomain squat",
    );

    // ---- related origins -------------------------------------------------
    let doc: Vec<String> = [
        "https://example.co.uk",
        "https://example.de",
        "https://shop.example.de",
        "https://brand-a.com",
        "https://brand-b.com",
        "https://brand-c.com",
        "https://brand-d.com",
        "https://brand-e.com",
        "https://brand-f.com",
        "http://insecure.example.com",
        "not a url",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut related = Vec::new();
    let mut rel = |rp: &str,
                   origins: &[String],
                   fetched_at: u64,
                   now: u64,
                   max_age: u64,
                   origin: &str,
                   expected: bool,
                   note: &str| {
        related.push(RelatedRow {
            id: format!("related-{:03}", related.len() + 1),
            rp_id: rp.into(),
            origins: origins.to_vec(),
            fetched_at,
            now,
            max_age_secs: max_age,
            origin: origin.into(),
            expected,
            note: note.into(),
        });
    };
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://example.com",
        true,
        "the relying party itself",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://login.example.com",
        true,
        "a host under the relying party",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://example.co.uk",
        true,
        "listed, label 1 (example)",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://example.de",
        true,
        "listed, same label (example)",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://shop.example.de",
        true,
        "listed, same label, another host",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-a.com",
        true,
        "listed, label 2",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-d.com",
        true,
        "listed, label 5: the last honoured",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-e.com",
        false,
        "label 6: beyond the five-label cap",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-f.com",
        false,
        "label 7: beyond the cap",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "http://insecure.example.com",
        true,
        "http, but a host under the relying party is always allowed",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "http://example.co.uk",
        false,
        "listed as https; http is another origin",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://example.co.uk:8443",
        false,
        "listed without a port; a port is another origin",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://www.example.co.uk",
        false,
        "listed origin is the bare host; www is another origin",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-a.com.example.net",
        false,
        "a squat on a listed origin",
    );
    rel(
        "example.com",
        &doc,
        1000,
        1000 + 3601,
        3600,
        "https://example.co.uk",
        false,
        "stale document: only the relying party's own hosts pass",
    );
    rel(
        "example.com",
        &doc,
        1000,
        1000 + 3601,
        3600,
        "https://account.example.com",
        true,
        "stale document, own host",
    );
    rel(
        "example.com",
        &doc,
        1000,
        1000 + 3600,
        3600,
        "https://example.co.uk",
        true,
        "exactly at the freshness edge is still fresh",
    );
    rel(
        "example.com",
        &[],
        1000,
        2000,
        3600,
        "https://example.co.uk",
        false,
        "empty document",
    );
    rel(
        "example.com",
        &[],
        1000,
        2000,
        3600,
        "https://example.com",
        true,
        "empty document, the relying party itself",
    );
    rel(
        "EXAMPLE.com",
        &doc,
        1000,
        2000,
        3600,
        "https://brand-b.com",
        true,
        "relying-party id case does not matter",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://notexample.com",
        false,
        "a host that merely ends in the letters",
    );
    rel(
        "example.com",
        &doc,
        1000,
        2000,
        3600,
        "https://example.com.example.net",
        false,
        "the relying party as a leading sub-domain elsewhere",
    );

    SiteSet {
        set_version: SET_VERSION,
        generated_by: "crates/matcher/tests/sites/gen.rs".into(),
        rows: b.rows.len(),
        related_rows: related.len(),
        matches: b.rows,
        related_origins: related,
    }
}

/// The committed form: one row per line, so a review reads one case per
/// line and a diff shows exactly which rows changed.
pub fn render(set: &SiteSet) -> String {
    fn rows<T: Serialize>(out: &mut String, items: &[T]) {
        for (i, r) in items.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&serde_json::to_string(r).expect("a row serialises"));
            out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
        }
    }
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"set_version\": {},\n", set.set_version));
    out.push_str(&format!(
        "  \"generated_by\": {},\n",
        serde_json::to_string(&set.generated_by).expect("a string serialises")
    ));
    out.push_str(&format!("  \"rows\": {},\n", set.rows));
    out.push_str(&format!("  \"related_rows\": {},\n", set.related_rows));
    out.push_str("  \"matches\": [\n");
    rows(&mut out, &set.matches);
    out.push_str("  ],\n  \"related_origins\": [\n");
    rows(&mut out, &set.related_origins);
    out.push_str("  ]\n}\n");
    out
}
