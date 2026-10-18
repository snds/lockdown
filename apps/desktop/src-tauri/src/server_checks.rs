//! What the window shows about the server itself
//! (docs/plan/27-web-server-finish.md §3–§4): the server's last successful
//! restore drill, as the server reports it, and the key-transparency log
//! mirrors each sync compares against. One call each into `lockdown-app`
//! (`server_checks`) on a worker thread; nothing here holds a secret.

/// The most mirror addresses one call takes, checked before the core.
pub const MAX_URLS: usize = 8;

/// Whether a list of mirror addresses is small enough to hand on.
pub fn urls_fit(urls: &[String]) -> bool {
    urls.len() <= MAX_URLS && urls.iter().all(|u| u.len() <= 512)
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::server_checks::{MirrorsView, RestoreDrillView};
    use lockdown_app::{App, AppError, AppResult};
    use tauri::State;

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

    #[tauri::command]
    pub async fn server_restore_drill(app: State<'_, App>) -> AppResult<RestoreDrillView> {
        blocking(&app, |a| a.restore_drill()).await
    }

    #[tauri::command]
    pub async fn kt_mirrors(app: State<'_, App>) -> AppResult<MirrorsView> {
        blocking(&app, |a| a.kt_mirrors()).await
    }

    #[tauri::command]
    pub async fn kt_mirrors_set(app: State<'_, App>, urls: Vec<String>) -> AppResult<MirrorsView> {
        if !super::urls_fit(&urls) {
            return Err(AppError::refused("at most eight mirror addresses"));
        }
        blocking(&app, move |a| a.set_kt_mirrors(urls)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_lists_are_bounded_before_they_reach_the_core() {
        assert!(urls_fit(&["https://m.example".into()]));
        assert!(!urls_fit(&vec!["https://m.example".to_string(); 9]));
        assert!(!urls_fit(&["x".repeat(513)]));
    }
}
