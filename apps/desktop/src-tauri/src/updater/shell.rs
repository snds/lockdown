//! The updater in the window (docs/plan/60-auto-update.md §1): Tauri's
//! updater plugin driven from Rust only, so no updater permission reaches
//! the webview (D88). The page asks through four commands and hears back
//! through one event.
//!
//! - **Check** ([`check`]): the channel's manifest, then, when it names a
//!   newer build, the bundle for this platform, downloaded in the
//!   background and checked against the built-in key (the plugin's own
//!   minisign check, with the signed version required to match the
//!   manifest's). Every request and every redirect must stay on the plan's
//!   hosts; nothing sent names this person, this device or this build.
//! - **Restart to update** ([`restart`]): the vault is locked exactly as
//!   the Lock button locks it, the bundle installed, the app relaunched.
//! - **On quit** ([`on_exit`]): a downloaded update not yet taken installs
//!   as the app closes, without relaunching it.
//! - **Automatically**: at launch and every six hours, while the switch in
//!   Settings is on. Never in the demo's confined home (D485).
//! - **macOS**: "Check for Updates…" in the app menu does what the button
//!   does.

use std::sync::{Arc, Mutex};

use lockdown_app::{App, AppError, AppResult};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use super::{
    Install, NETWORK_TIMEOUT, Off, Source, TestSource, UpdateState, UpdateStatus, allowed,
    built_in_key, is_newer, percent, plan,
};
use crate::clipboard::ClipboardGuard;

/// The event every change of [`UpdateStatus`] is sent on.
pub const EVENT: &str = "lockdown://update";

/// The macOS app menu's "Check for Updates…" item.
pub const MENU_ID: &str = "lockdown-check-for-updates";

/// The plugin, configured from Rust. Its `plugins.updater` block in
/// `tauri.conf.json` holds only what the plugin can't take from code: the
/// signed version required, and the passive Windows installer.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new().build()
}

/// What a check found.
enum Found {
    UpToDate,
    /// Downloaded and checked, waiting for a restart or the quit.
    Ready(Box<Update>, Vec<u8>),
    /// Newer, but this copy can't replace itself.
    Available(String),
}

struct Inner {
    status: UpdateStatus,
    /// The downloaded update and its checked bytes.
    ready: Option<(Box<Update>, Vec<u8>)>,
    /// A check or a download is running.
    busy: bool,
}

/// The updater's state, managed by the app.
pub struct Updates {
    source: Result<Source, Off>,
    install: Install,
    inner: Mutex<Inner>,
}

impl Updates {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn status(&self) -> UpdateStatus {
        self.lock().status.clone()
    }
}

fn emit(handle: &AppHandle, updates: &Updates) {
    let _ = handle.emit(EVENT, updates.status());
}

/// Sets up the updater: the plan, the status, the macOS menu item, and the
/// automatic check at launch and every six hours.
pub fn start(handle: &AppHandle, core: &App) {
    let version = handle.package_info().version.clone();
    let source = plan(&version, built_in_key(), TestSource::from_env());
    let auto = core.settings().map(|s| s.update_auto_check).unwrap_or(true);
    let updates = Arc::new(Updates {
        install: install_mode(),
        inner: Mutex::new(Inner {
            status: UpdateStatus::at_start(&version, &source, auto),
            ready: None,
            busy: false,
        }),
        source,
    });
    handle.manage(updates.clone());
    #[cfg(target_os = "macos")]
    if let Err(e) = install_menu(handle) {
        log::warn!("the Check for Updates menu item couldn't be added: {e}");
    }
    if updates.source.is_err() {
        return;
    }
    let handle = handle.clone();
    let core = core.clone();
    let _ = std::thread::Builder::new()
        .name("lockdown-updates".into())
        .spawn(move || {
            loop {
                if core.settings().map(|s| s.update_auto_check).unwrap_or(true) {
                    tauri::async_runtime::block_on(check(handle.clone(), false));
                }
                std::thread::sleep(super::CHECK_EVERY);
            }
        });
}

/// Checks the channel, and downloads a newer build in the background.
/// `asked`: the button or the menu, not the automatic check.
pub async fn check(handle: AppHandle, asked: bool) {
    let Some(updates) = handle
        .try_state::<Arc<Updates>>()
        .map(|s| s.inner().clone())
    else {
        return;
    };
    let Ok(source) = updates.source.clone() else {
        return;
    };
    {
        let mut inner = updates.lock();
        if inner.busy {
            return;
        }
        inner.status.asked = asked;
        if inner.ready.is_some() {
            // Already downloaded: say so again, and download nothing.
            drop(inner);
            emit(&handle, &updates);
            return;
        }
        inner.busy = true;
        inner.status.to(UpdateState::Checking);
    }
    emit(&handle, &updates);
    let found = match lockdown_sync::network_held() {
        Some(reason) => Err(format!("Lockdown didn't check for updates: {reason}")),
        None => find(&handle, &updates, &source).await,
    };
    {
        let mut inner = updates.lock();
        inner.busy = false;
        match found {
            Ok(Found::UpToDate) => inner.status.to(UpdateState::UpToDate),
            Ok(Found::Ready(update, bytes)) => {
                inner.status.to(UpdateState::Ready);
                inner.status.offered = Some(update.version.clone());
                inner.ready = Some((update, bytes));
            }
            Ok(Found::Available(version)) => {
                inner.status.to(UpdateState::Available);
                inner.status.offered = Some(version);
                inner.status.link = Some(source.page.clone());
                inner.status.link_why = updates.install.words().map(str::to_string);
            }
            Err(words) => {
                inner.status.to(UpdateState::Failed);
                inner.status.error = Some(words);
            }
        }
    }
    emit(&handle, &updates);
}

async fn find(
    handle: &AppHandle,
    updates: &Arc<Updates>,
    source: &Source,
) -> Result<Found, String> {
    allowed(&source.endpoint, &source.hosts)?;
    let endpoint = source
        .endpoint
        .parse()
        .map_err(|_| "the update address isn't a web address".to_string())?;
    // A redirect off the plan's hosts stops the request; the host is kept
    // to say which.
    let refused: Arc<Mutex<Option<String>>> = Arc::default();
    let hosts = source.hosts.clone();
    let seen = refused.clone();
    let updater = handle
        .updater_builder()
        .pubkey(source.pubkey.clone())
        .endpoints(vec![endpoint])
        .map_err(|e| words(&e, None))?
        .version_comparator(|current, release| is_newer(&current, &release.version))
        .timeout(NETWORK_TIMEOUT)
        .configure_client(move |client| {
            let hosts = hosts.clone();
            let seen = seen.clone();
            client.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("too many redirects");
                }
                match allowed(attempt.url().as_str(), &hosts) {
                    Ok(()) => attempt.follow(),
                    Err(why) => {
                        if let Ok(mut s) = seen.lock() {
                            *s = Some(why.clone());
                        }
                        attempt.error(why)
                    }
                }
            }))
        })
        .build()
        .map_err(|e| words(&e, None))?;
    let refused_words = || refused.lock().ok().and_then(|s| s.clone());
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| words(&e, refused_words()))?
    else {
        return Ok(Found::UpToDate);
    };
    allowed(update.download_url.as_str(), &source.hosts)?;
    if updates.install != Install::InPlace {
        return Ok(Found::Available(update.version.clone()));
    }
    {
        let mut inner = updates.lock();
        inner.status.to(UpdateState::Downloading);
        inner.status.offered = Some(update.version.clone());
        inner.status.percent = Some(0);
    }
    emit(handle, updates);
    let mut received = 0u64;
    let mut shown = Some(0u8);
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let now = percent(received, total);
                if now != shown {
                    shown = now;
                    updates.lock().status.percent = now;
                    emit(handle, updates);
                }
            },
            || {},
        )
        .await
        .map_err(|e| words(&e, refused_words()))?;
    Ok(Found::Ready(Box::new(update), bytes))
}

/// The plugin's error in plain words. Nothing here can carry a secret: the
/// requests hold none.
fn words(e: &tauri_plugin_updater::Error, refused: Option<String>) -> String {
    use tauri_plugin_updater::Error as E;
    if let Some(why) = refused {
        return format!("The update was refused: {why}.");
    }
    match e {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => {
            "The update was refused: its signature didn't match Lockdown's key.".into()
        }
        E::Reqwest(_) | E::Network(_) => {
            "Lockdown couldn't reach GitHub. Check your connection and try again.".into()
        }
        E::ReleaseNotFound | E::TargetNotFound(_) | E::TargetsNotFound(_) => {
            "The newest build has nothing for this computer yet.".into()
        }
        E::Serialization(_) | E::Semver(_) | E::InvalidUpdaterFormat => {
            "The update's description couldn't be read.".into()
        }
        E::InsecureTransportProtocol => "Updates come only over https.".into(),
        other => format!("The update didn't work: {other}."),
    }
}

/// Whether this copy can replace itself (§5 of the doc).
fn install_mode() -> Install {
    #[cfg(target_os = "linux")]
    {
        Install::LinuxPackage
    }
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().ok();
        let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
        match exe {
            Some(exe) if super::windows_per_user(&exe, local.as_deref()) => Install::InPlace,
            _ => Install::MachineWide,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = std::env::current_exe()
            .ok()
            .and_then(|exe| tauri_plugin_updater::extract_path_from_executable(&exe).ok());
        match bundle {
            Some(bundle) if mac_can_replace(&bundle) => Install::InPlace,
            _ => Install::NotWritable,
        }
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Install::LinuxPackage
    }
}

/// The plugin replaces the bundle with renames, which need write access to
/// the folder holding it and to the bundle itself; without them it would
/// ask for an administrator's password, which Lockdown never does.
#[cfg(target_os = "macos")]
fn mac_can_replace(bundle: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if bundle.extension().is_none_or(|e| e != "app") {
        return false;
    }
    let Some(folder) = bundle.parent() else {
        return false;
    };
    let probe = folder.join(format!(".lockdown-update-{}", std::process::id()));
    let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    else {
        return false;
    };
    let me = file.metadata().map(|m| m.uid()).ok();
    drop(file);
    let _ = std::fs::remove_file(&probe);
    std::fs::metadata(bundle).is_ok_and(|m| {
        let mode = m.mode();
        (Some(m.uid()) == me && mode & 0o200 != 0) || mode & 0o002 != 0
    })
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
    let updates = handle
        .try_state::<Arc<Updates>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppError::refused("there's no update to install"))?;
    let (update, bytes) = updates
        .lock()
        .ready
        .take()
        .ok_or_else(|| AppError::refused("there's no update to install yet"))?;
    lock_now(handle);
    if let Err(e) = update.install(&bytes) {
        let message = words(&e, None);
        let mut inner = updates.lock();
        inner.status.to(UpdateState::Failed);
        inner.status.error = Some(message.clone());
        drop(inner);
        emit(handle, &updates);
        return Err(AppError::refused(message));
    }
    handle.restart()
}

/// The app is closing: a downloaded update installs now, without a
/// relaunch. The vault is locked first, as on any quit.
pub fn on_exit(handle: &AppHandle) {
    let Some(updates) = handle
        .try_state::<Arc<Updates>>()
        .map(|s| s.inner().clone())
    else {
        return;
    };
    let Some((update, bytes)) = updates.lock().ready.take() else {
        return;
    };
    lock_now(handle);
    let _ = (*update).restart_after_install(false).install(&bytes);
}

#[cfg(target_os = "macos")]
fn install_menu(handle: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = match handle.menu() {
        Some(menu) => menu,
        None => {
            let menu = Menu::default(handle)?;
            handle.set_menu(menu.clone())?;
            menu
        }
    };
    let item = MenuItem::with_id(handle, MENU_ID, "Check for Updates…", true, None::<&str>)?;
    // The app menu is the first; "Check for Updates…" goes under "About".
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        app_menu.insert(&item, 1)?;
    }
    handle.on_menu_event(|app, event| {
        if event.id() == MENU_ID {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            tauri::async_runtime::spawn(check(app.clone(), true));
        }
    });
    Ok(())
}

/// The commands the page calls (appended to the handler list).
pub mod commands {
    use super::*;

    fn updates(handle: &AppHandle) -> AppResult<Arc<Updates>> {
        handle
            .try_state::<Arc<Updates>>()
            .map(|s| s.inner().clone())
            .ok_or_else(|| AppError::refused("the updater isn't running"))
    }

    /// Where updates stand.
    #[tauri::command]
    pub async fn update_status(handle: AppHandle) -> AppResult<UpdateStatus> {
        Ok(updates(&handle)?.status())
    }

    /// "Check for updates": checks now and starts the download; the
    /// outcome arrives on the `lockdown://update` event.
    #[tauri::command]
    pub async fn update_check(handle: AppHandle) -> AppResult<UpdateStatus> {
        let updates = updates(&handle)?;
        tauri::async_runtime::spawn(check(handle.clone(), true));
        Ok(updates.status())
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
        let updates = updates(&handle)?;
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut settings = app.settings()?;
            settings.update_auto_check = on;
            app.set_settings(settings).map(|_| ())
        })
        .await
        .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))??;
        let idle = {
            let mut inner = updates.lock();
            inner.status.auto_check = on;
            inner.status.state == UpdateState::Idle
        };
        emit(&handle, &updates);
        if on && idle {
            tauri::async_runtime::spawn(check(handle.clone(), false));
        }
        Ok(updates.status())
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
