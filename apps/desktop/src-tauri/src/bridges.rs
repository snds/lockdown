//! The webview's commands for bridges, imports and monitoring
//! (docs/plan/23-bridges.md): the clock check, the federated sign-in map,
//! fill fixes, aliases and alias monitoring, the guided incident flows,
//! set-up cards, the emergency sheet and exports locked to age keys. Each
//! is one call into `lockdown-app` on a worker thread; the gated ones take
//! a `gate` like `copy_secret` does. Nothing here carries a key: an alias
//! service's key goes in once as bytes and never comes back out.
//!
//! The checks that don't need Tauri sit outside the `shell` feature so
//! they are unit-tested without it.

use lockdown_app::{AppError, AppResult};

/// The most recipients the webview may send for one export (the core caps
/// it again).
pub const MAX_RECIPIENTS: usize = 20;

/// Trims, drops blank lines and caps the recipients the webview sent.
pub fn recipients_from(lines: Vec<String>) -> AppResult<Vec<String>> {
    let out: Vec<String> = lines
        .into_iter()
        .flat_map(|l| {
            l.split([',', '\n', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    if out.is_empty() {
        return Err(AppError::refused(
            "name at least one age public key to lock the file to",
        ));
    }
    if out.len() > MAX_RECIPIENTS {
        return Err(AppError::refused(format!(
            "a file can be locked to at most {MAX_RECIPIENTS} keys"
        )));
    }
    Ok(out)
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::bridges::aliases::{AliasMade, AliasOverview, MonitoringView};
    use lockdown_app::bridges::clock::ClockView;
    use lockdown_app::bridges::emergency::EmergencySheet;
    use lockdown_app::bridges::federation::FederationMap;
    use lockdown_app::bridges::fixes::FillFix;
    use lockdown_app::bridges::guides::GuideSources;
    use lockdown_app::bridges::incidents::{IncidentSummary, IncidentView};
    use lockdown_app::{App, AppError, AppResult, ExportDone, GateInput};
    use lockdown_security::guide::SetupGuide;
    use tauri::State;

    use crate::transfer::Picked;

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
    pub async fn clock_status(app: State<'_, App>) -> AppResult<Option<ClockView>> {
        blocking(&app, |a| a.clock_status()).await
    }

    #[tauri::command]
    pub async fn clock_check(app: State<'_, App>) -> AppResult<ClockView> {
        blocking(&app, |a| a.clock_check()).await
    }

    #[tauri::command]
    pub async fn federation_map(app: State<'_, App>) -> AppResult<FederationMap> {
        blocking(&app, |a| a.federation_map()).await
    }

    #[tauri::command]
    pub async fn federation_accept(
        app: State<'_, App>,
        pending: u64,
        item_id: Option<String>,
    ) -> AppResult<String> {
        blocking(&app, move |a| {
            a.federation_accept(pending, item_id.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub async fn federation_dismiss(app: State<'_, App>, pending: u64) -> AppResult<()> {
        blocking(&app, move |a| a.federation_dismiss(pending)).await
    }

    #[tauri::command]
    pub async fn federation_set_account(
        app: State<'_, App>,
        provider: String,
        item_id: String,
    ) -> AppResult<()> {
        blocking(&app, move |a| a.federation_set_account(&provider, &item_id)).await
    }

    #[tauri::command]
    pub async fn site_fix(app: State<'_, App>, item_id: String) -> AppResult<FillFix> {
        blocking(&app, move |a| a.site_fix(&item_id)).await
    }

    #[tauri::command]
    pub async fn site_fix_set(
        app: State<'_, App>,
        item_id: String,
        fix: FillFix,
    ) -> AppResult<FillFix> {
        blocking(&app, move |a| a.site_fix_set(&item_id, fix)).await
    }

    #[tauri::command]
    pub async fn alias_overview(app: State<'_, App>) -> AppResult<AliasOverview> {
        blocking(&app, |a| a.alias_overview()).await
    }

    /// The key arrives as bytes (never a JSON string) and goes straight
    /// into a new crown-jewel item.
    #[tauri::command]
    pub async fn alias_service_add(
        app: State<'_, App>,
        service: String,
        key: Vec<u8>,
        base: Option<String>,
        domain: Option<String>,
    ) -> AppResult<String> {
        let key = zeroize::Zeroizing::new(key);
        blocking(&app, move |a| {
            a.alias_service_add(&service, key.to_vec(), base.as_deref(), domain.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub async fn alias_generate(
        app: State<'_, App>,
        item_id: String,
        site: String,
        gate: Option<GateInput>,
    ) -> AppResult<AliasMade> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.alias_generate(&item_id, &site, gate)).await
    }

    #[tauri::command]
    pub async fn alias_monitoring(app: State<'_, App>) -> AppResult<MonitoringView> {
        blocking(&app, |a| a.alias_monitoring()).await
    }

    #[tauri::command]
    pub async fn alias_monitoring_set(app: State<'_, App>, on: bool) -> AppResult<MonitoringView> {
        blocking(&app, move |a| a.alias_monitoring_set(on)).await
    }

    #[tauri::command]
    pub async fn incidents(app: State<'_, App>) -> AppResult<Vec<IncidentSummary>> {
        blocking(&app, |a| a.incidents()).await
    }

    #[tauri::command]
    pub async fn incident(
        app: State<'_, App>,
        id: String,
        pick: Option<String>,
    ) -> AppResult<IncidentView> {
        blocking(&app, move |a| a.incident(&id, pick.as_deref())).await
    }

    #[tauri::command]
    pub async fn incident_mark(
        app: State<'_, App>,
        id: String,
        step: String,
        item_id: Option<String>,
        done: bool,
    ) -> AppResult<IncidentView> {
        blocking(&app, move |a| {
            a.incident_mark(&id, &step, item_id.as_deref(), done)
        })
        .await
    }

    #[tauri::command]
    pub async fn incident_reset(app: State<'_, App>, id: String) -> AppResult<IncidentView> {
        blocking(&app, move |a| a.incident_reset(&id)).await
    }

    #[tauri::command]
    pub async fn setup_guide(
        app: State<'_, App>,
        item_id: String,
    ) -> AppResult<Option<SetupGuide>> {
        blocking(&app, move |a| a.setup_guide(&item_id)).await
    }

    #[tauri::command]
    pub async fn guide_sources(app: State<'_, App>) -> AppResult<GuideSources> {
        Ok(app.guide_sources())
    }

    #[tauri::command]
    pub async fn emergency_sheet(app: State<'_, App>) -> AppResult<EmergencySheet> {
        blocking(&app, |a| a.emergency_sheet()).await
    }

    /// Writes the export locked to `recipients` to the place chosen under
    /// `target` (from `export_choose` with the `age` format), after the
    /// `Reveal` gate.
    #[tauri::command]
    pub async fn export_save_age(
        app: State<'_, App>,
        picked: State<'_, Picked>,
        target: u64,
        recipients: Vec<String>,
        vault: Option<String>,
        gate: Option<GateInput>,
    ) -> AppResult<ExportDone> {
        let gate = gate.unwrap_or_default();
        let recipients = super::recipients_from(recipients)?;
        let path = picked
            .get(target)
            .ok_or_else(|| AppError::refused("choose where to save first"))?;
        let done = blocking(&app, move |a| {
            a.export_to_recipients(path, recipients, vault, gate)
        })
        .await?;
        picked.take(target);
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipients_split_on_lines_commas_and_spaces_and_are_capped() {
        assert_eq!(
            recipients_from(vec!["age1a, age1b\n  age1c ".into(), String::new()]).unwrap(),
            ["age1a", "age1b", "age1c"]
        );
        assert!(recipients_from(vec!["  ".into()]).is_err());
        let many: Vec<String> = (0..=MAX_RECIPIENTS).map(|i| format!("age1{i}")).collect();
        assert!(recipients_from(many).is_err());
    }
}
