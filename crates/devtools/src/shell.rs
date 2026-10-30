//! The Tauri plugin (docs/plan/40-devtools.md §4): the desktop registers it
//! with one line when `dev-reset` or `demo-vault` is on. It brings:
//!
//! - **The Developer menu**: in the macOS menu bar, and in the main window's
//!   own menu on Windows and Linux. Each item tells the main window what
//!   was picked (the [`MENU_EVENT`] event); the page decides what to show.
//!   It works locked or not, and at first run.
//! - **Its commands**, `plugin:devtools|…`, granted to the main window only
//!   by a capability the plugin adds at start (no capability file names
//!   them, so the quick-access window, a web page, the extension, the
//!   native host and the CLI can't reach them).
//! - **The reset's second half**: a reset confirmed in the window clears
//!   the webview's storage and the keystore at once, leaves word in the data
//!   directory and relaunches; the new process erases every directory in
//!   the plugin's setup, before the app opens anything.
//! - **The demo**, made once the app's core is running.

use std::path::PathBuf;
#[cfg(feature = "demo")]
use std::sync::OnceLock;

use lockdown_app::{App, AppError, AppResult, Paths};
use serde::Serialize;
use tauri::ipc::CapabilityBuilder;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, Wry};

/// The window the commands and the menu's events are for.
pub const MAIN_WINDOW: &str = "main";
/// The event the menu sends the main window: the item's action.
pub const MENU_EVENT: &str = "devtools://menu";
/// The plugin's name: its commands are `plugin:devtools|<name>`.
pub const NAME: &str = "devtools";

/// The plugin's state.
pub struct Devtools {
    home: Paths,
    #[cfg(feature = "reset")]
    shell_dirs: Vec<(String, PathBuf)>,
    #[cfg(feature = "demo")]
    demo: OnceLock<std::sync::Arc<crate::demo::Demo>>,
}

/// Where the app lives, as the desktop resolves it (`LOCKDOWN_ROOT`,
/// `LOCKDOWN_DATA`, else this computer's place after the one-time move;
/// docs/plan/41-mac-paths.md). The reset erases what is here.
fn home() -> Result<Paths, String> {
    lockdown_app::paths::at_start()
}

/// The shell's own folders, each with what it holds, deduplicated.
#[cfg(feature = "reset")]
fn shell_dirs(app: &AppHandle) -> Vec<(String, PathBuf)> {
    let p = app.path();
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut add = |what: &str, dir: tauri::Result<PathBuf>| {
        if let Ok(dir) = dir
            && !out.iter().any(|(_, d)| *d == dir)
        {
            out.push((what.to_string(), dir));
        }
    };
    add("The app's settings folder", p.app_config_dir());
    add(
        "The app's own data, the web view's storage among it",
        p.app_data_dir(),
    );
    add("The app's local data", p.app_local_data_dir());
    add("The app's cache", p.app_cache_dir());
    add("The app's logs", p.app_log_dir());
    // WKWebView keeps a site's storage under ~/Library/WebKit/<identifier>.
    if cfg!(target_os = "macos")
        && let Ok(home) = p.home_dir()
    {
        add(
            "The web view's storage",
            Ok(home.join("Library/WebKit").join(&app.config().identifier)),
        );
    }
    out
}

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

async fn blocking<T, F>(f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| refused(format!("the call didn't finish: {e}")))?
}

/// What the page asks first: which tools this build has.
#[derive(Serialize)]
pub struct DevtoolsStatus {
    /// [`crate::MARKER`]: what a release check looks for.
    pub marker: &'static str,
    /// Whether the reset is built in.
    pub reset: bool,
    /// The demo, when it is built in.
    #[cfg(feature = "demo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demo: Option<crate::demo::DemoState>,
}

#[tauri::command]
fn status(handle: AppHandle, state: State<'_, Devtools>) -> DevtoolsStatus {
    let _ = &handle;
    let _ = &state;
    DevtoolsStatus {
        marker: crate::MARKER,
        reset: cfg!(feature = "reset"),
        #[cfg(feature = "demo")]
        demo: Some(
            state
                .demo(&handle)
                .map(|d| d.state())
                .unwrap_or(crate::demo::DemoState {
                    open: false,
                    came_from: None,
                    pack: None,
                    items: 0,
                }),
        ),
    }
}

#[cfg(feature = "reset")]
mod reset_commands {
    use super::*;
    use crate::keystore::OsKeystore;
    use crate::reset::{self, Places};

    /// What the dialog lists.
    #[derive(Serialize)]
    pub struct ResetPlan {
        #[serde(flatten)]
        plan: reset::Plan,
        /// The web view's own storage is cleared too.
        web_storage: bool,
    }

    impl Devtools {
        pub(super) fn places(&self) -> Places {
            Places {
                data: self.home.data.clone(),
                identity_root: self.home.root.clone(),
                shell_dirs: self.shell_dirs.clone(),
                home: std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(PathBuf::from),
            }
        }
    }

    #[tauri::command]
    pub async fn reset_plan(state: State<'_, Devtools>) -> AppResult<ResetPlan> {
        let places = state.places();
        blocking(move || {
            Ok(ResetPlan {
                plan: reset::plan(&places, &OsKeystore),
                web_storage: true,
            })
        })
        .await
    }

    /// Runs the reset the person confirmed by typing `reset`: locks the
    /// vault, clears every webview's storage and Lockdown's keystore
    /// entries now, leaves word for the next launch and relaunches. The
    /// new process erases the directories before anything opens them.
    #[tauri::command]
    pub async fn reset_run(
        handle: AppHandle,
        state: State<'_, Devtools>,
        confirm: String,
    ) -> AppResult<()> {
        if confirm.trim() != "reset" {
            return Err(refused("Type reset to confirm."));
        }
        if let Some(core) = handle.try_state::<App>() {
            let core = core.inner().clone();
            blocking(move || core.lock()).await?;
        }
        for (_, w) in handle.webview_windows() {
            w.clear_all_browsing_data()
                .map_err(|e| refused(format!("the web view's storage wasn't cleared: {e}")))?;
        }
        let places = state.places();
        blocking(move || {
            let keystore = OsKeystore;
            for account in reset::plan(&places, &keystore).keystore.entries {
                keystore
                    .delete(reset::SERVICE, &account)
                    .map_err(|e| refused(format!("the keystore entry '{account}' stayed: {e}")))?;
            }
            reset::mark_pending(&places.data).map_err(refused)
        })
        .await?;
        // Answer the page first, then go.
        let h = handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            h.request_restart();
        });
        Ok(())
    }

    use crate::reset::Keystore;

    /// At launch: a reset waiting from the run before is finished here.
    pub(super) fn finish(state: &Devtools) {
        if let Some(erased) = reset::finish_pending(&state.places(), &OsKeystore) {
            for failed in &erased.failed {
                eprintln!("lockdown: the reset couldn't remove {failed}");
            }
        }
    }
}

#[cfg(feature = "demo")]
mod demo_commands {
    use super::*;
    use std::sync::Arc;

    use crate::demo::{CameFrom, Demo, Opened, Pack, default_pack, pack::MAX_PACK};
    use tauri_plugin_dialog::DialogExt;

    impl Devtools {
        /// The demo, made the first time it is asked for (the app's core
        /// must be running).
        pub(super) fn demo(&self, handle: &AppHandle) -> AppResult<Arc<Demo>> {
            if let Some(d) = self.demo.get() {
                return Ok(d.clone());
            }
            let core = handle
                .try_state::<App>()
                .ok_or_else(|| refused("Lockdown hasn't started yet."))?
                .inner()
                .clone();
            Ok(self
                .demo
                .get_or_init(|| Arc::new(Demo::new(core, &self.home)))
                .clone())
        }
    }

    fn demo_of(handle: &AppHandle) -> AppResult<Arc<Demo>> {
        handle.state::<Devtools>().inner().demo(handle)
    }

    #[tauri::command]
    pub async fn demo_open(handle: AppHandle) -> AppResult<Opened> {
        let demo = demo_of(&handle)?;
        blocking(move || match demo.opened().map_err(refused)? {
            Some(open) => Ok(open),
            None => demo.open(default_pack()).map_err(refused),
        })
        .await
    }

    #[tauri::command]
    pub async fn demo_reset(handle: AppHandle) -> AppResult<Opened> {
        let demo = demo_of(&handle)?;
        blocking(move || {
            if demo.state().open {
                demo.reset().map_err(refused)
            } else {
                demo.open(default_pack()).map_err(refused)
            }
        })
        .await
    }

    #[tauri::command]
    pub async fn demo_leave(handle: AppHandle) -> AppResult<CameFrom> {
        let demo = demo_of(&handle)?;
        blocking(move || demo.leave().map_err(refused)).await
    }

    /// Picks a pack file in the OS's own dialog, reads and checks it, and
    /// opens the demo from it. `None` when the dialog was cancelled.
    #[tauri::command]
    pub async fn demo_load_pack(handle: AppHandle) -> AppResult<Option<Opened>> {
        let demo = demo_of(&handle)?;
        let h = handle.clone();
        let picked = blocking(move || {
            Ok(h.dialog()
                .file()
                .set_title("Load a demo pack")
                .add_filter("Demo pack or CXF file", &["json"])
                .blocking_pick_file())
        })
        .await?;
        let Some(path) = picked else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| {
            refused("That file can't be read from here; pick one on this computer.")
        })?;
        blocking(move || {
            let len = std::fs::metadata(&path)
                .map_err(|e| refused(format!("The file couldn't be read: {e}.")))?
                .len();
            if len > MAX_PACK as u64 {
                return Err(refused(format!(
                    "The pack is {len} bytes; the limit is {MAX_PACK}."
                )));
            }
            let bytes = std::fs::read(&path)
                .map_err(|e| refused(format!("The file couldn't be read: {e}.")))?;
            let pack = Pack::parse(&bytes).map_err(refused)?;
            demo.open(pack).map(Some).map_err(refused)
        })
        .await
    }
}

/// The Developer menu, where the platform puts an app's menus.
fn install_menu(handle: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    let developer = Submenu::with_id(handle, "devtools", "Developer", true)?;
    let items = crate::menu::items();
    for (i, (id, words, _)) in items.iter().enumerate() {
        if *id == "devtools.reset" && i > 0 {
            developer.append(&PredefinedMenuItem::separator(handle)?)?;
        }
        developer.append(&MenuItem::with_id(handle, *id, *words, true, None::<&str>)?)?;
    }
    if cfg!(target_os = "macos") {
        let menu = match handle.menu() {
            Some(m) => m,
            None => {
                let m = Menu::default(handle)?;
                handle.set_menu(m.clone())?;
                m
            }
        };
        menu.append(&developer)?;
    } else if let Some(main) = handle.get_webview_window(MAIN_WINDOW) {
        main.set_menu(Menu::with_items(handle, &[&developer])?)?;
    }
    handle.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if let Some((_, _, action)) = crate::menu::items().into_iter().find(|(i, _, _)| *i == id) {
            if let Some(main) = app.get_webview_window(MAIN_WINDOW) {
                let _ = main.show();
                let _ = main.set_focus();
            }
            let _ = app.emit_to(MAIN_WINDOW, MENU_EVENT, action);
        }
    });
    Ok(())
}

type Handler = dyn Fn(tauri::ipc::Invoke<Wry>) -> bool + Send + Sync;

fn common_handler() -> Box<Handler> {
    Box::new(tauri::generate_handler![status])
}

#[cfg(feature = "reset")]
fn reset_handler() -> Box<Handler> {
    Box::new(tauri::generate_handler![
        reset_commands::reset_plan,
        reset_commands::reset_run
    ])
}

#[cfg(feature = "demo")]
fn demo_handler() -> Box<Handler> {
    Box::new(tauri::generate_handler![
        demo_commands::demo_open,
        demo_commands::demo_reset,
        demo_commands::demo_leave,
        demo_commands::demo_load_pack
    ])
}

/// The plugin.
pub fn plugin() -> TauriPlugin<Wry> {
    Builder::<Wry>::new(NAME)
        .invoke_handler(|invoke| {
            let command = invoke.message.command();
            #[cfg(feature = "reset")]
            if command.starts_with("reset_") {
                return reset_handler()(invoke);
            }
            #[cfg(feature = "demo")]
            if command.starts_with("demo_") {
                return demo_handler()(invoke);
            }
            let _ = command;
            common_handler()(invoke)
        })
        .setup(|handle, _api| {
            let home = home()?;
            let state = Devtools {
                home,
                #[cfg(feature = "reset")]
                shell_dirs: shell_dirs(handle),
                #[cfg(feature = "demo")]
                demo: OnceLock::new(),
            };
            // Before the app's own setup opens anything.
            #[cfg(feature = "reset")]
            reset_commands::finish(&state);
            handle.manage(state);
            handle.add_capability(
                CapabilityBuilder::new("devtools")
                    .window(MAIN_WINDOW)
                    .permission("devtools:default"),
            )?;
            Ok(())
        })
        .on_event(|handle, event| {
            if let RunEvent::Ready = event {
                if let Err(e) = install_menu(handle) {
                    eprintln!("lockdown: the Developer menu didn't load: {e}");
                }
                // A demo left open by a run that quit inside it goes now.
                #[cfg(feature = "demo")]
                if let Some(state) = handle.try_state::<Devtools>() {
                    let _ = state.demo(handle);
                }
            }
        })
        .build()
}
