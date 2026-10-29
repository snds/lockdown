fn main() {
    // A build with LOCKDOWN_DEVTOOLS=off (a tagged release) must not carry
    // the developer tools (docs/plan/40-devtools.md §5).
    println!("cargo:rerun-if-env-changed=LOCKDOWN_DEVTOOLS");
    if std::env::var("LOCKDOWN_DEVTOOLS").as_deref() == Ok("off")
        && (std::env::var_os("CARGO_FEATURE_DEV_RESET").is_some()
            || std::env::var_os("CARGO_FEATURE_DEMO_VAULT").is_some())
    {
        panic!(
            "LOCKDOWN_DEVTOOLS=off, but dev-reset or demo-vault is on: a release never carries them"
        );
    }
    #[cfg(feature = "shell")]
    tauri_build::build();
}
