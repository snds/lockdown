//! One Tauri command per app operation. Every command runs the blocking
//! call on a worker thread (`spawn_blocking`), never on the main thread,
//! so a sync over the network never freezes the window. Errors cross as
//! `lockdown_app::AppError` (a code and plain words).

use std::time::Duration;

use lockdown_app::{App, AppError, AppResult, ErrorCode, LockPolicy};
use serde::Deserialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::clipboard::{self, ClipboardGuard};

fn failed(message: impl Into<String>) -> AppError {
    AppError {
        code: ErrorCode::Refused,
        message: message.into(),
    }
}

/// Runs `f` off the main thread and waits for it.
async fn blocking<T, F>(app: &App, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(App) -> AppResult<T> + Send + 'static,
{
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || f(app))
        .await
        .map_err(|e| failed(format!("the call didn't finish: {e}")))?
}

#[tauri::command]
async fn probe(app: State<'_, App>) -> AppResult<lockdown_app::Probe> {
    blocking(&app, |a| a.probe()).await
}

#[tauri::command]
async fn open_vault(app: State<'_, App>, create: bool) -> AppResult<lockdown_app::Status> {
    blocking(&app, move |a| a.open_or_create(create)).await
}

#[tauri::command]
async fn unlock(app: State<'_, App>) -> AppResult<lockdown_app::Status> {
    blocking(&app, |a| a.unlock()).await
}

#[tauri::command]
async fn lock(app: State<'_, App>, guard: State<'_, ClipboardGuard>) -> AppResult<()> {
    guard.forget();
    blocking(&app, |a| a.lock()).await
}

#[tauri::command]
async fn lock_state(app: State<'_, App>) -> AppResult<lockdown_app::LockState> {
    blocking(&app, |a| a.lock_state()).await
}

#[tauri::command]
async fn touch(app: State<'_, App>) -> AppResult<()> {
    blocking(&app, |a| a.touch()).await
}

#[tauri::command]
async fn set_hidden(app: State<'_, App>, hidden: bool) -> AppResult<lockdown_app::LockState> {
    blocking(&app, move |a| a.set_hidden(hidden)).await
}

/// The timers as the settings screen edits them; `null` means never.
#[derive(Deserialize)]
pub struct PolicyInput {
    idle_secs: Option<u64>,
    hidden_secs: Option<u64>,
    sleep_gap_secs: Option<u64>,
}

#[tauri::command]
async fn set_policy(
    app: State<'_, App>,
    policy: PolicyInput,
) -> AppResult<lockdown_app::LockState> {
    let policy = LockPolicy {
        idle: policy.idle_secs.map(Duration::from_secs),
        hidden: policy.hidden_secs.map(Duration::from_secs),
        sleep_gap: policy.sleep_gap_secs.map(Duration::from_secs),
    };
    blocking(&app, move |a| a.set_policy(policy)).await
}

#[tauri::command]
async fn list(app: State<'_, App>) -> AppResult<Vec<lockdown_app::ItemSummary>> {
    blocking(&app, |a| a.list()).await
}

/// Masked metadata only: the webview never gets a hidden value from here.
#[tauri::command]
async fn show(app: State<'_, App>, id: String) -> AppResult<lockdown_app::ItemDetail> {
    blocking(&app, move |a| a.show(&id, false)).await
}

/// One hidden field, as raw bytes (an `ArrayBuffer` in the webview, never
/// a JSON string). The webview wipes its copy when the field masks again.
#[tauri::command]
async fn reveal(app: State<'_, App>, id: String, field_id: String) -> AppResult<Response> {
    let revealed = blocking(&app, move |a| a.reveal(&id, &field_id)).await?;
    Ok(Response::new(revealed.into_bytes().to_vec()))
}

/// Copies one hidden field to the clipboard from the Rust side; the value
/// never reaches the webview. Cleared after `clear_after_secs` (default
/// 30, range 10–90) if the clipboard still holds it. Returns the seconds
/// until the clear, for the control's countdown.
#[tauri::command]
async fn copy_secret(
    app: State<'_, App>,
    guard: State<'_, ClipboardGuard>,
    handle: AppHandle,
    id: String,
    field_id: String,
    clear_after_secs: Option<u64>,
) -> AppResult<u64> {
    let revealed = blocking(&app, move |a| a.reveal(&id, &field_id)).await?;
    let text = std::str::from_utf8(revealed.expose())
        .map(|s| zeroize::Zeroizing::new(s.to_string()))
        .map_err(|_| failed("this field isn't text"))?;
    handle
        .clipboard()
        .write_text(text.as_str())
        .map_err(|e| failed(format!("couldn't copy: {e}")))?;
    let generation = guard.note_written(text.as_bytes());
    drop(text);
    drop(revealed);
    let after = clipboard::clear_after(clear_after_secs);
    let handle2 = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(after).await;
        let Some(guard) = handle2.try_state::<ClipboardGuard>() else {
            return;
        };
        let current = handle2.clipboard().read_text().ok();
        if guard.should_clear(generation, current.as_deref().map(str::as_bytes)) {
            let _ = handle2.clipboard().clear();
            guard.forget();
        }
    });
    Ok(after.as_secs())
}

/// Sleeps on the async runtime without pulling tokio into this crate's
/// dependencies by name.
async fn tokio_sleep(d: Duration) {
    let (tx, rx) = tauri::async_runtime::channel::<()>(1);
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.blocking_send(());
    });
    let mut rx = rx;
    let _ = rx.recv().await;
}

#[tauri::command]
async fn add_login(
    app: State<'_, App>,
    site: String,
    username: String,
    password: Vec<u8>,
    title: Option<String>,
) -> AppResult<String> {
    blocking(&app, move |a| {
        a.add_login(&site, &username, password, title.as_deref())
    })
    .await
}

#[tauri::command]
async fn edit_login(
    app: State<'_, App>,
    id: String,
    title: Option<String>,
    username: Option<String>,
    password: Option<Vec<u8>>,
) -> AppResult<u64> {
    blocking(&app, move |a| {
        a.edit_login(&id, title.as_deref(), username.as_deref(), password)
    })
    .await
}

#[tauri::command]
async fn trash(app: State<'_, App>, id: String) -> AppResult<()> {
    blocking(&app, move |a| a.trash(&id)).await
}

#[tauri::command]
async fn status(app: State<'_, App>) -> AppResult<lockdown_app::Status> {
    blocking(&app, |a| a.status()).await
}

/// What registering returns to the webview: the notes, and the kit page
/// as bytes (shown once in a secret field, then wiped).
#[tauri::command]
async fn account_register(
    app: State<'_, App>,
    server: String,
    login: String,
    password: String,
    secret_key: Option<String>,
) -> AppResult<Response> {
    let password = zeroize::Zeroizing::new(password);
    let secret_key = secret_key.map(zeroize::Zeroizing::new);
    let registered = blocking(&app, move |a| {
        a.account_register(
            &server,
            &login,
            &password,
            secret_key.as_deref().map(|s| s.as_str()),
        )
    })
    .await?;
    // Lines first as JSON, then a NUL, then the kit page's bytes.
    let mut body = serde_json::to_vec(&registered.lines).map_err(|e| failed(e.to_string()))?;
    body.push(0);
    if let Some(kit) = registered.kit {
        body.extend_from_slice(kit.expose());
    }
    Ok(Response::new(body))
}

#[tauri::command]
async fn account_login(
    app: State<'_, App>,
    server: String,
    login: String,
    password: String,
    secret_key: Option<String>,
) -> AppResult<Vec<String>> {
    let password = zeroize::Zeroizing::new(password);
    let secret_key = secret_key.map(zeroize::Zeroizing::new);
    blocking(&app, move |a| {
        a.account_login(
            &server,
            &login,
            &password,
            secret_key.as_deref().map(|s| s.as_str()),
        )
    })
    .await
}

#[tauri::command]
async fn recover(
    app: State<'_, App>,
    server: String,
    login: String,
    password: String,
    secret_key: String,
    recovery_code: String,
) -> AppResult<Vec<String>> {
    let password = zeroize::Zeroizing::new(password);
    let secret_key = zeroize::Zeroizing::new(secret_key);
    let recovery_code = zeroize::Zeroizing::new(recovery_code);
    blocking(&app, move |a| {
        a.recover(&server, &login, &password, &secret_key, &recovery_code)
    })
    .await
}

#[tauri::command]
async fn sync(app: State<'_, App>) -> AppResult<lockdown_app::SyncOutcome> {
    blocking(&app, |a| a.sync()).await
}

/// The kit page as bytes after a NUL-terminated JSON list of notes.
#[tauri::command]
async fn kit_create(
    app: State<'_, App>,
    replace: bool,
    password: Option<String>,
) -> AppResult<Response> {
    let password = password.map(zeroize::Zeroizing::new);
    let made = blocking(&app, move |a| {
        a.kit_create(replace, password.as_deref().map(|p| p.as_str()))
    })
    .await?;
    let mut body = serde_json::to_vec(&made.notes).map_err(|e| failed(e.to_string()))?;
    body.push(0);
    body.extend_from_slice(made.kit.expose());
    Ok(Response::new(body))
}

#[tauri::command]
async fn kit_verify(
    app: State<'_, App>,
    secret_key: String,
    recovery_code: String,
    password: Option<String>,
    drill: bool,
) -> AppResult<Vec<String>> {
    let secret_key = zeroize::Zeroizing::new(secret_key);
    let recovery_code = zeroize::Zeroizing::new(recovery_code);
    let password = password.map(zeroize::Zeroizing::new);
    blocking(&app, move |a| {
        a.kit_verify(
            &secret_key,
            &recovery_code,
            password.as_deref().map(|p| p.as_str()),
            drill,
        )
    })
    .await
}

#[tauri::command]
async fn device_list(app: State<'_, App>) -> AppResult<Vec<lockdown_app::DeviceInfo>> {
    blocking(&app, |a| a.device_list()).await
}

#[tauri::command]
async fn device_revoke(app: State<'_, App>, id: String) -> AppResult<Vec<String>> {
    blocking(&app, move |a| a.device_revoke(&id)).await
}

/// Opens an item's site in the default browser. Only http and https: the
/// webview never gets to launch anything else.
#[tauri::command]
async fn open_site(handle: AppHandle, url: String) -> AppResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(failed("only http and https links open from here"));
    }
    handle
        .opener()
        .open_url(url, None::<&str>)
        .map_err(|e| failed(format!("couldn't open the browser: {e}")))
}

/// Every command the webview may call.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        probe,
        open_vault,
        unlock,
        lock,
        lock_state,
        touch,
        set_hidden,
        set_policy,
        list,
        show,
        reveal,
        copy_secret,
        add_login,
        edit_login,
        trash,
        status,
        account_register,
        account_login,
        recover,
        sync,
        kit_create,
        kit_verify,
        device_list,
        device_revoke,
        open_site,
    ]
}
