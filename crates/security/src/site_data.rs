//! The security center's signed site data (docs/plan/23-bridges.md §3):
//! one file, in the matcher's signed-data format (09-extension-boundary §2,
//! D66), that carries what Lockdown knows about sites rather than about a
//! person:
//!
//! - **guides**: the two-step and passkey set-up notes from 2fa.directory
//!   (MIT) and the Passkeys Directory (CC BY 4.0), attributed by source;
//! - **providers**: the identity providers whose "Sign in with …" the
//!   browser extension records for the federated sign-in map, with the
//!   sign-in pages they send people to;
//! - **recipes**: a curated change-password page per site, the step after
//!   `/.well-known/change-password`;
//! - **fixes**: the community fill-fix list (which field is which, or
//!   never fill), read beside the person's own fixes.
//!
//! ```text
//! envelope (CBOR map) { format: "lockdown-site-data", v: 1, key_id, payload, signature }
//! signature = Ed25519 over "lockdown-site-data/v1\0" || payload
//! payload (canonical CBOR map) { site_data: N, published, sources, guides,
//!                                providers, recipes, fixes, … unknown fields kept … }
//! ```
//!
//! The envelope and its rules are the matcher's (`verify_envelope`): one
//! CBOR item, the format checked by name before the key is looked up,
//! `verify_strict`, the payload read only after the signature holds. The
//! signing prefix is this file's own, so a signature over matcher data is
//! never accepted here or the other way round. The publisher keys are the
//! matcher's (D63): the development key signs the built-in file, a release
//! build trusts only release keys. `site_data` only goes up; a host that
//! loads a newer file later hands its floor back to [`SiteData::load`].

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use ciborium::Value;
use lockdown_matcher::signed::{
    DataError, TrustedKeys, decode_exact, sign_envelope, to_canonical_cbor, verify_envelope,
};
use serde::{Deserialize, Serialize};

/// The envelope's `format`.
pub const FORMAT: &str = "lockdown-site-data";
/// Domain separation for this file's signatures.
pub const SIGNING_PREFIX: &[u8] = b"lockdown-site-data/v1\0";
/// The built-in signed file.
pub const BUILTIN_SIGNED: &[u8] = include_bytes!("../data/site-data.signed.cbor");

/// Where a part of the data came from, for the attribution line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The id guides name in `src`.
    pub id: String,
    /// Its name, as the attribution says it.
    pub name: String,
    /// Where it lives.
    pub url: String,
    /// The data's licence.
    pub licence: String,
    /// The commit the snapshot was cut from.
    #[serde(default)]
    pub commit: String,
    /// That commit's date.
    #[serde(default)]
    pub date: String,
}

/// What the directories add about one site's set-up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideEntry {
    /// The site's name.
    pub n: String,
    /// Its domains, the main one first.
    pub d: Vec<String>,
    /// The site's own notes on turning two-step sign-in on (2fa.directory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Its account-recovery page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<String>,
    /// The names of the site's own authenticator apps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// The names of the site's own devices.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<String>,
    /// Passkey support: `passwordless`, `mfa` or both joined by `+`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passkey: Option<String>,
    /// The site's page about passkeys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passkey_doc: Option<String>,
    /// Which [`Source`]s this entry came from.
    #[serde(default)]
    pub src: Vec<String>,
}

/// One page an identity provider sends people to when a site asks them
/// to sign in with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPage {
    /// The page's host, exactly.
    pub host: String,
    /// A part its path contains (`/o/oauth2/`).
    pub path: String,
    /// The query parameter naming where the person goes back to
    /// (`redirect_uri`, `openid.return_to`, `origin`).
    pub ret: String,
}

/// An identity provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    /// A short id (`google`).
    pub id: String,
    /// Its name, as buttons say it ("Google").
    pub name: String,
    /// The domains of the provider's own account, where its Login lives.
    pub domains: Vec<String>,
    /// Its sign-in pages.
    pub auth: Vec<AuthPage>,
}

/// A curated change-password page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// The domains it is for.
    pub d: Vec<String>,
    /// The page (always `https://`).
    pub url: String,
    /// What to do once there, when the page isn't the form itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// How a fix names a field on the page: by its `id`, `name` or
/// `autocomplete` attribute, exactly. Never a CSS selector: the page is
/// hostile and a selector is code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMatch {
    /// The field's `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The field's `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The field's `autocomplete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<String>,
}

/// The roles a fix may give a field.
pub const FIELD_ROLES: [&str; 5] = ["username", "password", "new_password", "code", "ignore"];

/// One field's fix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldFix {
    /// Which field.
    #[serde(rename = "match")]
    pub matcher: FieldMatch,
    /// What it is: one of [`FIELD_ROLES`].
    pub role: String,
}

/// A site's fill fix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteFix {
    /// The domains it is for.
    pub d: Vec<String>,
    /// Never offer a fill here.
    #[serde(default)]
    pub never: bool,
    /// Fields whose role the page's own markup gets wrong.
    #[serde(default)]
    pub fields: Vec<FieldFix>,
    /// Why, in words.
    #[serde(default)]
    pub note: String,
}

/// What the publisher signs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteDataPayload {
    /// The monotonic data version.
    pub site_data: u64,
    /// A date label for people.
    #[serde(default)]
    pub published: String,
    /// The sources, for attribution.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Set-up guides.
    #[serde(default)]
    pub guides: Vec<GuideEntry>,
    /// Identity providers.
    #[serde(default)]
    pub providers: Vec<Provider>,
    /// Change-password pages.
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    /// The community fill fixes.
    #[serde(default)]
    pub fixes: Vec<SiteFix>,
    /// Fields this build does not know, carried through.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SiteDataPayload {
    /// Checks what the signature can't: every address is `https://`, every
    /// role is known, every provider page has a host. A file failing this
    /// is refused whole.
    fn check(&self) -> Result<(), DataError> {
        let bad = |why: String| Err(DataError::BadPayload(why));
        for r in &self.recipes {
            if !r.url.starts_with("https://") || r.d.is_empty() {
                return bad(format!("a recipe for {:?} isn't an https page", r.d));
            }
        }
        for g in &self.guides {
            for url in [&g.recovery, &g.passkey_doc].into_iter().flatten() {
                if !url.starts_with("https://") {
                    return bad(format!(
                        "a guide for {} links to a page that isn't https",
                        g.n
                    ));
                }
            }
        }
        for f in &self.fixes {
            if f.d.is_empty() {
                return bad("a fix names no site".into());
            }
            for field in &f.fields {
                if !FIELD_ROLES.contains(&field.role.as_str()) {
                    return bad(format!(
                        "a fix gives a field the unknown role {}",
                        field.role
                    ));
                }
                let m = &field.matcher;
                if m.id.is_none() && m.name.is_none() && m.autocomplete.is_none() {
                    return bad("a fix names a field by nothing".into());
                }
            }
        }
        for p in &self.providers {
            if p.id.is_empty() || p.auth.iter().any(|a| a.host.is_empty() || a.ret.is_empty()) {
                return bad(format!("the provider {} is incomplete", p.name));
            }
        }
        Ok(())
    }
}

/// Signs a payload into envelope bytes (the `sign-site-data` example and
/// tests; release files are signed by the release service, D63).
pub fn sign(
    payload: &SiteDataPayload,
    key: &ed25519_dalek::SigningKey,
    key_id: &str,
) -> Result<Vec<u8>, DataError> {
    sign_envelope(
        FORMAT,
        SIGNING_PREFIX,
        &to_canonical_cbor(payload)?,
        key,
        key_id,
    )
}

/// Verifies envelope bytes; nothing in the payload is read before the
/// signature holds.
pub fn verify(bytes: &[u8], keys: &TrustedKeys) -> Result<(String, SiteDataPayload), DataError> {
    let (key_id, payload) = verify_envelope(bytes, FORMAT, SIGNING_PREFIX, keys)?;
    let payload: SiteDataPayload = decode_exact(&payload)?;
    payload.check()?;
    Ok((key_id, payload))
}

/// The data in force, indexed by domain.
#[derive(Clone, Debug)]
pub struct SiteData {
    /// `site_data` of the file.
    pub version: u64,
    /// Its `published` label.
    pub published: String,
    /// The key that signed it.
    pub key_id: String,
    payload: SiteDataPayload,
    guides: HashMap<String, usize>,
    recipes: HashMap<String, usize>,
    fixes: HashMap<String, usize>,
    provider_sites: HashMap<String, usize>,
}

fn index<'a>(lists: impl Iterator<Item = (usize, &'a Vec<String>)>) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for (i, domains) in lists {
        for d in domains {
            map.entry(d.to_ascii_lowercase()).or_insert(i);
        }
    }
    map
}

/// The entry for `host` in `map`: the host, then each parent domain down
/// to two labels.
fn by_host(map: &HashMap<String, usize>, host: &str) -> Option<usize> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let mut rest = host.as_str();
    loop {
        if let Some(i) = map.get(rest) {
            return Some(*i);
        }
        let (_, parent) = rest.split_once('.')?;
        if !parent.contains('.') {
            return None;
        }
        rest = parent;
    }
}

/// A sign-in page seen in the browser, as [`SiteData::provider_for_auth`]
/// reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSeen<'a> {
    /// The provider.
    pub provider: &'a Provider,
    /// The origin the page will send the person back to, when the address
    /// names one (`https://site.example`).
    pub return_origin: Option<String>,
}

impl SiteData {
    /// Verifies `bytes` and indexes them, refusing a version below `floor`.
    pub fn load(bytes: &[u8], keys: &TrustedKeys, floor: u64) -> Result<Self, DataError> {
        let (key_id, payload) = verify(bytes, keys)?;
        if payload.site_data < floor {
            return Err(DataError::Downgrade {
                offered: payload.site_data,
                newest: floor,
            });
        }
        Ok(Self::from_payload(key_id, payload))
    }

    fn from_payload(key_id: String, payload: SiteDataPayload) -> Self {
        Self {
            version: payload.site_data,
            published: payload.published.clone(),
            key_id,
            guides: index(payload.guides.iter().map(|g| &g.d).enumerate()),
            recipes: index(payload.recipes.iter().map(|r| &r.d).enumerate()),
            fixes: index(payload.fixes.iter().map(|f| &f.d).enumerate()),
            provider_sites: index(payload.providers.iter().map(|p| &p.domains).enumerate()),
            payload,
        }
    }

    /// Nothing at all (a build whose built-in file failed, which the
    /// crate's tests rule out).
    pub fn empty() -> Self {
        Self::from_payload(
            String::new(),
            SiteDataPayload {
                site_data: 0,
                published: String::new(),
                sources: Vec::new(),
                guides: Vec::new(),
                providers: Vec::new(),
                recipes: Vec::new(),
                fixes: Vec::new(),
                extra: BTreeMap::new(),
            },
        )
    }

    /// The built-in file under the build's trusted keys, read once.
    pub fn builtin() -> &'static Self {
        static DATA: OnceLock<SiteData> = OnceLock::new();
        DATA.get_or_init(|| {
            Self::load(BUILTIN_SIGNED, &TrustedKeys::builtin(), 0).unwrap_or_else(|_| Self::empty())
        })
    }

    /// The payload as verified.
    pub fn payload(&self) -> &SiteDataPayload {
        &self.payload
    }

    /// A source by id.
    pub fn source(&self, id: &str) -> Option<&Source> {
        self.payload.sources.iter().find(|s| s.id == id)
    }

    /// What the directories add for `host`.
    pub fn guide(&self, host: &str) -> Option<&GuideEntry> {
        by_host(&self.guides, host).map(|i| &self.payload.guides[i])
    }

    /// The curated change-password page for `host`.
    pub fn recipe(&self, host: &str) -> Option<&Recipe> {
        by_host(&self.recipes, host).map(|i| &self.payload.recipes[i])
    }

    /// The community fill fix for `host`.
    pub fn fix(&self, host: &str) -> Option<&SiteFix> {
        by_host(&self.fixes, host).map(|i| &self.payload.fixes[i])
    }

    /// Every identity provider, in the file's order.
    pub fn providers(&self) -> &[Provider] {
        &self.payload.providers
    }

    /// A provider by id.
    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.payload.providers.iter().find(|p| p.id == id)
    }

    /// The provider whose own account lives at `host` (a Login for
    /// `accounts.google.com` is the Google account).
    pub fn provider_for_site(&self, host: &str) -> Option<&Provider> {
        by_host(&self.provider_sites, host).map(|i| &self.payload.providers[i])
    }

    /// Whether `url` is one of a provider's sign-in pages, and where it
    /// would send the person back. The address is hostile: only `https`,
    /// the host compared exactly, the return address read as a URL and
    /// cut to its origin, and only when that is `https` (or this machine).
    pub fn provider_for_auth(&self, url: &str) -> Option<AuthSeen<'_>> {
        let parsed = url::Url::parse(url).ok()?;
        if parsed.scheme() != "https" {
            return None;
        }
        let host = parsed.host_str()?.to_ascii_lowercase();
        let path = parsed.path();
        for provider in &self.payload.providers {
            for page in &provider.auth {
                if page.host != host || !path.contains(page.path.as_str()) {
                    continue;
                }
                let return_origin = parsed
                    .query_pairs()
                    .find(|(k, _)| k == page.ret.as_str())
                    .and_then(|(_, v)| origin_of(&v));
                return Some(AuthSeen {
                    provider,
                    return_origin,
                });
            }
        }
        None
    }
}

/// The origin of `address` when it is an `https` URL (or `http` to this
/// machine), else `None`.
pub fn origin_of(address: &str) -> Option<String> {
    let u = url::Url::parse(address).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    match u.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return None,
    }
    Some(match u.port() {
        Some(p) => format!("{}://{host}:{p}", u.scheme()),
        None => format!("{}://{host}", u.scheme()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use lockdown_matcher::signed::{DEV_KEY_ID, TrustedKeys};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn keys() -> TrustedKeys {
        TrustedKeys::none()
            .with("test", &key().verifying_key().to_bytes())
            .unwrap()
    }

    fn payload(version: u64) -> SiteDataPayload {
        SiteDataPayload {
            site_data: version,
            published: "2026-10-18".into(),
            sources: vec![],
            guides: vec![GuideEntry {
                n: "Example".into(),
                d: vec!["example.com".into()],
                notes: Some("You need a phone number first.".into()),
                recovery: None,
                apps: vec![],
                devices: vec![],
                passkey: Some("passwordless".into()),
                passkey_doc: None,
                src: vec!["2fa".into()],
            }],
            providers: vec![Provider {
                id: "idp".into(),
                name: "Idp".into(),
                domains: vec!["idp.test".into()],
                auth: vec![AuthPage {
                    host: "login.idp.test".into(),
                    path: "/oauth2/".into(),
                    ret: "redirect_uri".into(),
                }],
            }],
            recipes: vec![Recipe {
                d: vec!["example.com".into()],
                url: "https://example.com/account/password".into(),
                note: None,
            }],
            fixes: vec![SiteFix {
                d: vec!["shop.test".into()],
                never: false,
                fields: vec![FieldFix {
                    matcher: FieldMatch {
                        name: Some("login_id".into()),
                        ..FieldMatch::default()
                    },
                    role: "username".into(),
                }],
                note: "The sign-in name field is unlabelled.".into(),
            }],
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn the_built_in_file_verifies_and_matches_its_sources() {
        let (key_id, built) = verify(BUILTIN_SIGNED, &TrustedKeys::builtin()).unwrap();
        assert_eq!(key_id, DEV_KEY_ID);
        let data = SiteData::builtin();
        assert!(data.version >= 1);
        assert!(
            data.guide("twitch.tv")
                .unwrap()
                .notes
                .as_deref()
                .unwrap()
                .contains("phone")
        );
        assert!(data.guide("www.github.com").unwrap().passkey.is_some());
        assert_eq!(data.source("passkeys").unwrap().licence, "CC BY 4.0");
        assert_eq!(data.source("2fa").unwrap().licence, "MIT");
        assert_eq!(data.provider("google").unwrap().name, "Google");
        assert_eq!(
            data.recipe("github.com").unwrap().url,
            "https://github.com/settings/security"
        );
        // The committed file is exactly what the example makes of the
        // committed sources (`cargo run -p lockdown-security --example
        // sign-site-data` refreshes it).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/site-data");
        let source = crate::site_data::payload_from_sources(
            &std::fs::read_to_string(root.join("curated.json")).unwrap(),
            &std::fs::read_to_string(root.join("guides.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            source, built,
            "site-data.signed.cbor is stale; run the sign-site-data example"
        );
    }

    #[test]
    fn a_changed_unknown_or_downgraded_file_is_refused() {
        let bytes = sign(&payload(5), &key(), "test").unwrap();
        let data = SiteData::load(&bytes, &keys(), 0).unwrap();
        assert_eq!(data.version, 5);
        assert_eq!(
            SiteData::load(&bytes, &keys(), 6).unwrap_err().code(),
            "downgrade"
        );
        assert_eq!(
            SiteData::load(&bytes, &TrustedKeys::none(), 0)
                .unwrap_err()
                .code(),
            "unknown_key"
        );
        for at in (0..bytes.len()).step_by(7) {
            let mut bad = bytes.clone();
            bad[at] ^= 0x10;
            assert!(SiteData::load(&bad, &keys(), 0).is_err(), "flip at {at}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(SiteData::load(&longer, &keys(), 0).is_err());
        // A matcher data file signed by the same key is not site data.
        let matcher = lockdown_matcher::signed::sign_envelope(
            lockdown_matcher::signed::FORMAT,
            lockdown_matcher::signed::SIGNING_PREFIX,
            &to_canonical_cbor(&payload(5)).unwrap(),
            &key(),
            "test",
        )
        .unwrap();
        assert!(SiteData::load(&matcher, &keys(), 0).is_err());
        // The same payload under this format but the matcher's prefix fails the signature.
        let crossed = sign_envelope(
            FORMAT,
            lockdown_matcher::signed::SIGNING_PREFIX,
            &to_canonical_cbor(&payload(5)).unwrap(),
            &key(),
            "test",
        )
        .unwrap();
        assert_eq!(
            SiteData::load(&crossed, &keys(), 0).unwrap_err().code(),
            "bad_signature"
        );
    }

    #[test]
    fn a_signed_file_with_unsafe_contents_is_still_refused() {
        let mut p = payload(1);
        p.recipes[0].url = "javascript:alert(1)".into();
        let bytes = sign(&p, &key(), "test").unwrap();
        assert_eq!(
            SiteData::load(&bytes, &keys(), 0).unwrap_err().code(),
            "bad_data"
        );
        let mut p = payload(1);
        p.fixes[0].fields[0].role = "click_here".into();
        assert!(SiteData::load(&sign(&p, &key(), "test").unwrap(), &keys(), 0).is_err());
        let mut p = payload(1);
        p.fixes[0].fields[0].matcher = FieldMatch::default();
        assert!(SiteData::load(&sign(&p, &key(), "test").unwrap(), &keys(), 0).is_err());
    }

    #[test]
    fn lookups_go_by_host_then_parent_domains() {
        let data = SiteData::load(&sign(&payload(1), &key(), "test").unwrap(), &keys(), 0).unwrap();
        assert!(data.guide("www.example.com").is_some());
        assert!(data.guide("example.org").is_none());
        assert!(data.recipe("a.b.example.com").is_some());
        assert_eq!(data.fix("shop.test").unwrap().fields[0].role, "username");
        assert_eq!(
            data.provider_for_site("accounts.idp.test").unwrap().id,
            "idp"
        );
        // Unknown fields ride through a re-sign.
        let mut p = payload(2);
        p.extra.insert("later".into(), Value::Integer(3.into()));
        let (_, back) = verify(&sign(&p, &key(), "test").unwrap(), &keys()).unwrap();
        assert_eq!(back.extra.get("later"), Some(&Value::Integer(3.into())));
    }

    #[test]
    fn a_providers_sign_in_page_is_read_as_hostile() {
        let data = SiteData::load(&sign(&payload(1), &key(), "test").unwrap(), &keys(), 0).unwrap();
        let seen = data
            .provider_for_auth(
                "https://login.idp.test/tenant/oauth2/authorize?client_id=1&redirect_uri=https%3A%2F%2Fshop.example%2Fcallback%3Fx%3D1",
            )
            .unwrap();
        assert_eq!(seen.provider.id, "idp");
        assert_eq!(seen.return_origin.as_deref(), Some("https://shop.example"));
        // Another host, plain http, another path: not a sign-in page.
        for url in [
            "https://login.idp.test.evil.net/oauth2/?redirect_uri=https://a.example",
            "http://login.idp.test/oauth2/?redirect_uri=https://a.example",
            "https://login.idp.test/profile",
            "not a url",
        ] {
            assert!(data.provider_for_auth(url).is_none(), "{url}");
        }
        // A return address that isn't a web origin is dropped, not trusted.
        for ret in [
            "javascript:alert(1)",
            "http%3A%2F%2Fshop.example",
            "data:text/html,x",
        ] {
            let url = format!("https://login.idp.test/oauth2/?redirect_uri={ret}");
            let seen = data.provider_for_auth(&url).unwrap();
            assert_eq!(seen.return_origin, None, "{ret}");
        }
        assert_eq!(
            origin_of("http://localhost:8080/x").as_deref(),
            Some("http://localhost:8080")
        );
    }
}

/// Builds the payload from the committed sources: the curated file
/// (version, providers, recipes, fixes) and the directory snapshot
/// (sources and guides). The `sign-site-data` example signs what this
/// returns; the test above checks the committed file is current.
pub fn payload_from_sources(curated: &str, guides: &str) -> Result<SiteDataPayload, String> {
    #[derive(Deserialize)]
    struct Snapshot {
        sources: Vec<Source>,
        guides: Vec<GuideEntry>,
    }
    let mut payload: SiteDataPayload =
        serde_json::from_str(curated).map_err(|e| format!("curated.json: {e}"))?;
    let snapshot: Snapshot =
        serde_json::from_str(guides).map_err(|e| format!("guides.json: {e}"))?;
    payload.sources = snapshot.sources;
    payload.guides = snapshot.guides;
    payload.check().map_err(|e| e.to_string())?;
    Ok(payload)
}
