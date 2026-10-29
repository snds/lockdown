//! The reminders around recovery (`password-reminder`, `recovery-kit`,
//! `recovery-drill`, `high-risk-class`; 04-security §2 "biometric-only users
//! get a periodic password reminder", §9 the drill; docs/plan/22-security-depth.md
//! §5–§8).
//!
//! - **The password reminder.** On a device whose unlock never asks for the
//!   account password (the desktop opens with its keystore and the platform
//!   check, D83), the app asks for it every 14 days by default (7, 14 or 30),
//!   so it isn't forgotten by the day a new device needs it. Any password
//!   the person types and the core checks (a gate, a sign-in) counts. Later
//!   moves it a day. The web vault, whose every unlock is the password,
//!   never shows it.
//! - **The first-run kit test.** A kit made is not yet a kit that works: the
//!   first run ends by typing it back in ([`App::kit_verify`] with `drill`).
//!   It can be skipped only with an explicit "I understand", and then the
//!   drill reminder comes back in a week.
//! - **The yearly drill.** Due a year after the last good one (the date is
//!   on the identity's profile, so every device agrees), or at once when
//!   the kit was never tested. Its checks look at the Secret Key kept here,
//!   each emergency contact, and which crown jewels they would get.
//! - **Crown jewels and emergency access**: [`App::set_legacy_opt_in`].
//!
//! The device-local state (when the password was last typed here, the
//! interval, what was snoozed) is `reminders.cbor` beside `settings.cbor`:
//! dates and a number, nothing secret.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ecosystem_identity::records::RecoveryState;
use lockdown_store::write_private;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::session::Paths;
pub use crate::session::recovery::DrillCheck;
use crate::{App, AppError, AppResult, ApprovalKind, ErrorCode, GateInput};

const FILE: &str = "reminders.cbor";
const FILE_V: u16 = 1;
/// The password reminder's default interval, in days.
pub const PASSWORD_EVERY_DAYS: u32 = 14;
/// The intervals a person may choose.
pub const PASSWORD_CHOICES: [u32; 3] = [7, 14, 30];
const DAY: i64 = 86_400;

/// What this device remembers for the reminders.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Kept {
    v: u16,
    #[serde(default)]
    last_password_at: Option<i64>,
    #[serde(default)]
    password_every_days: Option<u32>,
    #[serde(default)]
    password_snoozed_until: Option<i64>,
    #[serde(default)]
    drill_snoozed_until: Option<i64>,
    #[serde(default)]
    kit_test_skipped_at: Option<i64>,
}

fn path(paths: &Paths) -> PathBuf {
    paths.data.join(FILE)
}

fn load(file: &Path) -> Kept {
    std::fs::read(file)
        .ok()
        .and_then(|b| lockdown_crypto::cbor::from_slice::<Kept>(&b).ok())
        .filter(|k| k.v <= FILE_V)
        .unwrap_or(Kept {
            v: FILE_V,
            ..Kept::default()
        })
}

fn save(file: &Path, kept: &Kept) -> Result<(), String> {
    let mut kept = kept.clone();
    kept.v = FILE_V;
    let bytes = lockdown_crypto::cbor::to_canonical(&kept).map_err(|e| e.to_string())?;
    write_private(file, &bytes).map_err(|e| format!("couldn't write {}: {e}", file.display()))
}

fn now() -> i64 {
    lockdown_vault::Timestamp::now().0
}

/// The account password was just typed here and checked: the reminder
/// starts over. A failure to write the file never fails what was checked.
pub(crate) fn note_password(paths: &Paths) {
    let file = path(paths);
    let mut kept = load(&file);
    kept.last_password_at = Some(now());
    kept.password_snoozed_until = None;
    let _ = save(&file, &kept);
}

/// The password reminder as a shell shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PasswordReminder {
    /// Whether it applies here: an account exists and unlocking doesn't
    /// ask for the password.
    pub applies: bool,
    /// Whether to ask now.
    pub due: bool,
    /// When the password was last typed and checked on this device.
    pub last_at: Option<i64>,
    /// The interval, in days.
    pub every_days: u32,
    /// When it will be due.
    pub next_at: Option<i64>,
}

/// The kit and its drill as a shell shows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DrillReminder {
    /// `none`, `created` or `drilled`.
    pub kit: String,
    /// When the kit last passed a drill, on any device of the identity.
    pub last_drilled_at: Option<i64>,
    /// When the next one is due.
    pub next_at: Option<i64>,
    /// Whether to prompt now.
    pub due: bool,
    /// The kit was made but never tested, and the test wasn't skipped:
    /// the first run's last step is still to do.
    pub kit_test_pending: bool,
}

/// Both reminders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RemindersView {
    /// The account password.
    pub password: PasswordReminder,
    /// The recovery kit.
    pub drill: DrillReminder,
}

impl App {
    /// Whether this device's unlock is the account password itself (the
    /// web vault), so the password reminder never applies. The host says
    /// so once at start.
    pub fn set_unlock_asks_password(&self, yes: bool) -> AppResult<()> {
        self.call(move |core| {
            core.depth.unlock_asks_password = yes;
            Ok(())
        })
    }

    /// Where both reminders stand.
    pub fn reminders(&self) -> AppResult<RemindersView> {
        self.call(|core| {
            let file = path(&core.paths);
            let mut kept = load(&file);
            // A vault sealed under a master password asks for it at every
            // unlock, so the reminder has nothing to add (D252 under doc 39).
            let asks = core.depth.unlock_asks_password
                || crate::local_unlock::key_file(&core.paths.data)
                    == crate::local_unlock::KeyFile::Sealed;
            let kit_test_waits =
                !crate::onboarding::Onboarding::load(&core.paths.data).kit_test_due(now());
            let session = core.session()?;
            let has_account = session.has_account();
            let (state, last_drill, drill_due) = session.recovery_state();
            let now = now();
            // A device that never noted a password starts its clock now,
            // so a fresh install isn't asked at once.
            if has_account && kept.last_password_at.is_none() {
                kept.last_password_at = Some(now);
                let _ = save(&file, &kept);
            }
            let every = kept.password_every_days.unwrap_or(PASSWORD_EVERY_DAYS);
            let next = kept
                .last_password_at
                .map(|t| t + i64::from(every) * DAY)
                .map(|t| t.max(kept.password_snoozed_until.unwrap_or(0)));
            let applies = has_account && !asks;
            let snoozed = |until: Option<i64>| until.is_some_and(|u| now < u);
            let drill_next = match (state, last_drill) {
                (RecoveryState::None, _) => None,
                (_, None) => Some(now),
                (_, Some(at)) => Some(at + crate::session::recovery::DRILL_EVERY_SECS),
            };
            Ok(RemindersView {
                password: PasswordReminder {
                    applies,
                    due: applies && next.is_some_and(|n| now >= n),
                    last_at: kept.last_password_at,
                    every_days: every,
                    next_at: next,
                },
                drill: DrillReminder {
                    kit: match state {
                        RecoveryState::None => "none",
                        RecoveryState::Created => "created",
                        RecoveryState::Drilled => "drilled",
                    }
                    .into(),
                    last_drilled_at: last_drill,
                    next_at: drill_next,
                    due: drill_due && !snoozed(kept.drill_snoozed_until),
                    // The kit test waits a day after the first run made the
                    // kit: put it away and come back (Q4).
                    kit_test_pending: state != RecoveryState::None
                        && last_drill.is_none()
                        && kept.kit_test_skipped_at.is_none()
                        && !kit_test_waits,
                },
            })
        })
    }

    /// The person typed the account password for the reminder: checked
    /// with an OPAQUE sign-in (the password never leaves the device).
    pub fn password_check(&self, password: &str) -> AppResult<RemindersView> {
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| {
            core.clock.touch(Instant::now());
            if !core
                .session()?
                .check_password(&password)
                .map_err(AppError::refused)?
            {
                return Err(AppError::refused(
                    "that isn't your master password; try again",
                ));
            }
            note_password(&core.paths);
            Ok(())
        })?;
        self.reminders()
    }

    /// Asks again tomorrow.
    pub fn password_snooze(&self) -> AppResult<RemindersView> {
        self.call(|core| {
            let file = path(&core.paths);
            let mut kept = load(&file);
            kept.password_snoozed_until = Some(now() + DAY);
            save(&file, &kept).map_err(AppError::refused)
        })?;
        self.reminders()
    }

    /// How often the password reminder comes: 7, 14 or 30 days.
    pub fn set_password_reminder_days(&self, days: u32) -> AppResult<RemindersView> {
        if !PASSWORD_CHOICES.contains(&days) {
            return Err(AppError::refused(
                "the password reminder comes every 7, 14 or 30 days",
            ));
        }
        self.call(move |core| {
            let file = path(&core.paths);
            let mut kept = load(&file);
            kept.password_every_days = Some(days);
            save(&file, &kept).map_err(AppError::refused)
        })?;
        self.reminders()
    }

    /// Puts the drill off for a week.
    pub fn drill_snooze(&self) -> AppResult<RemindersView> {
        self.call(|core| {
            let file = path(&core.paths);
            let mut kept = load(&file);
            kept.drill_snoozed_until = Some(now() + 7 * DAY);
            save(&file, &kept).map_err(AppError::refused)
        })?;
        self.reminders()
    }

    /// Skips the first run's kit test: refused unless the person confirmed
    /// that an untested kit may fail when it's needed; the drill comes back
    /// in a week.
    pub fn kit_test_skip(&self, understood: bool) -> AppResult<RemindersView> {
        if !understood {
            return Err(AppError {
                code: ErrorCode::Refused,
                message: "An untested kit can fail on the day you need it, when every device is gone and nothing else gets you back in. Confirm that you understand to skip the test for now; Lockdown asks again in a week.".into(),
                detail: Some("confirm_skip".into()),
            });
        }
        self.call(|core| {
            let file = path(&core.paths);
            let mut kept = load(&file);
            kept.kit_test_skipped_at = Some(now());
            kept.drill_snoozed_until = Some(now() + 7 * DAY);
            save(&file, &kept).map_err(AppError::refused)
        })?;
        self.reminders()
    }

    /// The drill's checks beyond the kit: the Secret Key kept here, each
    /// emergency contact, and the crown jewels they would get.
    pub fn drill_checks(&self) -> AppResult<Vec<DrillCheck>> {
        self.with_session(|s| s.drill_checks())
    }

    /// Raises the account's password stretching to what this device does in
    /// about a second, when that is more (it also runs after every sign-in
    /// here). `None` when there was nothing to raise.
    pub fn strengthen_account(&self, password: &str) -> AppResult<Option<String>> {
        let password = Zeroizing::new(password.to_string());
        self.with_session(move |s| s.strengthen_account(&password))
    }

    /// Opts a crown jewel in to emergency access (its contact gets it after
    /// the wait, read only), behind the `Reveal` gate, or out of it, which
    /// asks nothing.
    pub fn set_legacy_opt_in(&self, id: &str, on: bool, gate: GateInput) -> AppResult<u64> {
        let id = id.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            if on {
                core.approve(
                    ApprovalKind::Reveal,
                    "let your emergency contacts have this protected item after their wait",
                    &gate,
                )?;
            }
            core.session()?
                .set_legacy_opt_in(&id, on)
                .map_err(AppError::refused)
        })
    }

    /// Whether a crown jewel is opted in to emergency access.
    pub fn legacy_opt_in(&self, id: &str) -> AppResult<bool> {
        let id = id.to_string();
        self.with_session(move |s| {
            let item =
                lockdown_vault::Id::parse(&id).map_err(|_| format!("'{id}' isn't an item id"))?;
            let fields = s
                .holding(item)?
                .open_item(item)
                .map_err(|e| e.to_string())?;
            Ok(lockdown_vault::legacy_opted_in(&fields))
        })
    }
}
