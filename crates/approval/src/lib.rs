//! `lockdown-approval`: the one approval service (03-architecture §2
//! `approval` row; brief §2.7 "the hard safety rule" and §2.9 point 4).
//!
//! Passkey QR sign-ins, device-code activations and (if ever) bridge QR
//! approvals all reduce to "approve a sign-in I am being shown". This crate
//! decides once, for all of them.
//!
//! **Status: the gates are real and wired.** The proximity rule, number
//! matching and the rate limit are implemented and tested, and the shells'
//! scan flow (`lockdown_app::scan`) runs every scanned or pasted sign-in
//! code through [`ApprovalService::evaluate`] before anything else. The
//! signed allow-list of activation pages and the reading of a scanned code
//! are [`activation`]. The "this wasn't me" report pipeline is not built.
//!
//! The proximity rule: a code is approved **only** when the person scanned it
//! from a screen in front of them. Codes that arrive by message, email, link
//! or paste are refused, and no setting can override this.

use serde::{Deserialize, Serialize};

pub mod activation;

/// What kind of sign-in is being approved.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalKind {
    /// A passkey cross-device sign-in (the hybrid QR flow).
    PasskeyQr,
    /// An OAuth device-authorization code (RFC 8628) shown by a TV, console or CLI.
    DeviceCode,
    /// A first-party bridge's QR approval (tolerated tier, D8).
    BridgeQr,
}

/// How the code reached Lockdown. Only [`CodeSource::ScannedFromScreen`]
/// passes the proximity gate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeSource {
    /// The camera read it from a screen in front of the person.
    ScannedFromScreen,
    /// Typed or pasted by hand.
    Pasted,
    /// Opened from a link.
    Link,
    /// Arrived in a message or email.
    Message,
    /// Read from an image file (a photo or a screenshot): it may have been
    /// sent, so it counts as a paste does.
    Image,
}

/// Number matching: the sign-in screen shows a number and the approver must
/// pick or type the same one, which defeats blind "tap yes" fatigue
/// (brief §2.9 point 4).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NumberMatch {
    /// The number the sign-in screen shows.
    pub expected: u16,
    /// The number the person entered on the approving device.
    pub entered: Option<u16>,
}

/// What the person is shown before approving.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ApprovalContext {
    /// The service, as the full domain (never a vendor nickname).
    pub service: String,
    /// The account on that service.
    pub account: String,
    /// The device asking, where the page shows it.
    pub device: Option<String>,
    /// The place, where known.
    pub location: Option<String>,
}

/// One request to approve a sign-in.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// What kind of sign-in.
    pub kind: ApprovalKind,
    /// How the code reached Lockdown: the proximity gate's input.
    pub source: CodeSource,
    /// Number matching, when the vendor supports it.
    pub number_match: Option<NumberMatch>,
    /// What the person sees.
    pub context: ApprovalContext,
}

/// The service's answer.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    /// Show the prompt; the person may approve.
    Prompt,
    /// Refused before any prompt. The reason is shown in full.
    Refused {
        /// Why, in the person's words.
        reason: String,
    },
}

/// A fixed-window rate limit on prompts, so a flood of requests cannot wear
/// a person down into approving one.
#[derive(Clone, Debug)]
pub struct RateLimit {
    /// How many prompts the window allows.
    pub max_prompts: u32,
    /// The window length in seconds.
    pub window_secs: u64,
    window_start: u64,
    count: u32,
}

impl RateLimit {
    /// A limit of `max_prompts` per `window_secs`.
    pub fn new(max_prompts: u32, window_secs: u64) -> Self {
        Self {
            max_prompts,
            window_secs,
            window_start: 0,
            count: 0,
        }
    }

    /// Records a prompt at `now` (seconds) and says whether it was allowed.
    pub fn allow(&mut self, now: u64) -> bool {
        if now.saturating_sub(self.window_start) >= self.window_secs {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= self.max_prompts {
            return false;
        }
        self.count += 1;
        true
    }
}

/// The approval service.
#[derive(Clone, Debug)]
pub struct ApprovalService {
    limit: RateLimit,
}

impl Default for ApprovalService {
    fn default() -> Self {
        // Five prompts a minute is generous for a person and hostile to a bot.
        Self::new(RateLimit::new(5, 60))
    }
}

impl ApprovalService {
    /// A service with this rate limit.
    pub fn new(limit: RateLimit) -> Self {
        Self { limit }
    }

    /// Decides whether `request` may even be shown, at time `now` (seconds).
    /// Gates run in order: proximity, context, number matching, rate limit.
    pub fn evaluate(&mut self, request: &ApprovalRequest, now: u64) -> ApprovalDecision {
        let refuse = |reason: &str| ApprovalDecision::Refused {
            reason: reason.to_string(),
        };
        if request.source != CodeSource::ScannedFromScreen {
            return refuse(
                "this code didn't come from a screen in front of you; Lockdown only approves codes you scan yourself. Codes sent by message, email, link or paste are how attackers get sign-ins approved",
            );
        }
        if request.context.service.is_empty() {
            return refuse("the service's full domain must be shown before approval");
        }
        if let Some(nm) = &request.number_match
            && nm.entered != Some(nm.expected)
        {
            return refuse("the number you entered doesn't match the one on the sign-in screen");
        }
        if !self.limit.allow(now) {
            return refuse(
                "too many sign-in prompts in a short time; if you didn't start these, choose \"this wasn't me\"",
            );
        }
        ApprovalDecision::Prompt
    }
}

/// The person says a prompt was not theirs. Recorded locally for now; the
/// server-side report lands with the server (03-architecture §5).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NotMeReport {
    /// The request that was refused.
    pub request: ApprovalRequest,
    /// When, in seconds.
    pub reported_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_code(source: CodeSource) -> ApprovalRequest {
        ApprovalRequest {
            kind: ApprovalKind::DeviceCode,
            source,
            number_match: None,
            context: ApprovalContext {
                service: "microsoft.com".into(),
                account: "me@example.com".into(),
                device: Some("Xbox".into()),
                location: None,
            },
        }
    }

    #[test]
    fn a_pasted_device_code_is_refused_by_the_proximity_rule() {
        let mut svc = ApprovalService::default();
        for src in [
            CodeSource::Pasted,
            CodeSource::Link,
            CodeSource::Message,
            CodeSource::Image,
        ] {
            let d = svc.evaluate(&device_code(src), 0);
            assert!(
                matches!(&d, ApprovalDecision::Refused { reason } if reason.contains("screen in front of you")),
                "{src:?}: {d:?}"
            );
        }
        assert_eq!(
            svc.evaluate(&device_code(CodeSource::ScannedFromScreen), 0),
            ApprovalDecision::Prompt
        );
    }

    #[test]
    fn number_matching_and_rate_limit() {
        let mut svc = ApprovalService::new(RateLimit::new(2, 60));
        let mut req = device_code(CodeSource::ScannedFromScreen);
        req.kind = ApprovalKind::PasskeyQr;
        req.number_match = Some(NumberMatch {
            expected: 42,
            entered: Some(41),
        });
        assert!(matches!(
            svc.evaluate(&req, 0),
            ApprovalDecision::Refused { .. }
        ));
        req.number_match = Some(NumberMatch {
            expected: 42,
            entered: Some(42),
        });
        assert_eq!(svc.evaluate(&req, 0), ApprovalDecision::Prompt);
        assert_eq!(svc.evaluate(&req, 1), ApprovalDecision::Prompt);
        assert!(matches!(
            svc.evaluate(&req, 2),
            ApprovalDecision::Refused { reason } if reason.contains("too many")
        ));
        // The window rolls over.
        assert_eq!(svc.evaluate(&req, 61), ApprovalDecision::Prompt);
    }
}
