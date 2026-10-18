//! The webview's commands for vault depth (docs/plan/21-vault-depth.md):
//! the person's own templates, link targets, the generator's preview and a
//! site's rules, the Wi-Fi guest card, sign-ins logged on someone else's
//! screen, the account's attachment room, and files attached from disk and
//! saved to disk a chunk at a time.
//!
//! Files from disk: the shell's own dialog picks the file (or the place to
//! save one), and the path stays in the shell under a number (`Picked`, as
//! export's does), so the webview never holds a file's bytes and the core
//! reads and writes the disk itself. The Wi-Fi card's code is a secret (the
//! network's password is in it) and comes back as a raw response, like a
//! reveal. The checks are outside the `shell` feature so they are
//! unit-tested without Tauri.

use lockdown_app::{AppError, AppResult};

/// The longest a sign-in elsewhere waits to remind: thirty days.
pub const MAX_REMIND_HOURS: u32 = 30 * 24;

/// Seconds until a sign-in elsewhere reminds, from the hours the person
/// picked (1 hour to 30 days).
pub fn remind_in_secs(hours: u32) -> AppResult<i64> {
    if !(1..=MAX_REMIND_HOURS).contains(&hours) {
        return Err(AppError::refused(format!(
            "a reminder is 1 hour to {} days away",
            MAX_REMIND_HOURS / 24
        )));
    }
    Ok(i64::from(hours) * 3600)
}

/// A name for the save dialog from an attachment's own: its last path
/// part, with nothing that could climb out of the folder the person picks.
pub fn save_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.');
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    if clean.is_empty() {
        "attachment".into()
    } else {
        clean
    }
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::{
        App, AppError, AppResult, AwayReminder, GateInput, GeneratorPreview, LinkTargetView,
        Recipe, SiteRuleView, StorageView, UserTemplateInput, UserTemplateView,
    };
    use tauri::ipc::Response;
    use tauri::{AppHandle, State};
    use tauri_plugin_dialog::DialogExt;

    use crate::transfer::{Chosen, Picked};

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
    pub async fn user_templates(app: State<'_, App>) -> AppResult<Vec<UserTemplateView>> {
        blocking(&app, |a| a.user_templates()).await
    }

    #[tauri::command]
    pub async fn save_user_template(
        app: State<'_, App>,
        vault: Option<String>,
        id: Option<String>,
        template: UserTemplateInput,
    ) -> AppResult<String> {
        blocking(&app, move |a| {
            a.save_user_template(vault.as_deref(), id.as_deref(), template)
        })
        .await
    }

    #[tauri::command]
    pub async fn delete_user_template(app: State<'_, App>, id: String) -> AppResult<()> {
        blocking(&app, move |a| a.delete_user_template(&id)).await
    }

    #[tauri::command]
    pub async fn link_targets(
        app: State<'_, App>,
        vault_id: String,
        except: Option<String>,
    ) -> AppResult<Vec<LinkTargetView>> {
        blocking(&app, move |a| a.link_targets(&vault_id, except.as_deref())).await
    }

    #[tauri::command]
    pub async fn generator_preview(
        app: State<'_, App>,
        recipe: Recipe,
        url: Option<String>,
        rule: Option<String>,
    ) -> AppResult<GeneratorPreview> {
        blocking(&app, move |a| {
            a.generator_preview(&recipe, url.as_deref(), rule.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub async fn site_rule(app: State<'_, App>, url: String) -> AppResult<Option<SiteRuleView>> {
        blocking(&app, move |a| Ok(a.site_rule(&url))).await
    }

    /// The Wi-Fi card's QR grid, raw: a secret, through the `Reveal` gate
    /// on a crown jewel.
    #[tauri::command]
    pub async fn wifi_qr(
        app: State<'_, App>,
        id: String,
        gate: Option<GateInput>,
    ) -> AppResult<Response> {
        let gate = gate.unwrap_or_default();
        let grid = blocking(&app, move |a| a.wifi_qr(&id, gate)).await?;
        Ok(Response::new(grid.into_bytes().to_vec()))
    }

    #[tauri::command]
    pub async fn log_away(
        app: State<'_, App>,
        id: String,
        place: String,
        remind_in_hours: u32,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let secs = super::remind_in_secs(remind_in_hours)?;
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.log_away(&id, &place, secs, gate)).await
    }

    #[tauri::command]
    pub async fn clear_away(
        app: State<'_, App>,
        id: String,
        at: i64,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.clear_away(&id, at, gate)).await
    }

    #[tauri::command]
    pub async fn away_reminders(app: State<'_, App>) -> AppResult<Vec<AwayReminder>> {
        blocking(&app, |a| a.away_reminders()).await
    }

    #[tauri::command]
    pub async fn storage_usage(app: State<'_, App>) -> AppResult<Option<StorageView>> {
        blocking(&app, |a| a.storage_usage()).await
    }

    /// The OS's open dialog for a file to attach; the path stays here.
    #[tauri::command]
    pub async fn attach_choose(
        handle: AppHandle,
        picked: State<'_, Picked>,
    ) -> AppResult<Option<Chosen>> {
        let path = tauri::async_runtime::spawn_blocking(move || {
            handle
                .dialog()
                .file()
                .set_title("Attach a file")
                .blocking_pick_file()
        })
        .await
        .map_err(|e| AppError::refused(format!("the file dialog didn't open: {e}")))?;
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| {
            AppError::refused("that file can't be read from here; pick one on this computer")
        })?;
        Ok(Some(picked.hold(path)))
    }

    /// Attaches the file chosen under `token`, read from the disk a chunk
    /// at a time. To a crown jewel, through the `Use` gate.
    #[tauri::command]
    pub async fn attach_chosen(
        app: State<'_, App>,
        picked: State<'_, Picked>,
        id: String,
        token: u64,
        gate: Option<GateInput>,
    ) -> AppResult<String> {
        let path = picked
            .get(token)
            .ok_or_else(|| AppError::refused("pick the file again"))?;
        let gate = gate.unwrap_or_default();
        let att = blocking(&app, move |a| a.attach_file(&id, path, None, gate)).await?;
        picked.take(token);
        Ok(att)
    }

    /// The OS's save dialog for an attached file, named after it.
    #[tauri::command]
    pub async fn attachment_save_choose(
        handle: AppHandle,
        picked: State<'_, Picked>,
        name: String,
    ) -> AppResult<Option<Chosen>> {
        let suggested = super::save_name(&name);
        let path = tauri::async_runtime::spawn_blocking(move || {
            handle
                .dialog()
                .file()
                .set_title("Save the file")
                .set_file_name(suggested)
                .blocking_save_file()
        })
        .await
        .map_err(|e| AppError::refused(format!("the save dialog didn't open: {e}")))?;
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| {
            AppError::refused(
                "that place can't be written from here; pick a folder on this computer",
            )
        })?;
        Ok(Some(picked.hold(path)))
    }

    /// Writes an attached file to the place chosen under `token`, a chunk at
    /// a time. A crown jewel's goes through the `Reveal` gate.
    #[tauri::command]
    pub async fn attachment_save(
        app: State<'_, App>,
        picked: State<'_, Picked>,
        id: String,
        att_id: String,
        token: u64,
        gate: Option<GateInput>,
    ) -> AppResult<()> {
        let path = picked
            .get(token)
            .ok_or_else(|| AppError::refused("choose where to save it again"))?;
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.save_attachment(&id, &att_id, path, gate)).await?;
        picked.take(token);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reminder_is_an_hour_to_a_month_away() {
        assert_eq!(remind_in_secs(1).unwrap(), 3600);
        assert_eq!(remind_in_secs(48).unwrap(), 48 * 3600);
        assert!(remind_in_secs(0).is_err());
        assert!(remind_in_secs(MAX_REMIND_HOURS + 1).is_err());
    }

    #[test]
    fn a_saved_file_keeps_its_own_name_and_nothing_else() {
        assert_eq!(save_name("lease.pdf"), "lease.pdf");
        assert_eq!(save_name("../../etc/passwd"), "passwd");
        assert_eq!(save_name("C:\\Users\\me\\scan.tiff"), "scan.tiff");
        assert_eq!(save_name(".hidden"), "hidden");
        assert_eq!(save_name("a\u{0}b"), "ab");
        assert_eq!(save_name("  "), "attachment");
        assert_eq!(save_name("dir/"), "attachment");
    }
}
