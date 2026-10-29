//! Test tooling for the post-quantum status line
//! (docs/plan/30-pq-status-line.md): stands in for a device running an
//! older Lockdown, by writing its record into an identity this machine
//! holds the root key of. The web vault's end-to-end suite uses it to show
//! "waiting for …" and then "on".
//!
//! ```text
//! cargo build -p lockdown-vault --example older-device
//! older-device --root DIR --keystore FILE add "Old laptop" 1    # prints the new device's id
//! older-device --root DIR --keystore FILE set <device-id> 2
//! ```
//!
//! `--root` is the identity root and `--keystore` this device's key file
//! (`device-key.cbor` in Lockdown's data directory). A level of `none`
//! writes a record with no mark at all: a build from before the rollout.
//! The record is signed by the identity's key as an enrolment is, so the
//! next `lockdown sync` publishes it. Not for real accounts: the device it
//! names has no keys of its own and can never sign in.

use std::path::PathBuf;
use std::process::ExitCode;

use ecosystem_identity::FileKeystore;
use lockdown_vault::Id;
use lockdown_vault::test_support::{add_device_at_level, set_device_level};

fn level(text: &str) -> Result<Option<u64>, String> {
    match text {
        "none" => Ok(None),
        n => n
            .parse()
            .map(Some)
            .map_err(|_| format!("'{n}' isn't a level (a number, or none)")),
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let mut root = None;
    let mut keystore = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = it.next().map(PathBuf::from),
            "--keystore" => keystore = it.next().map(PathBuf::from),
            _ => rest.push(a.as_str()),
        }
    }
    let usage = "usage: older-device --root DIR --keystore FILE (add NAME LEVEL | set ID LEVEL)";
    let (Some(root), Some(keystore)) = (root, keystore) else {
        return Err(usage.into());
    };
    let keystore = FileKeystore::open_or_create(&keystore).map_err(|e| e.to_string())?;
    let mut identity = lockdown_vault::identity::open_identity(&root, &keystore, None)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no identity at {}", root.display()))?;
    match rest.as_slice() {
        ["add", name, lvl] => add_device_at_level(&mut identity, &keystore, name, level(lvl)?)
            .map(|id| id.to_string())
            .map_err(|e| e.to_string()),
        ["set", id, lvl] => {
            let id = Id::parse(id).map_err(|e| e.to_string())?;
            set_device_level(&mut identity, &keystore, id, level(lvl)?)
                .map(|()| format!("{id} now marks level {lvl}"))
                .map_err(|e| e.to_string())
        }
        _ => Err(usage.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("older-device: {e}");
            ExitCode::FAILURE
        }
    }
}
