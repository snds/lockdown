//! The webview's commands for security depth (docs/plan/22-security-depth.md):
//! per-item exposure, crown jewels in emergency access, the reminders and
//! the drill, adding a device by approval, joining from another device,
//! security keys on the account, and checking a contact's keys with a
//! phrase. Each is one call into `lockdown-app` on a worker thread; the
//! gated ones take a `gate` as `copy_secret` does.
//!
//! The pages a security key needs (adding one, using one for a waiting
//! sign-in) are the server's own, opened from here in the system browser,
//! never by the webview. [`server_page`] checks that a page is on the
//! account's server and is http or https before anything opens it.
//!
//! The checks are outside the `shell` feature so they are unit-tested
//! without Tauri.

use lockdown_app::{AppError, AppResult};

/// Refuses a page that isn't http(s) on `server` (the account's server, as
/// the core names it), so nothing but the server's own page ever opens.
pub fn server_page(url: &str, server: &str) -> AppResult<()> {
    let server = server.trim_end_matches('/');
    let ok_scheme = url.starts_with("https://") || url.starts_with("http://");
    let on_server = url
        .strip_prefix(server)
        .is_some_and(|rest| rest.starts_with('/'));
    if ok_scheme && on_server && !server.is_empty() {
        Ok(())
    } else {
        Err(AppError::refused(
            "that page isn't on your account's server, so it wasn't opened",
        ))
    }
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::devices::{JoinView, LinkView, SecretKeyView, SecurityKeyList};
    use lockdown_app::key_check::{PhraseCheckView, PhraseIncoming};
    use lockdown_app::protection::ExposureView;
    use lockdown_app::reminders::{DrillCheck, RemindersView};
    use lockdown_app::{App, AppError, AppResult, GateInput, Status};
    use tauri::ipc::Response;
    use tauri::{AppHandle, State};
    use tauri_plugin_opener::OpenerExt;
    use zeroize::Zeroizing;

    use super::server_page;

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

    fn open_page(handle: &AppHandle, app: &App, url: &str) -> AppResult<()> {
        let server = app.status()?.account.map(|a| a.server).unwrap_or_default();
        open_on(handle, url, &server)
    }

    fn open_on(handle: &AppHandle, url: &str, server: &str) -> AppResult<()> {
        server_page(url, server)?;
        handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| AppError::refused(format!("couldn't open the browser: {e}")))
    }

    // --- exposure (§1) -------------------------------------------------------------

    #[tauri::command]
    pub async fn exposure(app: State<'_, App>, id: String) -> AppResult<ExposureView> {
        blocking(&app, move |a| a.exposure(&id)).await
    }

    #[tauri::command]
    pub async fn exposures(app: State<'_, App>) -> AppResult<Vec<ExposureView>> {
        blocking(&app, |a| a.exposures()).await
    }

    #[tauri::command]
    pub async fn set_exposure(
        app: State<'_, App>,
        id: String,
        tier: Option<String>,
        gate: Option<GateInput>,
    ) -> AppResult<ExposureView> {
        blocking(&app, move |a| {
            a.set_exposure(&id, tier.as_deref(), gate.unwrap_or_default())
        })
        .await
    }

    // --- crown jewels in emergency access (§8) -----------------------------------

    #[tauri::command]
    pub async fn legacy_opt_in(app: State<'_, App>, id: String) -> AppResult<bool> {
        blocking(&app, move |a| a.legacy_opt_in(&id)).await
    }

    #[tauri::command]
    pub async fn set_legacy_opt_in(
        app: State<'_, App>,
        id: String,
        on: bool,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        blocking(&app, move |a| {
            a.set_legacy_opt_in(&id, on, gate.unwrap_or_default())
        })
        .await
    }

    // --- reminders and the drill (§5–§7) ---------------------------------------

    #[tauri::command]
    pub async fn reminders(app: State<'_, App>) -> AppResult<RemindersView> {
        blocking(&app, |a| a.reminders()).await
    }

    #[tauri::command]
    pub async fn password_check(app: State<'_, App>, password: String) -> AppResult<RemindersView> {
        let password = Zeroizing::new(password);
        blocking(&app, move |a| a.password_check(&password)).await
    }

    #[tauri::command]
    pub async fn password_snooze(app: State<'_, App>) -> AppResult<RemindersView> {
        blocking(&app, |a| a.password_snooze()).await
    }

    #[tauri::command]
    pub async fn set_password_reminder_days(
        app: State<'_, App>,
        days: u32,
    ) -> AppResult<RemindersView> {
        blocking(&app, move |a| a.set_password_reminder_days(days)).await
    }

    #[tauri::command]
    pub async fn drill_snooze(app: State<'_, App>) -> AppResult<RemindersView> {
        blocking(&app, |a| a.drill_snooze()).await
    }

    #[tauri::command]
    pub async fn kit_test_skip(app: State<'_, App>, understood: bool) -> AppResult<RemindersView> {
        blocking(&app, move |a| a.kit_test_skip(understood)).await
    }

    #[tauri::command]
    pub async fn drill_checks(app: State<'_, App>) -> AppResult<Vec<DrillCheck>> {
        blocking(&app, |a| a.drill_checks()).await
    }

    #[tauri::command]
    pub async fn strengthen_account(
        app: State<'_, App>,
        password: String,
    ) -> AppResult<Option<String>> {
        let password = Zeroizing::new(password);
        blocking(&app, move |a| a.strengthen_account(&password)).await
    }

    // --- checking a contact's keys (§9) ----------------------------------------

    #[tauri::command]
    pub async fn phrase_invite(app: State<'_, App>, login: String) -> AppResult<PhraseCheckView> {
        blocking(&app, move |a| a.phrase_invite(&login)).await
    }

    #[tauri::command]
    pub async fn phrase_incoming(app: State<'_, App>) -> AppResult<Vec<PhraseIncoming>> {
        blocking(&app, |a| a.phrase_incoming()).await
    }

    #[tauri::command]
    pub async fn phrase_answer(
        app: State<'_, App>,
        verify_id: String,
        phrase: String,
    ) -> AppResult<PhraseCheckView> {
        let phrase = Zeroizing::new(phrase);
        blocking(&app, move |a| a.phrase_answer(&verify_id, &phrase)).await
    }

    #[tauri::command]
    pub async fn phrase_poll(app: State<'_, App>, verify_id: String) -> AppResult<PhraseCheckView> {
        blocking(&app, move |a| a.phrase_poll(&verify_id)).await
    }

    #[tauri::command]
    pub async fn phrase_checks(app: State<'_, App>) -> AppResult<Vec<PhraseCheckView>> {
        blocking(&app, |a| a.phrase_checks()).await
    }

    #[tauri::command]
    pub async fn phrase_cancel(app: State<'_, App>, verify_id: String) -> AppResult<()> {
        blocking(&app, move |a| a.phrase_cancel(&verify_id)).await
    }

    // --- security keys on the account (§4) -------------------------------------

    #[tauri::command]
    pub async fn security_keys(app: State<'_, App>) -> AppResult<SecurityKeyList> {
        blocking(&app, |a| a.security_keys()).await
    }

    /// Opens the server's page for adding a key in the system browser.
    #[tauri::command]
    pub async fn security_key_add(handle: AppHandle, app: State<'_, App>) -> AppResult<()> {
        blocking(&app, move |a| {
            let url = a.security_key_add_page()?;
            open_page(&handle, &a, &url)
        })
        .await
    }

    #[tauri::command]
    pub async fn security_key_remove(
        app: State<'_, App>,
        id: String,
        gate: Option<GateInput>,
    ) -> AppResult<SecurityKeyList> {
        blocking(&app, move |a| {
            a.security_key_remove(&id, gate.unwrap_or_default())
        })
        .await
    }

    /// Opens the page where a waiting sign-in's key is used.
    #[tauri::command]
    pub async fn second_factor_open(
        handle: AppHandle,
        app: State<'_, App>,
        server: String,
    ) -> AppResult<()> {
        blocking(&app, move |a| {
            let url = a.second_factor_page()?;
            open_on(&handle, &url, &server)
        })
        .await
    }

    #[tauri::command]
    pub async fn second_factor_wait(app: State<'_, App>) -> AppResult<Option<Vec<String>>> {
        blocking(&app, |a| a.second_factor_wait()).await
    }

    // --- adding a device by approval (§2–§3) ----------------------------------

    #[tauri::command]
    pub async fn link_offer(app: State<'_, App>) -> AppResult<LinkView> {
        blocking(&app, |a| a.link_offer()).await
    }

    #[tauri::command]
    pub async fn link_poll(app: State<'_, App>) -> AppResult<LinkView> {
        blocking(&app, |a| a.link_poll()).await
    }

    #[tauri::command]
    pub async fn link_approve(
        app: State<'_, App>,
        gate: Option<GateInput>,
    ) -> AppResult<Vec<String>> {
        blocking(&app, move |a| a.link_approve(gate.unwrap_or_default())).await
    }

    #[tauri::command]
    pub async fn link_decline(app: State<'_, App>) -> AppResult<()> {
        blocking(&app, |a| a.link_decline()).await
    }

    #[tauri::command]
    pub async fn secret_key_status(app: State<'_, App>) -> AppResult<SecretKeyView> {
        blocking(&app, |a| a.secret_key_status()).await
    }

    #[tauri::command]
    pub async fn join_start(
        app: State<'_, App>,
        server: String,
        invite: String,
    ) -> AppResult<JoinView> {
        let invite = Zeroizing::new(invite);
        blocking(&app, move |a| a.join_start(&server, &invite)).await
    }

    #[tauri::command]
    pub async fn join_poll(app: State<'_, App>) -> AppResult<JoinView> {
        blocking(&app, |a| a.join_poll()).await
    }

    #[tauri::command]
    pub async fn join_finish(
        app: State<'_, App>,
        login: String,
        password: String,
    ) -> AppResult<Status> {
        let password = Zeroizing::new(password);
        blocking(&app, move |a| a.join_finish(&login, &password)).await
    }

    #[tauri::command]
    pub async fn join_cancel(app: State<'_, App>) -> AppResult<()> {
        blocking(&app, |a| a.join_cancel()).await
    }

    /// Makes the account without a Secret Key; the answer is shaped as
    /// `account_register`'s (lines as JSON, a NUL, the kit page).
    #[tauri::command]
    pub async fn account_register_opted_out(
        app: State<'_, App>,
        server: String,
        login: String,
        password: String,
        understood: bool,
    ) -> AppResult<Response> {
        let password = Zeroizing::new(password);
        let registered = blocking(&app, move |a| {
            a.account_register_opted_out(&server, &login, &password, understood)
        })
        .await?;
        let mut body =
            serde_json::to_vec(&registered.lines).map_err(|e| AppError::refused(e.to_string()))?;
        body.push(0);
        if let Some(kit) = registered.kit {
            body.extend_from_slice(kit.expose());
        }
        Ok(Response::new(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_account_servers_own_pages_open() {
        let server = "https://vault.example.com";
        server_page("https://vault.example.com/vault/#/security-key/1", server).unwrap();
        server_page(
            "https://vault.example.com/vault/#/x",
            "https://vault.example.com/",
        )
        .unwrap();
        for bad in [
            "https://vault.example.com.evil.example/vault/",
            "https://evil.example/vault/#/security-key/1",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://vault.example.comx/",
        ] {
            assert!(server_page(bad, server).is_err(), "{bad}");
        }
        assert!(server_page("https://vault.example.com/", "").is_err());
    }
}
