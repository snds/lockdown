//! The webview's commands for sharing, households and emergency access
//! (docs/plan/18-sharing.md §8). Each is one call into `lockdown-app` on a
//! worker thread; the gated ones take a `gate` like `copy_secret` does.
//!
//! What crosses: names, ids, rights, fingerprints and plain words. A link
//! made here (a share link, a request link) carries its secret in its
//! fragment, so it comes back as raw bytes the webview shows once and
//! wipes, like a revealed value; what a link or an answer holds is fetched
//! one value at a time as raw bytes (`opened_value`, `opened_file`). Typed
//! secrets go in as byte arrays the webview zeroes after the call.
//!
//! The option parsing is outside the `shell` feature so it is unit-tested
//! without Tauri.

use lockdown_app::sharing::SendOptions;
use lockdown_app::{AppError, AppResult};

/// The share link's lifetime and view limit as the dialog sends them,
/// checked: 1 to 30 days, and 1 to 1,000 views or none.
pub fn send_options(days: u32, max_views: Option<u32>) -> AppResult<SendOptions> {
    if !(1..=30).contains(&days) {
        return Err(AppError::refused("a share link lasts 1 to 30 days"));
    }
    if max_views.is_some_and(|m| m == 0 || m > 1_000) {
        return Err(AppError::refused(
            "a share link opens 1 to 1,000 times, or without a limit until it expires",
        ));
    }
    Ok(SendOptions { days, max_views })
}

/// An empty password field means no password.
pub fn password_of(password: Option<Vec<u8>>) -> Option<Vec<u8>> {
    password.filter(|p| !p.is_empty())
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::sharing::{
        ContactView, EmergencyView, OpenedView, RequestView, SendView, SentShareView,
        SharingOverview, VaultSharingView,
    };
    use lockdown_app::{App, AppError, AppResult, GateInput};
    use tauri::ipc::Response;
    use tauri::{AppHandle, State};
    use zeroize::Zeroizing;

    use super::{password_of, send_options};
    use crate::clipboard::ClipboardGuard;
    use crate::commands::{Copied, copy_with_timer};

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

    fn bytes(r: lockdown_app::Revealed) -> Response {
        Response::new(r.into_bytes().to_vec())
    }

    #[tauri::command]
    pub async fn sharing_overview(app: State<'_, App>) -> AppResult<SharingOverview> {
        blocking(&app, |a| a.sharing_overview()).await
    }

    #[tauri::command]
    pub async fn contact_lookup(app: State<'_, App>, login: String) -> AppResult<ContactView> {
        blocking(&app, move |a| a.lookup_contact(&login)).await
    }

    #[tauri::command]
    pub async fn contact_verify(
        app: State<'_, App>,
        account: String,
        fingerprint: String,
    ) -> AppResult<ContactView> {
        blocking(&app, move |a| a.verify_contact(&account, &fingerprint)).await
    }

    #[tauri::command]
    pub async fn contact_forget(app: State<'_, App>, account: String) -> AppResult<()> {
        blocking(&app, move |a| a.forget_contact(&account)).await
    }

    #[tauri::command]
    pub async fn vault_sharing(app: State<'_, App>, vault_id: String) -> AppResult<VaultSharingView> {
        blocking(&app, move |a| a.vault_sharing(&vault_id)).await
    }

    #[tauri::command]
    pub async fn vault_share(
        app: State<'_, App>,
        vault_id: String,
        login: String,
        right: String,
        gate: Option<GateInput>,
    ) -> AppResult<VaultSharingView> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.share_vault(&vault_id, &login, &right, gate)).await
    }

    #[tauri::command]
    pub async fn vault_member_right(
        app: State<'_, App>,
        vault_id: String,
        account: String,
        right: String,
    ) -> AppResult<VaultSharingView> {
        blocking(&app, move |a| a.set_member_right(&vault_id, &account, &right)).await
    }

    #[tauri::command]
    pub async fn vault_member_remove(
        app: State<'_, App>,
        vault_id: String,
        account: String,
    ) -> AppResult<VaultSharingView> {
        blocking(&app, move |a| a.remove_member(&vault_id, &account)).await
    }

    #[tauri::command]
    pub async fn household_create(app: State<'_, App>, name: String) -> AppResult<String> {
        blocking(&app, move |a| a.create_household(&name)).await
    }

    #[tauri::command]
    pub async fn household_add(
        app: State<'_, App>,
        vault_id: String,
        login: String,
        right: String,
        gate: Option<GateInput>,
    ) -> AppResult<VaultSharingView> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.add_to_household(&vault_id, &login, &right, gate)).await
    }

    #[tauri::command]
    pub async fn item_share(
        app: State<'_, App>,
        id: String,
        login: String,
        right: String,
        gate: Option<GateInput>,
    ) -> AppResult<SentShareView> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.share_item(&id, &login, &right, gate)).await
    }

    #[tauri::command]
    pub async fn item_unshare(app: State<'_, App>, share_id: String) -> AppResult<()> {
        blocking(&app, move |a| a.unshare_item(&share_id)).await
    }

    #[tauri::command]
    pub async fn send_item(
        app: State<'_, App>,
        id: String,
        days: u32,
        max_views: Option<u32>,
        password: Option<Vec<u8>>,
        gate: Option<GateInput>,
    ) -> AppResult<Response> {
        let gate = gate.unwrap_or_default();
        let options = send_options(days, max_views)?;
        let password = password_of(password);
        blocking(&app, move |a| a.send_item(&id, password, options, gate).map(bytes)).await
    }

    #[tauri::command]
    pub async fn send_text(
        app: State<'_, App>,
        title: String,
        text: Vec<u8>,
        days: u32,
        max_views: Option<u32>,
        password: Option<Vec<u8>>,
    ) -> AppResult<Response> {
        let options = send_options(days, max_views)?;
        let password = password_of(password);
        blocking(&app, move |a| a.send_text(&title, text, password, options).map(bytes)).await
    }

    #[tauri::command]
    pub async fn sends(app: State<'_, App>) -> AppResult<Vec<SendView>> {
        blocking(&app, |a| a.sends()).await
    }

    #[tauri::command]
    pub async fn send_delete(app: State<'_, App>, send_id: String) -> AppResult<()> {
        blocking(&app, move |a| a.delete_send(&send_id)).await
    }

    #[tauri::command]
    pub async fn send_open(
        app: State<'_, App>,
        link: Vec<u8>,
        password: Option<Vec<u8>>,
    ) -> AppResult<OpenedView> {
        let link = Zeroizing::new(link);
        let password = password_of(password);
        blocking(&app, move |a| a.open_send(&link, password)).await
    }

    #[tauri::command]
    pub async fn opened_value(app: State<'_, App>, index: usize) -> AppResult<Response> {
        blocking(&app, move |a| a.opened_value(index).map(bytes)).await
    }

    /// Copies one value of what was opened on the clipboard path every
    /// secret takes (`copy_secret`): written here, cleared by the timer
    /// only while it is still this copy.
    #[tauri::command]
    pub async fn opened_copy(
        app: State<'_, App>,
        guard: State<'_, ClipboardGuard>,
        handle: AppHandle,
        index: usize,
        clear_after_secs: Option<u64>,
    ) -> AppResult<Copied> {
        let value = blocking(&app, move |a| a.opened_value(index)).await?;
        copy_with_timer(&guard, &handle, value, clear_after_secs)
    }

    /// Copies a link just made here (its key is in the fragment) on the
    /// same clipboard path and timer, so a link is never left on the
    /// clipboard any longer than a password would be.
    #[tauri::command]
    pub async fn link_copy(
        guard: State<'_, ClipboardGuard>,
        handle: AppHandle,
        link: Vec<u8>,
        clear_after_secs: Option<u64>,
    ) -> AppResult<Copied> {
        copy_with_timer(
            &guard,
            &handle,
            lockdown_app::Revealed::new(link),
            clear_after_secs,
        )
    }

    #[tauri::command]
    pub async fn opened_file(app: State<'_, App>) -> AppResult<Response> {
        blocking(&app, |a| a.opened_file().map(bytes)).await
    }

    #[tauri::command]
    pub async fn opened_save(app: State<'_, App>) -> AppResult<String> {
        blocking(&app, |a| a.save_opened()).await
    }

    #[tauri::command]
    pub async fn request_create(app: State<'_, App>, label: String, days: u32) -> AppResult<Response> {
        blocking(&app, move |a| a.create_request(&label, days).map(bytes)).await
    }

    #[tauri::command]
    pub async fn requests(app: State<'_, App>) -> AppResult<Vec<RequestView>> {
        blocking(&app, |a| a.requests()).await
    }

    #[tauri::command]
    pub async fn request_open(app: State<'_, App>, request_id: String) -> AppResult<OpenedView> {
        blocking(&app, move |a| a.open_answer(&request_id)).await
    }

    #[tauri::command]
    pub async fn request_delete(app: State<'_, App>, request_id: String) -> AppResult<()> {
        blocking(&app, move |a| a.delete_request(&request_id)).await
    }

    #[tauri::command]
    pub async fn request_label(app: State<'_, App>, link: Vec<u8>) -> AppResult<String> {
        let link = Zeroizing::new(link);
        blocking(&app, move |a| a.request_label(&link)).await
    }

    #[tauri::command]
    pub async fn request_answer(
        app: State<'_, App>,
        link: Vec<u8>,
        title: String,
        secret: Vec<u8>,
    ) -> AppResult<()> {
        let link = Zeroizing::new(link);
        blocking(&app, move |a| a.answer_request(&link, &title, secret)).await
    }

    #[tauri::command]
    pub async fn emergency_add(
        app: State<'_, App>,
        login: String,
        days: u32,
        vaults: Vec<String>,
        gate: Option<GateInput>,
    ) -> AppResult<Vec<EmergencyView>> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.add_emergency_contact(&login, days, vaults, gate)).await
    }

    #[tauri::command]
    pub async fn emergency_list(app: State<'_, App>) -> AppResult<Vec<EmergencyView>> {
        blocking(&app, |a| a.emergency()).await
    }

    #[tauri::command]
    pub async fn emergency_step(
        app: State<'_, App>,
        grant: String,
        action: String,
    ) -> AppResult<Vec<EmergencyView>> {
        blocking(&app, move |a| a.emergency_step(&grant, &action)).await
    }

    #[tauri::command]
    pub async fn emergency_open(app: State<'_, App>, grant: String) -> AppResult<Vec<EmergencyView>> {
        blocking(&app, move |a| a.emergency_open(&grant)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_options_are_checked_before_anything_is_sealed() {
        assert!(send_options(0, None).is_err());
        assert!(send_options(31, None).is_err());
        assert!(send_options(7, Some(0)).is_err());
        assert!(send_options(7, Some(1_001)).is_err());
        let ok = send_options(7, Some(3)).unwrap();
        assert_eq!((ok.days, ok.max_views), (7, Some(3)));
        assert!(send_options(30, None).unwrap().max_views.is_none());
        assert_eq!(password_of(Some(Vec::new())), None);
        assert_eq!(password_of(Some(b"x".to_vec())), Some(b"x".to_vec()));
    }
}
