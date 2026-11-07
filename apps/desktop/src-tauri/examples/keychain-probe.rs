//! What the session token's keychain items do across two builds
//! (docs/plan/60-auto-update.md §3), for CI's `keychain-macos` job
//! (`packaging/update/keychain-check.sh`). The `store-*` commands go
//! through the app's own store (`lockdown_desktop_lib::keychain::Keychain`),
//! dialogs off as the app runs them; the `raw-*` commands name an account
//! directly, also dialogs off, and print the keychain's status code.
//!
//! ```text
//! keychain-probe build                        which build this is (LOCKDOWN_PROBE_BUILD at compile time)
//! keychain-probe tag                          this build's identity tag
//! keychain-probe store-write <key> <value>    the app's write; prints written, or failed <why>
//! keychain-probe store-read <key>             the app's read; prints found <value> or absent
//! keychain-probe legacy-write <key> <value>   a write as builds before identity tags made it (the bare key)
//! keychain-probe raw-read <account>           prints found, absent, or refused <code>
//! keychain-probe raw-delete <account>         prints deleted, absent, or refused <code>
//! keychain-probe accounts                     every account under Lockdown's service, attributes only
//! ```

#[cfg(feature = "shell")]
fn main() {
    use lockdown_app::secrets::SecretStore;
    use lockdown_desktop_lib::keychain::{Keychain, this_build_tag};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match arg(0).as_str() {
        "build" => println!("{}", option_env!("LOCKDOWN_PROBE_BUILD").unwrap_or("-")),
        "tag" => println!("{}", this_build_tag().unwrap_or_else(|| "-".into())),
        "store-write" => match Keychain::for_this_build().set(&arg(1), &arg(2)) {
            Ok(()) => println!("written"),
            Err(e) => println!("failed {e}"),
        },
        "store-read" => match Keychain::for_this_build().get(&arg(1)) {
            Ok(Some(v)) => println!("found {v}"),
            Ok(None) => println!("absent"),
            Err(e) => println!("failed {e}"),
        },
        "legacy-write" => match (Keychain { tag: None }).set(&arg(1), &arg(2)) {
            Ok(()) => println!("written"),
            Err(e) => println!("failed {e}"),
        },
        "raw-read" | "raw-delete" | "accounts" => raw(&args),
        _ => fail(
            "usage: keychain-probe build|tag|store-write|store-read|legacy-write|raw-read|raw-delete|accounts",
        ),
    }
}

#[cfg(all(feature = "shell", target_os = "macos"))]
fn raw(args: &[String]) {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};
    use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
    use security_framework::os::macos::passwords::find_generic_password;
    const SERVICE: &str = "design.snds.lockdown";
    let _quiet = SecKeychain::disable_user_interaction().unwrap_or_else(|e| fail(&e.to_string()));
    let keychain = SecKeychain::default_for_domain(SecPreferencesDomain::User)
        .unwrap_or_else(|e| fail(&e.to_string()));
    let account = args.get(1).cloned().unwrap_or_default();
    match args[0].as_str() {
        "raw-read" => match find_generic_password(Some(&[keychain]), SERVICE, &account) {
            Ok(_) => println!("found"),
            Err(e) if e.code() == -25300 => println!("absent"),
            Err(e) => println!("refused {}", e.code()),
        },
        "raw-delete" => {
            // SecItemDelete by service and account, as this program.
            let gone = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(SERVICE)
                .account(&account)
                .delete();
            match gone {
                Ok(()) => println!("deleted"),
                Err(e) if e.code() == -25300 => println!("absent"),
                Err(e) => println!("refused {}", e.code()),
            }
        }
        _ => {
            let found = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(SERVICE)
                .load_attributes(true)
                .limit(Limit::All)
                .search();
            match found {
                Ok(results) => {
                    let mut accounts: Vec<String> = results
                        .iter()
                        .filter_map(|r| match r {
                            SearchResult::Dict(_) => {
                                r.simplify_dict().and_then(|d| d.get("acct").cloned())
                            }
                            _ => None,
                        })
                        .collect();
                    accounts.sort();
                    for a in accounts {
                        println!("{a}");
                    }
                }
                Err(e) if e.code() == -25300 => {}
                Err(e) => println!("refused {}", e.code()),
            }
        }
    }
}

#[cfg(all(feature = "shell", not(target_os = "macos")))]
fn raw(_: &[String]) {
    fail("the raw commands are a macOS check")
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
