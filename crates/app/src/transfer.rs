//! The app's import, export and backup calls (docs/plan/14-import-export.md),
//! on the same worker as everything else. Imports are previewed, then
//! kept; exports are written by the core to the path the person chose,
//! after the `Reveal` gate (D101, D103); a crown jewel leaves only when its
//! opt-in is on (D121), and turning that on asks first. Scheduled backups
//! run on the worker's tick while the vault is open.

use std::path::PathBuf;
use std::time::Instant;

use zeroize::Zeroizing;

use crate::session::transfer::{self as session_transfer, write_user_file};
use crate::{
    App, AppError, AppResult, ApprovalKind, BackupStatus, Core, ExportDone, ExportPreview,
    GateInput, ImportFormatView, ImportPreview, ImportReport,
};

impl Core {
    /// Runs a scheduled backup when one is due; quietly, since nobody asked:
    /// the outcome is on the Backups screen. Not activity: it never keeps
    /// the vault awake.
    pub(crate) fn backup_tick(&mut self) {
        let cfg = self.settings.backup.clone();
        if self.confined.is_some() {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.backup_due(&cfg, crate::session::now_secs()) {
            let _ = session.backup_run(&cfg);
        }
    }
}

impl App {
    /// The formats the import wizard lists. Static.
    pub fn import_formats(&self) -> Vec<ImportFormatView> {
        session_transfer::import_formats()
    }

    /// The format a file looks like, from its first bytes and its name.
    pub fn import_detect(&self, bytes: &[u8], name: &str) -> Option<String> {
        session_transfer::detect_format(bytes, name)
    }

    /// Parses a file and says what it holds; nothing is kept until
    /// [`App::import_commit`]. `format` may be `auto`.
    pub fn import_preview(
        &self,
        bytes: Vec<u8>,
        format: &str,
        file_name: &str,
        password: Option<Vec<u8>>,
        vault: Option<String>,
    ) -> AppResult<ImportPreview> {
        let format = format.to_string();
        let file_name = file_name.to_string();
        let bytes = Zeroizing::new(bytes);
        let password = password.map(Zeroizing::new);
        self.with_session(move |s| {
            s.import_preview(
                &bytes,
                &format,
                &file_name,
                password.as_deref().map(|p| p.as_slice()),
                vault.as_deref(),
            )
        })
    }

    /// Keeps the previewed import.
    pub fn import_commit(&self, token: u64) -> AppResult<ImportReport> {
        self.with_session(move |s| s.import_commit(token))
    }

    /// Drops the previewed import.
    pub fn import_cancel(&self) -> AppResult<()> {
        self.with_session(|s| {
            s.import_cancel();
            Ok(())
        })
    }

    /// What an export in `format` would carry and leave out: every vault,
    /// or the one `vault` names.
    pub fn export_preview(&self, format: &str, vault: Option<String>) -> AppResult<ExportPreview> {
        let format = format.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.refuse_confined()?;
            core.session()?
                .export_preview(&format, vault.as_deref())
                .map_err(AppError::refused)
        })
    }

    /// Writes every vault (or the one `vault` names) in `format` to `path` (the person chose it in the
    /// shell's own save dialog), after the `Reveal` gate. With `password`
    /// the file is sealed (the Lockdown format, KDBX); without one a
    /// Lockdown, CXF or CSV file is plain text, and `plain_confirmed`
    /// must say the person agreed to that.
    pub fn export_to(
        &self,
        path: PathBuf,
        format: &str,
        password: Option<Vec<u8>>,
        plain_confirmed: bool,
        vault: Option<String>,
        gate: GateInput,
    ) -> AppResult<ExportDone> {
        let format = format.to_string();
        let password = password.map(Zeroizing::new);
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.refuse_confined()?;
            let sealed = password.as_ref().is_some_and(|p| !p.is_empty())
                && matches!(format.as_str(), "lockdown" | "kdbx");
            if !sealed && !plain_confirmed {
                return Err(AppError::refused(
                    "this file won't be protected by a password; confirm that first",
                ));
            }
            core.approve(ApprovalKind::Reveal, "export the vault", &gate)?;
            let (bytes, preview) = core
                .session()?
                .export_file(
                    &format,
                    password.as_deref().map(|p| p.as_slice()),
                    vault.as_deref(),
                )
                .map_err(AppError::refused)?;
            write_user_file(&path, &bytes).map_err(AppError::refused)?;
            Ok(ExportDone {
                path: path.display().to_string(),
                bytes: bytes.len() as u64,
                preview,
            })
        })
    }

    /// Turns a crown jewel's "include in exports and backups" on or off
    /// (D121). On goes through the `Reveal` gate (the account password
    /// where one exists): it lets the secret leave the vault. Off asks
    /// nothing.
    pub fn set_export_opt_in(&self, id: &str, on: bool, gate: GateInput) -> AppResult<u64> {
        let id = id.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            if on {
                core.approve(
                    ApprovalKind::Reveal,
                    "let this protected item leave in exports and backups",
                    &gate,
                )?;
            }
            core.session()?
                .set_export_opt_in(&id, on)
                .map_err(AppError::refused)
        })
    }

    /// Automatic backups as they stand.
    pub fn backup_status(&self) -> AppResult<BackupStatus> {
        self.call(|core| {
            let cfg = core.settings.backup.clone();
            Ok(core.session()?.backup_status(&cfg))
        })
    }

    /// Turns automatic backups on (or changes them): the folder, how often,
    /// how many to keep, and, the first time or to change it, the backup
    /// password. Behind the `Reveal` gate, since it sends the vault to a
    /// folder. Writes one backup at once so the person sees it work.
    pub fn backup_setup(
        &self,
        folder: String,
        password: Option<Vec<u8>>,
        interval_hours: u32,
        keep_last: u32,
        gate: GateInput,
    ) -> AppResult<BackupStatus> {
        let password = password.map(Zeroizing::new);
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.refuse_confined()?;
            let dir = PathBuf::from(folder.trim());
            if !dir.is_absolute() || !dir.is_dir() {
                return Err(AppError::refused(format!(
                    "{} isn't a folder on this computer",
                    dir.display()
                )));
            }
            core.approve(ApprovalKind::Reveal, "back up the vault to a folder", &gate)?;
            let session = core.session()?;
            if let Some(pw) = password.as_ref().filter(|p| !p.is_empty()) {
                session.backup_set_password(pw).map_err(AppError::refused)?;
            } else if !session.backup_key_set() {
                return Err(AppError::refused(
                    "choose a backup password; you'll need it to restore",
                ));
            }
            let mut settings = core.settings.clone();
            settings.backup.enabled = true;
            settings.backup.folder = Some(dir.display().to_string());
            settings.backup.interval_hours = interval_hours;
            settings.backup.keep_last = keep_last;
            let settings = settings.clamped();
            settings
                .save(&core.paths.settings_file())
                .map_err(AppError::refused)?;
            core.settings = settings;
            let cfg = core.settings.backup.clone();
            let session = core.session()?;
            session.backup_run(&cfg).map_err(AppError::refused)?;
            Ok(session.backup_status(&cfg))
        })
    }

    /// Writes a backup now.
    pub fn backup_now(&self) -> AppResult<BackupStatus> {
        self.call(|core| {
            core.clock.touch(Instant::now());
            core.refuse_confined()?;
            let cfg = core.settings.backup.clone();
            let session = core.session()?;
            session.backup_run(&cfg).map_err(AppError::refused)?;
            Ok(session.backup_status(&cfg))
        })
    }

    /// Turns scheduled backups off; with `forget`, the backup password on
    /// this device goes too. The files in the folder stay.
    pub fn backup_off(&self, forget: bool) -> AppResult<BackupStatus> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let mut settings = core.settings.clone();
            settings.backup.enabled = false;
            settings
                .save(&core.paths.settings_file())
                .map_err(AppError::refused)?;
            core.settings = settings;
            let cfg = core.settings.backup.clone();
            let session = core.session()?;
            if forget {
                session.backup_forget_key();
            }
            Ok(session.backup_status(&cfg))
        })
    }

    /// Opens a backup from the folder for the import preview (then
    /// [`App::import_commit`]): with the key this device keeps when it made
    /// the backup, else with `password`.
    pub fn backup_restore_preview(
        &self,
        name: &str,
        password: Option<Vec<u8>>,
    ) -> AppResult<ImportPreview> {
        let name = name.to_string();
        let password = password.map(Zeroizing::new);
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let cfg = core.settings.backup.clone();
            core.session()?
                .backup_restore_preview(&cfg, &name, password.as_deref().map(|p| p.as_slice()))
                .map_err(AppError::refused)
        })
    }
}
