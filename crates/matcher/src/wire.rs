//! The JSON wire shape every host speaks to the matcher: the MV3 extension
//! through `lockdown-wasm`, the Tauri web view through a command, a shell
//! through UniFFI. One request type and one response type per call, every
//! field name `snake_case`, a schema version `v` on every response.
//!
//! Stability rule: within one `v`, fields are only ever **added**, never
//! renamed, removed or retyped, and a reader ignores fields it does not
//! know. A breaking change bumps `v` and keeps the old shape readable for
//! one release. The JSON Schema files under `extension/schema/` are the
//! published form of these structs; `tests/wire.rs` keeps them in step.
//!
//! Nothing secret crosses here. Addresses, rules and verdict text only.

use serde::{Deserialize, Serialize};

use crate::{
    FillContext, FillKind, FillVerdict, MatchDecision, MatchError, MatchRule, MatcherData, Origin,
    SavedUrl,
    related_origins::{RelatedOriginsCache, RelatedOriginsDoc},
    signed::DataError,
};

/// The schema version stamped on every response.
pub const SCHEMA_VERSION: u32 = 1;

/// `decide`: how does a saved address relate to the page?
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DecideRequest {
    /// The address saved on the item. Text without a scheme reads as `https://`.
    pub saved: String,
    /// The page's address.
    pub current: String,
    /// The item's rule for this address. `exact_host` when absent.
    #[serde(default)]
    pub rule: MatchRule,
}

/// A parsed address as the host sees it back: what the matcher compared.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OriginView {
    /// `https`, `http`, or another scheme.
    pub scheme: String,
    /// Lowercase ASCII host (punycode for internationalised names).
    pub host: String,
    /// The explicit port, if the address named one.
    pub port: Option<u16>,
    /// The path, `/` when none.
    pub path: String,
    /// The registrable domain by the Public Suffix List, if the host has one.
    pub registrable_domain: Option<String>,
}

impl From<&Origin> for OriginView {
    fn from(o: &Origin) -> Self {
        Self {
            scheme: o.scheme.clone(),
            host: o.host.clone(),
            port: o.port,
            path: o.path.clone(),
            registrable_domain: o.registrable_domain(),
        }
    }
}

/// The answer to [`DecideRequest`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DecideResponse {
    /// Schema version.
    pub v: u32,
    /// The decision: `exact`, `base_domain`, `equivalent`, `lookalike`, `no_match`.
    pub decision: MatchDecision,
    /// The saved address as parsed.
    pub saved: OriginView,
    /// The page address as parsed.
    pub current: OriginView,
}

/// `fill_check`: may the extension fill this field now?
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FillRequest {
    /// The top-level page's address.
    pub top: String,
    /// The address of the frame holding the field (equal to `top` when none).
    pub frame: String,
    /// The field and every ancestor are visible, on screen and not covered.
    pub field_visible: bool,
    /// The person already took a deliberate action for this fill.
    pub user_confirmed: bool,
    /// What is being filled: `login`, `code`, `card`, `passkey`, `identity`.
    pub kind: FillKind,
    /// The decision `decide` returned for the item on this frame.
    pub decision: MatchDecision,
}

/// What the extension may do.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Fill now.
    Fill,
    /// Ask first; `reason` says why.
    Confirm,
    /// Do not fill; `reason` says why.
    Refuse,
}

/// The answer to [`FillRequest`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FillResponse {
    /// Schema version.
    pub v: u32,
    /// The verdict.
    pub verdict: Verdict,
    /// Shown to the person for `confirm` and `refuse`; absent for `fill`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl From<FillVerdict> for FillResponse {
    fn from(v: FillVerdict) -> Self {
        let (verdict, reason) = match v {
            FillVerdict::Fill => (Verdict::Fill, None),
            FillVerdict::Confirm(r) => (Verdict::Confirm, Some(r)),
            FillVerdict::Refuse(r) => (Verdict::Refuse, Some(r)),
        };
        Self {
            v: SCHEMA_VERSION,
            verdict,
            reason,
        }
    }
}

/// `related_origins`: may this origin use the relying party's passkeys?
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RelatedOriginsRequest {
    /// The relying-party id, such as `example.com`.
    pub rp_id: String,
    /// The `origins` array of the fetched `/.well-known/webauthn` document.
    #[serde(default)]
    pub origins: Vec<String>,
    /// Seconds since the epoch when the document was fetched.
    pub fetched_at: u64,
    /// Seconds since the epoch now.
    pub now: u64,
    /// How long a fetched document stays fresh, in seconds.
    pub max_age_secs: u64,
    /// The origin asking.
    pub origin: String,
}

/// The answer to [`RelatedOriginsRequest`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RelatedOriginsResponse {
    /// Schema version.
    pub v: u32,
    /// Whether `origin` may use the relying party's passkeys.
    pub allowed: bool,
    /// The origins that survived the five-label cap, as `scheme://host[:port]`.
    pub effective: Vec<String>,
}

/// An error the host can show and branch on.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WireError {
    /// Schema version.
    pub v: u32,
    /// A stable code: `not_a_url`, `bad_data`.
    pub code: String,
    /// Plain-language text.
    pub message: String,
}

impl From<DataError> for WireError {
    fn from(e: DataError) -> Self {
        Self {
            v: SCHEMA_VERSION,
            code: e.code().into(),
            message: e.to_string(),
        }
    }
}

impl From<MatchError> for WireError {
    fn from(e: MatchError) -> Self {
        let code = match &e {
            MatchError::NotAUrl(_) => "not_a_url",
            MatchError::BadData(_) => "bad_data",
        };
        Self {
            v: SCHEMA_VERSION,
            code: code.into(),
            message: e.to_string(),
        }
    }
}

/// `load_matcher_data` and `matcher_data_status`: what the host learns
/// about the data in force.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DataStatus {
    /// Schema version.
    pub v: u32,
    /// The data version in force (`matcher_data` in the payload).
    pub version: u64,
    /// The newest version loaded or remembered: what the host persists.
    pub newest_loaded: u64,
    /// Which trusted key signed the data in force.
    pub key_id: String,
    /// The `published` label of the data in force.
    pub published: String,
    /// How many equivalent-domain groups are in force.
    pub groups: usize,
    /// How many confusable characters are mapped.
    pub confusables: usize,
}

impl DataStatus {
    /// Describes a state.
    pub fn of(state: &crate::DataState) -> Self {
        let d = state.data();
        Self {
            v: SCHEMA_VERSION,
            version: d.version,
            newest_loaded: state.newest_loaded(),
            key_id: d.key_id.clone(),
            published: d.published.clone(),
            groups: d.equivalent.len(),
            confusables: d.confusables.len(),
        }
    }
}

/// Runs `decide` on a wire request under the data in force.
pub fn decide(req: &DecideRequest, data: &MatcherData) -> Result<DecideResponse, WireError> {
    let saved = SavedUrl::parse(&req.saved, req.rule)?;
    let current = Origin::parse(&req.current)?;
    let decision = crate::decide_with(&saved, &current, data);
    Ok(DecideResponse {
        v: SCHEMA_VERSION,
        decision,
        saved: OriginView::from(&saved.origin),
        current: OriginView::from(&current),
    })
}

/// Runs `fill_check` on a wire request.
pub fn fill_check(req: &FillRequest) -> Result<FillResponse, WireError> {
    let ctx = FillContext {
        top: Origin::parse(&req.top)?,
        frame: Origin::parse(&req.frame)?,
        field_visible: req.field_visible,
        user_confirmed: req.user_confirmed,
        kind: req.kind,
    };
    Ok(crate::fill_check(&ctx, req.decision).into())
}

/// Runs the related-origins check on a wire request. The host passes the
/// document it fetched; the cache here lives for one call, so the host's
/// own cache decides when to refetch.
pub fn related_origins(req: &RelatedOriginsRequest) -> Result<RelatedOriginsResponse, WireError> {
    let origin = Origin::parse(&req.origin)?;
    let mut cache = RelatedOriginsCache::new(req.max_age_secs);
    cache.insert(
        &req.rp_id,
        RelatedOriginsDoc {
            origins: req.origins.clone(),
        },
        req.fetched_at,
    );
    let effective = cache
        .get(&req.rp_id, req.now)
        .map(|e| e.effective.clone())
        .unwrap_or_default();
    Ok(RelatedOriginsResponse {
        v: SCHEMA_VERSION,
        allowed: cache.allows(&req.rp_id, &origin, req.now),
        effective,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_round_trips_as_snake_case_json() {
        let req: DecideRequest = serde_json::from_str(
            r#"{"saved":"https://www.amazon.com","current":"https://www.amazon.co.uk/x"}"#,
        )
        .unwrap();
        assert_eq!(req.rule, MatchRule::ExactHost);
        let res = decide(&req, &MatcherData::builtin().unwrap()).unwrap();
        let json = serde_json::to_value(&res).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["decision"], "equivalent");
        assert_eq!(json["current"]["registrable_domain"], "amazon.co.uk");
        assert_eq!(json["saved"]["port"], serde_json::Value::Null);
    }

    #[test]
    fn errors_carry_a_stable_code() {
        let req = DecideRequest {
            saved: "not a url at all".into(),
            current: "https://example.com".into(),
            rule: MatchRule::ExactHost,
        };
        // "not a url at all" parses as a host with spaces? No: the url crate
        // refuses spaces in a host.
        let err = decide(&req, &MatcherData::empty()).unwrap_err();
        assert_eq!(err.code, "not_a_url");
        assert_eq!(err.v, SCHEMA_VERSION);
    }

    #[test]
    fn fill_check_wire_matches_the_rust_rule() {
        let base = FillRequest {
            top: "https://news.example".into(),
            frame: "https://login.bank.example".into(),
            field_visible: true,
            user_confirmed: false,
            kind: FillKind::Login,
            decision: MatchDecision::Exact,
        };
        let res = fill_check(&base).unwrap();
        assert_eq!(res.verdict, Verdict::Confirm);
        assert!(
            res.reason
                .unwrap()
                .contains("inside a page from news.example")
        );

        let same = FillRequest {
            frame: base.top.clone(),
            ..base.clone()
        };
        let res = fill_check(&same).unwrap();
        assert_eq!(res.verdict, Verdict::Fill);
        assert_eq!(
            serde_json::to_string(&res).unwrap(),
            r#"{"v":1,"verdict":"fill"}"#
        );

        let hidden = FillRequest {
            field_visible: false,
            ..same
        };
        let res = fill_check(&hidden).unwrap();
        assert_eq!(res.verdict, Verdict::Refuse);
        assert!(res.reason.unwrap().contains("clickjacking"));

        let json = r#"{"top":"https://a.example","frame":"https://a.example","field_visible":true,"user_confirmed":false,"kind":"passkey","decision":"exact"}"#;
        let req: FillRequest = serde_json::from_str(json).unwrap();
        assert_eq!(fill_check(&req).unwrap().verdict, Verdict::Confirm);
    }

    #[test]
    fn related_origins_wire_applies_the_cap_and_freshness() {
        let req = RelatedOriginsRequest {
            rp_id: "example.com".into(),
            origins: vec![
                "https://example.co.uk".into(),
                "https://brand-a.com".into(),
                "https://brand-b.com".into(),
                "https://brand-c.com".into(),
                "https://brand-d.com".into(),
                "https://brand-e.com".into(),
            ],
            fetched_at: 1000,
            now: 2000,
            max_age_secs: 3600,
            origin: "https://brand-e.com".into(),
        };
        let res = related_origins(&req).unwrap();
        assert!(!res.allowed, "sixth label is dropped");
        assert_eq!(res.effective.len(), 5);
        let ok = RelatedOriginsRequest {
            origin: "https://example.co.uk".into(),
            ..req.clone()
        };
        assert!(related_origins(&ok).unwrap().allowed);
        let stale = RelatedOriginsRequest {
            now: 1000 + 3601,
            ..ok
        };
        let res = related_origins(&stale).unwrap();
        assert!(!res.allowed);
        assert!(res.effective.is_empty());
    }
}
