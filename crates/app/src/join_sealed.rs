//! A computer that joins an account, sealed from the start
//! (docs/plan/44-cli-join-sealed.md; D514).
//!
//! Joining is signing in with the account's password and the kit's Secret
//! Key ([`App::account_login_sealed`]) or recovering with the whole kit
//! ([`App::recover_sealed`]) on a computer that holds nothing yet. Here the
//! new device's keys live only in memory until the sign-in has proved the
//! password; they are then sealed under that password, which becomes this
//! computer's master password, **before** the identity names them, the way
//! [`App::first_run_create`] seals before it creates. So:
//!
//! - no plain key file is ever written, not even for a moment;
//! - a wrong password writes nothing (the sign-in fails before any file);
//! - a kit or Secret Key the identity refuses, or a join that stops between
//!   the seal and the identity, leaves only the sealed file, and the next
//!   try opens it with the same password and carries on as the same
//!   device. The keys are kept on purpose: the server's log already
//!   recorded this device's sign-in, and a retry with fresh keys would show
//!   on the account's key transparency audit as a device the identity
//!   doesn't list.
//!
//! A recovery also seals the keys under the kit (the kit slot), so "Forgot
//! your password?" works on this computer later. The name the lock screen
//! shows is the account's login, as the desktop's join records it.
//!
//! The command line and the desktop both join this way. The desktop then
//! opens the vault with [`App::open_joined`], which pulls it from the
//! server as the first open of a joined computer does; its "let another
//! device approve this one" join seals the same way
//! (`devices_link::Joining::finish_sealed`;
//! docs/plan/45-desktop-join-sealed.md).

use std::path::Path;

use ecosystem_identity::RecoveryKit;
use zeroize::Zeroizing;

use crate::local_unlock::{self, DeviceKeys, KeyFile};
use crate::onboarding::{self, Onboarding, Stage};
use crate::session::second_factor::{Then, sign_in_new_device};
use crate::session::{Mode, Session};
use crate::{App, AppError, AppResult, Core, Status};

/// The keys a sealed join signs in with: fresh ones kept only in memory, or
/// the ones a join that stopped half way left here (a sealed file, opened
/// with `password`; a plain file from an older join, sealed by this one).
pub(crate) fn join_keys(data: &Path, password: &str) -> Result<DeviceKeys, String> {
    match local_unlock::key_file(data) {
        KeyFile::None => DeviceKeys::generate(),
        KeyFile::Sealed => DeviceKeys::open_with_password(data, password).map_err(|e| {
            if e == local_unlock::WRONG_PASSWORD {
                format!(
                    "This computer kept the keys of a join that didn't finish, sealed under another password. Join with the password you used then, or remove {} to start afresh.",
                    data.join(local_unlock::SEALED_FILE).display()
                )
            } else {
                e
            }
        }),
        KeyFile::Plain => DeviceKeys::open_plain(data),
    }
}

/// Seals `keys` under `password` (and `kit`, for a recovery) right before
/// the identity names them. A plain file an older join left goes once the
/// sealed one reads back (`DeviceKeys::seal`).
pub(crate) fn seal_for_join(
    data: &Path,
    keys: &DeviceKeys,
    password: &str,
    kit: Option<&RecoveryKit>,
) -> Result<(), String> {
    keys.seal(data, password, kit)
}

/// The lock screen's name for a joined computer: the account's login, cut
/// to what a name may hold.
fn lock_name(login: &str) -> String {
    onboarding::check_name(login).unwrap_or_else(|_| {
        login
            .chars()
            .filter(|c| !c.is_control())
            .take(onboarding::NAME_MAX)
            .collect::<String>()
            .trim()
            .to_string()
    })
}

/// Records a finished sealed join: the name (when none is kept yet) and the
/// first run done, as the desktop's `setMasterPassword` does for an account
/// on a server.
pub(crate) fn note_joined(data: &Path, login: &str) -> Result<(), String> {
    let mut o = Onboarding::load(data);
    if o.name.is_none() {
        o.name = Some(lock_name(login));
    }
    o.stage = Some(Stage::Done);
    o.save(data)
}

impl Core {
    /// Opens a computer a sealed join just finished, with the password it
    /// was sealed under: the vaults are pulled from the server (or the
    /// first one made, for an account with none), as the first open after
    /// a join does. Refused unless the keys are sealed and the identity
    /// names this device, so it never makes anything from nothing.
    pub(crate) fn open_joined(&mut self, password: &str) -> AppResult<Status> {
        if local_unlock::key_file(&self.paths.data) != KeyFile::Sealed {
            return Err(AppError::refused(
                "Nothing joined on this computer is sealed yet.",
            ));
        }
        if !self.paths.probe()?.identity_present {
            return Err(AppError::refused(
                "This computer's join didn't finish. Join again with the same password.",
            ));
        }
        let keys = DeviceKeys::open_with_password(&self.paths.data, password)
            .map_err(AppError::refused)?;
        let session = Session::open_with(
            self.paths.clone(),
            self.persona.as_deref(),
            Mode::Init,
            Some(keys),
        )?;
        self.adopt(session)
    }
}

impl App {
    /// Opens the vault on a computer a sealed join just finished
    /// ([`App::account_login_sealed`], [`App::recover_sealed`], after a
    /// security key or emailed approval answered too) with the password it
    /// was sealed under, pulling the vaults from the server.
    pub fn open_joined(&self, password: &str) -> AppResult<Status> {
        let password = Zeroizing::new(password.to_string());
        self.call(move |core| core.open_joined(&password))
    }

    /// [`App::account_login`] on a computer that holds nothing yet, sealed:
    /// signs in, enrols this device through the account slot (the password
    /// and, when the slot wants it, the Secret Key), with the device's keys
    /// sealed under the password before the identity names them. The
    /// password is this computer's master password from then on. An
    /// account that wants a security key answers `approval_needed`, as the
    /// unsealed join does, and [`App::second_factor_wait`] finishes it.
    pub fn account_login_sealed(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let (server, login) = (server.to_string(), login.to_string());
        let password = Zeroizing::new(password.to_string());
        let secret_key = secret_key.map(|s| Zeroizing::new(s.to_string()));
        self.call(move |core| {
            core.refuse_confined()?;
            crate::devices::start_waiting(
                core,
                sign_in_new_device(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    Then::Enrol { secret_key },
                    true,
                ),
            )
        })
    }

    /// [`App::recover`] sealed: the identity back with the kit and the
    /// account password, the device's keys sealed under the password and
    /// the kit before the identity names them.
    pub fn recover_sealed(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: &str,
        recovery_code: &str,
    ) -> AppResult<Vec<String>> {
        let (server, login) = (server.to_string(), login.to_string());
        let password = Zeroizing::new(password.to_string());
        let secret_key = Zeroizing::new(secret_key.to_string());
        let recovery_code = Zeroizing::new(recovery_code.to_string());
        self.call(move |core| {
            core.refuse_confined()?;
            crate::devices::start_waiting(
                core,
                sign_in_new_device(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    Then::Recover {
                        secret_key,
                        recovery_code,
                    },
                    true,
                ),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::keystore::DeviceKeystore;

    #[test]
    fn the_lock_name_is_the_login_cut_to_a_name() {
        assert_eq!(lock_name("me@example.com"), "me@example.com");
        let long = "x".repeat(80);
        assert_eq!(lock_name(&long).chars().count(), onboarding::NAME_MAX);
    }

    #[test]
    fn a_join_that_stopped_is_carried_on_as_the_same_device() {
        let dir = tempfile::tempdir().unwrap();
        let keys = join_keys(dir.path(), "pw one two three").unwrap();
        // Fresh keys are in memory only until the seal.
        assert_eq!(local_unlock::key_file(dir.path()), KeyFile::None);
        seal_for_join(dir.path(), &keys, "pw one two three", None).unwrap();
        assert_eq!(local_unlock::key_file(dir.path()), KeyFile::Sealed);
        assert!(!dir.path().join(local_unlock::PLAIN_FILE).exists());

        // The next try opens the same keys with the same password; another
        // password is refused in words that say what to do.
        let again = join_keys(dir.path(), "pw one two three").unwrap();
        assert_eq!(again.device_pub(), keys.device_pub());
        let err = join_keys(dir.path(), "another password").unwrap_err();
        assert!(err.contains("a join that didn't finish"), "{err}");
    }

    #[test]
    fn a_half_done_plain_join_is_sealed_by_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let plain = DeviceKeys::open_or_create_plain(dir.path()).unwrap();
        let keys = join_keys(dir.path(), "pw one two three").unwrap();
        assert_eq!(keys.device_pub(), plain.device_pub());
        seal_for_join(dir.path(), &keys, "pw one two three", None).unwrap();
        assert_eq!(local_unlock::key_file(dir.path()), KeyFile::Sealed);
        assert!(!dir.path().join(local_unlock::PLAIN_FILE).exists());
    }

    #[test]
    fn a_finished_join_names_the_lock_screen_once() {
        let dir = tempfile::tempdir().unwrap();
        note_joined(dir.path(), "me@example.com").unwrap();
        let o = Onboarding::load(dir.path());
        assert_eq!(o.name.as_deref(), Some("me@example.com"));
        assert_eq!(o.stage, Some(Stage::Done));
        let mut o = o;
        o.name = Some("Sean".into());
        o.save(dir.path()).unwrap();
        note_joined(dir.path(), "me@example.com").unwrap();
        assert_eq!(Onboarding::load(dir.path()).name.as_deref(), Some("Sean"));
    }
}
