fn main() {
    // A build with LOCKDOWN_DEVTOOLS=off (a tagged release) must not carry
    // the developer tools (docs/plan/40-devtools.md §5), nor the updater's
    // test-only endpoint and key (docs/plan/60-auto-update.md §4).
    println!("cargo:rerun-if-env-changed=LOCKDOWN_DEVTOOLS");
    if std::env::var("LOCKDOWN_DEVTOOLS").as_deref() == Ok("off")
        && (std::env::var_os("CARGO_FEATURE_DEV_RESET").is_some()
            || std::env::var_os("CARGO_FEATURE_DEMO_VAULT").is_some()
            || std::env::var_os("CARGO_FEATURE_UPDATE_TEST").is_some())
    {
        panic!(
            "LOCKDOWN_DEVTOOLS=off, but dev-reset, demo-vault or update-test is on: a release never carries them"
        );
    }
    updater_key();
    #[cfg(feature = "shell")]
    tauri_build::build();
}

/// The public key every update must be signed with
/// (docs/plan/60-auto-update.md §2): `updater.pub` beside this file, as
/// `tauri signer generate` writes it (one line of base64). Without the file
/// the app is built with updates off and says so in Settings. The app
/// checks the key's form when it starts; here it is only carried in.
fn updater_key() {
    const FILE: &str = "updater.pub";
    // Cargo reruns a build script whose watched file is missing on every
    // build, so the folder is watched while the file is absent: adding the
    // key then rebuilds, and a build without it stays cached.
    let path = std::path::Path::new(FILE);
    if path.exists() {
        println!("cargo:rerun-if-changed={FILE}");
    } else {
        println!("cargo:rerun-if-changed=.");
    }
    let key = std::fs::read_to_string(path).unwrap_or_default();
    let key = key.trim();
    assert!(
        key.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')),
        "{FILE} must hold one line of base64, as `tauri signer generate` writes the public key"
    );
    println!("cargo:rustc-env=LOCKDOWN_UPDATER_PUBKEY={key}");
}
