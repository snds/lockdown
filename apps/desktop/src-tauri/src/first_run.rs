//! The first run, the master password and finding a server, as Tauri
//! commands (docs/plan/39-first-run.md §1.2–§1.11). Each is a thin call
//! into `lockdown_app`; a kit page crosses as the once-page body
//! `kit_create` uses (the notes as JSON, a NUL, the page's bytes) and is
//! never a JSON string.

use serde::Serialize;

/// Whether unlocking with Touch ID or Windows Hello is on offer in this
/// build, and in what words (Q1: not on the ad-hoc signed build).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct QuickUnlock {
    /// `touch_id`, `windows_hello`, or `none`.
    pub kind: &'static str,
    /// Whether it can be turned on.
    pub available: bool,
    /// Why not, when it can't.
    pub why: Option<&'static str>,
}

/// This build's answer: the key Touch ID would release needs a keychain
/// item bound to the fingerprint, which only the signed Lockdown app can
/// make (R4), so no build offers it yet.
pub fn quick_unlock() -> QuickUnlock {
    if cfg!(target_os = "macos") {
        QuickUnlock {
            kind: "touch_id",
            available: false,
            why: Some("Touch ID unlock needs the signed Lockdown app."),
        }
    } else if cfg!(target_os = "windows") {
        QuickUnlock {
            kind: "windows_hello",
            available: false,
            why: Some("Windows Hello unlock needs the signed Lockdown app."),
        }
    } else {
        QuickUnlock {
            kind: "none",
            available: false,
            why: None,
        }
    }
}

/// The once-page body: the lines as JSON, a NUL, then the page.
pub fn once_page(lines: &[String], page: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut body = serde_json::to_vec(lines).map_err(|e| e.to_string())?;
    body.push(0);
    if let Some(page) = page {
        body.extend_from_slice(page);
    }
    Ok(body)
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::discovery::Candidate;
    use lockdown_app::{App, AppError, AppResult, MasterPasswordInput, Status};
    use serde::Deserialize;
    use tauri::State;
    use tauri::ipc::Response;
    use zeroize::Zeroizing;

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

    fn page(registered: lockdown_app::Registered) -> AppResult<Response> {
        let body = super::once_page(
            &registered.lines,
            registered.kit.as_ref().map(|k| k.expose()),
        )
        .map_err(AppError::refused)?;
        Ok(Response::new(body))
    }

    #[tauri::command]
    pub async fn first_run_create(
        app: State<'_, App>,
        name: String,
        password: String,
    ) -> AppResult<Response> {
        let password = Zeroizing::new(password);
        let made = blocking(&app, move |a| a.first_run_create(&name, &password)).await?;
        page(made)
    }

    #[tauri::command]
    pub async fn unlock_with_password(app: State<'_, App>, password: String) -> AppResult<Status> {
        let password = Zeroizing::new(password);
        blocking(&app, move |a| a.unlock_with_password(&password)).await
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MasterPasswordArgs {
        name: Option<String>,
        password: String,
        secret_key: Option<String>,
        recovery_code: Option<String>,
        #[serde(default)]
        new_kit: bool,
    }

    #[tauri::command]
    pub async fn set_master_password(
        app: State<'_, App>,
        input: MasterPasswordArgs,
    ) -> AppResult<Response> {
        let input = MasterPasswordInput {
            name: input.name,
            password: input.password,
            secret_key: input.secret_key,
            recovery_code: input.recovery_code,
            new_kit: input.new_kit,
        };
        let done = blocking(&app, move |a| a.set_master_password(input)).await?;
        page(done)
    }

    #[tauri::command]
    pub async fn password_reset_with_kit(
        app: State<'_, App>,
        secret_key: String,
        recovery_code: String,
        password: String,
    ) -> AppResult<Status> {
        let (secret_key, recovery_code, password) = (
            Zeroizing::new(secret_key),
            Zeroizing::new(recovery_code),
            Zeroizing::new(password),
        );
        blocking(&app, move |a| {
            a.password_reset_with_kit(&secret_key, &recovery_code, &password)
        })
        .await
    }

    #[tauri::command]
    pub async fn onboarding_stage(app: State<'_, App>, stage: String) -> AppResult<()> {
        blocking(&app, move |a| a.onboarding_stage(&stage)).await
    }

    #[tauri::command]
    pub async fn set_name(app: State<'_, App>, name: String) -> AppResult<String> {
        blocking(&app, move |a| a.set_name(&name)).await
    }

    #[tauri::command]
    pub async fn check_master_password(
        app: State<'_, App>,
        password: String,
        name: Option<String>,
    ) -> AppResult<()> {
        let password = Zeroizing::new(password);
        blocking(&app, move |a| {
            a.check_master_password(&password, name.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub fn quick_unlock_status() -> super::QuickUnlock {
        super::quick_unlock()
    }

    #[tauri::command]
    pub async fn discovery_candidates(
        app: State<'_, App>,
        network: bool,
    ) -> AppResult<Vec<Candidate>> {
        blocking(&app, move |a| a.discovery_candidates(network)).await
    }

    #[tauri::command]
    pub async fn discovery_check(app: State<'_, App>, address: String) -> AppResult<Candidate> {
        blocking(&app, move |a| a.discovery_check(&address)).await
    }

    #[tauri::command]
    pub async fn discovery_domain(app: State<'_, App>, login: String) -> AppResult<Vec<Candidate>> {
        blocking(&app, move |a| a.discovery_domain(&login)).await
    }

    #[tauri::command]
    pub async fn discovery_confirm(app: State<'_, App>, address: String) -> AppResult<Candidate> {
        blocking(&app, move |a| a.discovery_confirm(&address)).await
    }

    #[tauri::command]
    pub async fn discovery_forget(app: State<'_, App>, address: String) -> AppResult<()> {
        blocking(&app, move |a| a.discovery_forget(&address)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_once_page_body_is_lines_then_nul_then_the_page() {
        let body = once_page(&["a".into()], Some(b"KIT")).unwrap();
        assert_eq!(body, b"[\"a\"]\0KIT");
        assert_eq!(once_page(&[], None).unwrap(), b"[]\0");
    }

    #[test]
    fn quick_unlock_is_not_offered_by_this_build() {
        let q = quick_unlock();
        assert!(!q.available);
        if q.kind == "touch_id" {
            assert_eq!(
                q.why,
                Some("Touch ID unlock needs the signed Lockdown app.")
            );
        }
    }
}
