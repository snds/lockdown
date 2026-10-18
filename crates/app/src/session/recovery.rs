//! The recovery drill and the stretching around it (`recovery-kit`,
//! `recovery-drill`, `client-kdf`; 04-security §4, §9;
//! docs/plan/22-security-depth.md §5–§6).
//!
//! - **The last good drill** is a date on the identity's signed profile
//!   (`lockdown.drilled_at`, a Lockdown field the identity crate keeps as
//!   read), so every device of the identity knows it and the yearly prompt
//!   comes once for all of them.
//! - **The kit's stretching rises** when the recovery code is in hand (a
//!   new kit, a drill): this device calibrates Argon2id at or above the
//!   profile's signed parameters to about a second, and when that is more,
//!   signs the raised set into the profile and wraps the root key to the kit
//!   again under it. Parameters only ever rise; a reader refuses less (D32).
//! - **The account's stretching rises** when a stronger device signs in
//!   (D49's renegotiation): the password file is made again on the server
//!   under parameters this device calibrated, the account slot is written
//!   again under the new export key with the Secret Key this device keeps,
//!   and the parameters are signed into the profile
//!   (`lockdown.account_ksf`) so every device of the identity refuses a
//!   server that later announces less.
//! - **The drill's checks** look past the kit: the Secret Key kept here,
//!   and each emergency contact (still accepted, keys unchanged in the
//!   key-transparency log, fingerprint compared), and which crown jewels the
//!   contacts would get.

use std::time::Duration;

use ecosystem_identity::RecoveryKit;
use lockdown_crypto::Argon2Params;
use lockdown_crypto::auth::{ACCOUNT_KSF, AccountKsf, RegistrationClient, ksf_acceptable};
use lockdown_sync::HttpTransport;
use lockdown_sync::account_security::PasswordFinish;
use lockdown_vault::handover;
use lockdown_vault::{ItemClass, ItemState};
use serde::Serialize;
use serde_bytes::ByteBuf;

use super::{PreparedExportKey, Session, account_envelope, secret_key, sync_identity_records};
use ecosystem_identity::records::{KeyRef, KeyRefKind, RecoveryState};

/// The profile field holding the last good drill (seconds since the epoch).
pub const DRILLED_AT: &str = "lockdown.drilled_at";
/// The profile field holding the account's raised password stretching.
pub const ACCOUNT_KSF_FIELD: &str = "lockdown.account_ksf";
/// How often the drill comes round.
pub const DRILL_EVERY_SECS: i64 = 365 * 86_400;
/// What a calibration aims for on this device.
const CALIBRATE_TO: Duration = Duration::from_secs(1);

/// One line of the drill's checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DrillCheck {
    /// What was checked: `kit`, `secret-key`, `contact`, `crown-jewels`.
    pub what: String,
    /// Whether it is as it should be.
    pub ok: bool,
    /// In plain words.
    pub words: String,
}

fn now() -> i64 {
    lockdown_vault::Timestamp::now().0
}

impl Session {
    /// When the kit last passed a drill, on any device of the identity.
    pub fn drilled_at(&self) -> Option<i64> {
        handover::profile_extra_int(&self.identity, DRILLED_AT)
    }

    /// Whether the kit was made, drilled, and whether the yearly drill is due.
    pub(crate) fn recovery_state(&self) -> (RecoveryState, Option<i64>, bool) {
        let state = self.identity.profile().recovery;
        let last = self.drilled_at();
        let due = match (state, last) {
            (RecoveryState::None, _) => false,
            (_, None) => true,
            (_, Some(at)) => now() - at >= DRILL_EVERY_SECS,
        };
        (state, last, due)
    }

    /// Raises the profile's signed kit stretching to what this device
    /// calibrates, when that is more. Returns whether it rose. The caller
    /// wraps the kit again (or makes a new one) at once.
    pub(crate) fn raise_profile_kdf(&mut self) -> Result<bool, String> {
        let current = self.identity.profile().kdf;
        let calibrated = lockdown_crypto::kdf::calibrate_at_least(&current, CALIBRATE_TO)
            .map_err(|e| e.to_string())?;
        if calibrated == current {
            return Ok(false);
        }
        if let Some(head) =
            handover::raise_profile_kdf(&mut self.identity, self.keystore.as_ref(), calibrated)
                .map_err(|e| e.to_string())?
        {
            self.memory
                .remember_identity(head)
                .map_err(|e| e.to_string())?;
        }
        Ok(true)
    }

    /// After a drill passed with `kit`: the date on the profile, and the
    /// kit's stretching raised (and the kit wrapped again) when this device
    /// can afford more.
    pub(crate) fn after_drill(&mut self, kit: &RecoveryKit) -> Result<Vec<String>, String> {
        let mut lines = Vec::new();
        if self.raise_profile_kdf()? {
            let head = handover::rewrap_kit(&mut self.identity, self.keystore.as_ref(), kit)
                .map_err(|e| e.to_string())?;
            self.memory
                .remember_identity(head)
                .map_err(|e| e.to_string())?;
            lines.push(format!(
                "the kit's password stretching rose to {} for this device",
                self.identity.profile().kdf
            ));
        }
        if let Some(head) = handover::set_profile_int(
            &mut self.identity,
            self.keystore.as_ref(),
            DRILLED_AT,
            now(),
        )
        .map_err(|e| e.to_string())?
        {
            self.memory
                .remember_identity(head)
                .map_err(|e| e.to_string())?;
        }
        Ok(lines)
    }

    /// The stretching this device signs in under for the account: what the
    /// profile says it was raised to, never less.
    pub(crate) fn account_ksf_floor(&self) -> Option<Argon2Params> {
        handover::profile_params(&self.identity, ACCOUNT_KSF_FIELD)
    }

    /// Raises the account's password stretching when this device can afford
    /// more than the account's password file was made under: the password
    /// file is made again on the server, the account slot written again
    /// under the new export key. Needs the Secret Key kept here (or the
    /// account's opt-out). `Ok(None)` when nothing rose.
    pub fn strengthen_account(&mut self, password: &str) -> Result<Option<String>, String> {
        let current = self.account_ksf_floor().unwrap_or(ACCOUNT_KSF);
        let calibrated =
            lockdown_crypto::kdf::calibrate_at_least(&current.max(&ACCOUNT_KSF), CALIBRATE_TO)
                .map_err(|e| e.to_string())?;
        if calibrated == current || !ksf_acceptable(&calibrated) {
            return Ok(None);
        }
        let sk = match self.secret_key_state() {
            lockdown_vault::device_link::SecretKeyState::Held(sk) => Some(sk),
            lockdown_vault::device_link::SecretKeyState::OptedOut => None,
            lockdown_vault::device_link::SecretKeyState::NotHeld => {
                return Err(
                    "this device doesn't keep the Secret Key, which the account slot needs; run the recovery drill here first"
                        .into(),
                );
            }
        };
        let cfg = self.sync.as_ref().ok_or("no account on this device")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        // The new password file, under the stronger stretching.
        let (client, request) = RegistrationClient::start(password).map_err(|e| e.to_string())?;
        let reply = transport
            .password_start(&token, &request)
            .map_err(|e| e.to_string())?;
        let ksf = AccountKsf::at(calibrated).map_err(|e| e.to_string())?;
        let (upload, export_key) = client
            .finish_stretched(password, &reply.response, &ksf)
            .map_err(|e| e.to_string())?;
        transport
            .password_finish(
                &token,
                &PasswordFinish {
                    upload: ByteBuf::from(upload),
                    ksf: calibrated,
                },
            )
            .map_err(|e| e.to_string())?;
        // The account slot under the new export key, then the parameters on
        // the profile, then everything to the server.
        let profile_id = self.identity.profile().profile_id;
        if account_envelope(&self.identity).is_some() {
            let irk_id = self.identity.irk_id().map_err(|e| e.to_string())?;
            self.identity
                .retire_envelope(irk_id, KeyRef::new(KeyRefKind::Account, profile_id))
                .map_err(|e| e.to_string())?;
        }
        let mut prepared = PreparedExportKey(Some(export_key));
        self.identity
            .enable_account_unlock(&mut prepared, password, sk.as_ref())
            .map_err(|e| e.to_string())?;
        if let Some(head) = handover::set_profile_params(
            &mut self.identity,
            self.keystore.as_ref(),
            ACCOUNT_KSF_FIELD,
            &calibrated,
        )
        .map_err(|e| e.to_string())?
        {
            self.memory
                .remember_identity(head)
                .map_err(|e| e.to_string())?;
        }
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        Ok(Some(format!(
            "your account's password stretching rose to {calibrated}, what this device does in about a second"
        )))
    }

    /// What the drill checks beyond the kit itself. Needs the server for the
    /// contacts' keys; without it those lines say so.
    pub fn drill_checks(&mut self) -> Result<Vec<DrillCheck>, String> {
        let mut out = Vec::new();
        let (state, last, due) = self.recovery_state();
        out.push(DrillCheck {
            what: "kit".into(),
            ok: state != RecoveryState::None && !due,
            words: match (state, last) {
                (RecoveryState::None, _) => "No recovery kit yet. Make one: it is the way back in if every device is lost.".into(),
                (_, None) => "Your kit has never been tested. Test it now: type it back in and Lockdown checks it opens your identity.".into(),
                (_, Some(at)) if due => format!("Your kit was last tested {} days ago; test it again.", (now() - at) / 86_400),
                (_, Some(at)) => format!("Your kit passed its last test {} days ago.", (now() - at) / 86_400),
            },
        });
        let sk = secret_key::state_word(&self.secret_key_state());
        out.push(DrillCheck {
            what: "secret-key".into(),
            ok: sk != "not-kept",
            words: match sk {
                "kept" => "This device keeps your Secret Key, so it can hand it to a new device you approve.".into(),
                "opted-out" => "Your account has no Secret Key; the password alone protects it against a stolen server database.".into(),
                _ => "This device doesn't keep your Secret Key. A drill with the password here keeps it.".into(),
            },
        });
        // Crown jewels and the emergency contacts.
        let crown: Vec<(String, bool)> = self
            .vaults()
            .flat_map(|v| v.items().map(move |i| (v, i)))
            .filter(|(_, i)| i.class == ItemClass::CrownJewel && i.state == ItemState::Live)
            .filter_map(|(v, i)| {
                let fields = v.open_item(i.item_id).ok()?;
                let title = fields
                    .iter()
                    .find(|f| f.field_id == "title")
                    .and_then(|f| match &f.value {
                        lockdown_vault::FieldValue::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                Some((title, lockdown_vault::legacy_opted_in(&fields)))
            })
            .collect();
        if self.sync.is_some() {
            match self.emergency() {
                Ok(grants) => {
                    let mine: Vec<_> = grants.into_iter().filter(|g| g.mine).collect();
                    if mine.is_empty() {
                        out.push(DrillCheck {
                            what: "contact".into(),
                            ok: true,
                            words: "No emergency contact. One you trust can be given access to read your vaults after a wait you can stop.".into(),
                        });
                    }
                    for g in mine {
                        let active = matches!(g.state.as_str(), "accepted" | "invited");
                        let keys = self.lookup_contact(&g.other);
                        let (ok, words) = match (&keys, active) {
                            (Err(e), _) => (
                                false,
                                format!("{}: their keys couldn't be checked: {e}", g.other),
                            ),
                            (Ok(c), true) if c.verified => (
                                g.state == "accepted",
                                format!(
                                    "{}: {} as your emergency contact after {} days; keys unchanged and compared",
                                    g.other,
                                    if g.state == "accepted" {
                                        "ready"
                                    } else {
                                        "hasn't accepted yet"
                                    },
                                    g.wait_days
                                ),
                            ),
                            (Ok(_), true) => (
                                false,
                                format!(
                                    "{}: keys unchanged, but you haven't compared fingerprints",
                                    g.other
                                ),
                            ),
                            (Ok(_), false) => (
                                false,
                                format!(
                                    "{}: this emergency access is {}; name them again or someone else",
                                    g.other, g.state
                                ),
                            ),
                        };
                        out.push(DrillCheck {
                            what: "contact".into(),
                            ok,
                            words,
                        });
                    }
                }
                Err(e) => out.push(DrillCheck {
                    what: "contact".into(),
                    ok: false,
                    words: format!("Your emergency contacts couldn't be checked: {e}"),
                }),
            }
        }
        if !crown.is_empty() {
            let opted: Vec<&str> = crown.iter().filter(|c| c.1).map(|c| c.0.as_str()).collect();
            out.push(DrillCheck {
                what: "crown-jewels".into(),
                ok: true,
                words: if opted.is_empty() {
                    format!(
                        "{} crown jewel(s) stay out of emergency access; open one to include it.",
                        crown.len()
                    )
                } else {
                    format!(
                        "{} of {} crown jewels go to your emergency contacts after the wait: {}.",
                        opted.len(),
                        crown.len(),
                        opted.join(", ")
                    )
                },
            });
        }
        Ok(out)
    }
}
