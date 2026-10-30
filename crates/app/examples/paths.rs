//! Where this computer's Lockdown keeps its files, as the app decides it
//! at start (docs/plan/41-mac-paths.md): the one-time move, the identity
//! root and the data directory, the folder made by the probe, and the
//! browser host's answer beside the app's. The macOS CI job runs it with
//! a fresh `$HOME` and checks nothing lands in `Library/Group Containers`.
//!
//! ```text
//! cargo run -p lockdown-app --example paths
//! ```
//!
//! Prints `key=value` lines and exits non-zero when the app and the host
//! disagree or the probe fails.

fn main() {
    let check = lockdown_app::paths::mac_check();
    println!("signed={}", check.signed);
    println!("team={}", check.team.as_deref().unwrap_or(""));
    println!("groups={}", check.groups.join(","));
    if let Some(problem) = &check.problem {
        println!("signature_problem={problem}");
    }
    println!(
        "store={}",
        match &check.store {
            lockdown_app::paths::MacStore::ApplicationSupport => "application-support".to_string(),
            lockdown_app::paths::MacStore::GroupContainer(g) => format!("group-container:{g}"),
        }
    );
    let settled = lockdown_app::paths::settle_defaults();
    println!("settled={settled:?}");
    let paths = match lockdown_app::paths::at_start() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("root={}", paths.root.display());
    println!("data={}", paths.data.display());
    let host = match lockdown_native_messaging::endpoint::data_dir_from_env() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("the browser host found no data directory: {e}");
            std::process::exit(1);
        }
    };
    println!("host_data={}", host.display());
    if host != paths.data {
        eprintln!("the browser host and the app disagree");
        std::process::exit(1);
    }
    match paths.probe() {
        Ok(p) => println!("probe=identity_present:{}", p.identity_present),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
