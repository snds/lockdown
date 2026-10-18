//! The federated sign-in map (02-model §9; docs/plan/23-bridges.md §4):
//! which logins are entered with a Google, Apple, Microsoft, Steam or
//! other account, what falls with that account (the blast radius), and
//! what to fix before it is ever lost.
//!
//! **Where it comes from.** A login's own `sign_in_method` and
//! `sign_in_provider` (what the person typed in the editor), and the
//! browser extension's observations: a click on a "Sign in with …" button
//! on a page, and the provider's own sign-in page loading with a return
//! address. The extension only reports; the core decides. A page is
//! hostile, so:
//!
//! - a provider's sign-in page counts only when the signed site data names
//!   it ([`lockdown_security::site_data::SiteData::provider_for_auth`]),
//!   and only when its return address is on the same site (registrable
//!   domain) as the page the person was on in that tab;
//! - that strong evidence, for exactly one of the person's live, own,
//!   everyday logins on that site, is recorded on the login at once (a new
//!   version: `lockdown.signin`, and the editor's two sign-in fields when
//!   they are empty);
//! - anything weaker (a button click alone, several logins, none) waits in
//!   memory for the person's yes in the map, at most [`MAX_PENDING`], and
//!   is forgotten on lock;
//! - crown jewels are never touched from a page (they are never filled
//!   either, 02-model §3).
//!
//! **The provider's own account** is the login marked as it (the person
//! picked it in the map: `lockdown.provider`), else the one login saved
//! for one of the provider's own domains. The map warns when there is
//! none, when it has no second step, when it isn't a crown jewel (02-model
//! §3 suggests one for identity-provider logins), and when sites hang on
//! it with no password of their own.

use lockdown_matcher::Origin;
use lockdown_security::site_data::{Provider, SiteData};
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};

use crate::session::bridges::{LoginFacts, PROVIDER_FIELD, SIGNIN_FIELD};
use crate::{App, AppError, AppResult, session};

/// The most observations waiting for the person at once.
pub const MAX_PENDING: usize = 20;

/// What the extension saw.
#[derive(Clone, Debug, Deserialize)]
pub struct FederatedEvidence {
    /// The provider's id, as the extension matched it from the hints.
    pub provider: String,
    /// The page the person was on (the tab's top URL, from the browser).
    pub page: String,
    /// The provider's sign-in page that loaded, when the evidence is a
    /// redirect.
    #[serde(default)]
    pub auth_url: Option<String>,
}

/// A sign-in the extension saw that waits for the person.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    pub id: u64,
    pub provider: String,
    /// The site's origin (`https://shop.example`).
    pub origin: String,
    pub site: String,
    pub at: i64,
    /// Whether the provider's own page confirmed it.
    pub confirmed: bool,
}

/// What happened to one observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeenOutcome {
    /// Logins it was recorded on.
    pub recorded: Vec<String>,
    /// The waiting observation's number, when it waits for the person.
    pub pending: Option<u64>,
}

/// The reserved field on a login signed in to through a provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SignIn {
    pub provider: String,
    /// `redirect` (the provider's own page confirmed it), `button` or `you`.
    pub via: String,
    pub at: i64,
}

/// The reserved field on a provider's own login.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProviderOf {
    pub provider: String,
}

/// A login in the map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MapLogin {
    pub item_id: String,
    pub title: String,
    pub username: String,
    /// The site's host.
    pub host: String,
    /// It holds a password of its own.
    pub has_password: bool,
    /// It holds a one-time code or a passkey.
    pub second_step: bool,
    pub crown_jewel: bool,
}

/// One provider and everything that hangs on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderView {
    /// The provider's id (`google`), or `other:<name>` for one the signed
    /// data doesn't list.
    pub id: String,
    pub name: String,
    /// Its account in the vault, when Lockdown can tell which.
    pub account: Option<MapLogin>,
    /// Logins that could be it, when more than one is.
    pub candidates: Vec<MapLogin>,
    /// The sites entered through it: the blast radius.
    pub sites: Vec<MapLogin>,
    /// What to fix, most important first, in words.
    pub warnings: Vec<String>,
}

/// A waiting observation as the map shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PendingView {
    pub id: u64,
    pub provider: String,
    pub provider_name: String,
    pub host: String,
    /// Logins saved for that site, any of which it may be for.
    pub logins: Vec<MapLogin>,
    /// The provider's own sign-in page confirmed it.
    pub confirmed: bool,
}

/// The whole map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FederationMap {
    pub providers: Vec<ProviderView>,
    pub pending: Vec<PendingView>,
}

/// What the extension needs to notice sign-ins: each provider's id, name
/// and sign-in pages (from the signed site data, so the page list lives in
/// one place).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FederationHints {
    pub providers: Vec<HintProvider>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HintProvider {
    pub id: String,
    pub name: String,
    /// `{host, path}` pairs: a page whose host is `host` and whose path
    /// contains `path`.
    pub pages: Vec<(String, String)>,
}

fn site_key(origin: &Origin) -> String {
    origin
        .registrable_domain()
        .unwrap_or_else(|| origin.host.clone())
}

fn origin_text(o: &Origin) -> String {
    match o.port {
        Some(p) => format!("{}://{}:{p}", o.scheme, o.host),
        None => format!("{}://{}", o.scheme, o.host),
    }
}

fn host_of(f: &LoginFacts) -> String {
    f.urls
        .iter()
        .find_map(|u| Origin::parse(u).ok())
        .map(|o| o.host)
        .unwrap_or_default()
}

fn map_login(f: &LoginFacts) -> MapLogin {
    MapLogin {
        item_id: f.item_id.to_string(),
        title: f.title.clone(),
        username: f.username.clone(),
        host: host_of(f),
        has_password: f.has_password,
        second_step: f.has_second_step,
        crown_jewel: f.crown_jewel,
    }
}

/// The logins saved for `site` (a registrable domain or a host).
fn on_site<'a>(logins: &'a [LoginFacts], site: &str) -> Vec<&'a LoginFacts> {
    logins
        .iter()
        .filter(|f| {
            f.urls
                .iter()
                .filter_map(|u| Origin::parse(u).ok())
                .any(|o| site_key(&o) == site)
        })
        .collect()
}

/// The provider a login is signed in to through, by its reserved field,
/// else by what the person typed in "Signed in with".
fn provider_of_login(f: &LoginFacts, data: &SiteData) -> Option<(String, String)> {
    if let Some(s) = f
        .signin
        .as_deref()
        .and_then(|t| serde_json::from_str::<SignIn>(t).ok())
        && let Some(p) = data.provider(&s.provider)
    {
        return Some((p.id.clone(), p.name.clone()));
    }
    let typed = f.provider_text.trim();
    if typed.is_empty() {
        return None;
    }
    let lower = typed.to_ascii_lowercase();
    let lower = lower
        .trim_start_matches("sign in with ")
        .trim_start_matches("continue with ")
        .trim();
    let named = data.providers().iter().find(|p| {
        p.name.eq_ignore_ascii_case(lower)
            || p.id == lower
            || (p.id == "x" && lower == "twitter")
            || (p.id == "google" && lower == "gmail")
    });
    Some(match named {
        Some(p) => (p.id.clone(), p.name.clone()),
        None => (format!("other:{lower}"), typed.to_string()),
    })
}

fn account_for<'a>(
    provider: Option<&Provider>,
    id: &str,
    logins: &'a [LoginFacts],
) -> (Option<&'a LoginFacts>, Vec<&'a LoginFacts>) {
    let marked: Vec<&LoginFacts> = logins
        .iter()
        .filter(|f| {
            f.provider_of
                .as_deref()
                .and_then(|t| serde_json::from_str::<ProviderOf>(t).ok())
                .is_some_and(|p| p.provider == id)
        })
        .collect();
    if let Some(first) = marked.first() {
        return (Some(first), Vec::new());
    }
    let Some(provider) = provider else {
        return (None, Vec::new());
    };
    let on_domains: Vec<&LoginFacts> = logins
        .iter()
        .filter(|f| {
            f.urls
                .iter()
                .filter_map(|u| Origin::parse(u).ok())
                .any(|o| {
                    provider
                        .domains
                        .iter()
                        .any(|d| o.host == *d || o.host.ends_with(&format!(".{d}")))
                })
        })
        .collect();
    match on_domains.len() {
        1 => (Some(on_domains[0]), Vec::new()),
        _ => (None, on_domains),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn warnings(
    name: &str,
    account: Option<&LoginFacts>,
    candidates: usize,
    sites: &[&LoginFacts],
) -> Vec<String> {
    let mut w = Vec::new();
    let n = plural(sites.len(), "site", "sites");
    match account {
        None if candidates > 1 => w.push(format!(
            "You have {candidates} {name} logins saved. Pick which one you sign in to these {n} with, so Lockdown can check it."
        )),
        None => w.push(format!(
            "You sign in to {n} with {name}, but Lockdown has no {name} login saved. Save it, with its recovery codes, so losing it doesn't lock you out of all of them."
        )),
        Some(a) => {
            if !a.has_second_step {
                w.push(format!(
                    "Your {name} account has no two-step sign-in saved in Lockdown. If its password leaks, {n} go with it. Turn it on and keep the code here."
                ));
            }
            if !a.crown_jewel {
                w.push(format!(
                    "Make your {name} login a protected item: it opens {n}, so it deserves the extra check before use."
                ));
            }
        }
    }
    let only = sites.iter().filter(|s| !s.has_password).count();
    if only > 0 {
        w.push(format!(
            "{} no password of {} own: if you lose the {name} account, you lose {} too. Where a site lets you, add a password or a passkey.",
            if only == 1 { "1 of them has".to_string() } else { format!("{only} of them have") },
            if only == 1 { "its" } else { "their" },
            if only == 1 { "it" } else { "them" },
        ));
    }
    w
}

impl crate::Core {
    fn federation_view(&mut self) -> AppResult<FederationMap> {
        let data = SiteData::builtin();
        let logins = self.session()?.bridge_logins().map_err(AppError::refused)?;
        // Group logins by the provider they go through, keeping the
        // signed data's order, then other names A to Z.
        let mut groups: Vec<(String, String, Vec<&LoginFacts>)> = Vec::new();
        for f in &logins {
            let Some((id, name)) = provider_of_login(f, data) else {
                continue;
            };
            match groups.iter_mut().find(|g| g.0 == id) {
                Some(g) => g.2.push(f),
                None => groups.push((id, name, vec![f])),
            }
        }
        let rank = |id: &str| {
            data.providers()
                .iter()
                .position(|p| p.id == id)
                .unwrap_or(usize::MAX)
        };
        groups.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then(a.1.cmp(&b.1)));
        let providers = groups
            .into_iter()
            .map(|(id, name, mut sites)| {
                sites.sort_by_key(|a| a.title.to_lowercase());
                let (account, candidates) = account_for(data.provider(&id), &id, &logins);
                ProviderView {
                    warnings: warnings(&name, account, candidates.len(), &sites),
                    account: account.map(map_login),
                    candidates: candidates.iter().map(|c| map_login(c)).collect(),
                    sites: sites.iter().map(|s| map_login(s)).collect(),
                    id,
                    name,
                }
            })
            .collect();
        let pending = self
            .bridges
            .pending
            .iter()
            .map(|p| PendingView {
                id: p.id,
                provider: p.provider.clone(),
                provider_name: data
                    .provider(&p.provider)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| p.provider.clone()),
                host: p.site.clone(),
                logins: on_site(&logins, &p.site)
                    .into_iter()
                    .filter(|f| !f.crown_jewel && f.editable)
                    .map(map_login)
                    .collect(),
                confirmed: p.confirmed,
            })
            .collect();
        Ok(FederationMap { providers, pending })
    }

    fn federation_record_on(&mut self, item: Id, provider: &Provider, via: &str) -> AppResult<()> {
        let facts = self
            .session()?
            .bridge_logins()
            .map_err(AppError::refused)?
            .into_iter()
            .find(|f| f.item_id == item)
            .ok_or_else(|| AppError::refused("that login isn't live in an open vault"))?;
        if facts.crown_jewel {
            return Err(AppError::refused(
                "protected items aren't changed from a browser's observation; edit it in Lockdown",
            ));
        }
        let signin = serde_json::to_string(&SignIn {
            provider: provider.id.clone(),
            via: via.into(),
            at: session::now_secs(),
        })
        .map_err(|e| AppError::refused(e.to_string()))?;
        let mut also: Vec<(&str, &str, &str)> = Vec::new();
        if facts.provider_text.trim().is_empty() {
            also.push(("sign_in_provider", "Signed in with", &provider.name));
        }
        if facts.method.trim().is_empty() && !facts.has_password {
            also.push(("sign_in_method", "Sign-in method", "Another account"));
        }
        self.session()?
            .bridge_set(item, SIGNIN_FIELD, Some(signin), &also)
            .map_err(AppError::refused)?;
        Ok(())
    }
}

/// The site the person was on and the provider, checked: `page` must be a
/// web page; `auth_url`, when given, must be one of that provider's sign-in
/// pages returning to the same site.
fn judge<'a>(e: &FederatedEvidence, data: &'a SiteData) -> AppResult<(&'a Provider, Origin, bool)> {
    let provider = data
        .provider(&e.provider)
        .ok_or_else(|| AppError::refused("that isn't a sign-in provider Lockdown knows"))?;
    let page =
        Origin::parse(&e.page).map_err(|_| AppError::refused("the page isn't a web address"))?;
    if page.scheme != "https" && page.scheme != "http" {
        return Err(AppError::refused("the page isn't a web page"));
    }
    if provider
        .domains
        .iter()
        .any(|d| page.host == *d || page.host.ends_with(&format!(".{d}")))
    {
        return Err(AppError::refused("that's the provider's own site"));
    }
    let confirmed = match &e.auth_url {
        None => false,
        Some(url) => {
            let seen = data
                .provider_for_auth(url)
                .ok_or_else(|| AppError::refused("that page isn't the provider's sign-in page"))?;
            if seen.provider.id != provider.id {
                return Err(AppError::refused(
                    "that sign-in page belongs to another provider",
                ));
            }
            let back = seen
                .return_origin
                .and_then(|o| Origin::parse(&o).ok())
                .ok_or_else(|| {
                    AppError::refused("the sign-in page doesn't say where it returns to")
                })?;
            if site_key(&back) != site_key(&page) {
                return Err(AppError::refused(
                    "the sign-in page returns to another site than the one you were on",
                ));
            }
            true
        }
    };
    Ok((provider, page, confirmed))
}

impl App {
    /// The map: every provider with its account, its sites and warnings,
    /// and what the extension saw that waits for the person.
    pub fn federation_map(&self) -> AppResult<FederationMap> {
        self.call(|core| core.federation_view())
    }

    /// The providers and their sign-in pages, for the extension.
    pub fn federation_hints(&self) -> FederationHints {
        FederationHints {
            providers: SiteData::builtin()
                .providers()
                .iter()
                .map(|p| HintProvider {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    pages: p
                        .auth
                        .iter()
                        .map(|a| (a.host.clone(), a.path.clone()))
                        .collect(),
                })
                .collect(),
        }
    }

    /// What the extension saw (docs/plan/23-bridges.md §4): recorded on the
    /// one matching login when the provider's own page confirmed it, else
    /// kept for the person's yes.
    pub fn federation_seen(&self, evidence: FederatedEvidence) -> AppResult<SeenOutcome> {
        self.call(move |core| {
            let data = SiteData::builtin();
            let (provider, page, confirmed) = judge(&evidence, data)?;
            let site = site_key(&page);
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            let here: Vec<&LoginFacts> = on_site(&logins, &site)
                .into_iter()
                .filter(|f| !f.crown_jewel && f.editable)
                .collect();
            let already = |f: &LoginFacts| {
                provider_of_login(f, data).is_some_and(|(id, _)| id == provider.id)
            };
            if here.iter().any(|f| already(f)) {
                return Ok(SeenOutcome {
                    recorded: Vec::new(),
                    pending: None,
                });
            }
            if confirmed && here.len() == 1 {
                let item = here[0].item_id;
                core.federation_record_on(item, provider, "redirect")?;
                core.bridges
                    .pending
                    .retain(|p| !(p.site == site && p.provider == provider.id));
                return Ok(SeenOutcome {
                    recorded: vec![item.to_string()],
                    pending: None,
                });
            }
            let state = &mut core.bridges;
            if let Some(p) = state
                .pending
                .iter_mut()
                .find(|p| p.site == site && p.provider == provider.id)
            {
                p.confirmed |= confirmed;
                p.at = session::now_secs();
                return Ok(SeenOutcome {
                    recorded: Vec::new(),
                    pending: Some(p.id),
                });
            }
            if state.pending.len() >= MAX_PENDING {
                state.pending.remove(0);
            }
            state.next_pending += 1;
            let id = state.next_pending;
            state.pending.push(Pending {
                id,
                provider: provider.id.clone(),
                origin: origin_text(&page),
                site,
                at: session::now_secs(),
                confirmed,
            });
            Ok(SeenOutcome {
                recorded: Vec::new(),
                pending: Some(id),
            })
        })
    }

    /// The person's yes to a waiting observation, on `item` (one of the
    /// site's logins), or `None` to save a new login for the site that
    /// signs in through the provider.
    pub fn federation_accept(&self, pending: u64, item: Option<&str>) -> AppResult<String> {
        let item = item.map(str::to_string);
        let (provider, origin, site) = self.call(move |core| {
            let p = core
                .bridges
                .pending
                .iter()
                .find(|p| p.id == pending)
                .cloned()
                .ok_or_else(|| {
                    AppError::refused("that observation is gone; Lockdown forgets them on lock")
                })?;
            Ok((p.provider, p.origin, p.site))
        })?;
        let data = SiteData::builtin();
        let provider_v = data
            .provider(&provider)
            .ok_or_else(|| AppError::refused("that provider isn't known any more"))?
            .clone();
        let id = match item {
            Some(id) => {
                let id = Id::parse(&id).map_err(|_| AppError::refused("that isn't an item"))?;
                let site_logins: Vec<Id> = self.call(move |core| {
                    let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
                    Ok(on_site(&logins, &site).iter().map(|f| f.item_id).collect())
                })?;
                if !site_logins.contains(&id) {
                    return Err(AppError::refused("that login isn't for this site"));
                }
                id
            }
            None => {
                let new = self.add_item(
                    "login",
                    None,
                    false,
                    vec![
                        lockdown_vault::FieldChange::text("url.0", &origin),
                        lockdown_vault::FieldChange::text("sign_in_method", "Another account"),
                    ],
                )?;
                Id::parse(&new).map_err(|_| AppError::refused("the new login has no id"))?
            }
        };
        self.call(move |core| {
            core.federation_record_on(id, &provider_v, "you")?;
            core.bridges.pending.retain(|p| p.id != pending);
            Ok(id.to_string())
        })
    }

    /// Forgets a waiting observation.
    pub fn federation_dismiss(&self, pending: u64) -> AppResult<()> {
        self.call(move |core| {
            core.bridges.pending.retain(|p| p.id != pending);
            Ok(())
        })
    }

    /// Marks `item` as the provider's own account (one per provider: any
    /// other login marked for it is unmarked).
    pub fn federation_set_account(&self, provider: &str, item: &str) -> AppResult<()> {
        let provider = provider.to_string();
        let item = Id::parse(item).map_err(|_| AppError::refused("that isn't an item"))?;
        self.call(move |core| {
            if SiteData::builtin().provider(&provider).is_none() {
                return Err(AppError::refused(
                    "that isn't a sign-in provider Lockdown knows",
                ));
            }
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            if !logins.iter().any(|f| f.item_id == item) {
                return Err(AppError::refused("that login isn't live in an open vault"));
            }
            let marked = serde_json::to_string(&ProviderOf {
                provider: provider.clone(),
            })
            .map_err(|e| AppError::refused(e.to_string()))?;
            for f in &logins {
                let was = f
                    .provider_of
                    .as_deref()
                    .and_then(|t| serde_json::from_str::<ProviderOf>(t).ok())
                    .is_some_and(|p| p.provider == provider);
                if was && f.item_id != item {
                    core.session()?
                        .bridge_set(f.item_id, PROVIDER_FIELD, None, &[])
                        .map_err(AppError::refused)?;
                }
            }
            core.session()?
                .bridge_set(item, PROVIDER_FIELD, Some(marked), &[])
                .map_err(AppError::refused)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> &'static SiteData {
        SiteData::builtin()
    }

    fn evidence(page: &str, auth: Option<&str>) -> FederatedEvidence {
        FederatedEvidence {
            provider: "google".into(),
            page: page.into(),
            auth_url: auth.map(str::to_string),
        }
    }

    #[test]
    fn a_sign_in_page_counts_only_when_it_returns_to_the_same_site() {
        let ok = judge(
            &evidence(
                "https://www.shop.example/login",
                Some("https://accounts.google.com/o/oauth2/v2/auth?client_id=1&redirect_uri=https%3A%2F%2Fauth.shop.example%2Fcb"),
            ),
            data(),
        )
        .unwrap();
        assert!(ok.2);
        assert_eq!(site_key(&ok.1), "shop.example");
        // Another site's return address, another provider's page, a page on
        // the provider itself, and an unknown provider are all refused.
        for (page, auth) in [
            (
                "https://shop.example",
                "https://accounts.google.com/o/oauth2/v2/auth?redirect_uri=https%3A%2F%2Fevil.example%2Fcb",
            ),
            (
                "https://shop.example",
                "https://appleid.apple.com/auth/authorize?redirect_uri=https%3A%2F%2Fshop.example%2Fcb",
            ),
            (
                "https://mail.google.com",
                "https://accounts.google.com/o/oauth2/auth?redirect_uri=https%3A%2F%2Fmail.google.com",
            ),
            (
                "javascript:alert(1)",
                "https://accounts.google.com/o/oauth2/auth?redirect_uri=https%3A%2F%2Fshop.example",
            ),
        ] {
            assert!(
                judge(&evidence(page, Some(auth)), data()).is_err(),
                "{page} {auth}"
            );
        }
        let mut unknown = evidence("https://shop.example", None);
        unknown.provider = "myspace".into();
        assert!(judge(&unknown, data()).is_err());
        // A button alone is weak evidence, never confirmed.
        assert!(
            !judge(&evidence("https://shop.example", None), data())
                .unwrap()
                .2
        );
    }

    #[test]
    fn warnings_say_what_falls_with_the_account() {
        let f = |pw: bool, two: bool, crown: bool| LoginFacts {
            item_id: Id::new(),
            title: "t".into(),
            crown_jewel: crown,
            urls: vec!["https://a.example".into()],
            username: String::new(),
            has_password: pw,
            has_second_step: two,
            method: String::new(),
            provider_text: String::new(),
            signin: None,
            provider_of: None,
            fill: None,
            editable: true,
        };
        let (a, b) = (f(false, false, false), f(true, false, false));
        let sites = [&a, &b];
        let none = warnings("Google", None, 0, &sites);
        assert!(none[0].contains("no Google login saved"));
        assert!(none[1].starts_with("1 of them has no password of its own"));
        let account = f(true, false, false);
        let weak = warnings("Google", Some(&account), 0, &sites);
        assert!(weak[0].contains("no two-step sign-in"));
        assert!(weak[1].contains("protected item"));
        let strong = f(true, true, true);
        assert_eq!(
            warnings("Google", Some(&strong), 0, &[&b]),
            Vec::<String>::new()
        );
        assert!(warnings("Google", None, 2, &sites)[0].contains("Pick which one"));
    }
}
