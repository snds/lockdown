//! The Security screen's post-quantum status line (D346;
//! docs/plan/30-pq-status-line.md): one call into `lockdown-app`
//! (`App::pq_status`) on a worker thread. What crosses is the line in
//! words and the devices that hold it back, each by id and name; nothing
//! here holds a secret.

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::pq_status::PqStatusView;
    use lockdown_app::{App, AppError, AppResult};
    use tauri::State;

    #[tauri::command]
    pub async fn pq_status(app: State<'_, App>) -> AppResult<PqStatusView> {
        let app = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.pq_status())
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
    }
}
