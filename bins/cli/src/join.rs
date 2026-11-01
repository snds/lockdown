//! A computer added from the command line is sealed too
//! (docs/plan/44-cli-join-sealed.md; D514).
//!
//! `lockdown account login` and `lockdown recover` on a computer that
//! holds nothing yet join the account through the app's own core
//! (`App::account_login_sealed`, `App::recover_sealed`): the new device's
//! keys stay in memory until the sign-in proves the password, and are
//! sealed under it before the identity names them, so no plain key file is
//! ever written. The account's password is this computer's master password
//! from then on, as on a computer the desktop joins; every later command
//! asks for it.
//!
//! **The password** comes the way every master password does (D466): the
//! first stdin line with `--master-password-stdin`, `LOCKDOWN_MASTER_PASSWORD`,
//! or, at a terminal, a prompt with echo off. `--password-stdin` and
//! `LOCKDOWN_PASSWORD` keep their meaning, the account's password: given
//! as well, they must be the same password, and alone they never seal (doc
//! 43's "Do not" list). With no terminal and no master password the
//! command refuses in words before anything is written.

use std::io::IsTerminal;

use lockdown_app::ErrorCode;
use lockdown_vault::ItemState;
use zeroize::Zeroizing;

use crate::app::{App, Paths};
use crate::{core_app, master};

/// Said when there is nothing to seal with and no terminal to ask at.
pub const NEEDS_PASSWORD: &str = "Joining seals this computer with your account's password, which becomes its master password here. Give it with --master-password-stdin or LOCKDOWN_MASTER_PASSWORD, or run the command in a terminal to be asked.";

/// Said when the account's password and the master password differ.
pub const NOT_THE_SAME: &str = "Your account's password becomes this computer's master password, so they are one password. Give it once with --master-password-stdin or LOCKDOWN_MASTER_PASSWORD, or give the same one with --password-stdin or LOCKDOWN_PASSWORD.";

/// Said once a join has sealed this computer.
pub const SEALED: &str = "this computer is locked with your account's password as its master password; commands here ask for it (--master-password-stdin, LOCKDOWN_MASTER_PASSWORD, or the prompt)";

/// Said when the account wants a security key or an emailed approval for
/// a new device, which this command can't wait for.
pub const NEEDS_APPROVAL: &str = "Your account asks for a security key or an emailed approval before a new device joins, and the command line can't wait for one yet. Add this computer from the Lockdown app instead. Nothing was kept here.";

/// How this computer joins.
pub enum Way {
    /// Through the account slot: the password and, when the slot wants
    /// it, the kit's Secret Key (`account login`).
    Account {
        /// The kit's Secret Key, when given.
        secret_key: Option<String>,
    },
    /// With the whole recovery kit (`recover`).
    Kit {
        /// The kit's Secret Key.
        secret_key: String,
        /// The kit's recovery code.
        recovery_code: String,
    },
}

/// The master password a join seals with: from the D466 routes, never from
/// the account's own `--password-stdin` or `LOCKDOWN_PASSWORD`, which must
/// match it when given. Runs before anything is written.
fn password(
    server: &str,
    login: &str,
    account_password: Option<&str>,
) -> Result<Zeroizing<String>, String> {
    let password = match master::given() {
        Some(p) => p,
        None if std::io::stdin().is_terminal() => master::prompt(&format!(
            "Password for {login} at {server} (this computer's master password from now on): "
        ))?,
        None => return Err(NEEDS_PASSWORD.into()),
    };
    if let Some(given) = account_password
        && given != password.as_str()
    {
        return Err(NOT_THE_SAME.into());
    }
    Ok(password)
}

/// `lockdown account login` or `lockdown recover` on an empty computer.
pub fn join(
    paths: Paths,
    server: &str,
    login: &str,
    account_password: Option<String>,
    way: Way,
) -> Result<Vec<String>, String> {
    let account_password = account_password.map(Zeroizing::new);
    let password = password(
        server,
        login,
        account_password.as_deref().map(String::as_str),
    )?;
    let core = core_app(&paths);
    let joined = match &way {
        Way::Account { secret_key } => {
            core.account_login_sealed(server, login, &password, secret_key.as_deref())
        }
        Way::Kit {
            secret_key,
            recovery_code,
        } => core.recover_sealed(server, login, &password, secret_key, recovery_code),
    };
    drop(core);
    let mut lines = joined.map_err(|e| match e.code {
        ErrorCode::ApprovalNeeded => NEEDS_APPROVAL.to_string(),
        _ if e.message.contains("needs the Secret Key") => format!(
            "{}: pass it with --secret-key-stdin or LOCKDOWN_SECRET_KEY (or use `lockdown recover` with the whole kit)",
            e.message
        ),
        _ => e.message,
    })?;
    // The rest of this run opens the vault with the same password, without
    // asking again.
    master::keep(password);
    lines.push(SEALED.into());
    match way {
        Way::Account { .. } => lines.push("run `lockdown sync` to pull the vault".into()),
        Way::Kit { .. } => {
            // The vault: pulled the way `init` does on a device with an
            // account.
            let app = App::init(paths, None)?;
            lines.push(format!(
                "vault {}: {} ({} live items)",
                app.vault().context().vault_id,
                if app.vault_created() {
                    "created (the account had none)"
                } else {
                    "pulled"
                },
                app.vault()
                    .items()
                    .filter(|i| i.state == ItemState::Live && !i.kind.hidden())
                    .count()
            ));
        }
    }
    Ok(lines)
}
