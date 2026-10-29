//! The Tauri shell: a thin host around `lockdown-app`. It supplies what
//! only the shell can (the OS keychain, the clipboard, the browser, the
//! window) and exposes the app's command surface to the webview one
//! command per operation (docs/plan/10-desktop-shell.md). No secret is
//! ever a JavaScript string on its way out: `reveal` answers with raw
//! bytes, `copy_secret` never answers with the value at all.

pub mod approver;
pub mod bridges;
pub mod browser;
pub mod clipboard;
pub mod codes;
pub mod depth;
pub mod hygiene;
pub mod items;
pub mod live;
pub mod organization;
pub mod paper;
pub mod pq_status;
pub mod quick;
pub mod scan;
pub mod security;
pub mod security_depth;
pub mod server_checks;
pub mod sharing;
pub mod transfer;

#[cfg(feature = "shell")]
mod commands;

#[cfg(feature = "shell")]
mod browser_commands;

#[cfg(feature = "shell")]
mod passkey_commands;

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
        // No core file of a process that holds keys (docs/plan/26-desktop-finish.md §7).
        App::harden_process();
        let builder = tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_clipboard_manager::init())
            .plugin(tauri_plugin_dialog::init())
            .plugin(super::quick::commands::plugin())
            // The camera for scanning, in the main window only; every other
            // device a page asks for is refused (docs/plan/26-desktop-finish.md §3).
            .on_permission_request(|webview, kind| {
                use tauri::webview::{PermissionKind, PermissionResponse};
                match kind {
                    PermissionKind::Camera if super::hygiene::allows_camera(webview.label()) => {
                        PermissionResponse::Allow
                    }
                    _ => PermissionResponse::Deny,
                }
            })
            .plugin(tauri_plugin_notification::init());
        // The developer reset and the demo (docs/plan/40-devtools.md).
        #[cfg(any(feature = "dev-reset", feature = "demo-vault"))]
        let builder = builder.plugin(lockdown_devtools::shell::plugin());
        builder
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
                let data = paths.data.clone();
                let core = App::start(paths, None, LockPolicy::default());
                // Touch ID / Windows Hello where the machine has it; else
                // approvals go to the password (D101).
                if let Some(platform) = super::approver::PlatformApprover::detect() {
                    core.set_approver(Box::new(platform))?;
                }
                app.manage(core.clone());
                app.manage(super::clipboard::ClipboardGuard::default());
                app.manage(super::transfer::Picked::default());
                // The server's live channel (docs/plan/20-self-host-server.md §5).
                let live = super::live::commands::start(app.handle(), &core);
                app.manage(live);
                // The browser extension's endpoint (docs/plan/16-browser-extension.md).
                super::browser_commands::start(app.handle(), core.clone(), data);
                // Quick access from anywhere, and reveals kept out of captures
                // (docs/plan/26-desktop-finish.md §2, §6).
                app.manage(super::hygiene::CaptureCount::default());
                super::quick::commands::start(app.handle(), &core);
                // WebKitGTK keeps getUserMedia off until the host turns it on;
                // the permission handler above still answers each request.
                #[cfg(target_os = "linux")]
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.with_webview(|wv| {
                        use webkit2gtk::{SettingsExt, WebViewExt};
                        if let Some(settings) = WebViewExt::settings(&wv.inner()) {
                            settings.set_enable_media_stream(true);
                        }
                    });
                }
                Ok(())
            })
            // Losing the window to the dock counts as hidden (the core's
            // hidden-window timer decides when that becomes a lock).
            .on_window_event(|window, event| {
                // The main window only: the quick-access window hides on
                // every blur, which is not the app going out of view.
                if let tauri::WindowEvent::Focused(focused) = event
                    && window.label() == "main"
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
