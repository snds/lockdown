//! The first run, the master password and the ways back in
//! (docs/plan/39-first-run.md §1.2–§1.6, §1.10; D460, D461, D465, D467).
//!
//! - [`App::first_run_create`] makes everything from nothing: the device's
//!   keys sealed under the master password, the identity and the first
//!   vault, the local account on the server the app runs inside itself
//!   (`lockdown_sync::LOCAL_BASE`), and the recovery kit, whose page comes
//!   back once.
//! - [`App::unlock_with_password`] opens the vault with the master
//!   password, through this computer's own slot: no server, so it works
//!   offline and after a move to a real server.
//! - [`App::set_master_password`] is the migration for installs made
//!   before the master password (§1.6): a "This device only" install
//!   registers the local account with its kit (or a new one), an install
//!   with an account on a server checks the password there once; then the
//!   keys are sealed and the plain key file goes.
//! - [`App::password_reset_with_kit`] is "Forgot your password?": the kit
//!   opens the keys, the account's password is made again on its server
//!   with the device's own session, and the keys are sealed under the new
//!   password.

use std::time::{Instant, SystemTime};

use ecosystem_identity::RecoveryKit;
use zeroize::Zeroizing;

use crate::local_unlock::{self, DeviceKeys, KeyFile};
use crate::onboarding::{self, CreatedIdentity, Onboarding, Stage};
use crate::session::{Mode, Session};
use crate::types::{Registered, Status};
use crate::{App, AppError, AppResult, Core};

/// The lowest strength score a master password may have (Q3: "fair").
pub const MASTER_PASSWORD_FLOOR: u8 = 3;

/// What the migration needs beside the password.
#[derive(Clone, Debug, Default)]
pub struct MasterPasswordInput {
    /// The name the lock screen shows (an install with no account).
    pub name: Option<String>,
    /// The master password.
    pub password: String,
    /// The kit's Secret Key, when the identity has a kit and it is at hand.
    pub secret_key: Option<String>,
    /// The kit's recovery code, with the Secret Key.
    pub recovery_code: Option<String>,
    /// Make a new kit instead (the old page stops working).
    pub new_kit: bool,
}

impl Drop for MasterPasswordInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        if let Some(s) = self.secret_key.as_mut() {
            s.zeroize();
        }
        if let Some(s) = self.recovery_code.as_mut() {
            s.zeroize();
        }
    }
}

/// Refuses a master password under "fair", in the screen's words. The name
/// counts as a guessable word.
pub fn check_master_password(password: &str, name: Option<&str>) -> Result<(), String> {
    let context: Vec<&str> = name.into_iter().collect();
    match crate::strength::score(password.as_bytes(), &context)? {
        None => Err("Type a master password.".into()),
        Some(score) if score < MASTER_PASSWORD_FLOOR => Err(
            "This password protects everything else, so it needs to be at least fair. Add another word or two."
                .into(),
        ),
        Some(_) => Ok(()),
    }
}

fn now() -> i64 {
    lockdown_vault::Timestamp::now().0
}

impl Core {
    /// Takes `session` as the open one, as an unlock does.
    fn adopt(&mut self, session: Session) -> AppResult<Status> {
        let status = session.status()?;
        self.session = Some(session);
        self.last_reason = None;
        self.clock = LockClock::new(Instant::now(), SystemTime::now());
        Ok(status)
    }

    fn first_run_create(&mut self, name: &str, password: &str) -> AppResult<Registered> {
        self.refuse_confined()?;
        let name = onboarding::check_name(name).map_err(AppError::refused)?;
        check_master_password(password, Some(&name)).map_err(AppError::refused)?;
        let probe = self.paths.probe()?;
        if probe.identity_present || local_unlock::key_file(&self.paths.data) != KeyFile::None {
            return Err(AppError::refused(
                "Lockdown is already set up on this computer.",
            ));
        }
        if !lockdown_sync::local_server_installed() {
            return Err(AppError::refused(
                "This build can't keep a vault on this computer alone.",
            ));
        }
        // The keys are sealed before the identity names them, so a first
        // run that stops half way never leaves keys in the clear.
        let keys = DeviceKeys::generate().map_err(AppError::refused)?;
        keys.seal(&self.paths.data, password, None)
            .map_err(AppError::refused)?;
        let keys = DeviceKeys::open_with_password(&self.paths.data, password)
            .map_err(AppError::refused)?;
        let mut session = Session::open_with(
            self.paths.clone(),
            self.persona.as_deref(),
            Mode::Init,
            Some(keys),
        )?;
        let (registered, kit) = session
            .account_register_with(lockdown_sync::LOCAL_BASE, &name, password, None)
            .map_err(AppError::refused)?;
        let kit = kit.ok_or_else(|| AppError::refused("the recovery kit wasn't made"))?;
        session
            .device_keys()
            .reseal_kit(&self.paths.data, &kit)
            .map_err(AppError::refused)?;
        let mut onboarding = Onboarding::default();
        onboarding.stage = Some(Stage::AccountMade);
        onboarding.name = Some(name);
        onboarding.kit_made_at = Some(now());
        onboarding.created_identity = Some(CreatedIdentity {
            root: self.paths.root.display().to_string(),
            profile_id: kit.profile_id(),
        });
        onboarding
            .save(&self.paths.data)
            .map_err(AppError::refused)?;
        self.adopt(session)?;
        Ok(Registered {
            lines: vec!["Your vault is ready, and only your master password opens it.".into()],
            kit: registered.kit,
        })
    }

    fn unlock_with_password(&mut self, password: &str) -> AppResult<Status> {
        let keys = match local_unlock::key_file(&self.paths.data) {
            KeyFile::Sealed => DeviceKeys::open_with_password(&self.paths.data, password)
                .map_err(AppError::refused)?,
            KeyFile::Plain => {
                return Err(AppError::refused(
                    "This vault has no master password yet; unlock it, then choose one.",
                ));
            }
            KeyFile::None => {
                return Err(AppError::refused("Nothing is set up on this computer yet."));
            }
        };
        let session = Session::open_with(
            self.paths.clone(),
            self.persona.as_deref(),
            Mode::Open,
            Some(keys),
        )?;
        // A switch that stopped after the sealed file was written leaves
        // the plain one; the first unlock with the password removes it.
        local_unlock::remove_plain(&self.paths.data);
        self.adopt(session)
    }

    fn set_master_password(&mut self, input: &MasterPasswordInput) -> AppResult<Registered> {
        self.refuse_confined()?;
        let data = self.paths.data.clone();
        let session = self.session.as_mut().ok_or_else(AppError::locked)?;
        // Sealed with an account already: nothing to do. Sealed without
        // one is a first run that stopped before the account was made; it
        // finishes here as the migration does.
        if session.keys_sealed() && session.has_account() {
            return Err(AppError::refused(
                "This vault already has a master password.",
            ));
        }
        let password = input.password.as_str();
        if session.has_account() && !session.is_local_account() {
            // An account on a server: its password becomes the master
            // password, checked there once (§1.6 M3).
            if !session
                .check_password(password)
                .map_err(AppError::refused)?
            {
                return Err(AppError::refused(local_unlock::WRONG_PASSWORD));
            }
            session
                .seal_keys(password, None)
                .map_err(AppError::refused)?;
            let mut o = Onboarding::load(&data);
            if o.name.is_none() {
                o.name = session.account_login_name();
            }
            o.stage = Some(Stage::Done);
            o.save(&data).map_err(AppError::refused)?;
            return Ok(Registered {
                lines: vec!["Your master password now locks this vault.".into()],
                kit: None,
            });
        }
        let name = onboarding::check_name(input.name.as_deref().unwrap_or_default())
            .map_err(AppError::refused)?;
        check_master_password(password, Some(&name)).map_err(AppError::refused)?;
        if !lockdown_sync::local_server_installed() {
            return Err(AppError::refused(
                "This build can't keep a vault on this computer alone.",
            ));
        }
        let profile_id = session.profile_id();
        let has_kit = session.has_kit();
        let (kit, page) = if !has_kit || input.new_kit {
            if has_kit {
                let (made, kit) = session
                    .kit_create_with(true, None)
                    .map_err(AppError::refused)?;
                (Some(kit), Some(made.kit))
            } else {
                (None, None)
            }
        } else {
            let (Some(sk), Some(rc)) =
                (input.secret_key.as_deref(), input.recovery_code.as_deref())
            else {
                return Err(AppError::refused(
                    "Type the Secret Key and recovery code from your kit, or make a new kit.",
                ));
            };
            let kit = RecoveryKit::from_text(profile_id, sk, rc).map_err(|_| {
                AppError::refused(
                    "That kit doesn't match this vault. Check it letter by letter, or make a new kit.",
                )
            })?;
            session
                .kit_verify(&kit.secret_key_text(), &kit.recovery_code_text(), None, false)
                .map_err(|_| {
                    AppError::refused(
                        "That kit doesn't match this vault. Check it letter by letter, or make a new kit.",
                    )
                })?;
            (Some(kit), None)
        };
        let secret = kit.as_ref().map(|k| *k.secret_key().expose());
        let (registered, made) = session
            .account_register_with(lockdown_sync::LOCAL_BASE, &name, password, secret)
            .map_err(AppError::refused)?;
        let kit = kit
            .or(made)
            .ok_or_else(|| AppError::refused("the recovery kit wasn't made"))?;
        session
            .seal_keys(password, Some(&kit))
            .map_err(AppError::refused)?;
        let shown = page.or(registered.kit);
        let mut o = Onboarding::load(&data);
        o.name = Some(name);
        o.stage = Some(if shown.is_some() {
            Stage::AccountMade
        } else {
            Stage::Done
        });
        if shown.is_some() {
            o.kit_made_at = Some(now());
        }
        o.save(&data).map_err(AppError::refused)?;
        Ok(Registered {
            lines: vec!["Your master password now locks this vault.".into()],
            kit: shown,
        })
    }

    fn password_reset_with_kit(
        &mut self,
        secret_key: &str,
        recovery_code: &str,
        password: &str,
    ) -> AppResult<Status> {
        self.refuse_confined()?;
        let profile_id = local_unlock::profile_of(&self.paths.data)
            .map_err(AppError::refused)?
            .ok_or_else(|| {
                AppError::refused(
                    "This vault was locked before it had a recovery kit on this computer, so the kit can't open it here.",
                )
            })?;
        let kit = RecoveryKit::from_text(profile_id, secret_key, recovery_code).map_err(|_| {
            AppError::refused(
                "That kit doesn't open this vault. Check each character against the page.",
            )
        })?;
        let keys = DeviceKeys::open_with_kit(&self.paths.data, &kit).map_err(AppError::refused)?;
        let name = Onboarding::load(&self.paths.data).name;
        check_master_password(password, name.as_deref()).map_err(AppError::refused)?;
        let mut session = Session::open_with(
            self.paths.clone(),
            self.persona.as_deref(),
            Mode::Open,
            Some(keys),
        )?;
        if session.has_account() {
            session.change_account_password(password).map_err(|e| {
                if session.is_local_account() {
                    AppError::refused(e)
                } else {
                    AppError::refused(format!(
                        "Lockdown needs to reach {} to change your password there. Connect and try again. ({e})",
                        session.account_server().unwrap_or_default()
                    ))
                }
            })?;
        }
        session
            .seal_keys(password, Some(&kit))
            .map_err(AppError::refused)?;
        self.adopt(session)
    }
}

use crate::lock::LockClock;

impl App {
    /// First run: the master password, the local account and the kit
    /// (§1.2). The kit's page comes back once, as `Registered::kit`.
    pub fn first_run_create(&self, name: &str, password: &str) -> AppResult<Registered> {
        let name = name.to_string();
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| core.first_run_create(&name, &password))
    }

    /// Opens the vault with the master password (§1.3).
    pub fn unlock_with_password(&self, password: &str) -> AppResult<Status> {
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| core.unlock_with_password(&password))
    }

    /// The migration for an install without a master password (§1.6). The
    /// vault must be open; a new kit's page comes back once.
    pub fn set_master_password(&self, input: MasterPasswordInput) -> AppResult<Registered> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.set_master_password(&input)
        })
    }

    /// "Forgot your password?": the kit opens the vault and a new master
    /// password replaces the old one (§1.10).
    pub fn password_reset_with_kit(
        &self,
        secret_key: &str,
        recovery_code: &str,
        password: &str,
    ) -> AppResult<Status> {
        let secret_key = Zeroizing::new(secret_key.to_string());
        let recovery_code = Zeroizing::new(recovery_code.to_string());
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| core.password_reset_with_kit(&secret_key, &recovery_code, &password))
    }

    /// Records how far the first run got (`onboarding`), from a shell.
    pub fn onboarding_stage(&self, stage: &str) -> AppResult<()> {
        let stage = Stage::parse(stage)
            .ok_or_else(|| AppError::refused(format!("'{stage}' isn't a first-run stage")))?;
        self.call(move |core| {
            let mut o = Onboarding::load(&core.paths.data);
            o.stage = Some(stage);
            o.save(&core.paths.data).map_err(AppError::refused)
        })
    }

    /// Changes the name the lock screen shows (Settings → Lock).
    pub fn set_name(&self, name: &str) -> AppResult<String> {
        let name = onboarding::check_name(name).map_err(AppError::refused)?;
        self.call(move |core| {
            core.session()?;
            let mut o = Onboarding::load(&core.paths.data);
            o.name = Some(name.clone());
            o.save(&core.paths.data).map_err(AppError::refused)?;
            Ok(name)
        })
    }

    /// The strength word check a shell runs before Continue (the same rule
    /// the core applies when it saves).
    pub fn check_master_password(&self, password: &str, name: Option<&str>) -> AppResult<()> {
        check_master_password(password, name).map_err(AppError::refused)
    }
}
