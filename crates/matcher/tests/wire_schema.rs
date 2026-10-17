//! Keeps `extension/schema/*.schema.json` in step with `matcher::wire`.
//!
//! Not a full JSON Schema validator (that would be a dependency for one
//! test). It checks the two things that make a schema lie: a field the
//! Rust side emits that the schema does not declare, and a field the schema
//! requires that the Rust side does not emit. Enum values are checked too.

use std::collections::BTreeSet;

use lockdown_matcher::wire::{self, *};
use lockdown_matcher::{EquivalentDomains, FillKind, MatchDecision, MatchRule};
use serde_json::Value;

const DECIDE_REQUEST: &str = include_str!("../../../extension/schema/decide-request.schema.json");
const DECIDE_RESPONSE: &str = include_str!("../../../extension/schema/decide-response.schema.json");
const FILL_REQUEST: &str = include_str!("../../../extension/schema/fill-request.schema.json");
const FILL_RESPONSE: &str = include_str!("../../../extension/schema/fill-response.schema.json");
const RO_REQUEST: &str =
    include_str!("../../../extension/schema/related-origins-request.schema.json");
const RO_RESPONSE: &str =
    include_str!("../../../extension/schema/related-origins-response.schema.json");
const WIRE_ERROR: &str = include_str!("../../../extension/schema/wire-error.schema.json");

fn schema(text: &str) -> Value {
    serde_json::from_str(text).expect("schema file is JSON")
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every emitted field is declared; every required field is emitted.
fn check(schema: &Value, instance: &Value, path: &str) {
    let declared = keys(&schema["properties"]);
    let emitted = keys(instance);
    for k in &emitted {
        assert!(
            declared.contains(k),
            "{path}: `{k}` is emitted but not in the schema"
        );
    }
    if let Some(req) = schema["required"].as_array() {
        for r in req {
            let r = r.as_str().unwrap();
            assert!(
                emitted.contains(r),
                "{path}: `{r}` is required but not emitted"
            );
        }
    }
    for (k, v) in instance.as_object().unwrap() {
        let prop = &schema["properties"][k];
        if let Some(values) = prop["enum"].as_array() {
            assert!(
                values.contains(v),
                "{path}.{k}: {v} is not in the schema's enum"
            );
        }
        if let Some(c) = prop.get("const") {
            assert_eq!(v, c, "{path}.{k}");
        }
    }
}

fn check_with_defs(root: &Value, schema: &Value, instance: &Value, path: &str) {
    check(schema, instance, path);
    for (k, v) in instance.as_object().unwrap() {
        let prop = &schema["properties"][k];
        if let Some(r) = prop["$ref"].as_str() {
            let name = r.rsplit('/').next().unwrap();
            check(&root["$defs"][name], v, &format!("{path}.{k}"));
        }
    }
}

#[test]
fn decide_request_and_response_match_their_schemas() {
    let req = DecideRequest {
        saved: "https://www.amazon.com".into(),
        current: "https://www.amazon.co.uk:8443/a".into(),
        rule: MatchRule::BaseDomain,
    };
    check(
        &schema(DECIDE_REQUEST),
        &serde_json::to_value(&req).unwrap(),
        "decide-request",
    );
    for rule in [
        MatchRule::ExactHost,
        MatchRule::BaseDomain,
        MatchRule::Equivalent,
        MatchRule::StartsWith,
        MatchRule::Never,
    ] {
        let r = DecideRequest {
            rule,
            ..req.clone()
        };
        check(
            &schema(DECIDE_REQUEST),
            &serde_json::to_value(&r).unwrap(),
            "decide-request",
        );
    }
    let root = schema(DECIDE_RESPONSE);
    let res = wire::decide(&req, &EquivalentDomains::curated()).unwrap();
    check_with_defs(
        &root,
        &root,
        &serde_json::to_value(&res).unwrap(),
        "decide-response",
    );
    // Every decision value is in the enum.
    for d in [
        MatchDecision::Exact,
        MatchDecision::BaseDomain,
        MatchDecision::Equivalent,
        MatchDecision::Lookalike,
        MatchDecision::NoMatch,
    ] {
        let r = DecideResponse {
            decision: d,
            ..res.clone()
        };
        check_with_defs(
            &root,
            &root,
            &serde_json::to_value(&r).unwrap(),
            "decide-response",
        );
    }
}

#[test]
fn fill_request_and_response_match_their_schemas() {
    let kinds = [
        FillKind::Login,
        FillKind::Code,
        FillKind::Card,
        FillKind::Passkey,
        FillKind::Identity,
    ];
    for kind in kinds {
        for (frame, visible, confirmed) in [
            ("https://login.bank.example", true, false),
            ("https://news.example", false, false),
            ("https://news.example", true, true),
            ("https://news.example", true, false),
        ] {
            let req = FillRequest {
                top: "https://news.example".into(),
                frame: frame.into(),
                field_visible: visible,
                user_confirmed: confirmed,
                kind,
                decision: MatchDecision::Exact,
            };
            check(
                &schema(FILL_REQUEST),
                &serde_json::to_value(&req).unwrap(),
                "fill-request",
            );
            let res = wire::fill_check(&req).unwrap();
            check(
                &schema(FILL_RESPONSE),
                &serde_json::to_value(&res).unwrap(),
                "fill-response",
            );
        }
    }
}

#[test]
fn related_origins_and_errors_match_their_schemas() {
    let req = RelatedOriginsRequest {
        rp_id: "example.com".into(),
        origins: vec!["https://example.co.uk".into()],
        fetched_at: 1,
        now: 2,
        max_age_secs: 3600,
        origin: "https://example.co.uk".into(),
    };
    check(
        &schema(RO_REQUEST),
        &serde_json::to_value(&req).unwrap(),
        "related-origins-request",
    );
    let res = wire::related_origins(&req).unwrap();
    check(
        &schema(RO_RESPONSE),
        &serde_json::to_value(&res).unwrap(),
        "related-origins-response",
    );
    let err = wire::decide(
        &DecideRequest {
            saved: "".into(),
            current: "https://example.com".into(),
            rule: MatchRule::ExactHost,
        },
        &EquivalentDomains::none(),
    )
    .unwrap_err();
    check(
        &schema(WIRE_ERROR),
        &serde_json::to_value(&err).unwrap(),
        "wire-error",
    );
}
