//! Passkeys made and used through the core (docs/plan/19-passkeys.md):
//! Lockdown as the authenticator for the browser extension, with the
//! credential on the Login it belongs to and the private key never out of
//! this crate.
//!
//! - **Who asks** is the browser's record of the tab (`PasskeyPage`), and
//!   the RP ID is checked against it ([`lockdown_passkey::rp`]); only the
//!   page itself answers, not a frame inside it.
//! - **Presence** is a deliberate action in Lockdown's own frame, measured
//!   as a fill is: making a passkey takes one, signing in takes the second
//!   deliberate action `fill_check` asks of every passkey (04-security §12).
//! - **Verification** goes through the `Approve` gate (D101):
//!   `userVerification: "required"` runs it in full (the platform check,
//!   or the account password in the app's window, never a page; with
//!   neither, the request is refused rather than pretending);
//!   `"preferred"` runs the platform check where one is installed and is
//!   presence only otherwise; `"discouraged"` is presence only. The UV flag
//!   is set only when a real check ran.
//! - **Signals** from the site hide a passkey, never delete it, with undo
//!   (`passkey_restore`); a name change is a new version, so history keeps
//!   the old one.
//! - **Conditional create** (the automatic upgrade after a password
//!   sign-in) happens only when this core itself filled that login's
//!   password into that site within the last five minutes, or saw that
//!   login's saved password typed there (D312). It stays silent at the
//!   moment, and is said afterwards (D210 and D312 as changed by D375):
//!   the passkey carries `upgraded` (the site and when) on the item
//!   version, the item says so until it is opened once on this device
//!   ([`PasskeyView::upgrade`], [`App::passkey_upgrade_seen`]), and the
//!   shell toasts each one it hears of ([`App::passkey_upgrades`]).

use std::time::{Duration, Instant};

use lockdown_matcher::related_origins::RelatedOriginsDoc;
use lockdown_matcher::{FillContext, FillKind, FillVerdict, MatchDecision, Origin, fill_check};
use lockdown_passkey::encode::{b64url, from_b64url};
use lockdown_passkey::{self as pk, Caller, Ceremony, ClientData, Flags, RefusalName, rp_id_for};
use lockdown_vault::{FieldValue, Id, ItemClass, ItemKind, ItemState};
use serde::{Deserialize, Serialize};

use crate::approval::{self, Route};
use crate::session::passkeys::{self as store, Found, NewPasskey};
use crate::{App, AppError, AppResult, ApprovalKind, Core, GateInput};

pub use lockdown_passkey::{Refusal, RefusalName as RefusalKind};

/// How long after a password fill a site may upgrade that login to a
/// passkey without asking (conditional create).
pub const UPGRADE_WINDOW: Duration = Duration::from_secs(5 * 60);

/// The most password fills remembered for the upgrade rule.
const MAX_RECENT_FILLS: usize = 32;

/// The most upgrade notices held for the shell between its looks.
const MAX_UPGRADE_NOTICES: usize = 16;

/// Bytes that cross as base64url text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct B64(pub Vec<u8>);

impl Serialize for B64 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&b64url(&self.0))
    }
}

impl<'de> Deserialize<'de> for B64 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        from_b64url(&text)
            .map(B64)
            .ok_or_else(|| serde::de::Error::custom("not base64url"))
    }
}

/// How much the site wants the person verified (WebAuthn
/// `userVerification`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserVerification {
    Required,
    #[default]
    Preferred,
    Discouraged,
}

/// What the browser knows about the asking page, and what the person did
/// in Lockdown's frame.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PasskeyPage {
    /// The tab's top-level address, from the browser.
    pub top: String,
    /// The asking frame's address, from the browser.
    pub frame: String,
    /// Lockdown's frame (or the field, for the inline menu) was visible and
    /// uncovered when the person acted.
    #[serde(default)]
    pub visible: bool,
    /// The person took the deliberate action this step needs.
    #[serde(default)]
    pub user_confirmed: bool,
    /// The RP's `/.well-known/webauthn`, when the page named an RP ID it
    /// isn't under and the extension fetched it.
    #[serde(default)]
    pub related: Option<RelatedOriginsDoc>,
}

/// `navigator.credentials.create({publicKey})`, as the extension passes it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CreateRequest {
    pub rp_id: Option<String>,
    pub rp_name: String,
    pub user_id: B64,
    pub user_name: String,
    pub user_display_name: String,
    pub challenge: B64,
    /// `pubKeyCredParams[].alg`, in order.
    pub algs: Vec<i64>,
    pub exclude: Vec<B64>,
    pub user_verification: UserVerification,
    /// The `credProps` extension was asked for.
    pub cred_props: bool,
    /// `mediation: "conditional"`: the automatic upgrade.
    pub conditional: bool,
}

/// `navigator.credentials.get({publicKey})`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct GetRequest {
    pub rp_id: Option<String>,
    pub challenge: B64,
    pub allow: Vec<B64>,
    pub user_verification: UserVerification,
}

/// One passkey the page may be offered: no secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PasskeyOption {
    pub item_id: String,
    pub credential_id: B64,
    pub title: String,
    pub user_name: String,
    pub user_display_name: String,
}

/// The passkeys for a get, under the RP ID the request runs under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PasskeyOptions {
    pub rp_id: String,
    pub options: Vec<PasskeyOption>,
}

/// Where a new passkey would go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatePlan {
    pub rp_id: String,
    /// The vault already holds one of `excludeCredentials`.
    pub excluded: bool,
    /// The Login it joins, when one fits (same site, same user name).
    pub item_id: Option<String>,
    /// That Login's title, or the new Login's.
    pub title: String,
    pub user_name: String,
}

/// A passkey just made: all public, for the page.
#[derive(Clone, Debug, Serialize)]
pub struct Created {
    pub item_id: String,
    pub credential_id: B64,
    pub client_data_json: B64,
    pub attestation_object: B64,
    pub authenticator_data: B64,
    /// SubjectPublicKeyInfo DER.
    pub public_key: B64,
    pub public_key_algorithm: i64,
    pub transports: Vec<&'static str>,
    pub user_verified: bool,
    /// `credProps.rk`, when asked for: always discoverable.
    pub resident_key: Option<bool>,
}

/// A sign-in: all public, for the page.
#[derive(Clone, Debug, Serialize)]
pub struct Asserted {
    pub item_id: String,
    pub credential_id: B64,
    pub client_data_json: B64,
    pub authenticator_data: B64,
    pub signature: B64,
    pub user_handle: B64,
    pub user_verified: bool,
}

/// The core's answer to a ceremony.
#[derive(Debug)]
pub enum Outcome<T> {
    /// Done.
    Done(T),
    /// Ask the person for the second deliberate action first.
    Confirm(String),
    /// Not done; the page gets this DOMException, the person these words.
    Refused(Refusal),
}

/// A Signal from the site (WebAuthn L3 §5.1.10).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Signal {
    /// `signalUnknownCredential`: the site doesn't know this credential.
    UnknownCredential { rp_id: String, credential_id: B64 },
    /// `signalAllAcceptedCredentials`: these are all the site accepts for
    /// this user.
    AllAcceptedCredentials {
        rp_id: String,
        user_id: B64,
        all_accepted_credential_ids: Vec<B64>,
    },
    /// `signalCurrentUserDetails`: the account's current names.
    CurrentUserDetails {
        rp_id: String,
        user_id: B64,
        name: String,
        display_name: String,
    },
}

/// A passkey as the management view shows it: no secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PasskeyView {
    pub credential_id: B64,
    pub rp_id: String,
    pub user_name: String,
    pub user_display_name: String,
    /// The provider that made it, in words ("Lockdown", "Bitwarden",
    /// "Another provider"), from its AAGUID.
    pub provider: String,
    /// The AAGUID, when the passkey carries one.
    pub aaguid: Option<String>,
    /// `ES256 (P-256)`, `EdDSA (Ed25519)`, or why the key can't be read.
    pub algorithm: String,
    /// BE: the passkey may sync.
    pub backup_eligible: bool,
    /// BS: it does.
    pub backed_up: bool,
    /// Seconds since the epoch, when made here.
    pub created: Option<i64>,
    /// When this device last signed in with it (never synced).
    pub last_used: Option<i64>,
    /// Hidden by the site's Signal: why, in words, and when.
    pub hidden: Option<HiddenView>,
    /// Deleting it would leave this account with no way in that Lockdown
    /// holds (no password, no other passkey).
    pub only_sign_in: bool,
    /// The site added it by itself after a password sign-in (the automatic
    /// upgrade), and the item hasn't been opened on this device since.
    /// Added under D62.
    pub upgrade: Option<UpgradeView>,
}

/// An automatic upgrade the item still has to tell the person about:
/// "Added by github.com on 29 Oct 2026, after a password sign-in".
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpgradeView {
    /// The site that added it (the passkey's RP ID).
    pub site: String,
    /// When, in seconds since the epoch.
    pub at: i64,
}

/// One automatic upgrade made here, for the shell's toast ("github.com
/// added a passkey to this login", with Show). Nothing secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpgradeNotice {
    /// The login it joined, for Show.
    pub item_id: String,
    /// That login's title.
    pub title: String,
    /// The site that added it (the passkey's RP ID).
    pub site: String,
    /// When, in seconds since the epoch.
    pub at: i64,
}

/// A Signal's hide, for the badge and its Undo.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HiddenView {
    pub reason: String,
    pub at: i64,
}

/// One change to an item's fields, decided before the item is opened for it.
type FieldEdit = Box<dyn FnOnce(&mut Vec<lockdown_vault::PlainField>) -> Result<bool, String>>;

/// A password fill this core made, for the upgrade rule.
pub(crate) struct RecentFill {
    pub item_id: String,
    pub host: String,
    pub at: Instant,
}

fn refused<T>(name: RefusalName, message: impl Into<String>) -> AppResult<Outcome<T>> {
    Ok(Outcome::Refused(Refusal::new(name, message)))
}

/// The caller, refused unless the asking frame is the page itself.
fn caller_of(page: &PasskeyPage) -> Result<Caller, Refusal> {
    let caller = Caller::of(&page.frame)?;
    let top = Caller::of(&page.top).ok();
    if top.as_ref().map(|t| &t.origin) != Some(&caller.origin) {
        return Err(Refusal::new(
            RefusalName::NotAllowed,
            "Lockdown answers passkey requests from the page itself, not from a frame inside it",
        ));
    }
    Ok(caller)
}

/// The words a Signal's hide shows.
fn hidden_words(reason: &str) -> String {
    match reason {
        "unknown" => "The site said it no longer knows this passkey".into(),
        _ => "The site no longer lists this passkey for the account".into(),
    }
}

fn now() -> i64 {
    crate::session::now_secs()
}

impl Core {
    /// Notes a password fill (from `browser::fill`) for the upgrade rule.
    pub(crate) fn note_password_fill(&mut self, item_id: &str, frame: &str) {
        let Ok(o) = Origin::parse(frame) else { return };
        self.recent_fills
            .retain(|f| f.at.elapsed() < UPGRADE_WINDOW && f.item_id != item_id);
        self.recent_fills.push(RecentFill {
            item_id: item_id.to_string(),
            host: o.host,
            at: Instant::now(),
        });
        if self.recent_fills.len() > MAX_RECENT_FILLS {
            self.recent_fills.remove(0);
        }
    }

    /// User verification for one ceremony (see the module doc). `verified`
    /// is the method the shell already approved this request with in the
    /// app's window. `Ok(Ok(uv))` goes ahead with the UV flag `uv`.
    fn passkey_uv(
        &mut self,
        wanted: UserVerification,
        reason: &str,
        verified: Option<&str>,
    ) -> AppResult<Result<bool, Refusal>> {
        if let Some(method) = verified {
            return Ok(Ok(method != "window"));
        }
        let platform = self.approver.is_some();
        let platform_check = |core: &mut Core| match core.approve(
            ApprovalKind::Approve,
            reason,
            &GateInput::default(),
        ) {
            Ok(_) => Ok(Ok(true)),
            Err(e) if e.code == crate::ErrorCode::Refused => {
                Ok(Err(Refusal::new(RefusalName::NotAllowed, e.message)))
            }
            Err(e) => Err(e),
        };
        match wanted {
            UserVerification::Discouraged => Ok(Ok(false)),
            UserVerification::Preferred if platform => platform_check(self),
            UserVerification::Preferred => Ok(Ok(false)),
            UserVerification::Required => {
                let has_account = self.session()?.has_account();
                match approval::route(ApprovalKind::Approve, platform, has_account) {
                    Route::Platform => platform_check(self),
                    Route::Password => Err(AppError::approval_needed(Route::Password, reason)),
                    Route::Window => Ok(Err(Refusal::new(
                        RefusalName::NotAllowed,
                        "this site asks Lockdown to check it's you, and this computer has neither a fingerprint or face check nor a master password to check with",
                    ))),
                }
            }
        }
    }

    /// The Login a new passkey for `user_name` at `caller` joins: a live,
    /// everyday Login with that user name whose saved address is the RP's
    /// site or matches the page, most recently edited first.
    fn create_target(
        &mut self,
        caller: &Caller,
        rp_id: &str,
        user_name: &str,
    ) -> AppResult<Option<(Id, String, String)>> {
        let page = Origin::parse(&caller.origin).map_err(|e| AppError::refused(e.to_string()))?;
        let data = crate::browser::matcher_data()?;
        let session = self.session()?;
        let mut found: Vec<(i64, Id, String, String)> = Vec::new();
        for vault in session.vaults() {
            for item in vault.items() {
                if item.kind != ItemKind::Login
                    || item.state != ItemState::Live
                    || item.class == ItemClass::CrownJewel
                {
                    continue;
                }
                let fields = vault
                    .open_item(item.item_id)
                    .map_err(|e| AppError::refused(e.to_string()))?;
                let user = crate::browser::text(&fields, "user").unwrap_or_default();
                if !user.eq_ignore_ascii_case(user_name) {
                    continue;
                }
                let urls = crate::browser::saved_urls(&fields);
                let of_rp = urls.iter().any(|(u, _)| {
                    Origin::parse(u)
                        .is_ok_and(|o| o.host == rp_id || o.host.ends_with(&format!(".{rp_id}")))
                });
                let (decision, _) = crate::browser::best_decision(&urls, &page, &data);
                if !of_rp && !crate::browser::is_match(decision) {
                    continue;
                }
                let edited = item
                    .latest()
                    .map(|v| v.edited_at.0)
                    .map_err(|e| AppError::refused(e.to_string()))?;
                let title = crate::browser::text(&fields, "title").unwrap_or_default();
                found.push((edited, item.item_id, title, user));
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        Ok(found.into_iter().next().map(|(_, id, t, u)| (id, t, u)))
    }

    fn excluded(&mut self, rp_id: &str, exclude: &[B64]) -> AppResult<bool> {
        if exclude.is_empty() {
            return Ok(false);
        }
        let all = self
            .session()?
            .passkeys_for(rp_id)
            .map_err(AppError::refused)?;
        Ok(all
            .iter()
            .any(|f| exclude.iter().any(|e| e.0 == f.passkey.credential_id)))
    }

    fn passkey_create(
        &mut self,
        page: &PasskeyPage,
        req: &CreateRequest,
        verified: Option<&str>,
    ) -> AppResult<Outcome<Created>> {
        self.clock.touch(Instant::now());
        let caller = match caller_of(page) {
            Ok(c) => c,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        let rp_id = match rp_id_for(&caller, req.rp_id.as_deref(), page.related.as_ref()) {
            Ok(r) => r,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        if req.user_id.0.is_empty() || req.user_id.0.len() > pk::MAX_USER_HANDLE {
            return refused(
                RefusalName::Type,
                "the site's user id must be 1 to 64 bytes",
            );
        }
        if req.challenge.0.is_empty() {
            return refused(RefusalName::Type, "the site sent no challenge");
        }
        if let Err(r) = pk::choose_alg(&req.algs) {
            return Ok(Outcome::Refused(r));
        }
        // The Login it joins: for an upgrade, only one this core just filled
        // on this site, or whose saved password it just saw typed here
        // (`browser::save_proposal`); otherwise the one that fits.
        let target = if req.conditional {
            let recent: Vec<String> = self
                .recent_fills
                .iter()
                .filter(|f| f.at.elapsed() < UPGRADE_WINDOW && f.host == caller.host)
                .map(|f| f.item_id.clone())
                .collect();
            let fits = self.create_target(&caller, &rp_id, &req.user_name)?;
            match fits {
                Some((id, t, u)) if recent.contains(&id.to_string()) => Some((id, t, u)),
                _ => {
                    return refused(
                        RefusalName::NotAllowed,
                        "an automatic passkey needs a sign-in Lockdown just saw for this account here",
                    );
                }
            }
        } else {
            if !page.visible {
                return refused(
                    RefusalName::NotAllowed,
                    "something on the page covers Lockdown, so Lockdown won't make a passkey",
                );
            }
            if !page.user_confirmed {
                return Ok(Outcome::Confirm(format!(
                    "save a passkey for {rp_id} in Lockdown?"
                )));
            }
            self.create_target(&caller, &rp_id, &req.user_name)?
        };
        if self.excluded(&rp_id, &req.exclude)? {
            return refused(
                RefusalName::InvalidState,
                "Lockdown already holds a passkey for this account",
            );
        }
        let user_verified = if req.conditional {
            false
        } else {
            match self.passkey_uv(
                req.user_verification,
                &format!("save a passkey for {rp_id}"),
                verified,
            )? {
                Ok(uv) => uv,
                Err(r) => return Ok(Outcome::Refused(r)),
            }
        };
        let client = ClientData {
            ceremony: Ceremony::Create,
            challenge: req.challenge.0.clone(),
            origin: caller.origin.clone(),
            top_origin: None,
        };
        let flags = Flags {
            up: !req.conditional,
            uv: user_verified,
            be: true,
            bs: true,
        };
        let made = match pk::make_credential(&pk::MakeCredential {
            rp_id: &rp_id,
            client_data: &client,
            flags,
            algs: &req.algs,
        }) {
            Ok(m) => m,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        let new = NewPasskey {
            rp_id: rp_id.clone(),
            user_name: req.user_name.clone(),
            user_display_name: req.user_display_name.clone(),
            credential_id: made.credential_id.clone(),
            user_handle: req.user_id.0.clone(),
            pkcs8: made.pkcs8.clone(),
            aaguid: pk::aaguid_text(&pk::LOCKDOWN_AAGUID),
            created: now(),
            backup_eligible: flags.be,
            backed_up: flags.bs,
            // Silent now, said afterwards (D210, D312 as changed by D375).
            upgraded: req.conditional.then(|| store::Upgraded {
                site: rp_id.clone(),
                at: now(),
            }),
        };
        let session = self.session()?;
        // A passkey for the same site and the same user handle replaces the
        // old one where it is (WebAuthn §6.3.2 step 7).
        let same = session
            .passkeys_for(&rp_id)
            .map_err(AppError::refused)?
            .into_iter()
            .find(|f| f.passkey.user_handle == req.user_id.0);
        let item_id = match same {
            // A crown jewel changes only through its gate, in the app.
            Some(f) if f.crown_jewel => {
                return refused(
                    RefusalName::NotAllowed,
                    "this account's passkey is in a protected login; replace it from the Lockdown app",
                );
            }
            Some(f) => {
                let n = f.passkey.n;
                session
                    .passkey_edit(f.item_id, |fields| {
                        store::remove(fields, n);
                        store::append(fields, &new);
                        Ok(true)
                    })
                    .map_err(AppError::refused)?;
                f.item_id
            }
            None => {
                let title = if req.rp_name.trim().is_empty() {
                    rp_id.clone()
                } else {
                    req.rp_name.trim().to_string()
                };
                session
                    .passkey_add(target.map(|t| t.0), &caller.origin, &title, &new)
                    .map_err(AppError::refused)?
            }
        };
        if let Some(u) = &new.upgraded {
            let title = session
                .passkeys_on(item_id)
                .ok()
                .and_then(|(_, f)| crate::browser::text(&f, "title"))
                .unwrap_or_else(|| rp_id.clone());
            self.passkey_upgrades.push(UpgradeNotice {
                item_id: item_id.to_string(),
                title,
                site: u.site.clone(),
                at: u.at,
            });
            if self.passkey_upgrades.len() > MAX_UPGRADE_NOTICES {
                self.passkey_upgrades.remove(0);
            }
        }
        Ok(Outcome::Done(Created {
            item_id: item_id.to_string(),
            credential_id: B64(made.credential_id),
            client_data_json: B64(made.client_data_json.into_bytes()),
            attestation_object: B64(made.attestation_object),
            authenticator_data: B64(made.authenticator_data),
            public_key: B64(made.public_key),
            public_key_algorithm: made.alg.cose(),
            transports: vec!["internal", "hybrid"],
            user_verified,
            resident_key: req.cred_props.then_some(true),
        }))
    }

    /// The passkeys a get may offer: live, everyday (a crown jewel is never
    /// offered to a page, 02-model §3), not hidden by a Signal, and in the
    /// allow list when the site sent one.
    fn passkey_offer(&mut self, rp_id: &str, allow: &[B64]) -> AppResult<Vec<Found>> {
        let all = self
            .session()?
            .passkeys_for(rp_id)
            .map_err(AppError::refused)?;
        let session = self.session()?;
        let kept_to_app = |id: lockdown_vault::Id| session.kept_to_app(id).unwrap_or(true);
        Ok(all
            .into_iter()
            .filter(|f| !f.crown_jewel && f.passkey.hidden.is_none() && !kept_to_app(f.item_id))
            .filter(|f| allow.is_empty() || allow.iter().any(|a| a.0 == f.passkey.credential_id))
            .collect())
    }

    fn passkey_assert(
        &mut self,
        page: &PasskeyPage,
        req: &GetRequest,
        item_id: &str,
        credential_id: &B64,
        verified: Option<&str>,
    ) -> AppResult<Outcome<Asserted>> {
        self.clock.touch(Instant::now());
        let caller = match caller_of(page) {
            Ok(c) => c,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        let rp_id = match rp_id_for(&caller, req.rp_id.as_deref(), page.related.as_ref()) {
            Ok(r) => r,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        if req.challenge.0.is_empty() {
            return refused(RefusalName::Type, "the site sent no challenge");
        }
        let offered = self.passkey_offer(&rp_id, &req.allow)?;
        let Some(chosen) = offered.into_iter().find(|f| {
            f.item_id.to_string() == item_id && f.passkey.credential_id == credential_id.0
        }) else {
            return refused(
                RefusalName::NotAllowed,
                "that passkey isn't one Lockdown offers to this site",
            );
        };
        // Presence: the second deliberate action, on a frame nothing covers.
        let origin = Origin::parse(&caller.origin).map_err(|e| AppError::refused(e.to_string()))?;
        let ctx = FillContext {
            top: origin.clone(),
            frame: origin,
            field_visible: page.visible,
            user_confirmed: page.user_confirmed,
            kind: FillKind::Passkey,
        };
        match fill_check(&ctx, MatchDecision::Exact) {
            FillVerdict::Fill => {}
            FillVerdict::Confirm(r) => return Ok(Outcome::Confirm(r)),
            FillVerdict::Refuse(r) => return refused(RefusalName::NotAllowed, r),
        }
        // Ask each time (`protection`): the sign-in needs the check, whatever
        // the site asked for.
        let asks = self
            .session()?
            .exposure_tier(chosen.item_id)
            .map_err(AppError::refused)?
            == lockdown_vault::exposure::Exposure::Ask;
        let wanted = if asks {
            UserVerification::Required
        } else {
            req.user_verification
        };
        let user_verified = match self.passkey_uv(
            wanted,
            &format!("sign in to {rp_id} with your passkey"),
            verified,
        )? {
            Ok(uv) => uv,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        let client = ClientData {
            ceremony: Ceremony::Get,
            challenge: req.challenge.0.clone(),
            origin: caller.origin.clone(),
            top_origin: None,
        };
        let flags = Flags {
            up: true,
            uv: user_verified,
            be: chosen.passkey.backup_eligible,
            bs: chosen.passkey.backed_up,
        };
        let session = self.session()?;
        let key = session
            .passkey_key(chosen.item_id, &chosen.passkey.credential_id)
            .map_err(AppError::refused)?;
        let assertion = match pk::get_assertion(&key, &rp_id, &client, flags) {
            Ok(a) => a,
            Err(r) => return Ok(Outcome::Refused(r)),
        };
        drop(key);
        // A sign-in is a use (D151); a failure to note it never fails it.
        let _ = session.note_passkey_use(chosen.item_id, &chosen.passkey.credential_id);
        Ok(Outcome::Done(Asserted {
            item_id: chosen.item_id.to_string(),
            credential_id: B64(chosen.passkey.credential_id),
            client_data_json: B64(assertion.client_data_json.into_bytes()),
            authenticator_data: B64(assertion.authenticator_data),
            signature: B64(assertion.signature),
            user_handle: B64(chosen.passkey.user_handle),
            user_verified,
        }))
    }

    fn passkey_signal(&mut self, page: &str, signal: &Signal) -> AppResult<Result<u32, Refusal>> {
        let caller = match Caller::of(page) {
            Ok(c) => c,
            Err(r) => return Ok(Err(r)),
        };
        let rp_named = match signal {
            Signal::UnknownCredential { rp_id, .. }
            | Signal::AllAcceptedCredentials { rp_id, .. }
            | Signal::CurrentUserDetails { rp_id, .. } => rp_id,
        };
        // A Signal never reaches past the page's own site (no related origins).
        let rp_id = match rp_id_for(&caller, Some(rp_named), None) {
            Ok(r) => r,
            Err(r) => return Ok(Err(r)),
        };
        let session = self.session()?;
        let all = session.passkeys_for(&rp_id).map_err(AppError::refused)?;
        let at = now();
        let mut changed = 0u32;
        // A crown jewel is never offered to a page, and a page's Signal
        // never changes one (its changes go through the gate in the app).
        for found in all.into_iter().filter(|f| !f.crown_jewel) {
            let p = &found.passkey;
            let edit: Option<FieldEdit> = match signal {
                Signal::UnknownCredential { credential_id, .. } => {
                    (p.credential_id == credential_id.0 && p.hidden.is_none()).then(|| {
                        let n = p.n;
                        Box::new(move |f: &mut Vec<_>| {
                            store::set_text(
                                f,
                                n,
                                "hidden",
                                "hidden by the site",
                                Some(&format!("unknown {at}")),
                            );
                            Ok(true)
                        }) as FieldEdit
                    })
                }
                Signal::AllAcceptedCredentials {
                    user_id,
                    all_accepted_credential_ids,
                    ..
                } if p.user_handle == user_id.0 => {
                    let listed = all_accepted_credential_ids
                        .iter()
                        .any(|c| c.0 == p.credential_id);
                    let n = p.n;
                    match (&p.hidden, listed) {
                        (None, false) => Some(Box::new(move |f: &mut Vec<_>| {
                            store::set_text(
                                f,
                                n,
                                "hidden",
                                "hidden by the site",
                                Some(&format!("not-accepted {at}")),
                            );
                            Ok(true)
                        })),
                        // Listed again: a hide by this same Signal comes undone.
                        (Some(h), true) if h.reason == "not-accepted" => {
                            Some(Box::new(move |f: &mut Vec<_>| {
                                store::set_text(f, n, "hidden", "", None);
                                Ok(true)
                            }))
                        }
                        _ => None,
                    }
                }
                Signal::CurrentUserDetails {
                    user_id,
                    name,
                    display_name,
                    ..
                } if p.user_handle == user_id.0
                    && (p.user_name != *name || p.user_display_name != *display_name) =>
                {
                    let (n, name, display) = (p.n, name.clone(), display_name.clone());
                    Some(Box::new(move |f: &mut Vec<_>| {
                        store::set_text(f, n, "user_name", "user name", Some(&name));
                        store::set_text(f, n, "user_display_name", "display name", Some(&display));
                        Ok(true)
                    }))
                }
                _ => None,
            };
            if let Some(edit) = edit
                && session
                    .passkey_edit(found.item_id, edit)
                    .map_err(AppError::refused)?
                    .is_some()
            {
                changed += 1;
            }
        }
        Ok(Ok(changed))
    }
}

/// The view of each passkey on an item's fields.
fn views_of(
    session: &crate::session::Session,
    item_id: Id,
    fields: &[lockdown_vault::PlainField],
) -> Vec<PasskeyView> {
    let groups = store::groups(fields);
    let has_password = fields
        .iter()
        .any(|f| f.field_id == "pw" && matches!(&f.value, FieldValue::Text(t) if !t.is_empty()));
    let method = crate::browser::text(fields, "sign_in_method").unwrap_or_default();
    let through_another = method == "Another account" || method == "Single sign-on";
    let visible = groups.iter().filter(|p| p.hidden.is_none()).count();
    groups
        .iter()
        .map(|p| {
            let key = fields
                .iter()
                .find(|f| f.field_id == format!("passkey.{}.key", p.n));
            let algorithm = match key.map(|k| &k.value) {
                Some(FieldValue::Bytes(b)) => match pk::public_key_of(b) {
                    Ok((alg, _)) => alg.name().to_string(),
                    Err(r) => r.message,
                },
                _ => "no key".into(),
            };
            let provider = p
                .aaguid
                .as_deref()
                .and_then(pk::provider_name)
                .map(str::to_string)
                .unwrap_or_else(|| match &p.aaguid {
                    Some(_) => "Another provider".into(),
                    None => "Another provider (imported)".into(),
                });
            let others = visible - usize::from(p.hidden.is_none());
            PasskeyView {
                credential_id: B64(p.credential_id.clone()),
                rp_id: p.rp_id.clone(),
                user_name: p.user_name.clone(),
                user_display_name: p.user_display_name.clone(),
                provider,
                aaguid: p.aaguid.clone(),
                algorithm,
                backup_eligible: p.backup_eligible,
                backed_up: p.backed_up,
                created: p.created,
                last_used: session.passkey_last_use(item_id, &p.credential_id),
                hidden: p.hidden.as_ref().map(|h| HiddenView {
                    reason: hidden_words(&h.reason),
                    at: h.at,
                }),
                only_sign_in: !has_password && !through_another && others == 0,
                upgrade: p.upgraded.as_ref().and_then(|u| {
                    let seen = session.passkey_upgrade_seen(item_id, &p.credential_id);
                    (seen.is_none_or(|at| at < u.at)).then(|| UpgradeView {
                        site: u.site.clone(),
                        at: u.at,
                    })
                }),
            }
        })
        .collect()
}

impl App {
    /// The passkeys a get at `page` may offer, under its RP ID. Nothing
    /// secret.
    pub fn passkey_options(
        &self,
        page: PasskeyPage,
        req: GetRequest,
    ) -> AppResult<Result<PasskeyOptions, Refusal>> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let caller = match caller_of(&page) {
                Ok(c) => c,
                Err(r) => return Ok(Err(r)),
            };
            let rp_id = match rp_id_for(&caller, req.rp_id.as_deref(), page.related.as_ref()) {
                Ok(r) => r,
                Err(r) => return Ok(Err(r)),
            };
            let options = core
                .passkey_offer(&rp_id, &req.allow)?
                .into_iter()
                .map(|f| PasskeyOption {
                    item_id: f.item_id.to_string(),
                    credential_id: B64(f.passkey.credential_id),
                    title: f.title,
                    user_name: f.passkey.user_name,
                    user_display_name: f.passkey.user_display_name,
                })
                .collect();
            Ok(Ok(PasskeyOptions { rp_id, options }))
        })
    }

    /// Where a create at `page` would put the passkey, and whether the
    /// vault already holds an excluded one. Nothing secret.
    pub fn passkey_plan(
        &self,
        page: PasskeyPage,
        req: CreateRequest,
    ) -> AppResult<Result<CreatePlan, Refusal>> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let caller = match caller_of(&page) {
                Ok(c) => c,
                Err(r) => return Ok(Err(r)),
            };
            let rp_id = match rp_id_for(&caller, req.rp_id.as_deref(), page.related.as_ref()) {
                Ok(r) => r,
                Err(r) => return Ok(Err(r)),
            };
            let excluded = core.excluded(&rp_id, &req.exclude)?;
            let target = core.create_target(&caller, &rp_id, &req.user_name)?;
            Ok(Ok(match target {
                Some((id, title, user)) => CreatePlan {
                    rp_id,
                    excluded,
                    item_id: Some(id.to_string()),
                    title,
                    user_name: user,
                },
                None => CreatePlan {
                    title: if req.rp_name.trim().is_empty() {
                        rp_id.clone()
                    } else {
                        req.rp_name.trim().to_string()
                    },
                    rp_id,
                    excluded,
                    item_id: None,
                    user_name: req.user_name,
                },
            }))
        })
    }

    /// Makes a passkey for `page` and stores it on its Login. `verified`
    /// is the method the shell approved this request with in its window
    /// after an [`crate::ErrorCode::ApprovalNeeded`]; `None` otherwise.
    pub fn passkey_create(
        &self,
        page: PasskeyPage,
        req: CreateRequest,
        verified: Option<String>,
    ) -> AppResult<Outcome<Created>> {
        self.call(move |core| core.passkey_create(&page, &req, verified.as_deref()))
    }

    /// Signs in at `page` with the passkey `credential_id` on `item_id`.
    pub fn passkey_get(
        &self,
        page: PasskeyPage,
        req: GetRequest,
        item_id: &str,
        credential_id: B64,
        verified: Option<String>,
    ) -> AppResult<Outcome<Asserted>> {
        let item_id = item_id.to_string();
        self.call(move |core| {
            core.passkey_assert(&page, &req, &item_id, &credential_id, verified.as_deref())
        })
    }

    /// Applies a Signal from `page`; answers how many passkeys changed.
    pub fn passkey_signal(&self, page: &str, signal: Signal) -> AppResult<Result<u32, Refusal>> {
        let page = page.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.passkey_signal(&page, &signal)
        })
    }

    /// The passkeys on one item, for the management view.
    pub fn passkeys(&self, item_id: &str) -> AppResult<Vec<PasskeyView>> {
        let item_id = item_id.to_string();
        self.with_session(move |s| {
            let id = s.passkey_item(&item_id)?;
            let (_, fields) = s.passkeys_on(id)?;
            Ok(views_of(s, id, &fields))
        })
    }

    /// Deletes one passkey from an item (a new version, so history keeps
    /// it); a crown jewel's through the `Use` gate, as any change to one.
    /// The shell warns first when the view says `only_sign_in`.
    pub fn passkey_delete(
        &self,
        item_id: &str,
        credential_id: B64,
        gate: GateInput,
    ) -> AppResult<u64> {
        let item_id = item_id.to_string();
        self.call(move |core| {
            core.gate_if_crown(
                &item_id,
                ApprovalKind::Use,
                "delete this protected passkey",
                &gate,
            )?;
            let session = core.session()?;
            let id = session.passkey_item(&item_id).map_err(AppError::refused)?;
            session
                .passkey_edit(id, |fields| {
                    let Some(p) = store::groups(fields)
                        .into_iter()
                        .find(|p| p.credential_id == credential_id.0)
                    else {
                        return Err("that passkey isn't on this item".into());
                    };
                    store::remove(fields, p.n);
                    Ok(true)
                })
                .map_err(AppError::refused)?
                .ok_or_else(|| AppError::refused("nothing changed"))
        })
    }

    /// Undoes a Signal's hide: the passkey is offered to the site again.
    pub fn passkey_restore(&self, item_id: &str, credential_id: B64) -> AppResult<u64> {
        let item_id = item_id.to_string();
        self.with_session(move |s| {
            let id = s.passkey_item(&item_id)?;
            s.passkey_edit(id, |fields| {
                let Some(p) = store::groups(fields)
                    .into_iter()
                    .find(|p| p.credential_id == credential_id.0)
                else {
                    return Err("that passkey isn't on this item".into());
                };
                if p.hidden.is_none() {
                    return Ok(false);
                }
                store::set_text(fields, p.n, "hidden", "", None);
                Ok(true)
            })?
            .ok_or_else(|| "that passkey isn't hidden".to_string())
        })
    }

    /// The automatic upgrades made here since the last call, oldest first,
    /// for the shell's toast; each is handed out once. The item says so
    /// too, until it is opened ([`PasskeyView::upgrade`]).
    pub fn passkey_upgrades(&self) -> AppResult<Vec<UpgradeNotice>> {
        self.call(|core| Ok(std::mem::take(&mut core.passkey_upgrades)))
    }

    /// The person opened `item_id`: the notice of each automatic upgrade
    /// on it shows no more on this device. Answers how many it closed.
    pub fn passkey_upgrade_seen(&self, item_id: &str) -> AppResult<u32> {
        let item_id = item_id.to_string();
        self.with_session(move |s| {
            let id = s.passkey_item(&item_id)?;
            s.note_upgrades_seen(id)
        })
    }

    /// Whether this app offers itself as the browser's passkey provider:
    /// the person's setting, and no Lockdown OS provider on this machine
    /// (none exists yet on any platform, D122).
    pub fn passkeys_in_browser(&self) -> AppResult<bool> {
        self.call(|core| Ok(core.settings.browser_passkeys))
    }
}
