//! What a keychain item does across two builds with different code
//! identities (docs/plan/60-auto-update.md §3), for CI's `keychain-macos`
//! job (`packaging/update/keychain-check.sh`). It goes through the same
//! store the app installs (`lockdown_desktop_lib::keychain`).
//!
//! ```text
//! keychain-probe build                  which build this is (LOCKDOWN_PROBE_BUILD at compile time)
//! keychain-probe mode                   the store's mode for this binary's own signature: keep | read-only
//! keychain-probe write <key> <value>    writes as a keeping store does (as every build did before this slice)
//! keychain-probe quiet-read <key>       reads with the keychain's dialogs off; prints found, absent or refused <code>
//! keychain-probe store-read <key>       the app's own read for this build; prints found or absent
//! ```
//!
//! Exit status 0 unless the call itself failed.

#[cfg(feature = "shell")]
fn main() {
    use lockdown_app::secrets::SecretStore;
    use lockdown_desktop_lib::keychain::{Keychain, Mode, mode};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match arg(0).as_str() {
        "build" => println!("{}", option_env!("LOCKDOWN_PROBE_BUILD").unwrap_or("-")),
        "mode" => println!(
            "{}",
            match mode() {
                Mode::Keep => "keep",
                Mode::ReadOnly => "read-only",
            }
        ),
        "write" => Keychain { mode: Mode::Keep }
            .set(&arg(1), &arg(2))
            .map(|()| println!("written"))
            .unwrap_or_else(|e| fail(&e)),
        "quiet-read" => quiet_read(&arg(1)),
        "store-read" => match Keychain::for_this_build().get(&arg(1)) {
            Ok(Some(_)) => println!("found"),
            Ok(None) => println!("absent"),
            Err(e) => fail(&e),
        },
        _ => fail("usage: keychain-probe build|mode|write|quiet-read|store-read"),
    }
}

/// The raw answer under suppression, so the job can tell "absent" from
/// "macOS would have asked".
#[cfg(all(feature = "shell", target_os = "macos"))]
fn quiet_read(key: &str) {
    use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
    use security_framework::os::macos::passwords::find_generic_password;
    let _quiet = SecKeychain::disable_user_interaction().unwrap_or_else(|e| fail(&e.to_string()));
    let keychain = SecKeychain::default_for_domain(SecPreferencesDomain::User)
        .unwrap_or_else(|e| fail(&e.to_string()));
    match find_generic_password(Some(&[keychain]), "design.snds.lockdown", key) {
        Ok(_) => println!("found"),
        Err(e) if e.code() == -25300 => println!("absent"),
        Err(e) => println!("refused {}", e.code()),
    }
}

#[cfg(all(feature = "shell", not(target_os = "macos")))]
fn quiet_read(_: &str) {
    fail("quiet-read is a macOS check")
}

#[cfg(feature = "shell")]
fn fail(why: &str) -> ! {
    eprintln!("keychain-probe: {why}");
    std::process::exit(1)
}

#[cfg(not(feature = "shell"))]
fn main() {
    eprintln!("keychain-probe needs the `shell` feature");
    std::process::exit(2);
}
