//! What the clients show about the server itself
//! (docs/plan/27-web-server-finish.md §3–§4): the server's own restore
//! drills, as it reports them, and the key-transparency log mirrors this
//! device compares the server's log against.
//!
//! **The restore drill** comes from the server (`GET
//! /v1/server/restore-drill`), read from the operator's log there, never
//! made up here: a server that runs no drills, or is older than the
//! endpoint, says so in words. It is information, not a check: a lying
//! server could claim a drill it never ran, which is why the vault's own
//! guarantees never rest on it.
//!
//! **Mirrors** the person adds are kept in the settings file
//! (`kt_mirrors`), changed only here; each sync compares them, and the
//! ones the server names, with the log it verified
//! (`session::mirrors`).

use serde::Serialize;

use crate::types::MirrorCheck;
use crate::{App, AppError, AppResult};

/// The server's restore drills, for Settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RestoreDrillView {
    /// Whether the server answered (an older one has no such report).
    pub reported: bool,
    /// Whether backups and drills run on a schedule there.
    pub scheduled: bool,
    /// When the newest drill ran.
    pub last_run_at: Option<i64>,
    /// Whether it passed.
    pub last_passed: Option<bool>,
    /// When the newest passing drill ran: the last successful restore.
    pub last_success_at: Option<i64>,
    /// Whether that drill also signed a client in to the restored copy and
    /// read an item back.
    pub client_read_back: bool,
    /// The line Settings shows, in plain words.
    pub words: String,
}

/// The mirrors, for Settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MirrorsView {
    /// The ones the person added.
    pub configured: Vec<String>,
    /// What every known mirror said at the last sync.
    pub checks: Vec<MirrorCheck>,
}

/// The words for a drill report (`None`: the server has no such report).
pub fn drill_words(r: Option<&lockdown_sync::wire::RestoreDrill>) -> String {
    use lockdown_sync::engine::civil_date;
    let Some(r) = r else {
        return "This server doesn't report restore drills (it may be an older release).".into();
    };
    let mut words = match r.last_success_at {
        Some(at) => format!(
            "The server last proved its backup restores on {}{}.",
            civil_date(at),
            if r.client_read_back {
                ", with a sign-in to the restored copy that read an item back"
            } else {
                ""
            }
        ),
        None if r.last_run_at.is_some() => "No restore drill has passed on this server yet.".into(),
        None if r.scheduled => "The server runs restore drills, but none has run yet.".into(),
        None => "This server doesn't run restore drills. Ask whoever runs it to schedule backups (--backup-dir).".into(),
    };
    if r.last_passed == Some(false) {
        words.push_str(&format!(
            " The newest drill, on {}, failed: tell whoever runs the server.",
            civil_date(r.last_run_at.unwrap_or_default())
        ));
    }
    words
}

impl App {
    /// The server's restore drills, as it reports them. Not activity.
    pub fn restore_drill(&self) -> AppResult<RestoreDrillView> {
        let (server, token) = self.live_account().ok_or_else(|| {
            AppError::refused("the restore drill is read from the account's server; sign in first")
        })?;
        let report = lockdown_sync::HttpTransport::new(&server)
            .and_then(|t| t.restore_drill(&token))
            .map_err(|e| AppError::refused(e.to_string()))?;
        let words = drill_words(report.as_ref());
        let r = report.clone().unwrap_or_default();
        Ok(RestoreDrillView {
            reported: report.is_some(),
            scheduled: r.scheduled,
            last_run_at: r.last_run_at,
            last_passed: r.last_passed,
            last_success_at: r.last_success_at,
            client_read_back: r.client_read_back,
            words,
        })
    }

    /// The mirrors the person added, and every known mirror's last word.
    pub fn kt_mirrors(&self) -> AppResult<MirrorsView> {
        let configured = self.settings()?.kt_mirrors;
        let checks = self.call(|core| {
            let server = core
                .session
                .as_ref()
                .and_then(|s| s.account_session())
                .and_then(Result::ok)
                .map(|(server, _)| server);
            Ok(server
                .map(|s| crate::session::mirrors::last(&core.paths.data, &s))
                .unwrap_or_default())
        })?;
        Ok(MirrorsView { configured, checks })
    }

    /// Sets the mirrors the person added (at most eight, each `https://`,
    /// or `http://` to this machine). Compared from the next sync on.
    pub fn set_kt_mirrors(&self, urls: Vec<String>) -> AppResult<MirrorsView> {
        let mut clean = Vec::new();
        for u in urls.iter().map(|u| u.trim()).filter(|u| !u.is_empty()) {
            let n = lockdown_sync::mirror::normalise_mirror(u).ok_or_else(|| {
                AppError::refused(format!(
                    "{u} isn't a mirror address: it must start with https:// (or http:// to this computer)"
                ))
            })?;
            if !clean.contains(&n) {
                clean.push(n);
            }
        }
        if clean.len() > lockdown_sync::mirror::MAX_MIRRORS {
            return Err(AppError::refused("at most eight mirrors"));
        }
        self.call(move |core| {
            let mut settings = core.settings.clone();
            settings.kt_mirrors = clean;
            settings
                .save(&core.paths.settings_file())
                .map_err(AppError::refused)?;
            core.settings = settings;
            Ok(())
        })?;
        self.kt_mirrors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_sync::wire::RestoreDrill;

    #[test]
    fn the_drill_words_say_what_the_server_reported_and_nothing_more() {
        assert!(drill_words(None).contains("doesn't report"));
        assert!(drill_words(Some(&RestoreDrill::default())).contains("doesn't run restore drills"));
        let good = RestoreDrill {
            scheduled: true,
            last_run_at: Some(1_792_316_980),
            last_passed: Some(true),
            last_success_at: Some(1_792_316_980),
            client_read_back: true,
        };
        let w = drill_words(Some(&good));
        assert!(
            w.contains("2026-10-18") && w.contains("read an item back"),
            "{w}"
        );
        let failed = RestoreDrill {
            last_run_at: Some(1_792_400_000),
            last_passed: Some(false),
            client_read_back: false,
            ..good
        };
        let w = drill_words(Some(&failed));
        assert!(w.contains("2026-10-18") && w.contains("failed"), "{w}");
        assert!(!w.contains("read an item back"));
        let never = RestoreDrill {
            scheduled: true,
            ..RestoreDrill::default()
        };
        assert!(drill_words(Some(&never)).contains("none has run yet"));
    }
}
