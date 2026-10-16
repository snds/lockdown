//! Related origins for passkeys (02-model §10; WebAuthn Level 3 "Related
//! Origin Requests").
//!
//! A relying party may publish `https://<rp_id>/.well-known/webauthn` with
//! `{"origins": [...]}` so a passkey registered for `example.com` also works
//! on `example.co.uk`. Browsers honour at most **five distinct labels**
//! (the left-most label of each origin's registrable domain) and ignore the
//! rest; so does this cache. Related origins count for passkeys only; they
//! never widen password matching.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Origin;

/// The maximum number of distinct labels honoured, as browsers do.
pub const MAX_LABELS: usize = 5;

/// The document at `/.well-known/webauthn`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RelatedOriginsDoc {
    /// Origins allowed to use the relying party's passkeys.
    #[serde(default)]
    pub origins: Vec<String>,
}

/// One cached document with when it was fetched.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CachedOrigins {
    /// The parsed document.
    pub doc: RelatedOriginsDoc,
    /// Seconds since the epoch when it was fetched.
    pub fetched_at: u64,
    /// The origins that survive the five-label cap, as `scheme://host[:port]`.
    pub effective: Vec<String>,
}

/// The cache, keyed by relying-party id.
#[derive(Clone, Debug, Default)]
pub struct RelatedOriginsCache {
    entries: BTreeMap<String, CachedOrigins>,
    /// How long an entry stays fresh, in seconds.
    pub max_age_secs: u64,
}

fn origin_key(o: &Origin) -> String {
    match o.port {
        Some(p) => format!("{}://{}:{}", o.scheme, o.host, p),
        None => format!("{}://{}", o.scheme, o.host),
    }
}

/// Applies the label cap: origins are taken in order; a new label beyond
/// the fifth distinct one is ignored, as are unparsable entries.
pub fn effective_origins(doc: &RelatedOriginsDoc) -> Vec<String> {
    let mut labels: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for text in &doc.origins {
        let Ok(o) = Origin::parse(text) else { continue };
        if o.scheme != "https" {
            continue;
        }
        let Some(label) = o.registrable_label() else {
            continue;
        };
        if !labels.contains(&label) {
            if labels.len() >= MAX_LABELS {
                continue;
            }
            labels.insert(label);
        }
        out.push(origin_key(&o));
    }
    out
}

impl RelatedOriginsCache {
    /// A cache whose entries stay fresh for `max_age_secs`.
    pub fn new(max_age_secs: u64) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_age_secs,
        }
    }

    /// Stores a fetched document for `rp_id`.
    pub fn insert(&mut self, rp_id: &str, doc: RelatedOriginsDoc, fetched_at: u64) {
        let effective = effective_origins(&doc);
        self.entries.insert(
            rp_id.to_ascii_lowercase(),
            CachedOrigins {
                doc,
                fetched_at,
                effective,
            },
        );
    }

    /// The cached entry for `rp_id`, if fresh at `now`.
    pub fn get(&self, rp_id: &str, now: u64) -> Option<&CachedOrigins> {
        self.entries
            .get(&rp_id.to_ascii_lowercase())
            .filter(|e| now.saturating_sub(e.fetched_at) <= self.max_age_secs)
    }

    /// Whether `origin` may use passkeys for `rp_id`: the relying party's own
    /// domain always may; otherwise the origin must be in the fresh,
    /// capped list.
    pub fn allows(&self, rp_id: &str, origin: &Origin, now: u64) -> bool {
        let rp = rp_id.to_ascii_lowercase();
        if origin.host == rp || origin.host.ends_with(&format!(".{rp}")) {
            return true;
        }
        self.get(&rp, now)
            .is_some_and(|e| e.effective.iter().any(|o| *o == origin_key(origin)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_label_cap_and_freshness() {
        let doc: RelatedOriginsDoc = serde_json::from_str(
            r#"{"origins": [
                "https://example.co.uk", "https://example.de", "https://shop.example.de",
                "https://brand-a.com", "https://brand-b.com", "https://brand-c.com",
                "https://brand-d.com", "https://brand-e.com", "https://brand-f.com",
                "http://insecure.example", "not a url"
            ]}"#,
        )
        .unwrap();
        let eff = effective_origins(&doc);
        // Labels: example, brand-a, brand-b, brand-c, brand-d = 5. brand-e, brand-f dropped.
        assert!(eff.contains(&"https://shop.example.de".to_string()));
        assert!(eff.contains(&"https://brand-d.com".to_string()));
        assert!(!eff.contains(&"https://brand-e.com".to_string()));
        assert!(!eff.iter().any(|o| o.starts_with("http://")));

        let mut cache = RelatedOriginsCache::new(3600);
        cache.insert("example.com", doc, 1000);
        let ok = Origin::parse("https://example.de").unwrap();
        let dropped = Origin::parse("https://brand-f.com").unwrap();
        let own = Origin::parse("https://login.example.com").unwrap();
        assert!(cache.allows("example.com", &ok, 2000));
        assert!(!cache.allows("example.com", &dropped, 2000));
        assert!(cache.allows("example.com", &own, 2000));
        // Stale: only the relying party's own hosts pass.
        assert!(!cache.allows("example.com", &ok, 1000 + 3601));
        assert!(cache.allows("example.com", &own, 1000 + 3601));
    }
}
