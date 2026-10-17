//! `lockdown`: the command-line tool (03-architecture §2 `cli` row and §4).
//!
//! The vault commands are the proof that the store and the identity
//! adoption path work end to end from a shell: `init` opens or creates the
//! ecosystem identity at the platform root and the persona's first vault;
//! `add`, `list`, `show` and `status` open the same vault again with
//! nothing but that root and this device's keystore (D11). `code` and
//! `match` need no vault. `account register` and `account login` make
//! this device an account on a Lockdown server; `sync` pulls, merges and
//! pushes (03-architecture §6); `device list` and `device revoke` manage
//! the identity's devices (D73). The SSH agent and local-socket pairing
//! are later.
//!
//! `kit create`, `kit verify` and `recover` are the recovery kit (D69,
//! D70): the Secret Key half protects the account slot beside the password.
//! `attach`, `attachments` and `fetch` are files on items (02-model §6);
//! their blobs travel with the item on `sync`. `audit` and `checkpoint`
//! are key transparency (04-security §6): `sync` checks the server's
//! public log before and after every run; `audit` prints what it checked;
//! `checkpoint` exports a signed page another device can verify.
//!
//! A password, a Secret Key or a recovery code never arrives as an
//! argument, where it would land in shell history and `ps`: they are read
//! from standard input (`--password-stdin`, `--secret-key-stdin`,
//! `--kit-stdin`, one per line in that order) or from `LOCKDOWN_PASSWORD`,
//! `LOCKDOWN_SECRET_KEY` and `LOCKDOWN_RECOVERY_CODE`.

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
    /// Attach a file to an item (a new version; the file is sealed in
    /// chunks under its own key).
    Attach {
        /// The item id, as `list` prints it.
        id: String,
        /// The file to attach. Its name and type are sealed with it.
        file: PathBuf,
    },
    /// List an item's attachments.
    Attachments {
        /// The item id, as `list` prints it.
        id: String,
    },
    /// Write an attachment out to a file (fetched from the server first
    /// when this device doesn't hold it).
    Fetch {
        /// The item id, as `list` prints it.
        id: String,
        /// The attachment's file name (or its id), as `attachments` prints it.
        name: String,
        /// Where to write it. Refused if the file exists.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },
    /// Where everything is and the vault's newest head.
    Status,
    /// The account on a Lockdown server.
    Account {
        #[command(subcommand)]
        what: AccountWhat,
    },
    /// Pull from the server, merge, push, and advance the vault head. The
    /// server's public log is checked first and after.
    Sync,
    /// Print and verify every entry about this account in the server's
    /// public log (key transparency). Changes nothing.
    Audit,
    /// Export a signed checkpoint (the server's tree head and this device's
    /// newest heads) to keep or compare with another device; or verify one.
    Checkpoint {
        /// Write the page here. Default: lockdown-checkpoint-<date>.txt in
        /// the current directory. Refused if the file exists.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Verify a checkpoint page another device exported instead.
        #[arg(long, value_name = "FILE", conflicts_with = "out")]
        verify: Option<PathBuf>,
    },
    /// The devices enrolled in the identity.
    Device {
        #[command(subcommand)]
        what: DeviceWhat,
    },
    /// The recovery kit: one printed page with the Secret Key and the
    /// recovery code.
    Kit {
        #[command(subcommand)]
        what: KitWhat,
    },
    /// On a device with nothing on it: get the identity and the vault back
    /// from the server with the recovery kit and the account password.
    Recover(RecoverArgs),
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
enum DeviceWhat {
    /// List every device, this one and revoked ones marked.
    List,
    /// Revoke another device: nothing it signs from now on is accepted by
    /// the server or by any other device. Needs the account.
    Revoke {
        /// The device id (or a unique prefix of at least 8 characters, or
        /// its exact name), as `device list` prints it.
        id: String,
    },
}

#[derive(Subcommand, Debug)]
enum KitWhat {
    /// Make the recovery kit and print it (or save it where you say). With
    /// an account, the account password is needed: its slot takes the
    /// new Secret Key.
    Create {
        /// Replace a kit that exists. The old page stops working.
        #[arg(long)]
        replace: bool,
        /// Write the kit to this file (readable only by you) instead of
        /// printing it. Refused if the file exists.
        #[arg(long, value_name = "FILE")]
        save: Option<PathBuf>,
        /// Read the account password from standard input (else
        /// LOCKDOWN_PASSWORD).
        #[arg(long)]
        password_stdin: bool,
    },
    /// Type the kit back in and check it opens this identity. Changes
    /// nothing unless --drill.
    Verify {
        /// Read the Secret Key and the recovery code from standard input,
        /// one per line (else LOCKDOWN_SECRET_KEY and LOCKDOWN_RECOVERY_CODE).
        #[arg(long)]
        kit_stdin: bool,
        /// Also give the account password (first line of standard input,
        /// or LOCKDOWN_PASSWORD) to check the Secret Key against the
        /// account slot.
        #[arg(long)]
        password_stdin: bool,
        /// Record a passing check on the profile (the recovery drill).
        #[arg(long)]
        drill: bool,
    },
}

#[derive(Args, Debug)]
struct RecoverArgs {
    /// The server, as a URL (https://host[:port]).
    #[arg(long, value_name = "URL")]
    server: String,
    /// The login name the account is filed under.
    #[arg(long, value_name = "NAME")]
    login: String,
    /// Read the account password from the first line of standard input
    /// (else LOCKDOWN_PASSWORD).
    #[arg(long)]
    password_stdin: bool,
    /// Read the Secret Key and the recovery code from the next two lines
    /// of standard input (else LOCKDOWN_SECRET_KEY and LOCKDOWN_RECOVERY_CODE).
    #[arg(long)]
    kit_stdin: bool,
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
    /// Read the kit's Secret Key from the next line of standard input
    /// (else LOCKDOWN_SECRET_KEY). Needed to enrol a new device through the
    /// account, and to register when a kit already exists.
    #[arg(long)]
    secret_key_stdin: bool,
    /// Registration only: write the recovery kit that is made to this file
    /// (readable only by you) instead of printing it.
    #[arg(long, value_name = "FILE")]
    kit_file: Option<PathBuf>,
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

/// Secrets read from standard input, one per line, in the order the flags
/// name them: password, Secret Key, recovery code. Each is taken at most
/// once; the environment stands in for a line that was not asked for.
struct Secrets {
    lines: std::vec::IntoIter<String>,
}

impl Secrets {
    /// Reads standard input when any `*_stdin` flag asks for it.
    fn read(any_stdin: bool) -> Result<Self, String> {
        let mut text = String::new();
        if any_stdin {
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| format!("couldn't read from stdin: {e}"))?;
        }
        Ok(Self {
            lines: text
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
                .into_iter(),
        })
    }

    fn take(
        &mut self,
        from_stdin: bool,
        var: &str,
        what: &str,
        flag: &str,
    ) -> Result<String, String> {
        if from_stdin {
            return match self.lines.next() {
                Some(line) if !line.trim().is_empty() => Ok(line),
                _ => Err(format!("no {what} on stdin")),
            };
        }
        match std::env::var(var) {
            Ok(v) if !v.trim().is_empty() => Ok(v),
            _ => Err(format!("no {what} given: pass {flag} or set {var}")),
        }
    }

    fn password(&mut self, from_stdin: bool) -> Result<String, String> {
        self.take(
            from_stdin,
            "LOCKDOWN_PASSWORD",
            "password",
            "--password-stdin",
        )
    }

    fn secret_key(&mut self, from_stdin: bool) -> Result<String, String> {
        self.take(
            from_stdin,
            "LOCKDOWN_SECRET_KEY",
            "Secret Key",
            "--secret-key-stdin (or --kit-stdin)",
        )
    }

    fn recovery_code(&mut self, from_stdin: bool) -> Result<String, String> {
        self.take(
            from_stdin,
            "LOCKDOWN_RECOVERY_CODE",
            "recovery code",
            "--kit-stdin",
        )
    }

    /// The Secret Key when asked for or set in the environment; `None`
    /// when neither.
    fn secret_key_if_given(&mut self, from_stdin: bool) -> Result<Option<String>, String> {
        if from_stdin || std::env::var_os("LOCKDOWN_SECRET_KEY").is_some_and(|v| !v.is_empty()) {
            self.secret_key(from_stdin).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The password when asked for or set in the environment.
    fn password_if_given(&mut self, from_stdin: bool) -> Result<Option<String>, String> {
        if from_stdin || std::env::var_os("LOCKDOWN_PASSWORD").is_some_and(|v| !v.is_empty()) {
            self.password(from_stdin).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// The password, from stdin or the environment, never from an argument.
fn read_password(from_stdin: bool) -> Result<String, String> {
    Secrets::read(from_stdin)?.password(from_stdin)
}

/// Refuses a save target that is already taken, before anything is
/// written, so a kit that can't be delivered never rewrites the slot.
fn kit_target_free(save: Option<&PathBuf>) -> Result<(), String> {
    match save {
        Some(path) if path.exists() => Err(format!(
            "{} exists already; choose another file for the kit",
            path.display()
        )),
        _ => Ok(()),
    }
}

/// Writes the recovery kit where the person said (readable only by them;
/// never over a file that exists), or returns it to print.
fn deliver_kit(text: String, save: Option<&PathBuf>) -> Result<Vec<String>, String> {
    match save {
        Some(path) => {
            if path.exists() {
                return Err(format!(
                    "{} exists already; choose another file for the kit",
                    path.display()
                ));
            }
            lockdown_store::write_private(path, text.as_bytes())
                .map_err(|e| format!("couldn't write the kit to {}: {e}", path.display()))?;
            Ok(vec![format!(
                "recovery kit saved to {} (readable only by you); print it and put the page away",
                path.display()
            )])
        }
        None => Ok(text.lines().map(str::to_string).collect()),
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
        Command::Attach { id, file } => {
            let mut app = App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?;
            let (att_id, name, size) = app.attach_file(&id, &file)?;
            Ok(vec![format!(
                "attached {name} ({size} bytes) to {id} as {att_id}"
            )])
        }
        Command::Attachments { id } => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?.attachment_lines(&id)
        }
        Command::Fetch { id, name, out } => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?
                .fetch_attachment(&id, &name, &out)
        }
        Command::Status => {
            App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?.status_lines()
        }
        Command::Account { what } => {
            let paths = Paths::resolve(cli.root, cli.data)?;
            match what {
                AccountWhat::Register(args) => {
                    let mut secrets = Secrets::read(args.password_stdin || args.secret_key_stdin)?;
                    let password = secrets.password(args.password_stdin)?;
                    let secret_key = secrets.secret_key_if_given(args.secret_key_stdin)?;
                    let (mut lines, kit) = App::open_existing(paths, persona)?.account_register(
                        &args.server,
                        &args.login,
                        &password,
                        secret_key.as_deref(),
                    )?;
                    if let Some(text) = kit {
                        lines.push(String::new());
                        lines.extend(deliver_kit(text, args.kit_file.as_ref())?);
                    }
                    Ok(lines)
                }
                AccountWhat::Login(args) => {
                    let mut secrets = Secrets::read(args.password_stdin || args.secret_key_stdin)?;
                    let password = secrets.password(args.password_stdin)?;
                    let secret_key = secrets.secret_key_if_given(args.secret_key_stdin)?;
                    if app::root_is_empty(&paths)? {
                        App::enrol_from_account(
                            paths,
                            &args.server,
                            &args.login,
                            &password,
                            secret_key.as_deref(),
                        )
                    } else {
                        App::open_existing(paths, persona)?.account_login(
                            &args.server,
                            &args.login,
                            &password,
                            secret_key.as_deref(),
                        )
                    }
                }
            }
        }
        Command::Kit { what } => {
            let paths = Paths::resolve(cli.root, cli.data)?;
            match what {
                KitWhat::Create {
                    replace,
                    save,
                    password_stdin,
                } => {
                    let mut secrets = Secrets::read(password_stdin)?;
                    let password = secrets.password_if_given(password_stdin)?;
                    let target = save.clone();
                    let (text, notes) = App::open_existing(paths, persona)?.kit_create(
                        replace,
                        password.as_deref(),
                        || kit_target_free(target.as_ref()),
                    )?;
                    let mut lines = deliver_kit(text, save.as_ref())?;
                    lines.push(String::new());
                    lines.extend(notes);
                    Ok(lines)
                }
                KitWhat::Verify {
                    kit_stdin,
                    password_stdin,
                    drill,
                } => {
                    let mut secrets = Secrets::read(kit_stdin || password_stdin)?;
                    let password = secrets.password_if_given(password_stdin)?;
                    let secret_key = secrets.secret_key(kit_stdin)?;
                    let recovery_code = secrets.recovery_code(kit_stdin)?;
                    App::open_existing(paths, persona)?.kit_verify(
                        &secret_key,
                        &recovery_code,
                        password.as_deref(),
                        drill,
                    )
                }
            }
        }
        Command::Recover(args) => {
            let paths = Paths::resolve(cli.root, cli.data)?;
            let mut secrets = Secrets::read(args.password_stdin || args.kit_stdin)?;
            let password = secrets.password(args.password_stdin)?;
            let secret_key = secrets.secret_key(args.kit_stdin)?;
            let recovery_code = secrets.recovery_code(args.kit_stdin)?;
            App::recover(
                paths,
                &args.server,
                &args.login,
                &password,
                &secret_key,
                &recovery_code,
            )
        }
        Command::Sync => App::open_for_sync(Paths::resolve(cli.root, cli.data)?, persona)?.sync(),
        Command::Audit => App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?.audit(),
        Command::Checkpoint { out, verify } => {
            let mut app = App::open_existing(Paths::resolve(cli.root, cli.data)?, persona)?;
            match verify {
                Some(file) => {
                    let text = std::fs::read_to_string(&file)
                        .map_err(|e| format!("couldn't read {}: {e}", file.display()))?;
                    app.checkpoint_verify(&text)
                }
                None => {
                    let path = out.unwrap_or_else(|| {
                        PathBuf::from(format!(
                            "lockdown-checkpoint-{}.txt",
                            lockdown_sync::engine::civil_date(now as i64)
                        ))
                    });
                    if path.exists() {
                        return Err(format!(
                            "{} exists already; choose another file with --out",
                            path.display()
                        ));
                    }
                    let (cp, text) = app.checkpoint_export()?;
                    lockdown_store::write_private(&path, text.as_bytes())
                        .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
                    Ok(vec![
                        format!(
                            "checkpoint written to {} (tree head of {} entries, fingerprint {})",
                            path.display(),
                            cp.sth.size,
                            &cp.fingerprint().map_err(|e| e.to_string())?.to_hex()[..16]
                        ),
                        "keep it, or compare it on another device with `lockdown checkpoint --verify FILE`".into(),
                    ])
                }
            }
        }
        Command::Device { what } => {
            let paths = Paths::resolve(cli.root, cli.data)?;
            match what {
                DeviceWhat::List => App::open_existing(paths, persona)?.device_lines(),
                DeviceWhat::Revoke { id } => App::open_existing(paths, persona)?.device_revoke(&id),
            }
        }
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
        for args in [
            RecoverArgs::augment_args(clap::Command::new("x")),
            AccountArgs::augment_args(clap::Command::new("x")),
        ] {
            assert!(
                args.get_arguments()
                    .all(|a| !["secret_key", "recovery_code", "kit"].contains(&a.get_id().as_str()))
            );
        }
        // With neither source set, a clear error.
        unsafe { std::env::remove_var("LOCKDOWN_PASSWORD") };
        assert!(read_password(false).is_err());
    }

    #[test]
    fn stdin_secrets_come_in_the_flag_order() {
        let mut secrets = Secrets {
            lines: vec!["pw".to_string(), "A1-x".to_string(), "code".to_string()].into_iter(),
        };
        assert_eq!(secrets.password(true).unwrap(), "pw");
        assert_eq!(secrets.secret_key(true).unwrap(), "A1-x");
        assert_eq!(secrets.recovery_code(true).unwrap(), "code");
        assert!(secrets.recovery_code(true).is_err());
    }
}
