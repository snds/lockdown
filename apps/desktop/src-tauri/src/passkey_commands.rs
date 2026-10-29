//! The webview's passkey commands (docs/plan/19-passkeys.md §6): the
//! management view of an item's passkeys, a delete (through the `Use` gate
//! for a crown jewel), the undo of a site's Signal, and the browser's
//! passkey requests that wait for the person in the app, and the notice
//! after a site's automatic upgrade. Nothing here
//! carries a key: the core signs with it in place.

use std::sync::Arc;

use lockdown_app::passkeys::{B64, PasskeyView, UpgradeNotice};
use lockdown_app::{App, AppError, AppResult, GateInput};
use tauri::{AppHandle, Manager, State};

use crate::browser::Endpoint;
use crate::browser::passkeys::ApprovalView;

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

fn credential(text: &str) -> AppResult<B64> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|_| AppError::refused("that isn't a passkey's id"))
}

/// The passkeys on one item.
#[tauri::command]
pub async fn passkeys(app: State<'_, App>, item_id: String) -> AppResult<Vec<PasskeyView>> {
    blocking(&app, move |a| a.passkeys(&item_id)).await
}

/// Deletes one passkey from an item; a new version, so history keeps it.
#[tauri::command]
pub async fn passkey_delete(
    app: State<'_, App>,
    item_id: String,
    credential_id: String,
    gate: Option<GateInput>,
) -> AppResult<u64> {
    let gate = gate.unwrap_or_default();
    blocking(&app, move |a| {
        a.passkey_delete(&item_id, credential(&credential_id)?, gate)
    })
    .await
}

/// Undoes a site's Signal: the passkey is offered again.
#[tauri::command]
pub async fn passkey_restore(
    app: State<'_, App>,
    item_id: String,
    credential_id: String,
) -> AppResult<u64> {
    blocking(&app, move |a| {
        a.passkey_restore(&item_id, credential(&credential_id)?)
    })
    .await
}

/// The automatic passkey upgrades made since the last call, for the toast
/// (D210, D312 as changed by D375); each once.
#[tauri::command]
pub async fn passkey_upgrades(app: State<'_, App>) -> AppResult<Vec<UpgradeNotice>> {
    blocking(&app, |a| a.passkey_upgrades()).await
}

/// The person opened the item: its upgrade notices show no more here.
#[tauri::command]
pub async fn passkey_upgrade_seen(app: State<'_, App>, item_id: String) -> AppResult<u32> {
    blocking(&app, move |a| a.passkey_upgrade_seen(&item_id)).await
}

fn endpoint(handle: &AppHandle) -> AppResult<Arc<Endpoint>> {
    handle
        .try_state::<Arc<Endpoint>>()
        .map(|e| Arc::clone(&e))
        .ok_or_else(|| AppError::refused("the browser connection isn't running"))
}

/// Passkey requests from browsers waiting for the person.
#[tauri::command]
pub async fn browser_passkey_pending(handle: AppHandle) -> AppResult<Vec<ApprovalView>> {
    let e = endpoint(&handle)?;
    tauri::async_runtime::spawn_blocking(move || e.passkey_pending())
        .await
        .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))
}

/// The person's answer to one; an approval goes through the `Approve` gate.
#[tauri::command]
pub async fn browser_passkey_answer(
    handle: AppHandle,
    request_id: String,
    approve: bool,
    gate: Option<GateInput>,
) -> AppResult<()> {
    let e = endpoint(&handle)?;
    let gate = gate.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || e.passkey_answer(&request_id, approve, gate))
        .await
        .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
}
