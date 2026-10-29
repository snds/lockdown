//! The master password on the command line (docs/plan/39-first-run.md §1.5,
//! D466).
//!
//! Every command that opens a vault sealed under a master password takes
//! it from the global `--master-password-stdin` (the first line of
//! standard input, before any line the command itself reads), from
//! `LOCKDOWN_MASTER_PASSWORD`, or, when standard input is a terminal and
//! neither is given, from a prompt that doesn't echo. It is never an
//! argument. `LOCKDOWN_PASSWORD` keeps its older meaning (the account's
//! password, or a login's password for `add`), so a script that sets it
//! for one of those never unlocks by accident.
//!
//! The stdin lines that follow the master password are kept for the
//! command's own secrets ([`take_rest`]).

use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::Path;
use std::sync::Mutex;

use zeroize::Zeroizing;

/// The variable that holds the master password for a script.
pub const VAR: &str = "LOCKDOWN_MASTER_PASSWORD";

/// Where the master password comes from in this run.
enum Source {
    /// The first stdin line, read already.
    Given(Zeroizing<String>),
    /// The variable, or a prompt.
    Elsewhere,
}

static SOURCE: Mutex<Option<Source>> = Mutex::new(None);
static REST: Mutex<Option<Zeroizing<String>>> = Mutex::new(None);

/// Reads standard input once when `--master-password-stdin` was given: the
/// first line is the master password, the rest waits for the command.
pub fn read_stdin(from_stdin: bool) -> Result<(), String> {
    let source = if from_stdin {
        let mut text = Zeroizing::new(String::new());
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("couldn't read from stdin: {e}"))?;
        let (first, rest) = match text.split_once('\n') {
            Some((first, rest)) => (first.trim_end_matches('\r'), rest),
            None => (text.as_str(), ""),
        };
        if first.is_empty() {
            return Err("no master password on stdin".into());
        }
        *REST.lock().unwrap_or_else(|p| p.into_inner()) = Some(Zeroizing::new(rest.to_string()));
        Source::Given(Zeroizing::new(first.to_string()))
    } else {
        Source::Elsewhere
    };
    *SOURCE.lock().unwrap_or_else(|p| p.into_inner()) = Some(source);
    Ok(())
}

/// The stdin lines after the master password, when it was read from there.
pub fn take_rest() -> Option<Zeroizing<String>> {
    REST.lock().unwrap_or_else(|p| p.into_inner()).take()
}

/// The master password for the vault in `data`.
pub fn master_password(data: &Path) -> Result<Zeroizing<String>, String> {
    if let Some(Source::Given(p)) = SOURCE.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        return Ok(p.clone());
    }
    if let Ok(v) = std::env::var(VAR)
        && !v.is_empty()
    {
        return Ok(Zeroizing::new(v));
    }
    if std::io::stdin().is_terminal() {
        let name = lockdown_app::onboarding::Onboarding::load(data)
            .name
            .unwrap_or_else(|| "this vault".into());
        return prompt(&format!("Master password for {name}: "));
    }
    Err(format!(
        "This vault is locked with a master password. Give it with --master-password-stdin or {VAR}, or run the command in a terminal to be asked."
    ))
}

/// A new master password: the stdin line or the variable when one was
/// given (`from`), else asked twice at a terminal.
pub fn new_password(from: Option<Zeroizing<String>>) -> Result<Zeroizing<String>, String> {
    if let Some(p) = from {
        return Ok(p);
    }
    if !std::io::stdin().is_terminal() {
        return Err(
            "no master password given: pass --password-stdin, set LOCKDOWN_PASSWORD, or run the command in a terminal to be asked"
                .into(),
        );
    }
    let first = prompt("Choose a master password: ")?;
    let again = prompt("Type it again: ")?;
    if *first != *again {
        return Err("The two passwords don't match yet.".into());
    }
    Ok(first)
}

/// Said once per command on an install that has no master password yet.
pub fn no_master_password_notice() {
    eprintln!(
        "lockdown: This vault has no master password yet. Run lockdown password set --name NAME, or open the Lockdown app."
    );
}

/// Asks on the terminal without echoing what is typed.
pub fn prompt(text: &str) -> Result<Zeroizing<String>, String> {
    if cfg!(not(unix)) {
        return Err(format!(
            "this terminal can't hide what you type here; give the password with --master-password-stdin or {VAR}"
        ));
    }
    let mut err = std::io::stderr();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
    let _echo = EchoOff::new();
    let mut line = Zeroizing::new(String::new());
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("couldn't read the password: {e}"))?;
    drop(_echo);
    let _ = err.write_all(b"\n");
    let trimmed = Zeroizing::new(line.trim_end_matches(['\n', '\r']).to_string());
    if trimmed.is_empty() {
        return Err("no password was typed".into());
    }
    Ok(trimmed)
}

/// The terminal's echo, off until dropped.
struct EchoOff {
    #[cfg(unix)]
    saved: Option<libc::termios>,
}

impl EchoOff {
    fn new() -> Self {
        #[cfg(unix)]
        {
            // SAFETY: `termios` is plain data, filled by `tcgetattr` before use;
            // both calls take the stdin descriptor, which stays open.
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
                    return Self { saved: None };
                }
                let saved = t;
                t.c_lflag &= !libc::ECHO;
                t.c_lflag |= libc::ECHONL;
                if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) != 0 {
                    return Self { saved: None };
                }
                Self { saved: Some(saved) }
            }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(saved) = self.saved {
            // SAFETY: restores the settings read in `new` on the same descriptor.
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved);
            }
        }
    }
}
