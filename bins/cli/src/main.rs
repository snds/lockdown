//! `lockdown`: the command-line tool (03-architecture §2 `cli` row and §4).
//!
//! The vault commands are the proof that the store and the identity
//! adoption path work end to end from a shell: `init` opens or creates the
//! ecosystem identity at the platform root and the persona's first vault;
//! `add`, `list`, `show` and `status` open the same vault again with
//! nothing but that root and this device's keystore (D11). `code` and
//! `match` need no vault. `account register` and `account login` make
//! this device an account on a Lockdown server; `sync` pulls, merges and
//! pushes (03-architecture §6). The SSH agent and local-socket pairing
//! are later.
//!
//! A password never arrives as an argument, where it would land in shell
//! history and `ps`: `--password-stdin` or `LOCKDOWN_PASSWORD`.

mod app;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use crate::app::{App, Paths};

/// Every way you sign in. Locked down, even from us.
#[derive(Parser, Debug)]
#[command(name = "lockdown", version, about)]
struct Cli {
    /// The ecosystem identity root. Default: the platform's shared place
    /// (Linux: $XDG_DATA_HOME/snds/identity).
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    /// Lockdown's own data directory (device key, vault file, memory).
    /// Default: the `lockdown` directory beside the identity root.
    #[arg(long, global = true, value_name = "DIR")]
    data: Option<PathBuf>,
    /// The persona to use, by id or label, when the identity has several.
    #[arg(long, global = true, value_name = "ID|LABEL")]
    persona: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Open the ecosystem identity at the root (or create it) and the
    /// persona's vault (or create it). Safe to run again.
    Init,
    /// Add an item to the vault.
    Add {
        #[command(subcommand)]
        what: AddWhat,
    },
    /// List the items in the vault.
    List,
    /// Show one item. Hidden fields are masked unless --reveal.
    Show {
        /// The item id, as `list` prints it.
        id: String,
        /// Print hidden fields in the clear.
        #[arg(long)]
        reveal: bool,
    },
    /// Change a login's title, username or password (a new version; the
    /// old one stays in history).
    Edit(EditLogin),
    /// Where everything is and the vault's newest head.
    Status,
    /// The account on a Lockdown server.
    Account {
        #[command(subcommand)]
        what: AccountWhat,
    },
    /// Pull from the server, merge, push, and advance the vault head.
    Sync,
    /// Print the current one-time code for an otpauth:// or steam:// URI.
    Code {
        /// The URI. The secret is read from it and never printed.
        uri: String,
    },
    /// Say how a saved URL matches the page you are on.
    Match {
        /// The URL saved in the item.
        saved: String,
        /// The URL of the current page.
        current: String,
    },
}

#[derive(Subcommand, Debug)]
enum AddWhat {
    /// A login: site, username, password.
    Login(AddLogin),
}

#[derive(Subcommand, Debug)]
enum AccountWhat {
    /// Make an account on a server for this identity (OPAQUE: the
    /// password never leaves this device).
    Register(AccountArgs),
    /// Log in. On a device that holds the identity: a fresh session. On an
    /// empty one: enrol this device from the account and its identity backup.
    Login(AccountArgs),
}

#[derive(Args, Debug)]
struct AccountArgs {
    /// The server, as a URL (https://host[:port]).
    #[arg(long, value_name = "URL")]
    server: String,
    /// The login name the account is filed under.
    #[arg(long, value_name = "NAME")]
    login: String,
    /// Read the account password from the first line of standard input.
    /// Without it, the LOCKDOWN_PASSWORD environment variable is used.
    #[arg(long)]
    password_stdin: bool,
}

#[derive(Args, Debug)]
struct EditLogin {
    /// The item id, as `list` prints it.
    id: String,
    /// A new title.
    #[arg(long)]
    title: Option<String>,
    /// A new username.
    #[arg(long, value_name = "NAME")]
    username: Option<String>,
    /// Read a new password from the first line of standard input.
    #[arg(long)]
    password_stdin: bool,
}

#[derive(Args, Debug)]
struct AddLogin {
    /// The site, as a URL.
    #[arg(long, value_name = "URL")]
    site: String,
    /// The username.
    #[arg(long, value_name = "NAME")]
    username: String,
    /// Read the password from the first line of standard input.
    /// Without it, the LOCKDOWN_PASSWORD environment variable is used.
    #[arg(long)]
    password_stdin: bool,
    /// A title. Default: the site's host.
    #[arg(long)]
    title: Option<String>,
}

/// Runs `code` and returns the lines to print.
fn code_lines(uri: &str, now: u64) -> Result<Vec<String>, String> {
    let code = lockdown_codes::parse_uri(uri).map_err(|e| e.to_string())?;
    let value = code.generate_at(now).map_err(|e| e.to_string())?;
    let mut lines = vec![value];
    if let Some(left) = code.next_code_in(now) {
        lines.push(format!("valid for {} s", left.as_secs()));
    }
    Ok(lines)
}

/// Runs `match` and returns the line to print.
fn match_line(saved: &str, current: &str) -> Result<String, String> {
    let saved = lockdown_matcher::SavedUrl::parse(saved, lockdown_matcher::MatchRule::ExactHost)
        .map_err(|e| e.to_string())?;
    let current = lockdown_matcher::Origin::parse(current).map_err(|e| e.to_string())?;
    let decision = lockdown_matcher::decide(
        &saved,
        &current,
        &lockdown_matcher::EquivalentDomains::curated(),
    );
    Ok(format!("{decision:?}"))
}

/// The password, from stdin or the environment, never from an argument.
fn read_password(from_stdin: bool) -> Result<String, String> {
    if from_stdin {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("couldn't read the password from stdin: {e}"))?;
        let line = text.lines().next().unwrap_or("").to_string();
        if line.is_empty() {
            return Err("no password on stdin".into());
        }
        return Ok(line);
    }
    match std::env::var("LOCKDOWN_PASSWORD") {
        Ok(p) if !p.is_empty() => Ok(p),
        _ => Err("no password given: pass --password-stdin or set LOCKDOWN_PASSWORD".into()),
    }
}

fn run(cli: Cli) -> Result<Vec<String>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let persona = cli.persona.as_deref();
    match cli.command {
        Command::Code { uri } => code_lines(&uri, now),
        Command::Match { saved, current } => match_line(&saved, &current).map(|l| vec![l]),
        Command::Init => {
            let app = App::init(Paths::resolve(cli.root, cli.data)?, persona)?;
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
        Command::Add { what } => {
            let AddWhat::Login(login) = what;
            let password = read_password(login.password_stdin)?;
            let mut app = App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?;
            let id = app.add_login(
                &login.site,
                &login.username,
                &password,
                login.title.as_deref(),
            )?;
            Ok(vec![format!("added {id}")])
        }
        Command::List => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?.list_lines()
        }
        Command::Show { id, reveal } => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?
                .show_lines(&id, reveal)
        }
        Command::Edit(edit) => {
            let password = if edit.password_stdin {
                Some(read_password(true)?)
            } else {
                None
            };
            let mut app = App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?;
            let n = app.edit_login(
                &edit.id,
                edit.title.as_deref(),
                edit.username.as_deref(),
                password.as_deref(),
            )?;
            Ok(vec![format!("saved version {n} of {}", edit.id)])
        }
        Command::Status => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?.status_lines()
        }
        Command::Account { what } => {
            let paths = Paths::resolve(cli.root, cli.data)?;
            match what {
                AccountWhat::Register(args) => {
                    let password = read_password(args.password_stdin)?;
                    App::open_existing(paths, persona)?.account_register(
                        &args.server,
                        &args.login,
                        &password,
                    )
                }
                AccountWhat::Login(args) => {
                    let password = read_password(args.password_stdin)?;
                    if app::root_is_empty(&paths)? {
                        App::enrol_from_account(paths, &args.server, &args.login, &password)
                    } else {
                        App::open_existing(paths, persona)?.account_login(
                            &args.server,
                            &args.login,
                            &password,
                        )
                    }
                }
            }
        }
        Command::Sync => App::open_for_sync(Paths::resolve(cli.root, cli.data)?, persona)?.sync(),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lockdown: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_prints_the_rfc_vector_and_never_the_secret() {
        // RFC 6238, SHA-1, 8 digits, T = 59 → 94287082.
        let uri = "otpauth://totp/Example:alice?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8&period=30";
        let lines = code_lines(uri, 59).unwrap();
        assert_eq!(lines[0], "94287082");
        assert_eq!(lines[1], "valid for 1 s");
        assert!(!lines.join("\n").contains("GEZDGNBV"));
        assert!(code_lines("otpauth://totp/x", 0).is_err());
    }

    #[test]
    fn match_reports_the_decision() {
        assert_eq!(
            match_line(
                "https://accounts.example.com/login",
                "https://accounts.example.com/"
            )
            .unwrap(),
            "Exact"
        );
        assert_eq!(
            match_line("https://example.com", "https://other.example").unwrap(),
            "NoMatch"
        );
    }

    #[test]
    fn the_data_dir_sits_beside_the_root() {
        let p = Paths::resolve(Some(PathBuf::from("/x/snds/identity")), None).unwrap();
        assert_eq!(p.data, PathBuf::from("/x/snds/lockdown"));
        let p = Paths::resolve(
            Some(PathBuf::from("/x/identity")),
            Some(PathBuf::from("/elsewhere")),
        )
        .unwrap();
        assert_eq!(p.data, PathBuf::from("/elsewhere"));
    }

    #[test]
    fn no_password_source_is_an_error_not_an_argument() {
        // The password is never a CLI flag.
        assert!(
            AddLogin::augment_args(clap::Command::new("x"))
                .get_arguments()
                .all(|a| a.get_id() != "password")
        );
        assert!(
            AccountArgs::augment_args(clap::Command::new("x"))
                .get_arguments()
                .all(|a| a.get_id() != "password")
        );
        assert!(
            EditLogin::augment_args(clap::Command::new("x"))
                .get_arguments()
                .all(|a| a.get_id() != "password")
        );
        // With neither source set, a clear error.
        unsafe { std::env::remove_var("LOCKDOWN_PASSWORD") };
        assert!(read_password(false).is_err());
    }
}
