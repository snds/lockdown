//! `lockdown-policy`: the policy engine (03-architecture §2 `policy` row;
//! 02-model §3 crown-jewel rules; brief §2.9 point 5).
//!
//! **Status: skeleton with the evaluation order real.** The 25+ named
//! organization policies are not modelled yet; what is settled is *how* a
//! decision is reached, and that is implemented and tested here so every
//! client evaluates identically.
//!
//! Evaluation order, highest first:
//! 1. Hard rules that no policy can lower: a crown-jewel item is never
//!    autofilled, never given to an agent, never in a bulk export unless the
//!    person opted that item in (02-model §3).
//! 2. Organization `Deny` rules.
//! 3. Personal `Deny` rules.
//! 4. Any `StepUp` rule (fresh biometric, full password).
//! 5. Otherwise `Allow`.

pub mod managed;

use serde::{Deserialize, Serialize};

/// What is being asked.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Fill a field into a page or app.
    Autofill,
    /// Show a hidden value on screen.
    Reveal,
    /// Copy a value to the clipboard.
    Copy,
    /// Export one item or the vault.
    Export,
    /// Share an item or vault with another persona.
    Share,
    /// Create a Send (a time-boxed share with an outsider).
    Send,
    /// Give a value to an AI agent or automation.
    AgentAccess,
    /// Release through a legacy vault.
    LegacyRelease,
}

/// Facts about the item the request is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Subject {
    /// The item is in the crown-jewel class (02-model §3).
    pub crown_jewel: bool,
    /// The person opted this crown-jewel item into bulk export and legacy release.
    pub export_opt_in: bool,
    /// The person opted this crown-jewel item into agent access.
    pub agent_opt_in: bool,
}

/// Where a policy comes from. Organization outranks personal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// Set by the person for themselves.
    Personal,
    /// Set by an organization administrator.
    Organization,
}

/// What a matching rule asks for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    /// Refuse, with a reason the person reads.
    Deny(String),
    /// Allow after a fresh biometric or the full password.
    StepUp(String),
    /// Allow outright (only meaningful to document intent; it never
    /// overrides a deny).
    Allow,
}

/// One rule: for this action, this effect.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rule {
    /// The action it governs.
    pub action: Action,
    /// What it asks for.
    pub effect: Effect,
}

/// A set of rules from one scope.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Policy {
    /// Where it came from.
    pub scope: Scope,
    /// Its rules, in no particular order; the engine orders them.
    pub rules: Vec<Rule>,
}

/// The outcome of evaluation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyDecision {
    /// Go ahead.
    Allow,
    /// Go ahead after a second deliberate action.
    RequireStepUp {
        /// Why, in the person's words.
        reason: String,
    },
    /// Refused.
    Deny {
        /// Why, in the person's words.
        reason: String,
        /// Where the refusal came from.
        source: Source,
    },
}

/// Who refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// A rule no policy can lower (02-model §3).
    HardRule,
    /// An organization policy.
    Organization,
    /// A personal policy.
    Personal,
}

/// The crown-jewel hard rules (02-model §3). `None` when no hard rule applies.
fn hard_rule(action: Action, subject: &Subject) -> Option<PolicyDecision> {
    if !subject.crown_jewel {
        return None;
    }
    let deny = |reason: &str| {
        Some(PolicyDecision::Deny {
            reason: reason.to_string(),
            source: Source::HardRule,
        })
    };
    let step_up = |reason: &str| {
        Some(PolicyDecision::RequireStepUp {
            reason: reason.to_string(),
        })
    };
    match action {
        Action::Autofill => deny("crown-jewel items are never autofilled"),
        Action::AgentAccess if !subject.agent_opt_in => {
            deny("crown-jewel items are never given to agents unless you opt this item in")
        }
        Action::Export | Action::LegacyRelease if !subject.export_opt_in => deny(
            "crown-jewel items stay out of bulk export and legacy release unless you opt this item in",
        ),
        Action::Reveal | Action::Export => {
            step_up("revealing or exporting a crown-jewel item needs your full account password")
        }
        Action::Copy
        | Action::Share
        | Action::Send
        | Action::AgentAccess
        | Action::LegacyRelease => {
            step_up("using a crown-jewel item needs a fresh biometric check and a deliberate tap")
        }
    }
}

/// Evaluates `action` on `subject` under `policies`, in the order the module
/// documentation gives. Deterministic: the same inputs give the same answer
/// on every client.
pub fn evaluate(action: Action, subject: &Subject, policies: &[Policy]) -> PolicyDecision {
    if let Some(d) = hard_rule(action, subject) {
        // A hard deny wins outright; a hard step-up still lets a policy deny.
        if matches!(d, PolicyDecision::Deny { .. }) {
            return d;
        }
        if let Some(deny) = first_deny(action, policies) {
            return deny;
        }
        return d;
    }
    if let Some(deny) = first_deny(action, policies) {
        return deny;
    }
    for scope in [Scope::Organization, Scope::Personal] {
        for p in policies.iter().filter(|p| p.scope == scope) {
            for r in p.rules.iter().filter(|r| r.action == action) {
                if let Effect::StepUp(reason) = &r.effect {
                    return PolicyDecision::RequireStepUp {
                        reason: reason.clone(),
                    };
                }
            }
        }
    }
    PolicyDecision::Allow
}

fn first_deny(action: Action, policies: &[Policy]) -> Option<PolicyDecision> {
    for (scope, source) in [
        (Scope::Organization, Source::Organization),
        (Scope::Personal, Source::Personal),
    ] {
        for p in policies.iter().filter(|p| p.scope == scope) {
            for r in p.rules.iter().filter(|r| r.action == action) {
                if let Effect::Deny(reason) = &r.effect {
                    return Some(PolicyDecision::Deny {
                        reason: reason.clone(),
                        source,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(scope: Scope, action: Action, effect: Effect) -> Policy {
        Policy {
            scope,
            rules: vec![Rule { action, effect }],
        }
    }

    #[test]
    fn evaluation_order_holds() {
        let plain = Subject::default();
        let jewel = Subject {
            crown_jewel: true,
            ..Default::default()
        };
        // 5. Nothing applies: allow.
        assert_eq!(
            evaluate(Action::Autofill, &plain, &[]),
            PolicyDecision::Allow
        );
        // 1. Hard rule beats a personal allow and an organization allow.
        let allows = [
            policy(Scope::Organization, Action::Autofill, Effect::Allow),
            policy(Scope::Personal, Action::Autofill, Effect::Allow),
        ];
        assert!(matches!(
            evaluate(Action::Autofill, &jewel, &allows),
            PolicyDecision::Deny {
                source: Source::HardRule,
                ..
            }
        ));
        // 2 before 3: organization deny is reported over a personal deny.
        let denies = [
            policy(
                Scope::Personal,
                Action::Export,
                Effect::Deny("personal".into()),
            ),
            policy(
                Scope::Organization,
                Action::Export,
                Effect::Deny("org".into()),
            ),
        ];
        assert_eq!(
            evaluate(Action::Export, &plain, &denies),
            PolicyDecision::Deny {
                reason: "org".into(),
                source: Source::Organization
            }
        );
        // 3 before 4: a personal deny beats an organization step-up.
        let mixed = [
            policy(
                Scope::Organization,
                Action::Share,
                Effect::StepUp("org step".into()),
            ),
            policy(Scope::Personal, Action::Share, Effect::Deny("mine".into())),
        ];
        assert!(matches!(
            evaluate(Action::Share, &plain, &mixed),
            PolicyDecision::Deny {
                source: Source::Personal,
                ..
            }
        ));
        // 4: step-up when only that applies.
        let step = [policy(
            Scope::Personal,
            Action::Copy,
            Effect::StepUp("fresh biometric".into()),
        )];
        assert_eq!(
            evaluate(Action::Copy, &plain, &step),
            PolicyDecision::RequireStepUp {
                reason: "fresh biometric".into()
            }
        );
    }

    #[test]
    fn crown_jewel_opt_ins_are_per_item_and_still_step_up() {
        let opted = Subject {
            crown_jewel: true,
            export_opt_in: true,
            agent_opt_in: true,
        };
        assert!(matches!(
            evaluate(Action::Export, &opted, &[]),
            PolicyDecision::RequireStepUp { .. }
        ));
        assert!(matches!(
            evaluate(Action::AgentAccess, &opted, &[]),
            PolicyDecision::RequireStepUp { .. }
        ));
        // Opting in never unlocks autofill.
        assert!(matches!(
            evaluate(Action::Autofill, &opted, &[]),
            PolicyDecision::Deny {
                source: Source::HardRule,
                ..
            }
        ));
        // A policy deny still applies on top of a hard step-up.
        let deny = [policy(
            Scope::Organization,
            Action::Export,
            Effect::Deny("no export".into()),
        )];
        assert!(matches!(
            evaluate(Action::Export, &opted, &deny),
            PolicyDecision::Deny {
                source: Source::Organization,
                ..
            }
        ));
    }
}
