//! The old unsealed join, kept for tests only
//! (docs/plan/56-no-plain-joins.md).
//!
//! Before every join was sealed (docs/plan/44-cli-join-sealed.md,
//! 45-desktop-join-sealed.md), [`App::account_login`] and [`App::recover`]
//! on an empty computer wrote the new device's keys as the plain key file
//! (`device-key.cbor`) and enrolled them. Those calls now refuse there.
//! Installs joined that way still exist on people's computers, so the
//! tests that prove they keep opening, and that the migration seals them,
//! need a way to make one. These two functions are that way, under names
//! that say what they make.
//!
//! This module is compiled only for this crate's own unit tests and with
//! the `test-support` feature, which only `[dev-dependencies]` turn on. A
//! build of the desktop, the command line or the server has neither, so
//! these functions don't exist there (the `no_plain_joins` test and the
//! `desktop-bare` CI job check it).

use zeroize::Zeroizing;

use crate::devices::start_waiting;
use crate::session::second_factor::{Then, sign_in_new_device};
use crate::{App, AppResult};

impl App {
    /// **Test fixture.** Joins this empty computer to the account the old
    /// way: signs in, writes the plain key file and enrols it through the
    /// account slot (the password and, when the slot wants it, the Secret
    /// Key). An account that wants a security key answers
    /// `approval_needed`, and [`App::second_factor_wait`] finishes it. Open
    /// with `open_or_create(true)` afterwards, as such installs always did.
    pub fn account_login_plain(
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
            start_waiting(
                core,
                sign_in_new_device(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    Then::Enrol { secret_key },
                    false,
                ),
            )
        })
    }

    /// **Test fixture.** Recovers onto this empty computer the old way:
    /// the identity back with the kit and the account password, and the
    /// device's keys in the plain key file. The same wait for a security
    /// key as [`App::account_login_plain`].
    pub fn recover_plain(
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
            start_waiting(
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
                    false,
                ),
            )
        })
    }
}
