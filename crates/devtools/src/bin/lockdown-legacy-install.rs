//! `lockdown-legacy-install --root DIR [--data DIR]`: makes a passwordless
//! install, as Lockdown did before every vault had a master password, for
//! the end-to-end tests that check such an install still opens
//! (docs/plan/43-cli-sealed.md). A developer tool: never in a release.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut data: Option<PathBuf> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--root") => root = args.next().map(PathBuf::from),
            Some("--data") => data = args.next().map(PathBuf::from),
            _ => {
                eprintln!("usage: lockdown-legacy-install --root DIR [--data DIR]");
                return ExitCode::from(2);
            }
        }
    }
    let Some(root) = root else {
        eprintln!("usage: lockdown-legacy-install --root DIR [--data DIR]");
        return ExitCode::from(2);
    };
    match lockdown_devtools::legacy::make(&root, data.as_deref()) {
        Ok(data) => {
            println!(
                "a passwordless install for tests at {} (data {})",
                root.display(),
                data.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lockdown-legacy-install: {e}");
            ExitCode::FAILURE
        }
    }
}
