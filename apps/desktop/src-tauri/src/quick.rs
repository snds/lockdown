//! Quick access from anywhere (docs/plan/26-desktop-finish.md §1–§2): the
//! webview's commands over the core's one search index (`lockdown_app::quick`),
//! the code and fill actions, and the system-wide shortcut that opens a
//! small quick-access window without bringing up the main one.
//!
//! The shortcut is registered from Rust with Tauri's global-shortcut plugin;
//! no permission for it reaches either webview. It works on macOS, on
//! Windows and on Linux under X11. A Wayland session gives no global keys
//! to an application (only the compositor's own settings can bind one), so
//! there the registration fails and Settings says so in words; the in-app
//! Ctrl+K palette still works. The quick-access window is the same page
//! (`index.html?window=quick`) drawing only the palette; it hides when it
//! loses focus, on Escape, and after an action, and it shows nothing while
//! the vault is locked but the unlock button.

/// The shortcut, in the plugin's words.
pub const HOTKEY: &str = "CommandOrControl+Shift+Space";
/// The quick-access window's label (also listed in the capability file).
pub const QUICK_WINDOW: &str = "quick";
/// The page that window loads.
pub const QUICK_URL: &str = "index.html?window=quick";

/// The shortcut as people read it on this computer.
pub fn hotkey_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘⇧Space"
    } else {
        "Ctrl+Shift+Space"
    }
}

/// Why a registration failed, in words: on Linux a Wayland session is the
/// usual reason, named so the person knows it isn't a fault.
pub fn hotkey_failure(error: &str, wayland: bool) -> String {
    if wayland {
        "This computer runs Wayland, which doesn't let an app set a shortcut for the whole screen. Bind a shortcut to `lockdown-desktop` in your desktop's keyboard settings instead, or use Ctrl+K in Lockdown's window.".into()
    } else {
        format!(
            "Lockdown couldn't take {} for quick access ({error}); another app may be using it.",
            hotkey_label()
        )
    }
}

/// Whether this is a Wayland session (Linux only).
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland"))
}

/// The address a "Fill in the browser" may open: `http` or `https` only.
pub fn openable(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// What Settings shows about the shortcut.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HotkeyStatus {
    /// Whether the person wants it.
    pub on: bool,
    /// Whether it is registered now.
    pub registered: bool,
    /// The keys, as people read them here.
    pub label: String,
    /// Why it isn't registered, when it is wanted and isn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[cfg(feature = "shell")]
pub mod commands {
    use std::sync::Mutex;

    use lockdown_app::quick::{FillStarted, QuickHit};
    use lockdown_app::{App, AppError, AppResult, GateInput};
    use tauri::ipc::Response;
    use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    use tauri_plugin_opener::OpenerExt;

    use super::{HOTKEY, HotkeyStatus, QUICK_URL, QUICK_WINDOW, hotkey_failure, hotkey_label};

    /// Why the last registration failed, if it did.
    #[derive(Default)]
    pub struct HotkeyState(Mutex<Option<String>>);

    async fn blocking<T, F>(app: &App, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(App) -> AppResult<T> + Send + 'static,
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || f(app))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
    }

    /// The plugin, with the handler that toggles the quick-access window.
    pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle(app);
                }
            })
            .build()
    }

    /// Registers the shortcut when the settings want it; remembers why not.
    pub fn start(handle: &AppHandle, core: &App) {
        handle.manage(HotkeyState::default());
        let on = core
            .settings()
            .map(|s| s.quick_access_hotkey)
            .unwrap_or(true);
        apply(handle, on);
    }

    fn apply(handle: &AppHandle, on: bool) {
        let shortcuts = handle.global_shortcut();
        let problem = if on {
            if shortcuts.is_registered(HOTKEY) {
                None
            } else {
                shortcuts
                    .register(HOTKEY)
                    .err()
                    .map(|e| hotkey_failure(&e.to_string(), super::is_wayland()))
            }
        } else {
            let _ = shortcuts.unregister(HOTKEY);
            None
        };
        if let Some(state) = handle.try_state::<HotkeyState>()
            && let Ok(mut p) = state.0.lock()
        {
            *p = problem;
        }
    }

    /// Shows the quick-access window (made on first use), or hides it when
    /// it is in front.
    fn toggle(handle: &AppHandle) {
        if let Some(w) = handle.get_webview_window(QUICK_WINDOW) {
            if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
                let _ = w.hide();
            } else {
                let _ = w.show();
                let _ = w.set_focus();
                // The page puts the search back in focus and clears it.
                let _ = w.emit_to(QUICK_WINDOW, "lockdown://quick-shown", ());
            }
            return;
        }
        let built =
            WebviewWindowBuilder::new(handle, QUICK_WINDOW, WebviewUrl::App(QUICK_URL.into()))
                .title("Lockdown quick access")
                .inner_size(640.0, 440.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .center()
                .focused(true)
                .content_protected(true)
                .build();
        if let Ok(w) = built {
            let h = handle.clone();
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event
                    && let Some(w) = h.get_webview_window(QUICK_WINDOW)
                {
                    let _ = w.hide();
                }
            });
        }
    }

    /// The core's search: ranked, typo-tolerant, live items only.
    #[tauri::command]
    pub async fn quick_search(
        app: State<'_, App>,
        query: String,
        limit: Option<usize>,
    ) -> AppResult<Vec<QuickHit>> {
        let limit = limit.unwrap_or(50);
        blocking(&app, move |a| a.search(&query, limit)).await
    }

    /// The code valid now, to show in quick access: raw bytes, like a
    /// reveal; a crown jewel's goes through the `Use` gate.
    #[tauri::command]
    pub async fn quick_code(
        app: State<'_, App>,
        item_id: String,
        field_id: String,
        gate: Option<GateInput>,
    ) -> AppResult<Response> {
        let gate = gate.unwrap_or_default();
        let code = blocking(&app, move |a| a.quick_code(&item_id, &field_id, gate)).await?;
        Ok(Response::new(code.expose().to_vec()))
    }

    /// "Fill in the browser": the core remembers the request and answers
    /// the login's own website, which the shell opens (http and https only).
    #[tauri::command]
    pub async fn quick_fill(
        app: State<'_, App>,
        handle: AppHandle,
        item_id: String,
    ) -> AppResult<FillStarted> {
        let started = blocking(&app, move |a| a.quick_fill(&item_id)).await?;
        if !super::openable(&started.url) {
            return Err(AppError::refused(
                "only http and https addresses open from here",
            ));
        }
        handle
            .opener()
            .open_url(started.url.clone(), None::<&str>)
            .map_err(|e| AppError::refused(format!("couldn't open the browser: {e}")))?;
        hide_quick(&handle);
        Ok(started)
    }

    fn hide_quick(handle: &AppHandle) {
        if let Some(w) = handle.get_webview_window(QUICK_WINDOW) {
            let _ = w.hide();
        }
    }

    /// Hides the quick-access window (Escape, or after an action).
    #[tauri::command]
    pub async fn quick_hide(handle: AppHandle) -> AppResult<()> {
        hide_quick(&handle);
        Ok(())
    }

    /// Opens an item in the main window, from quick access.
    #[tauri::command]
    pub async fn quick_open_main(handle: AppHandle, item_id: Option<String>) -> AppResult<()> {
        hide_quick(&handle);
        if let Some(main) = handle.get_webview_window("main") {
            let _ = main.unminimize();
            let _ = main.show();
            let _ = main.set_focus();
            let _ = main.emit_to("main", "lockdown://open-item", item_id);
        }
        Ok(())
    }

    /// Whether the shortcut is on and working, for Settings.
    #[tauri::command]
    pub async fn quick_hotkey_status(
        app: State<'_, App>,
        handle: AppHandle,
        hotkey: State<'_, HotkeyState>,
    ) -> AppResult<HotkeyStatus> {
        let on = app.settings()?.quick_access_hotkey;
        let problem = hotkey.0.lock().ok().and_then(|p| p.clone());
        Ok(HotkeyStatus {
            on,
            registered: handle.global_shortcut().is_registered(HOTKEY),
            label: hotkey_label().into(),
            problem: problem.filter(|_| on),
        })
    }

    /// Turns the shortcut on or off, in the settings file and at once.
    #[tauri::command]
    pub async fn quick_hotkey_set(
        app: State<'_, App>,
        handle: AppHandle,
        hotkey: State<'_, HotkeyState>,
        on: bool,
    ) -> AppResult<HotkeyStatus> {
        let mut settings = app.settings()?;
        settings.quick_access_hotkey = on;
        app.set_settings(settings)?;
        apply(&handle, on);
        quick_hotkey_status(app, handle.clone(), hotkey).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcut_and_its_words() {
        assert_eq!(HOTKEY, "CommandOrControl+Shift+Space");
        assert!(hotkey_label().contains("Space"));
        assert!(hotkey_failure("taken", true).contains("Wayland"));
        let other = hotkey_failure("already registered", false);
        assert!(other.contains("already registered") && other.contains(hotkey_label()));
        assert!(openable("https://example.com/login"));
        assert!(openable("http://router.local/"));
        assert!(!openable("file:///etc/passwd"));
        assert!(!openable("javascript:alert(1)"));
        assert!(QUICK_URL.contains("window=quick"));
    }
}
