//! One Tauri command per app operation. Every command runs the blocking
//! call on a worker thread (`spawn_blocking`), never on the main thread,
//! so a sync over the network never freezes the window. Errors cross as
//! `lockdown_app::AppError` (a code and plain words).

use std::time::Duration;

use lockdown_app::{
    App, AppError, AppResult, ApprovalKind, ErrorCode, GateInput, LockPolicy, Settings,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::clipboard::{self, ClipboardGuard};
use crate::items::{FieldChangeInput, into_changes};

fn failed(message: impl Into<String>) -> AppError {
    AppError {
        code: ErrorCode::Refused,
        message: message.into(),
        detail: None,
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
async fn reveal(
    app: State<'_, App>,
    id: String,
    field_id: String,
    gate: Option<GateInput>,
) -> AppResult<Response> {
    let gate = gate.unwrap_or_default();
    let revealed = blocking(&app, move |a| a.reveal(&id, &field_id, gate)).await?;
    Ok(Response::new(revealed.into_bytes().to_vec()))
}

/// Copies one hidden field to the clipboard from the Rust side; the value
/// never reaches the webview. Cleared after `clear_after_secs` (default
/// 30, range 10–90) if the clipboard still holds it. Returns the seconds
/// until the clear, for the control's countdown. A crown jewel's copy goes
/// through the `Use` gate first (D101).
#[tauri::command]
async fn copy_secret(
    app: State<'_, App>,
    guard: State<'_, ClipboardGuard>,
    handle: AppHandle,
    id: String,
    field_id: String,
    clear_after_secs: Option<u64>,
    gate: Option<GateInput>,
) -> AppResult<u64> {
    let gate = gate.unwrap_or_default();
    let revealed = blocking(&app, move |a| a.reveal_to_use(&id, &field_id, gate)).await?;
    copy_with_timer(&guard, &handle, revealed, clear_after_secs)
}

/// Copies the code valid now for one code field (`which` = `next` copies
/// the one after it, as the display offers in a code's last seconds), on
/// the same clipboard path and timer as `copy_secret`; the set-up never
/// leaves the core. A code inside a crown jewel goes through the `Use`
/// gate first (D101).
// Three of the eight are Tauri's injected handles; the five the webview sends
// mirror `copy_secret` plus `which` and `gate`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn copy_code(
    app: State<'_, App>,
    guard: State<'_, ClipboardGuard>,
    handle: AppHandle,
    item_id: String,
    field_id: String,
    clear_after_secs: Option<u64>,
    which: Option<String>,
    gate: Option<GateInput>,
) -> AppResult<u64> {
    let next = which.as_deref() == Some("next");
    let gate = gate.unwrap_or_default();
    let revealed = blocking(&app, move |a| a.code_now(&item_id, &field_id, next, gate)).await?;
    copy_with_timer(&guard, &handle, revealed, clear_after_secs)
}

/// The clipboard rule (clipboard.rs): write, remember a hash and a
/// generation, clear later only if both still match.
fn copy_with_timer(
    guard: &ClipboardGuard,
    handle: &AppHandle,
    revealed: lockdown_app::Revealed,
    clear_after_secs: Option<u64>,
) -> AppResult<u64> {
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

/// Whether the clipboard still holds what Lockdown last copied, for the
/// copy control's own countdown (unicron's SecretField asks before it
/// says "Cleared"). `hash` is the hex SHA-256 the component computed for
/// the bytes it handed over, or `null` when the shell copied by reference
/// and the component never had them; it is compared with the shell's own
/// record only, never with the clipboard (clipboard.rs).
#[tauri::command]
async fn clipboard_holds(
    guard: State<'_, ClipboardGuard>,
    handle: AppHandle,
    hash: Option<String>,
) -> AppResult<bool> {
    let current = handle.clipboard().read_text().ok();
    Ok(guard.holds(
        current.as_deref().map(str::as_bytes),
        hash.as_deref().filter(|h| !h.is_empty()),
    ))
}

/// Clears the clipboard now, under the same rule as the timer: only if it
/// still holds what Lockdown wrote. The component's countdown calls this
/// at expiry; the shell's own timer runs regardless.
#[tauri::command]
async fn clipboard_clear(guard: State<'_, ClipboardGuard>, handle: AppHandle) -> AppResult<()> {
    let current = handle.clipboard().read_text().ok();
    if guard.holds(current.as_deref().map(str::as_bytes), None) {
        handle
            .clipboard()
            .clear()
            .map_err(|e| failed(format!("couldn't clear the clipboard: {e}")))?;
        guard.forget();
    }
    Ok(())
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

/// A crown jewel's edit goes through the `Use` gate (D110), so this takes
/// a `gate` like `edit_item`.
#[tauri::command]
async fn edit_login(
    app: State<'_, App>,
    id: String,
    title: Option<String>,
    username: Option<String>,
    password: Option<Vec<u8>>,
    gate: Option<GateInput>,
) -> AppResult<u64> {
    let gate = gate.unwrap_or_default();
    blocking(&app, move |a| {
        a.edit_login(&id, title.as_deref(), username.as_deref(), password, gate)
    })
    .await
}

/// Every kind's template, for the "New item" list, the editor and the
/// detail; and the kinds a field added by hand may take. Static data.
#[derive(Serialize)]
pub struct Templates {
    kinds: Vec<lockdown_app::TemplateView>,
    custom_kinds: Vec<lockdown_app::CustomKindView>,
}

#[tauri::command]
async fn templates(app: State<'_, App>) -> AppResult<Templates> {
    Ok(Templates {
        kinds: app.templates(),
        custom_kinds: app.custom_kinds(),
    })
}

/// Makes an item of any kind a person may make (D110). Secrets arrive as
/// byte arrays inside `fields` (items.rs) and are wiped after the call.
#[tauri::command]
async fn add_item(
    app: State<'_, App>,
    kind: String,
    title: Option<String>,
    crown_jewel: Option<bool>,
    fields: Vec<FieldChangeInput>,
) -> AppResult<String> {
    let changes = into_changes(fields)?;
    blocking(&app, move |a| {
        a.add_item(
            &kind,
            title.as_deref(),
            crown_jewel.unwrap_or(false),
            changes,
        )
    })
    .await
}

/// Saves a new version from an edit that names every field of the item:
/// `keep` for a secret the person did not retype, so it never crosses
/// back. A crown jewel's edit goes through the `Use` gate first.
#[tauri::command]
async fn edit_item(
    app: State<'_, App>,
    id: String,
    title: Option<String>,
    changes: Vec<FieldChangeInput>,
    gate: Option<GateInput>,
) -> AppResult<u64> {
    let changes = into_changes(changes)?;
    let gate = gate.unwrap_or_default();
    blocking(&app, move |a| {
        a.edit_item(&id, title.as_deref(), changes, gate)
    })
    .await
}

#[tauri::command]
async fn trash(app: State<'_, App>, id: String) -> AppResult<()> {
    blocking(&app, move |a| a.trash(&id)).await
}

/// Brings a trashed item back (the Deleted view's Restore).
#[tauri::command]
async fn restore(app: State<'_, App>, id: String) -> AppResult<()> {
    blocking(&app, move |a| a.restore(&id)).await
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
async fn device_revoke(
    app: State<'_, App>,
    id: String,
    gate: Option<GateInput>,
) -> AppResult<Vec<String>> {
    let gate = gate.unwrap_or_default();
    blocking(&app, move |a| a.device_revoke(&id, gate)).await
}

// --- settings, approvals, codes, personas, audit, import and export ------------

#[tauri::command]
async fn get_settings(app: State<'_, App>) -> AppResult<Settings> {
    blocking(&app, |a| a.settings()).await
}

#[tauri::command]
async fn set_settings(app: State<'_, App>, settings: Settings) -> AppResult<Settings> {
    blocking(&app, move |a| a.set_settings(settings)).await
}

/// Which check a kind would go to right now, so the UI asks the right way.
#[derive(Serialize)]
pub struct ApprovalRoute {
    /// `platform`, `password` or `window`.
    route: &'static str,
    /// `Touch ID`, `Windows Hello`, `password` or `window`.
    method: String,
}

#[tauri::command]
async fn approval_route(app: State<'_, App>, kind: ApprovalKind) -> AppResult<ApprovalRoute> {
    let (route, method) = blocking(&app, move |a| a.approval_route(kind)).await?;
    Ok(ApprovalRoute {
        route: route.as_str(),
        method,
    })
}

#[tauri::command]
async fn codes(app: State<'_, App>) -> AppResult<Vec<lockdown_app::CodeView>> {
    blocking(&app, |a| a.codes()).await
}

#[tauri::command]
async fn add_code(
    app: State<'_, App>,
    uri: String,
    title: Option<String>,
    into: Option<String>,
) -> AppResult<String> {
    let uri = zeroize::Zeroizing::new(uri);
    blocking(&app, move |a| {
        a.add_code(&uri, title.as_deref(), into.as_deref())
    })
    .await
}

#[tauri::command]
async fn personas(app: State<'_, App>) -> AppResult<Vec<lockdown_app::PersonaInfo>> {
    blocking(&app, |a| a.personas()).await
}

#[tauri::command]
async fn audit(app: State<'_, App>) -> AppResult<Vec<String>> {
    blocking(&app, |a| a.audit()).await
}

/// The file's bytes arrive from the webview's own file picker (the File
/// API, no `fs` capability); the parser treats them as hostile.
#[tauri::command]
async fn import_items(
    app: State<'_, App>,
    bytes: Vec<u8>,
    format: String,
) -> AppResult<lockdown_app::ImportReport> {
    blocking(&app, move |a| a.import_items(bytes, &format)).await
}

/// The vault as one file, as raw bytes, behind the `Reveal` gate.
#[tauri::command]
async fn export_items(
    app: State<'_, App>,
    format: String,
    gate: Option<GateInput>,
) -> AppResult<Response> {
    let gate = gate.unwrap_or_default();
    let bytes = blocking(&app, move |a| a.export_items(&format, gate)).await?;
    Ok(Response::new(bytes.into_bytes().to_vec()))
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

/// Opens this computer's date and time settings, for the code display's
/// "Fix clock": macOS and Windows have a URL for the pane; elsewhere this
/// answers `false` and the UI says where to look instead.
#[tauri::command]
async fn open_clock_settings(handle: AppHandle) -> AppResult<bool> {
    let url = if cfg!(target_os = "macos") {
        Some("x-apple.systempreferences:com.apple.preference.datetime")
    } else if cfg!(windows) {
        Some("ms-settings:dateandtime")
    } else {
        None
    };
    let Some(url) = url else {
        return Ok(false);
    };
    handle
        .opener()
        .open_url(url, None::<&str>)
        .map_err(|e| failed(format!("couldn't open the clock settings: {e}")))?;
    Ok(true)
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
        copy_code,
        clipboard_holds,
        clipboard_clear,
        add_login,
        edit_login,
        templates,
        add_item,
        edit_item,
        trash,
        restore,
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
        open_clock_settings,
        get_settings,
        set_settings,
        approval_route,
        codes,
        add_code,
        personas,
        audit,
        import_items,
        export_items,
    ]
}
