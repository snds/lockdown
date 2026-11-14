//! The updater in the window (docs/plan/60-auto-update.md §1), through the
//! shared update kit's glue (`app_updates::shell`): Tauri's updater plugin
//! driven from Rust only, so no updater permission reaches the webview
//! (D88). The page asks through five commands and hears back through one
//! event.
//!
//! - **Check**: the channel's manifest, then, when it names a newer build,
//!   the bundle for this platform, downloaded in the background and checked
//!   against the built-in key. Every request and every redirect must stay on
//!   the plan's hosts; nothing sent names this person, this device or this
//!   build.
//! - **Restart to update** ([`restart`]): the vault is locked exactly as
//!   the Lock button locks it, the bundle installed, the app relaunched.
//! - **Reload app**: the vault is locked and the app closes and opens again
//!   (with an update downloaded, this is Restart to update).
//! - **On quit** ([`on_exit`]): a downloaded update not yet taken installs
//!   as the app closes, without relaunching it.
//! - **Automatically**: at launch and every six hours, while the switch in
//!   Settings is on. Never in the demo's confined home (D485).
//! - **macOS**: "Check for Updates…" in the app menu does what the button
//!   does.
//!
//! What's Lockdown's own is [`LockdownHost`]: the switch kept in
//! `settings.cbor`, the vault locked before the app closes, the demo's held
//! network, and the end-to-end hooks.

use std::sync::Arc;

use app_updates::shell::{self as kit, Config, Host};
use lockdown_app::{App, AppError, AppResult};
use tauri::{AppHandle, Manager, State};

use super::{FEED, Install, UpdateStatus, built_in_key, test_source_from_env};
use crate::clipboard::ClipboardGuard;

/// The event every change of [`UpdateStatus`] is sent on.
pub const EVENT: &str = "lockdown://update";

/// The macOS app menu's "Check for Updates…" item.
pub const MENU_ID: &str = "lockdown-check-for-updates";

/// The plugin, configured from Rust. Its `plugins.updater` block in
/// `tauri.conf.json` holds only what the plugin can't take from code: the
/// signed version required, and the passive Windows installer.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry, tauri_plugin_updater::Config> {
    kit::plugin()
}

/// What the kit asks of Lockdown.
pub struct LockdownHost {
    core: App,
}

impl Host for LockdownHost {
    fn auto_check(&self) -> bool {
        self.core
            .settings()
            .map(|s| s.update_auto_check)
            .unwrap_or(true)
    }

    fn set_auto_check(&self, on: bool) -> Result<(), String> {
        save_auto_check(&self.core, on).map_err(|e| e.message)
    }

    fn before_restart(&self, handle: &AppHandle) {
        lock_now(handle);
    }

    fn network_held(&self) -> Option<String> {
        lockdown_sync::network_held()
    }

    fn on_status(&self, _handle: &AppHandle, _status: &UpdateStatus) {
        #[cfg(feature = "update-test")]
        e2e::on_status(_handle, _status);
    }

    fn on_started(&self, _handle: &AppHandle, _status: &UpdateStatus, _install: Install) {
        #[cfg(feature = "update-test")]
        e2e::started(_status, _install, &self.core);
    }
}

fn save_auto_check(app: &App, on: bool) -> AppResult<()> {
    let mut settings = app.settings()?;
    settings.update_auto_check = on;
    app.set_settings(settings).map(|_| ())
}

/// Sets up the updater: the plan, the status, the macOS menu item, and the
/// automatic check at launch and every six hours.
pub fn start(handle: &AppHandle, core: &App) {
    let test = test_source_from_env();
    // The end-to-end run's settings also come from a file in the data
    // directory: Windows' installer starts the new build through Explorer,
    // with Explorer's environment, not the old build's.
    #[cfg(feature = "update-test")]
    let test = test.or_else(|| e2e::source(core));
    let config = Config {
        feed: FEED,
        key: built_in_key(),
        test,
        event: EVENT,
        menu_id: MENU_ID,
        // Lockdown follows the web feed only. The kit's local channel (a
        // build this computer makes for itself) is never on.
        local: None,
    };
    kit::start(
        handle,
        config,
        Arc::new(LockdownHost { core: core.clone() }),
    );
}

/// Locks the vault and drops the clipboard's pending copy, exactly as the
/// Lock button does (`commands::lock`).
fn lock_now(handle: &AppHandle) {
    if let Some(guard) = handle.try_state::<ClipboardGuard>() {
        guard.forget();
    }
    if let Some(core) = handle.try_state::<App>() {
        let _ = core.lock();
    }
}

/// "Restart to update": lock, install, relaunch. On Windows the passive
/// installer closes the app and starts the new one itself.
pub fn restart(handle: &AppHandle) -> AppResult<()> {
    kit::restart(handle).map_err(AppError::refused)
}

/// The app is closing: a downloaded update installs now, without a
/// relaunch. The vault is locked first, as on any quit.
pub fn on_exit(handle: &AppHandle) {
    kit::on_exit(handle);
}

/// The commands the page calls (appended to the handler list).
pub mod commands {
    use super::*;

    /// Where updates stand.
    #[tauri::command]
    pub async fn update_status(handle: AppHandle) -> AppResult<UpdateStatus> {
        kit::status(&handle).map_err(AppError::refused)
    }

    /// "Check for updates": checks now and starts the download; the
    /// outcome arrives on the `lockdown://update` event.
    #[tauri::command]
    pub async fn update_check(handle: AppHandle) -> AppResult<UpdateStatus> {
        kit::check_now(&handle).map_err(AppError::refused)
    }

    /// "Restart to update".
    #[tauri::command]
    pub async fn update_restart(handle: AppHandle) -> AppResult<()> {
        tauri::async_runtime::spawn_blocking(move || restart(&handle))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
    }

    /// The "Check automatically" switch, saved in `settings.cbor`.
    #[tauri::command]
    pub async fn update_auto_set(
        handle: AppHandle,
        app: State<'_, App>,
        on: bool,
    ) -> AppResult<UpdateStatus> {
        kit::status(&handle).map_err(AppError::refused)?;
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || save_auto_check(&app, on))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))??;
        kit::auto_check_saved(&handle, on).map_err(AppError::refused)
    }

    /// "Reload app": the vault is locked, and the app closes and opens
    /// again.
    #[tauri::command]
    pub async fn app_reload(handle: AppHandle) -> AppResult<()> {
        tauri::async_runtime::spawn_blocking(move || kit::reload(&handle))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
            .map_err(AppError::refused)
    }
}

/// The end-to-end run's hooks (`update-test` builds only; CI's
/// `update-e2e` job, docs/plan/60-auto-update.md §7): build A and build B
/// write what they see to `LOCKDOWN_UPDATE_TEST_REPORT`, a ready update
/// restarts at once with `LOCKDOWN_UPDATE_TEST_AUTORESTART=1`, and with
/// `LOCKDOWN_UPDATE_TEST_PASSWORD` the build opens the job's throwaway
/// vault and reports how many items it holds. None of it exists in a
/// shipped build: build.rs refuses the feature in a release, and
/// `packaging/devtools/check.sh` looks for its marker.
#[cfg(feature = "update-test")]
mod e2e {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The vault was opened (or failed to), so a restart may come: the
    /// run sees each build open the vault before it moves on.
    static OPENED: AtomicBool = AtomicBool::new(true);

    use super::super::{TestSource, UpdateState};
    use super::{App, AppHandle, Install, UpdateStatus};

    /// `update-test.json` in the data directory (the run writes it), read
    /// once: `endpoint`, `pubkey`, `report`, `autorestart`, `password`.
    static FILE: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

    fn load(core: &App) {
        let value = core
            .probe()
            .ok()
            .and_then(|p| {
                std::fs::read(std::path::Path::new(&p.data).join("update-test.json")).ok()
            })
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(serde_json::Value::Null);
        let _ = FILE.set(value);
    }

    /// A run setting: the environment's `LOCKDOWN_UPDATE_TEST_<NAME>`, else
    /// the file's `<name>`.
    fn setting(name: &str) -> Option<String> {
        std::env::var(format!("LOCKDOWN_UPDATE_TEST_{}", name.to_uppercase()))
            .ok()
            .or_else(|| {
                FILE.get()
                    .and_then(|f| f.get(name))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
    }

    /// The test source from the file, when the environment had none.
    pub fn source(core: &App) -> Option<TestSource> {
        load(core);
        Some(TestSource {
            endpoint: setting("endpoint")?,
            pubkey: setting("pubkey")?,
        })
    }

    fn report(line: &str) {
        let Some(path) = setting("report") else {
            return;
        };
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    pub fn started(status: &UpdateStatus, install: Install, core: &App) {
        std::hint::black_box(super::super::TEST_MARKER);
        if FILE.get().is_none() {
            load(core);
        }
        report(&format!(
            "started {} enabled={} install={install:?}",
            status.version, status.enabled
        ));
        let Some(password) = setting("password") else {
            return;
        };
        let core = core.clone();
        let version = status.version.clone();
        OPENED.store(false, Ordering::SeqCst);
        std::thread::spawn(move || {
            let opened = core
                .unlock_with_password(&password)
                .and_then(|_| core.list());
            match opened {
                Ok(items) => report(&format!("opened {version} items={}", items.len())),
                Err(e) => report(&format!("open-failed {version} {}", e.message)),
            }
            OPENED.store(true, Ordering::SeqCst);
        });
    }

    pub fn on_status(handle: &AppHandle, status: &UpdateStatus) {
        report(&format!(
            "state {} {:?} offered={} error={}",
            status.version,
            status.state,
            status.offered.as_deref().unwrap_or("-"),
            status.error.as_deref().unwrap_or("-")
        ));
        if status.state == UpdateState::Ready && setting("autorestart").as_deref() == Some("1") {
            let handle = handle.clone();
            std::thread::spawn(move || {
                for _ in 0..600 {
                    if OPENED.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                report("restarting");
                if let Err(e) = super::restart(&handle) {
                    report(&format!("restart-failed {}", e.message));
                }
            });
        }
    }
}

/// The updater's own signature check (the plugin's `Update::verify`, the
/// check `download` and `install` run), driven through Tauri's mock
/// runtime with fixtures signed by a throwaway key whose secret half was
/// never kept. CI's `updater` job also signs a bundle with a key made in
/// the job and runs `ci_signed_bundle_verifies` on it.
#[cfg(all(test, feature = "update-test"))]
mod plugin_tests {
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
    use tauri_plugin_updater::{Updater, UpdaterExt};

    use super::super::{MAIN_ENDPOINT, is_newer};

    const KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQyOUE3MTU4MUNBMTA3RkQKUldUOUI2RWNXSEdhMHBLeWVGS0xVTEZFZy83Q0xiL3VjN1NDdWxLQ25MSUxOYTJLZDNaUUhtSHkK";
    const OTHER_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEMxNDIxMjg4OUJFQjMyQTYKUldTbU11dWJpQkpDd1J5YTFEMm02b0VLUDAxbzZrd1JnZ2R2VU9MNVVCakVGejRLdTFOczZJcFoK";
    const BUNDLE: &[u8] = b"lockdown update test bundle\n";
    /// Signed with `--app-version 0.1.0-main.20261107120000`.
    const SIGNED_FOR_VERSION: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUOUI2RWNXSEdhMGdwcVpnTzNCWlJtRDliNlMvRHlBaUNiNlV2aGJsMWJwQ3lkZ1c4emZKSE92djJPVXJuSTBDQThaRHRBNXNreC8xM1FubEtuLy90VUswWGtBYmVGYWdvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MDE3MTU3CWZpbGU6YnVuZGxlLmJpbgl2ZXJzaW9uOjAuMS4wLW1haW4uMjAyNjExMDcxMjAwMDAKYUxkU0ZyemhubE1ET3RxR2dEd3hBT1VjckJMMmRKQS9nQVQ1RFhmUDgvb1pHSi9FQVJ0a1NwMVBvRHVUM3JIY2FZelM4eU1zeU44bjhiNzR1TldmQ2c9PQo=";
    /// Signed with no version (an older CLI's signature).
    const SIGNED_WITHOUT_VERSION: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUOUI2RWNXSEdhMHVtZmFmQlZyVkRVSDNscm1uaGp5b0V4dFlwdndVZEU1aTdlYXFoWmhGVXZmeUladWlrSk1uYjZyRURzcVNuS09WWUo0WUxiUEs0Y3p2dGlHd0tkMndjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MDE3MTU4CWZpbGU6YnVuZGxlLmJpbgpCYnc2NytpemxpUkZnY3VEcDF1OSs2TEJIS0Zlc3R3MEw2Qi9VZlpwS2x5cjhZQk1OWHcwMmRpTnUzSjhJaThUZkhqYWczbzdzSlB1K3RjRVdQcjBCZz09Cg==";
    const OFFERED: &str = "0.1.0-main.20261107120000";

    fn updater(current: &str, key: &str) -> Updater {
        let mut context = mock_context::<MockRuntime, _>(noop_assets());
        context.package_info_mut().version = current.parse().unwrap();
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "", "requireSignedVersion": true }),
        );
        let app = mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        app.updater_builder()
            .pubkey(key)
            .endpoints(vec![MAIN_ENDPOINT.parse().unwrap()])
            .unwrap()
            .version_comparator(|current, release| is_newer(&current, &release.version))
            .build()
            .unwrap()
    }

    fn manifest(version: &str, signature: &str) -> serde_json::Value {
        let entry = serde_json::json!({
            "url": "https://github.com/snds/lockdown/releases/download/main-build/Lockdown.app.tar.gz",
            "signature": signature,
        });
        serde_json::json!({
            "version": version,
            "platforms": {
                "darwin-aarch64": entry, "darwin-x86_64": entry,
                "windows-x86_64": entry, "linux-x86_64": entry,
            },
        })
    }

    fn check(
        current: &str,
        key: &str,
        version: &str,
        signature: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let update = updater(current, key)
            .restore_update(manifest(version, signature))
            .map_err(|e| e.to_string())?
            .ok_or("not an update")?;
        update.verify(bytes).map_err(|e| e.to_string())
    }

    const CURRENT: &str = "0.1.0-main.20261107093000";

    #[test]
    fn a_bundle_signed_with_the_key_for_its_version_is_accepted() {
        check(CURRENT, KEY, OFFERED, SIGNED_FOR_VERSION, BUNDLE).unwrap();
    }

    #[test]
    fn a_changed_bundle_is_refused() {
        let mut changed = BUNDLE.to_vec();
        changed[0] ^= 1;
        assert!(check(CURRENT, KEY, OFFERED, SIGNED_FOR_VERSION, &changed).is_err());
    }

    #[test]
    fn another_key_is_refused() {
        assert!(check(CURRENT, OTHER_KEY, OFFERED, SIGNED_FOR_VERSION, BUNDLE).is_err());
    }

    #[test]
    fn an_unsigned_or_garbled_signature_is_refused() {
        for bad in ["", "not base64", "dW50cnVzdGVkIGNvbW1lbnQ6IG5vdGhpbmcK"] {
            assert!(
                check(CURRENT, KEY, OFFERED, bad, BUNDLE).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_signature_for_another_version_or_none_is_refused() {
        // The manifest isn't signed: a newer number beside an older
        // bundle's signature must not install that bundle.
        let e = check(
            CURRENT,
            KEY,
            "0.1.0-main.20261201000000",
            SIGNED_FOR_VERSION,
            BUNDLE,
        )
        .unwrap_err();
        assert!(e.contains("version"), "{e}");
        assert!(check(CURRENT, KEY, OFFERED, SIGNED_WITHOUT_VERSION, BUNDLE).is_err());
    }

    #[test]
    fn the_same_or_an_older_version_is_not_an_update() {
        for current in [OFFERED, "0.1.0-main.20261201000000", "0.1.0"] {
            let found = updater(current, KEY)
                .restore_update(manifest(OFFERED, SIGNED_FOR_VERSION))
                .unwrap();
            assert!(found.is_none(), "{current} took {OFFERED}");
        }
    }

    /// A bundle CI signed with a throwaway key made in the job: the folder
    /// in `LOCKDOWN_UPDATE_TEST_SIGNED` holds `bundle`, `bundle.sig`,
    /// `key.pub` and `version`. Without it this test has nothing to check.
    #[test]
    fn ci_signed_bundle_verifies() {
        let Some(dir) = std::env::var_os("LOCKDOWN_UPDATE_TEST_SIGNED") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .unwrap()
                .trim()
                .to_string()
        };
        let bundle = std::fs::read(dir.join("bundle")).unwrap();
        let (key, signature, version) = (read("key.pub"), read("bundle.sig"), read("version"));
        check(CURRENT, &key, &version, &signature, &bundle).unwrap();
        let mut changed = bundle.clone();
        changed.push(0);
        assert!(check(CURRENT, &key, &version, &signature, &changed).is_err());
        assert!(check(CURRENT, KEY, &version, &signature, &bundle).is_err());
        println!(
            "the updater's own check accepted the CI-signed bundle and refused it changed or under another key"
        );
    }
}
