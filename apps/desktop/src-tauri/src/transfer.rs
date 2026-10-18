//! Import, export and backups as the window offers them
//! (docs/plan/14-import-export.md). The save and folder dialogs are the
//! operating system's own, opened from here (the dialog plugin is used from
//! Rust only; the webview gets no dialog or `fs` permission), so the path a
//! file is written to is one the person picked in a native dialog and the
//! core writes it; the export's bytes never cross to the webview.
//!
//! Without the `shell` feature this module is the naming rules, tested
//! without Tauri.

/// The extension an export in `format` is saved with.
pub fn extension_for(format: &str, sealed: bool) -> &'static str {
    match (format, sealed) {
        ("lockdown", true) => "lockdown",
        ("lockdown", false) => "lockdown.json",
        ("cxf", _) => "cxf.json",
        ("csv", _) => "csv",
        ("kdbx", _) => "kdbx",
        _ => "bin",
    }
}

/// The name the save dialog suggests: `lockdown-export-2026-10-18.lockdown`.
pub fn suggested_name(format: &str, sealed: bool, date: &str) -> String {
    format!("lockdown-export-{date}.{}", extension_for(format, sealed))
}

/// The dialog's filter for `format`: a name and the extensions.
pub fn filter_for(format: &str, sealed: bool) -> (&'static str, &'static [&'static str]) {
    match (format, sealed) {
        ("lockdown", true) => ("Lockdown backup", &["lockdown"]),
        ("lockdown", false) => ("Lockdown export (plain text)", &["json"]),
        ("cxf", _) => ("CXF file (plain text)", &["json"]),
        ("csv", _) => ("CSV file (plain text)", &["csv"]),
        ("kdbx", _) => ("KeePass database", &["kdbx"]),
        _ => ("File", &["*"]),
    }
}

/// Today's date as `YYYY-MM-DD` (UTC), for a suggested file name.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    lockdown_cxf_date(secs)
}

fn lockdown_cxf_date(secs: i64) -> String {
    // Howard Hinnant's civil-from-days, as the importers use.
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Places the person chose in a native dialog, held on the Rust side
/// under one-time numbers so the webview can name a place but never make
/// one up.
#[derive(Default)]
pub struct Picked {
    inner: std::sync::Mutex<(u64, std::collections::BTreeMap<u64, std::path::PathBuf>)>,
}

/// A chosen place, as the webview sees it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Chosen {
    /// The number to hand back.
    pub token: u64,
    /// The path, to show.
    pub path: String,
    /// The file or folder name, to show.
    pub name: String,
}

impl Picked {
    /// Holds `path` and answers with its number.
    pub fn hold(&self, path: std::path::PathBuf) -> Chosen {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.0 += 1;
        let token = g.0;
        // Only the newest few are kept: a place is chosen to be used.
        while g.1.len() >= 4 {
            let first = *g.1.keys().next().unwrap_or(&0);
            g.1.remove(&first);
        }
        let chosen = Chosen {
            token,
            path: path.display().to_string(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
        };
        g.1.insert(token, path);
        chosen
    }

    /// The place held under `token`.
    pub fn get(&self, token: u64) -> Option<std::path::PathBuf> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.1.get(&token).cloned()
    }

    /// Lets go of `token`'s place.
    pub fn take(&self, token: u64) -> Option<std::path::PathBuf> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.1.remove(&token)
    }
}

#[cfg(feature = "shell")]
pub(crate) mod commands {
    use lockdown_app::{
        App, AppError, AppResult, BackupStatus, ErrorCode, ExportDone, ExportPreview, GateInput,
        ImportFormatView, ImportPreview, ImportReport,
    };
    use tauri::{AppHandle, State};
    use tauri_plugin_dialog::DialogExt;

    use super::{Chosen, Picked, filter_for, suggested_name, today};

    fn failed(message: impl Into<String>) -> AppError {
        AppError {
            code: ErrorCode::Refused,
            message: message.into(),
            detail: None,
        }
    }

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

    /// The formats the import wizard lists.
    #[tauri::command]
    pub(crate) async fn import_formats(app: State<'_, App>) -> AppResult<Vec<ImportFormatView>> {
        Ok(app.import_formats())
    }

    /// The format a file looks like (its first bytes and its name).
    #[tauri::command]
    pub(crate) async fn import_detect(
        app: State<'_, App>,
        bytes: Vec<u8>,
        file_name: String,
    ) -> AppResult<Option<String>> {
        let bytes = zeroize::Zeroizing::new(bytes);
        Ok(app.import_detect(&bytes, &file_name))
    }

    /// Parses the file the person picked (the webview's own file input,
    /// read with the File API) and answers with counts and words only.
    #[tauri::command]
    pub(crate) async fn import_preview(
        app: State<'_, App>,
        bytes: Vec<u8>,
        format: String,
        file_name: String,
        password: Option<Vec<u8>>,
        vault: Option<String>,
    ) -> AppResult<ImportPreview> {
        blocking(&app, move |a| {
            a.import_preview(bytes, &format, &file_name, password, vault)
        })
        .await
    }

    /// Keeps the previewed import.
    #[tauri::command]
    pub(crate) async fn import_commit(app: State<'_, App>, token: u64) -> AppResult<ImportReport> {
        blocking(&app, move |a| a.import_commit(token)).await
    }

    /// Drops the previewed import.
    #[tauri::command]
    pub(crate) async fn import_cancel(app: State<'_, App>) -> AppResult<()> {
        blocking(&app, |a| a.import_cancel()).await
    }

    /// What an export would carry and leave out.
    #[tauri::command]
    pub(crate) async fn export_preview(
        app: State<'_, App>,
        format: String,
        vault: Option<String>,
    ) -> AppResult<ExportPreview> {
        blocking(&app, move |a| a.export_preview(&format, vault)).await
    }

    /// Asks where to save, in the OS's own dialog. The path stays here,
    /// under a one-time number; the webview gets the number and the file's
    /// name to show, and hands the number to `export_save`. `None` when
    /// the person closed the dialog.
    #[tauri::command]
    pub(crate) async fn export_choose(
        handle: AppHandle,
        picked: State<'_, Picked>,
        format: String,
        sealed: bool,
    ) -> AppResult<Option<Chosen>> {
        let (name, exts) = filter_for(&format, sealed);
        let suggested = suggested_name(&format, sealed, &today());
        let path = tauri::async_runtime::spawn_blocking(move || {
            handle
                .dialog()
                .file()
                .set_title("Save the export")
                .set_file_name(suggested)
                .add_filter(name, exts)
                .blocking_save_file()
        })
        .await
        .map_err(|e| failed(format!("the save dialog didn't open: {e}")))?;
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| {
            failed("that place can't be written from here; pick a folder on this computer")
        })?;
        Ok(Some(picked.hold(path)))
    }

    /// Has the core write the export to the place chosen under `target`,
    /// after the `Reveal` gate. A plain file needs `plain_confirmed`. The
    /// chosen place stays held until the write succeeds, so the gate can
    /// ask and call again.
    // Two of the eight are Tauri's injected states; the six the webview
    // sends are the place, the format, the password, the yes, the vault, the gate.
    #[allow(clippy::too_many_arguments)]
    #[tauri::command]
    pub(crate) async fn export_save(
        app: State<'_, App>,
        picked: State<'_, Picked>,
        target: u64,
        format: String,
        password: Option<Vec<u8>>,
        plain_confirmed: bool,
        vault: Option<String>,
        gate: Option<GateInput>,
    ) -> AppResult<ExportDone> {
        let gate = gate.unwrap_or_default();
        let path = picked
            .get(target)
            .ok_or_else(|| failed("choose where to save first"))?;
        let done = blocking(&app, move |a| {
            a.export_to(path, &format, password, plain_confirmed, vault, gate)
        })
        .await?;
        picked.take(target);
        Ok(done)
    }

    /// Turns a crown jewel's "include in exports and backups" on (behind
    /// the `Reveal` gate) or off.
    #[tauri::command]
    pub(crate) async fn set_export_opt_in(
        app: State<'_, App>,
        id: String,
        on: bool,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.set_export_opt_in(&id, on, gate)).await
    }

    /// Automatic backups as they stand.
    #[tauri::command]
    pub(crate) async fn backup_status(app: State<'_, App>) -> AppResult<BackupStatus> {
        blocking(&app, |a| a.backup_status()).await
    }

    /// The OS's folder picker. The folder stays here under a one-time
    /// number; the webview gets the number and the path to show (a path is
    /// not a secret), and hands the number to `backup_setup`.
    #[tauri::command]
    pub(crate) async fn backup_pick_folder(
        handle: AppHandle,
        picked: State<'_, Picked>,
    ) -> AppResult<Option<Chosen>> {
        let path = tauri::async_runtime::spawn_blocking(move || {
            handle
                .dialog()
                .file()
                .set_title("Choose a folder for backups")
                .blocking_pick_folder()
        })
        .await
        .map_err(|e| failed(format!("the folder dialog didn't open: {e}")))?;
        match path {
            None => Ok(None),
            Some(p) => {
                let p = p
                    .into_path()
                    .map_err(|_| failed("pick a folder on this computer"))?;
                Ok(Some(picked.hold(p)))
            }
        }
    }

    /// Turns backups on or changes them, behind the `Reveal` gate: the
    /// folder chosen under `folder` (or the one in use when `None`), how
    /// often, how many to keep, and the backup password the first time.
    #[tauri::command]
    pub(crate) async fn backup_setup(
        app: State<'_, App>,
        picked: State<'_, Picked>,
        folder: Option<u64>,
        password: Option<Vec<u8>>,
        interval_hours: u32,
        keep_last: u32,
        gate: Option<GateInput>,
    ) -> AppResult<BackupStatus> {
        let gate = gate.unwrap_or_default();
        let dir = match folder {
            Some(t) => picked
                .get(t)
                .map(|p| p.display().to_string())
                .ok_or_else(|| failed("choose the folder again"))?,
            None => blocking(&app, |a| a.settings())
                .await?
                .backup
                .folder
                .ok_or_else(|| failed("choose a folder for backups first"))?,
        };
        let status = blocking(&app, move |a| {
            a.backup_setup(dir, password, interval_hours, keep_last, gate)
        })
        .await?;
        if let Some(t) = folder {
            picked.take(t);
        }
        Ok(status)
    }

    /// Writes a backup now.
    #[tauri::command]
    pub(crate) async fn backup_now(app: State<'_, App>) -> AppResult<BackupStatus> {
        blocking(&app, |a| a.backup_now()).await
    }

    /// Turns scheduled backups off; `forget` also drops the backup password here.
    #[tauri::command]
    pub(crate) async fn backup_off(app: State<'_, App>, forget: bool) -> AppResult<BackupStatus> {
        blocking(&app, move |a| a.backup_off(forget)).await
    }

    /// Opens a backup from the folder for the import preview.
    #[tauri::command]
    pub(crate) async fn backup_restore_preview(
        app: State<'_, App>,
        name: String,
        password: Option<Vec<u8>>,
    ) -> AppResult<ImportPreview> {
        blocking(&app, move |a| a.backup_restore_preview(&name, password)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_say_whether_the_file_is_sealed() {
        assert_eq!(
            suggested_name("lockdown", true, "2026-10-18"),
            "lockdown-export-2026-10-18.lockdown"
        );
        assert_eq!(
            suggested_name("lockdown", false, "2026-10-18"),
            "lockdown-export-2026-10-18.lockdown.json"
        );
        assert_eq!(extension_for("kdbx", true), "kdbx");
        assert_eq!(filter_for("csv", false).0, "CSV file (plain text)");
        assert_eq!(lockdown_cxf_date(0), "1970-01-01");
        assert_eq!(lockdown_cxf_date(1_792_281_600), "2026-10-18");
        assert_eq!(today().len(), 10);
    }

    #[test]
    fn a_chosen_place_is_named_by_number_and_used_once() {
        let p = Picked::default();
        let a = p.hold("/home/ada/export.lockdown".into());
        assert_eq!(a.name, "export.lockdown");
        assert_eq!(p.get(a.token).unwrap().display().to_string(), a.path);
        assert!(
            p.get(a.token + 1).is_none(),
            "a number never handed out names nothing"
        );
        assert!(p.take(a.token).is_some());
        assert!(p.get(a.token).is_none());
        for i in 0..10 {
            p.hold(format!("/tmp/{i}").into());
        }
        assert!(p.inner.lock().unwrap().1.len() <= 4);
    }
}
