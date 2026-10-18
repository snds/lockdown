//! Live sync and the session list in the window
//! (docs/plan/20-self-host-server.md §5–§6). The core keeps the server's
//! live channel open while the vault is unlocked (`lockdown_app::live`);
//! each notice it raises reaches the webview as one window event,
//! [`EVENT`], carrying the notice as JSON (`{kind, …}`): plain words and
//! counts, never a value. The webview redraws on `synced`, re-reads alerts
//! or sessions on `alerts` and `sessions`, and says so on `session_ended`.
//!
//! The session commands are one call each into `lockdown-app` on a worker
//! thread.

/// The window event every live notice is sent as.
pub const EVENT: &str = "lockdown://live";

/// Whether a session id is one the server could have issued (32 hex
/// digits), checked before it goes anywhere.
pub fn is_session_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(feature = "shell")]
pub mod commands {
    use std::sync::Arc;

    use lockdown_app::live::{LiveHandle, LiveNotice, SessionView};
    use lockdown_app::{App, AppError, AppResult};
    use tauri::{AppHandle, Emitter, State};

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

    /// Starts the live channel; the handle lives as long as the window.
    pub fn start(handle: &AppHandle, core: &App) -> LiveHandle {
        let emitter = handle.clone();
        core.start_live(Arc::new(move |notice: LiveNotice| {
            let _ = emitter.emit(super::EVENT, notice);
        }))
    }

    #[tauri::command]
    pub async fn session_list(app: State<'_, App>) -> AppResult<Vec<SessionView>> {
        blocking(&app, |a| a.sessions()).await
    }

    #[tauri::command]
    pub async fn session_end(app: State<'_, App>, id: String) -> AppResult<()> {
        if !super::is_session_id(&id) {
            return Err(AppError::refused("that isn't a session"));
        }
        blocking(&app, move |a| a.end_session(&id)).await
    }

    #[tauri::command]
    pub async fn session_end_others(app: State<'_, App>) -> AppResult<u64> {
        blocking(&app, |a| a.end_other_sessions()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_checked_before_they_go_anywhere() {
        assert!(is_session_id(&"ab".repeat(16)));
        assert!(!is_session_id("ab"));
        assert!(!is_session_id(&"zz".repeat(16)));
        assert!(!is_session_id(&format!("{}/../x", "a".repeat(27))));
    }
}
