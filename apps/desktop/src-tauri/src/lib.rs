//! The Tauri shell: a thin host around `lockdown-app`. It supplies what
//! only the shell can (the OS keychain, the clipboard, the browser, the
//! window) and exposes the app's command surface to the webview one
//! command per operation (docs/plan/10-desktop-shell.md). No secret is
//! ever a JavaScript string on its way out: `reveal` answers with raw
//! bytes, `copy_secret` never answers with the value at all.

pub mod approver;
pub mod clipboard;
pub mod codes;
pub mod items;
pub mod organization;
pub mod security;
pub mod transfer;

#[cfg(feature = "shell")]
mod commands;

#[cfg(feature = "shell")]
mod shell {
    use std::path::PathBuf;
    use std::time::Duration;

    use lockdown_app::secrets::SecretStore;
    use lockdown_app::{App, LockPolicy, Paths};
    use tauri::Manager;

    /// The keychain service every entry is filed under.
    const SERVICE: &str = "design.snds.lockdown";

    /// The session token in the OS keychain.
    struct Keychain;

    impl SecretStore for Keychain {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            let entry = keyring::Entry::new(SERVICE, key)
                .map_err(|e| format!("the keychain isn't available: {e}"))?;
            match entry.get_password() {
                Ok(v) => Ok(Some(v)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("couldn't read from the keychain: {e}")),
            }
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            let entry = keyring::Entry::new(SERVICE, key)
                .map_err(|e| format!("the keychain isn't available: {e}"))?;
            entry
                .set_password(value)
                .map_err(|e| format!("couldn't save to the keychain: {e}"))
        }

        fn delete(&self, key: &str) {
            if let Ok(e) = keyring::Entry::new(SERVICE, key) {
                let _ = e.delete_credential();
            }
        }
    }

    /// Where the app lives: the platform identity root (D15) and the
    /// `lockdown` directory beside it (D47), unless `LOCKDOWN_ROOT` or
    /// `LOCKDOWN_DATA` say otherwise (a second profile for testing).
    fn paths() -> Result<Paths, String> {
        let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Paths::resolve(env("LOCKDOWN_ROOT"), env("LOCKDOWN_DATA"))
    }

    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_clipboard_manager::init())
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                if cfg!(debug_assertions) {
                    // Nothing logged here ever names a value; the app's
                    // errors are plain words about what happened.
                    app.handle().plugin(
                        tauri_plugin_log::Builder::default()
                            .level(log::LevelFilter::Info)
                            .build(),
                    )?;
                }
                lockdown_app::secrets::install(Box::new(Keychain));
                let paths = paths()?;
                let core = App::start(paths, None, LockPolicy::default());
                // Touch ID / Windows Hello where the machine has it; else
                // approvals go to the password (D101).
                if let Some(platform) = super::approver::PlatformApprover::detect() {
                    core.set_approver(Box::new(platform))?;
                }
                app.manage(core);
                app.manage(super::clipboard::ClipboardGuard::default());
                app.manage(super::transfer::Picked::default());
                Ok(())
            })
            // Losing the window to the dock counts as hidden (the core's
            // hidden-window timer decides when that becomes a lock).
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::Focused(focused) = event
                    && let Some(core) = window.try_state::<App>()
                {
                    let _ = core.set_hidden(!focused && !window.is_visible().unwrap_or(true));
                }
            })
            .invoke_handler(super::commands::handler())
            .run(tauri::generate_context!())
            .expect("the Lockdown window starts");
        // Give the worker a moment to drop the keys on the way out.
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(feature = "shell")]
pub use shell::run;
