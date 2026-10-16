//! `lockdown`: the command-line tool (03-architecture §2 `cli` row and §4).
//!
//! **Status: skeleton with two real commands.** `code` prints the current
//! one-time code for an `otpauth://` or `steam://` URI and how long it is
//! valid; `match` says how a saved URL matches a current one. The vault,
//! the SSH agent and the local-socket pairing arrive with the store.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Every way you sign in. Locked down, even from us.
#[derive(Parser, Debug)]
#[command(name = "lockdown", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out = match cli.command {
        Command::Code { uri } => code_lines(&uri, now),
        Command::Match { saved, current } => match_line(&saved, &current).map(|l| vec![l]),
    };
    match out {
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
}
