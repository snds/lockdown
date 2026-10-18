//! The browser extension's side of the shell: the endpoint started with
//! the window, its clipboard path, and the commands the pairing screen
//! calls (docs/plan/16-browser-extension.md §3). The endpoint itself is in
//! `browser/` and needs no Tauri.

use std::sync::Arc;

use lockdown_app::{App, AppError, AppResult, ErrorCode, GateInput, Revealed};
use lockdown_native_messaging::manifest::{Browser, Target};
use tauri::{AppHandle, Manager};

use crate::browser::pairing::{PairedView, PendingView};
use crate::browser::{CopySink, Endpoint, EndpointConfig, EndpointStatus};
use crate::clipboard::ClipboardGuard;

fn failed(message: impl Into<String>) -> AppError {
    AppError {
        code: ErrorCode::Refused,
        message: message.into(),
        detail: None,
    }
}

/// A copy the extension asked for goes the same way as one from the window:
/// core → clipboard on the Rust side, the person's timer, the clear only for
/// our own copy (D85, D119).
struct ShellClipboard {
    handle: AppHandle,
}

impl CopySink for ShellClipboard {
    fn copy(&self, value: Revealed) -> Result<u64, String> {
        let guard = self
            .handle
            .try_state::<ClipboardGuard>()
            .ok_or("the clipboard isn't ready")?;
        let secs = self
            .handle
            .try_state::<App>()
            .and_then(|a| a.settings().ok())
            .map(|s| s.clipboard_clear_secs);
        crate::commands::copy_with_timer(&guard, &self.handle, value, secs)
            .map(|c| c.clears_in_secs)
            .map_err(|e| e.message)
    }
}

/// Starts the endpoint beside the window. A machine without the host or
/// without a transport still gets an endpoint, idle, that says why.
pub fn start(handle: &AppHandle, core: App, data: std::path::PathBuf) {
    let sink = Arc::new(ShellClipboard {
        handle: handle.clone(),
    });
    let started = EndpointConfig::for_app(data).and_then(|cfg| Endpoint::start(core, sink, cfg));
    if let Ok(endpoint) = started {
        handle.manage(Arc::new(endpoint));
    }
}

fn endpoint(handle: &AppHandle) -> AppResult<Arc<Endpoint>> {
    handle
        .try_state::<Arc<Endpoint>>()
        .map(|e| Arc::clone(&e))
        .ok_or_else(|| failed("the browser connection isn't running"))
}

async fn blocking<T, F>(handle: AppHandle, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<Endpoint>) -> AppResult<T> + Send + 'static,
{
    let e = endpoint(&handle)?;
    tauri::async_runtime::spawn_blocking(move || f(e))
        .await
        .map_err(|e| failed(format!("the call didn't finish: {e}")))?
}

/// The socket's state and the browsers on this machine.
#[tauri::command]
pub async fn browser_status(handle: AppHandle) -> AppResult<EndpointStatus> {
    blocking(handle, |e| Ok(e.status())).await
}

/// Pairing requests waiting for the person, each with its code.
#[tauri::command]
pub async fn browser_pending(handle: AppHandle) -> AppResult<Vec<PendingView>> {
    blocking(handle, |e| Ok(e.pending())).await
}

/// The person's answer to a pairing request; an approval goes through the
/// `Approve` gate (D101).
#[tauri::command]
pub async fn browser_answer(
    handle: AppHandle,
    request_id: String,
    approve: bool,
    gate: Option<GateInput>,
) -> AppResult<()> {
    let gate = gate.unwrap_or_default();
    blocking(handle, move |e| e.answer(&request_id, approve, gate)).await
}

/// Every paired browser.
#[tauri::command]
pub async fn browser_paired(handle: AppHandle) -> AppResult<Vec<PairedView>> {
    blocking(handle, |e| Ok(e.paired())).await
}

/// Forgets a paired browser.
#[tauri::command]
pub async fn browser_unpair(handle: AppHandle, client_id: String) -> AppResult<()> {
    blocking(handle, move |e| e.unpair(&client_id).map_err(failed)).await
}

/// Writes the native-messaging manifest for these browsers (every detected
/// one when the list is empty).
#[tauri::command]
pub async fn browser_install(handle: AppHandle, browsers: Vec<Browser>) -> AppResult<Vec<Target>> {
    blocking(handle, move |e| e.install(&browsers).map_err(failed)).await
}
