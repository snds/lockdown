//! Exports locked to public keys (docs/plan/23-bridges.md §11): the
//! Lockdown document sealed with age to one or more `age1…` recipients,
//! written by the core to the place the person picked, after the `Reveal`
//! gate like every export (D101, D141).

use std::path::PathBuf;
// The clock and the gate are only used by the native export; the web
// vault's stand-in below refuses in words (doc 17 §10).
#[cfg(feature = "native")]
use std::time::Instant;

#[cfg(feature = "native")]
use crate::ApprovalKind;
use crate::{App, AppError, AppResult, ExportDone, GateInput};

impl App {
    /// Writes every vault (or the one `vault` names) to `path`, sealed to
    /// `recipients`, after the `Reveal` gate.
    #[cfg(feature = "native")]
    pub fn export_to_recipients(
        &self,
        path: PathBuf,
        recipients: Vec<String>,
        vault: Option<String>,
        gate: GateInput,
    ) -> AppResult<ExportDone> {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            // Refuse a bad key before asking for anything.
            for r in recipients.iter().filter(|r| !r.trim().is_empty()) {
                lockdown_cxf::age_file::recipient(r).map_err(AppError::refused)?;
            }
            core.approve(ApprovalKind::Reveal, "export the vault", &gate)?;
            let (bytes, preview) = core
                .session()?
                .export_to_recipients(&recipients, vault.as_deref())
                .map_err(AppError::refused)?;
            crate::session::transfer::write_user_file(&path, &bytes).map_err(AppError::refused)?;
            Ok(ExportDone {
                path: path.display().to_string(),
                bytes: bytes.len() as u64,
                preview,
            })
        })
    }

    /// In a build without age (the web vault), said plainly.
    #[cfg(not(feature = "native"))]
    pub fn export_to_recipients(
        &self,
        path: PathBuf,
        recipients: Vec<String>,
        vault: Option<String>,
        gate: GateInput,
    ) -> AppResult<ExportDone> {
        let _ = (path, recipients, vault, gate);
        Err(AppError::refused(
            "exports locked to a public key are made in the desktop app",
        ))
    }
}
