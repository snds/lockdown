//! Approving a TV, console or other device's sign-in from Lockdown
//! (docs/plan/26-desktop-finish.md §3; brief §2.7; 04-security §12): the
//! scan, the proximity gate, the signed allow-list, the person's yes, the
//! real page.
//!
//! 1. **Read.** A camera frame or an image file comes in as grey pixels
//!    and is decoded here ([`App::scan_image`]; `rqrr`, nothing decoded in
//!    the webview); a paste comes in as text ([`App::scan_text`]). Either
//!    way the text is read as hostile by `lockdown_approval::activation`.
//! 2. **The gate first.** Whatever it is — a device code, a passkey QR, a
//!    Steam QR — the request goes through the one approval service before
//!    anything else: the proximity rule (only a code scanned by the camera
//!    from a screen in front of the person passes; a paste or an image
//!    file is refused, with no setting to change that), the full domain
//!    shown, and the rate limit. The same text scanned again within a
//!    few seconds is answered from the last outcome, so a camera that sees
//!    one code for a while doesn't spend the rate limit.
//! 3. **The list.** A device code must be on a page of the signed
//!    allow-list, and the address opened is rebuilt from the list.
//! 4. **The person's yes.** [`App::activation_approve`] runs the `Approve`
//!    gate (Touch ID, Windows Hello, the account password, or the window's
//!    confirm, D101), counts a use of the login picked (D151), and asks the
//!    browser extension, through quick access's waiting fill, to offer that
//!    login first on the page; the shell opens the page. The fill is the
//!    extension's ordinary fill with its checks (D158). Nothing here signs
//!    anything in by itself: Lockdown stays on the browser side (brief §2.7).
//!
//! A passkey QR and a Steam QR stop after the gate with a sentence: the
//! hybrid flow needs the phone's Bluetooth link to the computer showing the
//! QR, and Steam approval is parked (D8).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lockdown_approval::activation::{ActivationPages, Scanned, classify};
use lockdown_approval::{
    ApprovalContext, ApprovalDecision, ApprovalRequest, ApprovalService, CodeSource,
};
use serde::{Deserialize, Serialize};

use crate::{App, AppError, AppResult, ApprovalKind, GateInput, quick::PendingFill};

/// The largest image read for a code, per side.
pub const MAX_SIDE: u32 = 4096;
/// How long an activation waits for the person's yes.
pub const ACTIVATION_WAIT: Duration = Duration::from_secs(300);
/// How long the same text scanned again is answered from the last outcome.
const SAME_SCAN: Duration = Duration::from_secs(20);

/// Where a scanned or pasted text came from: the proximity gate's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanSource {
    /// The camera, pointed at a screen in front of the person.
    Camera,
    /// An image file (a photo or a screenshot).
    Image,
    /// Typed or pasted.
    Pasted,
}

impl ScanSource {
    fn code_source(self) -> CodeSource {
        match self {
            Self::Camera => CodeSource::ScannedFromScreen,
            Self::Image => CodeSource::Image,
            Self::Pasted => CodeSource::Pasted,
        }
    }
}

/// A login that could sign in on the activation page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActivationAccount {
    pub item_id: String,
    pub title: String,
    pub username: String,
}

/// What a scan or a paste was, and what happens next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScanOutcome {
    /// Whether the image held a code at all (always `true` for text).
    pub found: bool,
    /// `device_code`, `passkey_qr`, `steam_qr`, `unlisted`, `other`, or
    /// `none` when an image held no code.
    pub kind: String,
    /// `approve` (the person may say yes), `refused` (the reason says
    /// why) or `unsupported` (the gate passed; this computer can't finish it).
    pub decision: String,
    /// Why, in the person's words, when not `approve`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// For a device code: the service, its full domain and the code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Whether the page opens with the code in it; else the person types it.
    pub code_in_url: bool,
    /// The activation waiting for the person's yes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_id: Option<String>,
    /// The logins that fit the page, best first; never a crown jewel or an
    /// item kept to the app (they never fill in a browser).
    pub accounts: Vec<ActivationAccount>,
}

impl ScanOutcome {
    fn none() -> Self {
        Self {
            found: false,
            kind: "none".into(),
            decision: "refused".into(),
            reason: Some("No code was found. Hold the code steady and closer.".into()),
            service: None,
            host: None,
            code: None,
            code_in_url: false,
            activation_id: None,
            accounts: Vec::new(),
        }
    }

    fn of(kind: &str, decision: &str, reason: Option<String>) -> Self {
        Self {
            found: true,
            kind: kind.into(),
            decision: decision.into(),
            reason,
            ..Self::none()
        }
    }
}

/// An approved activation: what the shell opens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActivationStarted {
    pub url: String,
    pub host: String,
    pub service: String,
    /// The code, for the shell to show large when the page doesn't take it.
    pub code: String,
    pub code_in_url: bool,
}

#[derive(Clone, Debug)]
struct Pending {
    url: String,
    host: String,
    service: String,
    code: String,
    code_in_url: bool,
    at: Instant,
}

/// The scan flow's state: the approval service (its rate limit outlives a
/// lock, so locking and unlocking doesn't reset it), the last outcome, and
/// activations waiting for a yes (dropped on lock).
pub(crate) struct ScanState {
    service: ApprovalService,
    last: Option<(lockdown_crypto::Hash32, Instant, ScanOutcome)>,
    pending: BTreeMap<String, Pending>,
    started: Instant,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            service: ApprovalService::default(),
            last: None,
            pending: BTreeMap::new(),
            started: Instant::now(),
        }
    }
}

impl ScanState {
    pub(crate) fn forget(&mut self) {
        self.pending.clear();
        self.last = None;
    }
}

/// Decodes the first QR code in a grey image (`luma` is `width × height`
/// bytes, row by row); `None` when there is none or the image is malformed.
pub fn decode_qr(width: u32, height: u32, luma: &[u8]) -> Option<String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    if luma.len() != w * h {
        return None;
    }
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| luma[y * w + x]);
    img.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, text)| text))
}

/// The prefix of a contact's QR code (D190).
pub const CONTACT_QR_PREFIX: &str = "lockdown:contact:v1:";

/// A contact's QR code read from a frame (the contacts screen's scan):
/// its text when the image holds one, else `None`. A contact's code is a
/// fingerprint, not a secret; `verify_contact` checks it against the pin.
pub fn contact_qr(width: u32, height: u32, luma: &[u8]) -> Option<String> {
    decode_qr(width, height, luma).filter(|t| t.starts_with(CONTACT_QR_PREFIX) && t.len() < 512)
}

impl App {
    /// A camera frame or an image file, as grey pixels: decoded here, then
    /// read as [`App::scan_text`] reads text.
    pub fn scan_image(
        &self,
        width: u32,
        height: u32,
        luma: Vec<u8>,
        source: ScanSource,
    ) -> AppResult<ScanOutcome> {
        if source == ScanSource::Pasted {
            return Err(AppError::refused(
                "an image comes from the camera or a file",
            ));
        }
        match decode_qr(width, height, &luma) {
            Some(text) => self.scan_text(&text, source),
            None => {
                // Still locked-checked, as every scan is.
                self.call(|core| core.session().map(|_| ScanOutcome::none()))
            }
        }
    }

    /// What `text` (a QR code's content, or a paste) is, through the
    /// proximity gate and the signed list; a device code that passes waits
    /// for the person's yes under its `activation_id`.
    pub fn scan_text(&self, text: &str, source: ScanSource) -> AppResult<ScanOutcome> {
        let text = text.to_string();
        let scanned = classify(&text, ActivationPages::builtin());
        let accounts = match &scanned {
            Scanned::DeviceCode(d) => self.accounts_for(&d.url)?,
            _ => Vec::new(),
        };
        self.call(move |core| {
            core.session()?;
            let now = Instant::now();
            let key = lockdown_crypto::hash::hash(format!("{source:?}\0{text}").as_bytes());
            if let Some((k, at, outcome)) = &core.scan.last
                && *k == key
                && now.duration_since(*at) < SAME_SCAN
                && outcome.decision != "approve"
            {
                return Ok(outcome.clone());
            }
            let outcome = core.scan_decide(scanned, source, accounts, now);
            core.scan.last = Some((key, now, outcome.clone()));
            Ok(outcome)
        })
    }

    /// The logins that fit an activation page, best first.
    fn accounts_for(&self, url: &str) -> AppResult<Vec<ActivationAccount>> {
        Ok(self
            .page_matches(url)?
            .logins
            .into_iter()
            .filter(|l| !l.crown_jewel && l.password)
            .map(|l| ActivationAccount {
                item_id: l.item_id,
                title: l.title,
                username: l.username,
            })
            .collect())
    }

    /// The person's yes to an activation: the `Approve` gate, then the
    /// page to open. With `item_id`, that login is the one the browser
    /// extension offers first on the page, and the approval counts as its
    /// use; without, the person signs in by hand.
    pub fn activation_approve(
        &self,
        activation_id: &str,
        item_id: Option<&str>,
        gate: GateInput,
    ) -> AppResult<ActivationStarted> {
        let id = activation_id.to_string();
        let item = item_id.map(str::to_string);
        let (pending, item) = self.call(move |core| {
            core.scan
                .pending
                .retain(|_, p| p.at.elapsed() < ACTIVATION_WAIT);
            let pending =
                core.scan.pending.get(&id).cloned().ok_or_else(|| {
                    AppError::refused("that sign-in code has expired; scan it again")
                })?;
            let reason = match &item {
                Some(_) => format!("sign a device in to {} ({})", pending.service, pending.host),
                None => format!("open {} for a device's sign-in", pending.host),
            };
            core.approve(ApprovalKind::Approve, &reason, &gate)?;
            core.scan.pending.remove(&id);
            Ok((pending, item))
        })?;
        if let Some(item) = &item {
            // Only a login that fits the page, as the scan listed it.
            if !self
                .accounts_for(&pending.url)?
                .iter()
                .any(|a| &a.item_id == item)
            {
                return Err(AppError::refused(
                    "that login isn't one for this page; pick one Lockdown listed",
                ));
            }
            let fill = PendingFill {
                item_id: item.clone(),
                host: pending.host.clone(),
                at: Instant::now(),
            };
            self.call(move |core| {
                core.quick_fills.retain(|p| p.item_id != fill.item_id);
                core.quick_fills.push(fill);
                Ok(())
            })?;
            self.note_use(item)?;
        }
        Ok(ActivationStarted {
            url: pending.url,
            host: pending.host,
            service: pending.service,
            code: pending.code,
            code_in_url: pending.code_in_url,
        })
    }

    /// The person said no, or closed the sheet: the activation is gone.
    pub fn activation_cancel(&self, activation_id: &str) -> AppResult<()> {
        let id = activation_id.to_string();
        self.call(move |core| {
            core.scan.pending.remove(&id);
            Ok(())
        })
    }
}

impl crate::Core {
    fn scan_decide(
        &mut self,
        scanned: Scanned,
        source: ScanSource,
        accounts: Vec<ActivationAccount>,
        now: Instant,
    ) -> ScanOutcome {
        let kind = match &scanned {
            Scanned::DeviceCode(_) => "device_code",
            Scanned::PasskeyQr => "passkey_qr",
            Scanned::SteamQr => "steam_qr",
            Scanned::Unlisted { .. } => "unlisted",
            Scanned::Other => "other",
        };
        let Some(approval_kind) = scanned.kind() else {
            let reason = match &scanned {
                Scanned::Unlisted { host } if !host.is_empty() => format!(
                    "{host} isn't an activation page Lockdown knows. Lockdown opens only the pages on its signed list, so a code can't send you somewhere else."
                ),
                _ => "This isn't a sign-in code Lockdown approves. Scan the code a TV or console shows when it asks you to sign in.".to_string(),
            };
            return ScanOutcome::of(kind, "refused", Some(reason));
        };
        let service = match &scanned {
            Scanned::DeviceCode(d) => d.host.clone(),
            Scanned::SteamQr => "s.team".into(),
            _ => "a passkey sign-in on another device".into(),
        };
        let request = ApprovalRequest {
            kind: approval_kind,
            source: source.code_source(),
            number_match: None,
            context: ApprovalContext {
                service,
                account: String::new(),
                device: None,
                location: None,
            },
        };
        let secs = now.duration_since(self.scan.started).as_secs();
        if let ApprovalDecision::Refused { reason } = self.scan.service.evaluate(&request, secs) {
            let mut out = ScanOutcome::of(kind, "refused", Some(capitalised(&reason)));
            if let Scanned::DeviceCode(d) = &scanned {
                out.service = Some(d.service.clone());
                out.host = Some(d.host.clone());
            }
            return out;
        }
        match scanned {
            Scanned::DeviceCode(d) => {
                self.scan
                    .pending
                    .retain(|_, p| p.at.elapsed() < ACTIVATION_WAIT);
                let id = lockdown_vault::Id::new().to_string();
                self.scan.pending.insert(
                    id.clone(),
                    Pending {
                        url: d.url.clone(),
                        host: d.host.clone(),
                        service: d.service.clone(),
                        code: d.code.clone(),
                        code_in_url: d.code_in_url,
                        at: now,
                    },
                );
                ScanOutcome {
                    found: true,
                    kind: kind.into(),
                    decision: "approve".into(),
                    reason: None,
                    service: Some(d.service),
                    host: Some(d.host),
                    code: Some(d.code),
                    code_in_url: d.code_in_url,
                    activation_id: Some(id),
                    accounts,
                }
            }
            Scanned::PasskeyQr => ScanOutcome::of(
                kind,
                "unsupported",
                Some("This is a passkey sign-in from another device. Finishing it needs a phone's Bluetooth link to the computer showing the code, which Lockdown on this computer doesn't have yet; use your phone's camera, or sign in with a passkey Lockdown holds on that computer.".into()),
            ),
            _ => ScanOutcome::of(
                kind,
                "unsupported",
                Some("This is Steam's sign-in code. Lockdown doesn't approve Steam sign-ins: it shows your Steam Guard codes, and Steam's own app approves these.".into()),
            ),
        }
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str() + ".",
        None => String::new(),
    }
}
