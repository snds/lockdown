//! Every new vault sealed, on the command line too
//! (docs/plan/43-cli-sealed.md).
//!
//! `lockdown init` makes the vault the way the app's first run does
//! (docs/plan/39-first-run.md §1.2, §1.3): the device's keys sealed under a
//! master password, the local account on the server that runs inside the
//! command, and the recovery kit, printed or saved once. There is no way to
//! make a vault without a master password any more.
//!
//! - **At a terminal** with nothing given, it asks for the name (the
//!   computer account's user name offered) and the master password twice
//!   with echo off, after one line on what the password is for.
//! - **In a script**, `--name` with the password from
//!   `--master-password-stdin` or `LOCKDOWN_MASTER_PASSWORD` (or, as before,
//!   `init --password-stdin` or `LOCKDOWN_PASSWORD`) runs without a prompt.
//! - **Neither**: refused, in words naming both ways, before anything is
//!   written.
//! - **Already set up**: opens what is there and prints its status, as
//!   `init` always has, so running it again is safe. An install made
//!   before this (no master password) still opens and says so.
//!
//! `account register` on a vault sealed this way moves its local account to
//! the server named (D473, the app's "Move to your own server"): the same
//! identity, kit and vaults, registered there with the master password as
//! the account's password.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use lockdown_app::local_unlock::{self, KeyFile};
use zeroize::Zeroizing;

use crate::app::{App, Paths};
use crate::master;
use crate::{InitArgs, Secrets, core_app, deliver_kit, kit_target_free};

/// The line said at a terminal before the password is asked for: what it
/// protects, that nobody can recover it, and the way back (doc 39 screen 1).
pub const EXPECTATION: &str = "Your master password opens your vault, and Lockdown can't see it or reset it. If you forget it, only your recovery kit gets you back in; you'll get the kit next.";

/// Said when there is no terminal to ask at and something is missing.
pub const NEEDS_NAME_AND_PASSWORD: &str = "lockdown init needs a name and a master password. Give them with --name NAME and --master-password-stdin (or LOCKDOWN_MASTER_PASSWORD), or run lockdown init in a terminal to be asked.";

/// Said after a printed kit: where it went.
pub const KIT_PRINTED: &str = "Your recovery kit is printed above, shown once. Print it or save it now and keep it away from this computer: if you forget your master password, it's the only way back in.";

/// `lockdown init`.
pub fn init(paths: Paths, args: InitArgs, persona: Option<&str>) -> Result<Vec<String>, String> {
    match local_unlock::key_file(&paths.data) {
        KeyFile::Sealed | KeyFile::Plain => return again(paths, args, persona),
        KeyFile::None => {}
    }
    if !crate::app::root_is_empty(&paths)? {
        return Err(format!(
            "An ecosystem identity is already at {}, and this computer has no Lockdown keys for it. Join it with lockdown account login or lockdown recover, or choose another --root.",
            paths.root.display()
        ));
    }
    kit_target_free(args.save.as_ref())?;
    let terminal = std::io::stdin().is_terminal();
    let password = given_password(&args)?;
    if (args.name.is_none() || password.is_none()) && !terminal {
        // Nothing has been written yet, and nothing will be.
        return Err(NEEDS_NAME_AND_PASSWORD.into());
    }
    if args.name.is_none() || password.is_none() {
        eprintln!("{EXPECTATION}");
    }
    let name = match &args.name {
        Some(n) => n.clone(),
        None => ask_name()?,
    };
    let name = lockdown_app::onboarding::check_name(&name)?;
    let password = match password {
        Some(p) => {
            lockdown_app::check_master_password(&p, Some(&name))?;
            p
        }
        None => ask_new_password(&name)?,
    };
    let core = core_app(&paths);
    let made = core
        .first_run_create(&name, &password)
        .map_err(|e| e.message)?;
    core.onboarding_stage("done").map_err(|e| e.message)?;
    let mut lines = vec![format!(
        "vault created for {name}; it opens only with your master password"
    )];
    if let Some(kit) = made.kit {
        let text = String::from_utf8_lossy(kit.expose()).into_owned();
        lines.push(String::new());
        lines.extend(deliver_kit(text, args.save.as_ref())?);
        if args.save.is_none() {
            lines.push(String::new());
            lines.push(KIT_PRINTED.into());
        }
    }
    Ok(lines)
}

/// `init` on a computer already set up: opens what is there.
fn again(paths: Paths, args: InitArgs, persona: Option<&str>) -> Result<Vec<String>, String> {
    if args.name.is_some() || args.password_stdin || args.save.is_some() {
        return Err(
            "Lockdown is already set up on this computer. lockdown password set gives an older install a master password."
                .into(),
        );
    }
    let app = App::init(paths, persona)?;
    let mut lines = vec![format!(
        "identity {}, vault {}",
        if app.identity_created {
            "created"
        } else {
            "attached"
        },
        if app.vault_created() {
            "created"
        } else {
            "opened"
        }
    )];
    lines.extend(app.status_lines()?);
    Ok(lines)
}

/// The new master password when a script gave it: the first stdin line
/// (`--master-password-stdin`), `LOCKDOWN_MASTER_PASSWORD`, or, as `init
/// --name` took it before, `init --password-stdin` or `LOCKDOWN_PASSWORD`.
fn given_password(args: &InitArgs) -> Result<Option<Zeroizing<String>>, String> {
    let master = master::given();
    let mut secrets = Secrets::read(args.password_stdin)?;
    let account = secrets.password_if_given(args.password_stdin)?;
    match (master, account) {
        (Some(_), Some(_)) if args.password_stdin => Err(
            "Give the master password once: --master-password-stdin or init --password-stdin, not both."
                .into(),
        ),
        (Some(m), _) => Ok(Some(m)),
        (None, Some(a)) => Ok(Some(Zeroizing::new(a))),
        (None, None) => Ok(None),
    }
}

/// The computer account's user name, offered as the name.
fn os_user_name() -> Option<String> {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(|v| v.trim().to_string())
        .find(|v| !v.is_empty() && lockdown_app::onboarding::check_name(v).is_ok())
}

/// Asks for the name at the terminal, offering the user name.
fn ask_name() -> Result<String, String> {
    let offered = os_user_name();
    let question = match &offered {
        Some(n) => format!("Your name, shown when Lockdown is locked [{n}]: "),
        None => "Your name, shown when Lockdown is locked: ".to_string(),
    };
    let mut err = std::io::stderr();
    let _ = err.write_all(question.as_bytes());
    let _ = err.flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("couldn't read the name: {e}"))?;
    let typed = line.trim();
    match (typed.is_empty(), offered) {
        (true, Some(n)) => Ok(n),
        _ => Ok(typed.to_string()),
    }
}

/// Asks for the new master password twice with echo off, refusing a weak
/// one by the app's rule before the second ask.
fn ask_new_password(name: &str) -> Result<Zeroizing<String>, String> {
    let first = master::prompt("Choose a master password: ")?;
    lockdown_app::check_master_password(&first, Some(name))?;
    let again = master::prompt("Type it again: ")?;
    if *first != *again {
        return Err("The two passwords don't match yet.".into());
    }
    Ok(first)
}

/// `account register` on a sealed vault: the local account moves to
/// `server` under `login` (D473), with the master password as the account's
/// password and the kit's Secret Key this computer keeps.
pub fn register(
    paths: &Paths,
    server: &str,
    login: &str,
    account_password: Option<String>,
    secret_key_given: bool,
    kit_file: Option<&std::path::PathBuf>,
) -> Result<Vec<String>, String> {
    let account_password = account_password.map(Zeroizing::new);
    if secret_key_given {
        return Err(
            "This vault keeps its recovery kit's Secret Key and uses it for the account; leave --secret-key-stdin and LOCKDOWN_SECRET_KEY out."
                .into(),
        );
    }
    if kit_file.is_some() {
        return Err(
            "No new recovery kit is made here: your vault keeps the kit it has. Leave --kit-file out."
                .into(),
        );
    }
    let password = master::master_password(&paths.data)?;
    if let Some(given) = &account_password
        && **given != *password
    {
        return Err(
            "On a vault with a master password, the account's password is the master password; give that, or leave --password-stdin and LOCKDOWN_PASSWORD out."
                .into(),
        );
    }
    let core = core_app(paths);
    core.unlock_with_password(&password)
        .map_err(|e| e.message)?;
    let outcome = core
        .move_to_server(server, login, &password)
        .map_err(|e| e.message)?;
    let mut lines = vec![
        format!("account {login} registered at {server}; your vault moved there from this computer"),
        "your recovery kit stays the same; its Secret Key protects the account beside your master password".into(),
        format!(
            "synced: pushed {}, pulled {}, vault head seq {}",
            outcome.pushed, outcome.pulled, outcome.head_seq
        ),
    ];
    lines.extend(outcome.lines);
    Ok(lines)
}

/// Whether the vault in `data` is sealed under a master password.
pub fn is_sealed(data: &Path) -> bool {
    local_unlock::key_file(data) == KeyFile::Sealed
}
