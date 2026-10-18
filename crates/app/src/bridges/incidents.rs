//! The seven guided incident flows (docs/plan/23-bridges.md §6) with the
//! vault filled in: each step lists the items it is about (the breached
//! site's login, the logins sharing its password, every code, the
//! mailbox and the accounts using that address, a provider's account and
//! its sites, the most important logins first), and the person ticks
//! steps and items off as they go.
//!
//! Progress stays on this device: one small file per persona,
//! `incidents-<persona>.cbor` beside `remembered.cbor`, sealed under the
//! persona's App Data Key (like the recency record), holding the flow,
//! when it started, what was picked (an item id, an address or a
//! provider) and which steps and items are done. A file that doesn't open
//! is read as no progress.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lockdown_matcher::Origin;
use lockdown_security::incident::{Action, Affects, INCIDENTS, Incident, Scope, incident};
use lockdown_security::site_data::SiteData;
use serde::{Deserialize, Serialize};

use crate::security::SecurityReport;
use crate::session::bridges::LoginFacts;
use crate::{App, AppError, AppResult, session};

const AAD: &[u8] = b"lockdown/incidents/v1";
const FILE_V: u16 = 1;

/// One flow's progress.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Progress {
    started_at: i64,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    steps: BTreeSet<String>,
    #[serde(default)]
    items: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Serialize, Deserialize, Default)]
struct Body {
    v: u16,
    flows: BTreeMap<String, Progress>,
}

/// A flow in the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IncidentSummary {
    pub id: &'static str,
    pub title: &'static str,
    pub when: &'static str,
    pub scope: Scope,
    /// When the person started it, if they did.
    pub started_at: Option<i64>,
    pub steps_done: usize,
    pub steps: usize,
}

/// Something the person may pick to start a flow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScopeChoice {
    pub value: String,
    pub label: String,
}

/// An item a step is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StepItem {
    pub item_id: String,
    pub title: String,
    pub host: String,
    pub done: bool,
}

/// One step as the flow shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StepView {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub action: Action,
    pub done: bool,
    pub items: Vec<StepItem>,
}

/// A flow, filled in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IncidentView {
    pub id: &'static str,
    pub title: &'static str,
    pub when: &'static str,
    pub scope: Scope,
    /// What was picked, when the flow needs a pick.
    pub picked: Option<String>,
    /// What may be picked.
    pub choices: Vec<ScopeChoice>,
    pub started_at: Option<i64>,
    pub steps: Vec<StepView>,
    /// Every step is done.
    pub finished: bool,
}

fn host(f: &LoginFacts) -> String {
    f.urls
        .iter()
        .find_map(|u| Origin::parse(u).ok())
        .map(|o| o.host)
        .unwrap_or_default()
}

fn on_domain(f: &LoginFacts, domain: &str) -> bool {
    f.urls
        .iter()
        .filter_map(|u| Origin::parse(u).ok())
        .any(|o| o.host == domain || o.host.ends_with(&format!(".{domain}")))
}

fn looks_like_email(text: &str) -> bool {
    lockdown_security::alias::plain_address(text)
}

/// The logins that are a mailbox for `email`: on the address's own
/// domain, or on its provider's (a gmail.com address lives at Google).
fn mailbox<'a>(logins: &'a [LoginFacts], email: &str) -> Vec<&'a LoginFacts> {
    let Some((_, domain)) = email.rsplit_once('@') else {
        return Vec::new();
    };
    let domain = domain.to_ascii_lowercase();
    let data = SiteData::builtin();
    let mut domains = vec![domain.clone()];
    if let Some(p) = data.provider_for_site(&domain) {
        domains.extend(p.domains.iter().cloned());
    }
    logins
        .iter()
        .filter(|f| domains.iter().any(|d| on_domain(f, d)))
        .filter(|f| {
            f.username.eq_ignore_ascii_case(email)
                || f.username
                    .split('@')
                    .next()
                    .is_some_and(|u| email.split('@').next() == Some(u))
        })
        .collect()
}

/// Logins on any provider's own domains: the accounts others hang on.
fn provider_accounts(logins: &[LoginFacts]) -> Vec<&LoginFacts> {
    let data = SiteData::builtin();
    logins
        .iter()
        .filter(|f| {
            data.providers()
                .iter()
                .any(|p| p.domains.iter().any(|d| on_domain(f, d)))
        })
        .collect()
}

struct Ctx<'a> {
    logins: &'a [LoginFacts],
    report: &'a SecurityReport,
    codes: &'a [(lockdown_vault::Id, String, bool)],
    crown: &'a [(lockdown_vault::Id, String)],
    map: &'a super::federation::FederationMap,
}

fn item(id: &str, title: &str, host: &str, done: &BTreeSet<String>) -> StepItem {
    StepItem {
        item_id: id.to_string(),
        title: title.to_string(),
        host: host.to_string(),
        done: done.contains(id),
    }
}

fn affected(
    affects: Affects,
    picked: Option<&str>,
    ctx: &Ctx<'_>,
    done: &BTreeSet<String>,
) -> Vec<StepItem> {
    let login = |f: &LoginFacts| item(&f.item_id.to_string(), &f.title, &host(f), done);
    let by_id = |id: &str| {
        ctx.logins
            .iter()
            .find(|f| f.item_id.to_string() == id)
            .map(login)
    };
    let mut out: Vec<StepItem> = match affects {
        Affects::Nothing => Vec::new(),
        Affects::Picked => picked.and_then(by_id).into_iter().collect(),
        Affects::SamePassword => {
            let Some(p) = picked else { return Vec::new() };
            ctx.report
                .findings
                .iter()
                .filter(|f| f.kind == "reused" && f.item_id.as_deref() == Some(p))
                .flat_map(|f| f.related.iter())
                .filter_map(|r| by_id(&r.id).or_else(|| Some(item(&r.id, &r.title, "", done))))
                .collect()
        }
        Affects::ReusedOrBreached => ctx
            .report
            .findings
            .iter()
            .filter(|f| matches!(f.kind.as_str(), "reused" | "breached"))
            .filter_map(|f| {
                let id = f.item_id.as_deref()?;
                by_id(id)
                    .or_else(|| Some(item(id, f.item_title.as_deref().unwrap_or(""), "", done)))
            })
            .collect(),
        Affects::Codes => ctx
            .codes
            .iter()
            .map(|(id, title, _)| {
                let h = ctx
                    .logins
                    .iter()
                    .find(|f| f.item_id == *id)
                    .map(host)
                    .unwrap_or_default();
                item(&id.to_string(), title, &h, done)
            })
            .collect(),
        Affects::Mailbox => picked
            .map(|e| mailbox(ctx.logins, e).into_iter().map(login).collect())
            .unwrap_or_default(),
        Affects::UsingEmail => {
            let Some(e) = picked else { return Vec::new() };
            let boxes: Vec<_> = mailbox(ctx.logins, e).iter().map(|f| f.item_id).collect();
            ctx.logins
                .iter()
                .filter(|f| f.username.eq_ignore_ascii_case(e) && !boxes.contains(&f.item_id))
                .map(login)
                .collect()
        }
        Affects::ProviderAccount => {
            let Some(p) = picked else { return Vec::new() };
            ctx.map
                .providers
                .iter()
                .filter(|x| x.id == p)
                .flat_map(|x| x.account.iter().chain(x.candidates.iter()))
                .map(|a| item(&a.item_id, &a.title, &a.host, done))
                .collect()
        }
        Affects::SignsInWithProvider => {
            let Some(p) = picked else { return Vec::new() };
            ctx.map
                .providers
                .iter()
                .filter(|x| x.id == p)
                .flat_map(|x| x.sites.iter())
                .map(|a| item(&a.item_id, &a.title, &a.host, done))
                .collect()
        }
        Affects::MostImportant => important(ctx)
            .into_iter()
            .map(|(id, title, h)| item(&id, &title, &h, done))
            .collect(),
        Affects::EveryLogin => {
            let first: BTreeSet<String> = important(ctx).into_iter().map(|(id, _, _)| id).collect();
            ctx.logins
                .iter()
                .filter(|f| !first.contains(&f.item_id.to_string()))
                .map(login)
                .collect()
        }
    };
    let mut seen = BTreeSet::new();
    out.retain(|i| seen.insert(i.item_id.clone()));
    out
}

/// Crown jewels, then the accounts others sign in through, then mailboxes.
fn important(ctx: &Ctx<'_>) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = ctx
        .crown
        .iter()
        .map(|(id, t)| (id.to_string(), t.clone(), String::new()))
        .collect();
    for f in provider_accounts(ctx.logins) {
        out.push((f.item_id.to_string(), f.title.clone(), host(f)));
    }
    let mut seen = BTreeSet::new();
    out.retain(|(id, _, _)| seen.insert(id.clone()));
    out
}

fn choices(scope: Scope, ctx: &Ctx<'_>) -> Vec<ScopeChoice> {
    let mut out = match scope {
        Scope::None => Vec::new(),
        Scope::Item => ctx
            .logins
            .iter()
            .map(|f| ScopeChoice {
                value: f.item_id.to_string(),
                label: match host(f) {
                    h if h.is_empty() => f.title.clone(),
                    h => format!("{} ({h})", f.title),
                },
            })
            .collect(),
        Scope::Email => {
            let set: BTreeSet<String> = ctx
                .logins
                .iter()
                .map(|f| f.username.trim().to_ascii_lowercase())
                .filter(|u| looks_like_email(u))
                .collect();
            set.into_iter()
                .map(|e| ScopeChoice {
                    label: e.clone(),
                    value: e,
                })
                .collect()
        }
        Scope::Provider => {
            let data = SiteData::builtin();
            let mut v: Vec<ScopeChoice> = ctx
                .map
                .providers
                .iter()
                .map(|p| ScopeChoice {
                    value: p.id.clone(),
                    label: p.name.clone(),
                })
                .collect();
            for p in data.providers() {
                if !v.iter().any(|c| c.value == p.id) {
                    v.push(ScopeChoice {
                        value: p.id.clone(),
                        label: p.name.clone(),
                    });
                }
            }
            v
        }
    };
    if scope == Scope::Item {
        out.sort_by_key(|a| a.label.to_lowercase());
    }
    out
}

fn build(flow: &'static Incident, progress: Option<&Progress>, ctx: &Ctx<'_>) -> IncidentView {
    let empty = Progress::default();
    let p = progress.unwrap_or(&empty);
    let picked = p.scope.clone();
    let steps: Vec<StepView> = flow
        .steps
        .iter()
        .map(|s| {
            let done_items = p.items.get(s.id).cloned().unwrap_or_default();
            StepView {
                id: s.id,
                title: s.title,
                detail: s.detail,
                action: s.action,
                done: p.steps.contains(s.id),
                items: affected(s.affects, picked.as_deref(), ctx, &done_items),
            }
        })
        .collect();
    IncidentView {
        id: flow.id,
        title: flow.title,
        when: flow.when,
        scope: flow.scope,
        finished: steps.iter().all(|s| s.done),
        steps,
        choices: choices(flow.scope, ctx),
        picked,
        started_at: progress.map(|p| p.started_at),
    }
}

fn path_for(data: &std::path::Path, persona: lockdown_vault::Id) -> PathBuf {
    data.join(format!("incidents-{persona}.cbor"))
}

impl crate::Core {
    fn incidents_load(&mut self) -> AppResult<Body> {
        let session = self.session()?;
        let path = path_for(&session.data_dir(), session.bridge_persona());
        Ok(std::fs::read(&path)
            .ok()
            .and_then(|b| session.bridge_open(AAD, &b))
            .and_then(|plain| lockdown_crypto::cbor::from_slice::<Body>(&plain).ok())
            .filter(|b| b.v <= FILE_V)
            .unwrap_or_default())
    }

    fn incidents_save(&mut self, body: &Body) -> AppResult<()> {
        let session = self.session()?;
        let path = path_for(&session.data_dir(), session.bridge_persona());
        let plain = zeroize::Zeroizing::new(
            lockdown_crypto::cbor::to_canonical(body)
                .map_err(|e| AppError::refused(e.to_string()))?,
        );
        let bytes = session
            .bridge_seal(AAD, &plain)
            .map_err(AppError::refused)?;
        lockdown_store::write_private(&path, &bytes)
            .map_err(|e| AppError::refused(format!("couldn't write {}: {e}", path.display())))
    }

    fn incident_view(
        &mut self,
        flow: &'static Incident,
        report: &SecurityReport,
        map: &super::federation::FederationMap,
    ) -> AppResult<IncidentView> {
        let body = self.incidents_load()?;
        let session = self.session()?;
        let logins: Vec<LoginFacts> = session
            .bridge_logins()
            .map_err(AppError::refused)?
            .into_iter()
            .filter(|f| !f.crown_jewel)
            .collect();
        let codes = session.bridge_code_items().map_err(AppError::refused)?;
        let crown = session.bridge_crown_jewels();
        let ctx = Ctx {
            logins: &logins,
            report,
            codes: &codes,
            crown: &crown,
            map,
        };
        Ok(build(flow, body.flows.get(flow.id), &ctx))
    }
}

fn flow(id: &str) -> AppResult<&'static Incident> {
    incident(id).ok_or_else(|| AppError::refused("that isn't one of the guided flows"))
}

impl App {
    /// The seven flows, with how far the person got in each.
    pub fn incidents(&self) -> AppResult<Vec<IncidentSummary>> {
        self.call(|core| {
            let body = core.incidents_load()?;
            Ok(INCIDENTS
                .iter()
                .map(|i| {
                    let p = body.flows.get(i.id);
                    IncidentSummary {
                        id: i.id,
                        title: i.title,
                        when: i.when,
                        scope: i.scope,
                        started_at: p.map(|p| p.started_at),
                        steps_done: p.map_or(0, |p| {
                            i.steps.iter().filter(|s| p.steps.contains(s.id)).count()
                        }),
                        steps: i.steps.len(),
                    }
                })
                .collect())
        })
    }

    fn incident_with(
        &self,
        id: &str,
        change: impl FnOnce(&'static Incident, &mut Body) -> AppResult<bool> + Send + 'static,
    ) -> AppResult<IncidentView> {
        let flow = flow(id)?;
        let report = self.security_report()?;
        let map = self.federation_map()?;
        self.call(move |core| {
            let mut body = core.incidents_load()?;
            if change(flow, &mut body)? {
                body.v = FILE_V;
                core.incidents_save(&body)?;
            }
            core.incident_view(flow, &report, &map)
        })
    }

    /// One flow, filled in from the vault. With `pick`, the flow starts (or
    /// restarts) on that item, address or provider.
    pub fn incident(&self, id: &str, pick: Option<&str>) -> AppResult<IncidentView> {
        let pick = pick
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        self.incident_with(id, move |flow, body| {
            let Some(pick) = pick else {
                return Ok(false);
            };
            if flow.scope == Scope::None {
                return Err(AppError::refused(
                    "this flow is about everything; there's nothing to pick",
                ));
            }
            if pick.len() > 254 {
                return Err(AppError::refused("that's too long to pick"));
            }
            let fresh = body
                .flows
                .get(flow.id)
                .is_none_or(|p| p.scope.as_deref() != Some(&pick));
            if fresh {
                body.flows.insert(
                    flow.id.to_string(),
                    Progress {
                        started_at: session::now_secs(),
                        scope: Some(pick),
                        ..Progress::default()
                    },
                );
            }
            Ok(fresh)
        })
    }

    /// Ticks a step (with `item`, one of its items) done or not done.
    pub fn incident_mark(
        &self,
        id: &str,
        step: &str,
        item: Option<&str>,
        done: bool,
    ) -> AppResult<IncidentView> {
        let (step, item) = (step.to_string(), item.map(str::to_string));
        self.incident_with(id, move |flow, body| {
            let step = flow
                .steps
                .iter()
                .find(|s| s.id == step)
                .ok_or_else(|| AppError::refused("that isn't a step of this flow"))?;
            if flow.scope != Scope::None
                && body.flows.get(flow.id).is_none_or(|p| p.scope.is_none())
            {
                return Err(AppError::refused("pick what this is about first"));
            }
            let p = body
                .flows
                .entry(flow.id.to_string())
                .or_insert_with(|| Progress {
                    started_at: session::now_secs(),
                    ..Progress::default()
                });
            match item {
                Some(i) => {
                    lockdown_vault::Id::parse(&i)
                        .map_err(|_| AppError::refused("that isn't an item"))?;
                    let set = p.items.entry(step.id.to_string()).or_default();
                    if done {
                        set.insert(i);
                    } else {
                        set.remove(&i);
                    }
                }
                None => {
                    if done {
                        p.steps.insert(step.id.to_string());
                    } else {
                        p.steps.remove(step.id);
                    }
                }
            }
            Ok(true)
        })
    }

    /// Forgets a flow's progress.
    pub fn incident_reset(&self, id: &str) -> AppResult<IncidentView> {
        self.incident_with(id, |flow, body| Ok(body.flows.remove(flow.id).is_some()))
    }
}
