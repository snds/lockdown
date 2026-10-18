//! Scheduled backups and restore drills (`official-self-host`; 05-self-host
//! §4; docs/plan/25-self-host-2.md §4).
//!
//! With `--backup-dir` and the Backup Key (`LOCKDOWN_BACKUP_KEY` or
//! `--backup-key-file`), the server backs itself up on a schedule into that
//! folder (`lockdown-<UTC time>.ldbk`, the `LDSRVBK1` format of
//! [`crate::backup`]), keeps the newest few, and on its own schedule runs a
//! **restore drill**: the newest backup is opened with the Backup Key,
//! restored into a scratch database (a temporary SQLite file, or a scratch
//! schema in the same Postgres database), and checked against the live
//! server:
//!
//! 1. every file in it matches its hash and the database copy opens
//!    (`backup verify`);
//! 2. the server key file in it is the live one (so passwords and sessions
//!    would still work);
//! 3. its key-transparency log is the start of the live log, leaf for leaf
//!    (a backup from another history fails);
//! 4. every vault's head chain is whole (one head per step, the last one
//!    the vault's), and no head in it is newer than the live server's (a
//!    live server older than its backup has been rolled back);
//! 5. every attachment body the database names came back, at its size and
//!    under its own BLAKE3 name.
//!
//! The result (passed, or what failed) goes into the operator's log, which
//! the admin page shows ("last successful restore", a red banner while the
//! newest drill failed), and into the server's log. Nothing secret is
//! logged: file names, counts and the step that failed.
//!
//! The Backup Key is then held in the server's memory for as long as it
//! runs, which the person accepts by giving it (D226 kept it out of the
//! server entirely; flagged for Sean).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::AppState;
use crate::backup::{self, BackupKey, RestoreTarget};
use crate::db::{Dialect, params};
use crate::storage::{SqlStorage, Storage};

/// The schedule.
pub struct Schedule {
    dir: PathBuf,
    key: BackupKey,
    server_key: PathBuf,
    every: Duration,
    keep: usize,
    drill_every: Duration,
    running: AtomicBool,
}

impl std::fmt::Debug for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schedule")
            .field("dir", &self.dir)
            .field("every", &self.every)
            .field("keep", &self.keep)
            .finish_non_exhaustive()
    }
}

impl Schedule {
    /// Backups into `dir` (made user-only) every `every`, the newest `keep`
    /// kept, a drill every `drill_every`; `server_key` is the live key file.
    pub fn new(
        dir: &Path,
        key: BackupKey,
        server_key: &Path,
        every: Duration,
        keep: usize,
        drill_every: Duration,
    ) -> Result<Self, String> {
        crate::keys::create_private_dir(dir)
            .map_err(|e| format!("couldn't make the backup folder {}: {e}", dir.display()))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            key,
            server_key: server_key.to_path_buf(),
            every: every.max(Duration::from_secs(3600)),
            keep: keep.max(1),
            drill_every: drill_every.max(Duration::from_secs(3600)),
            running: AtomicBool::new(false),
        })
    }

    /// The backup folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many backups are kept.
    pub fn keep(&self) -> usize {
        self.keep
    }

    /// Seconds between backups.
    pub fn every_secs(&self) -> u64 {
        self.every.as_secs()
    }

    /// Seconds between drills.
    pub fn drill_every_secs(&self) -> u64 {
        self.drill_every.as_secs()
    }
}

/// One backup file in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupFile {
    /// Its name.
    pub name: String,
    /// Its size.
    pub size: u64,
    /// When it was written (seconds since the Unix epoch).
    pub modified: i64,
    path: PathBuf,
}

/// The backups in `dir`, newest first (by name, which is the UTC time).
pub fn list_backups(dir: &Path) -> Result<Vec<BackupFile>, String> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("couldn't list {}: {e}", dir.display())),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !(name.starts_with("lockdown-") && name.ends_with(".ldbk")) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_secs()).ok())
            .unwrap_or(0);
        out.push(BackupFile {
            name,
            size: meta.len(),
            modified,
            path: entry.path(),
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// `lockdown-20261018-093000.ldbk`.
fn file_name(now: i64) -> String {
    let date = lockdown_sync::engine::civil_date(now).replace('-', "");
    let rem = now.rem_euclid(86_400);
    format!(
        "lockdown-{date}-{:02}{:02}{:02}.ldbk",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn live_store(state: &AppState) -> Result<&SqlStorage, String> {
    state
        .storage
        .sql()
        .ok_or_else(|| "this server's store can't be backed up by the schedule".to_string())
}

/// Makes a backup now, keeps the newest `keep`, and records the result.
pub fn backup_now(state: &AppState, s: &Schedule) -> Result<String, String> {
    let now = state.now();
    let result = (|| {
        let store = live_store(state)?;
        let mut name = file_name(now);
        let mut n = 1;
        while s.dir.join(&name).exists() {
            n += 1;
            name = file_name(now).replace(".ldbk", &format!("-{n}.ldbk"));
        }
        let out = s.dir.join(&name);
        let summary = backup::create(
            &backup::Sources {
                storage: store,
                server_key: &s.server_key,
            },
            &s.key,
            &out,
            now,
        )?;
        let mut removed = 0;
        for old in list_backups(&s.dir)?.into_iter().skip(s.keep) {
            if std::fs::remove_file(&old.path).is_ok() {
                removed += 1;
            }
        }
        Ok::<_, String>(format!(
            "{name}: {} files ({} attachments), {}{}",
            summary.files,
            summary.blobs,
            crate::admin::size(summary.bytes),
            if removed > 0 {
                format!("; {removed} older one(s) removed")
            } else {
                String::new()
            }
        ))
    })();
    let (ok, detail) = match &result {
        Ok(d) => (true, d.clone()),
        Err(e) => (false, e.clone()),
    };
    eprintln!(
        "lockdown-server: backup {}: {detail}",
        if ok { "made" } else { "failed" }
    );
    if let Ok(store) = live_store(state) {
        let _ = store.record_event("backup", ok, &detail, now);
    }
    result
}

/// What a drill found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrillReport {
    /// Whether every check passed.
    pub passed: bool,
    /// The backup it used.
    pub file: String,
    /// What it checked or what failed, in words.
    pub detail: String,
}

/// Runs a restore drill on the newest backup now and records the result.
pub fn drill_now(state: &AppState, s: &Schedule) -> DrillReport {
    let now = state.now();
    let newest = list_backups(&s.dir).ok().and_then(|v| v.into_iter().next());
    let report = match newest {
        None => DrillReport {
            passed: false,
            file: String::new(),
            detail: "there is no backup to restore yet".into(),
        },
        Some(file) => match drill(state, s, &file.path) {
            Ok(detail) => DrillReport {
                passed: true,
                file: file.name.clone(),
                detail: format!("{}: {detail}", file.name),
            },
            Err(why) => DrillReport {
                passed: false,
                file: file.name.clone(),
                detail: format!("{}: {why}", file.name),
            },
        },
    };
    eprintln!(
        "lockdown-server: restore drill {}: {}",
        if report.passed { "passed" } else { "FAILED" },
        report.detail
    );
    if let Ok(store) = live_store(state) {
        let _ = store.record_event("drill", report.passed, &report.detail, now);
    }
    report
}

/// The drill itself: restore into scratch, then the checks of the module
/// docs. `Ok` says what passed; `Err` names the first step that failed.
fn drill(state: &AppState, s: &Schedule, file: &Path) -> Result<String, String> {
    let live = live_store(state)?;
    // 1. Every file matches its hash; the database copy opens.
    let (_, facts) = backup::verify(file, &s.key).map_err(|e| format!("it doesn't open: {e}"))?;
    let work = tempfile::Builder::new()
        .prefix(".lockdown-drill-")
        .tempdir_in(&s.dir)
        .map_err(|e| format!("couldn't make a scratch folder: {e}"))?;
    let data = work.path().join("data");
    let scratch_pg = match (live.dialect(), live.pg_options()) {
        (Dialect::Postgres, Some(options)) => {
            let suffix = lockdown_crypto::rand::random_16().map_err(|e| e.to_string())?;
            let mut o = options.clone();
            o.schema = Some(format!(
                "lockdown_drill_{}",
                data_encoding::HEXLOWER.encode(&suffix[..6])
            ));
            o.exclusive = false;
            o.drop_schema_on_close = false;
            Some(o)
        }
        _ => None,
    };
    backup::restore(
        file,
        &s.key,
        &RestoreTarget {
            data: data.clone(),
            database: scratch_pg.clone(),
            bodies: None,
        },
    )
    .map_err(|e| format!("it doesn't restore: {e}"))?;
    let restored = match scratch_pg {
        Some(mut o) => {
            o.drop_schema_on_close = true;
            SqlStorage::open_postgres(o)
        }
        None => SqlStorage::open(&data.join(backup::DB_NAME)),
    }
    .map_err(|e| format!("the restored database doesn't open: {e}"))?
    .with_blob_dir(&data.join("blobs"))
    .map_err(|e| e.to_string())?;
    let counts = restored.counts().map_err(|e| e.to_string())?;
    if counts != facts {
        return Err("the restored database doesn't hold what the backup's copy does".into());
    }
    // 2. The server key file is the live one.
    let restored_key = std::fs::read(data.join(backup::KEY_NAME))
        .map_err(|_| "the restored key file is missing".to_string())?;
    let live_key = std::fs::read(&s.server_key)
        .map_err(|_| "the live server key file couldn't be read".to_string())?;
    if restored_key != live_key {
        return Err(
            "its server key file isn't this server's: passwords and sessions would stop working"
                .into(),
        );
    }
    // 3. Its public log is the start of the live one.
    let theirs = restored.log_leaf_hashes().map_err(|e| e.to_string())?;
    let ours = live.log_leaf_hashes().map_err(|e| e.to_string())?;
    if theirs.len() > ours.len() || ours[..theirs.len()] != theirs[..] {
        return Err("its key-transparency log isn't the start of this server's log".into());
    }
    // 4. Every vault's head chain is whole and not ahead of the live one.
    let vaults: Vec<(String, i64, Option<Vec<u8>>)> = restored
        .conn()
        .query_map(
            "SELECT vault_id, head_seq, head_hash FROM vaults ORDER BY vault_id",
            params![],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    for (vault, seq, hash) in &vaults {
        let id = lockdown_vault::Id::parse(vault).map_err(|_| "a bad vault id".to_string())?;
        let heads = restored.heads_since(id, 0).map_err(|e| e.to_string())?;
        let whole = heads.len() as i64 == *seq
            && heads.iter().enumerate().all(|(i, h)| h.seq == i as u64 + 1)
            && heads.last().map(|h| h.hash.0.to_vec()) == *hash;
        if !whole {
            return Err(format!("vault {vault}'s head chain isn't whole"));
        }
        if let Some(now_row) = live.vault(id).map_err(|e| e.to_string())? {
            if now_row.head_seq < u64::try_from(*seq).unwrap_or(0) {
                return Err(format!(
                    "vault {vault} is further on in the backup than on the live server: the live server may have been rolled back"
                ));
            }
            if now_row.head_seq == u64::try_from(*seq).unwrap_or(0)
                && now_row.head_hash.map(|h| h.0.to_vec()) != *hash
            {
                return Err(format!(
                    "vault {vault} has a different head at the same step in the backup and on the live server"
                ));
            }
        }
    }
    // 5. Every attachment body came back, whole.
    let mut bodies = 0;
    for (vault, hash, size) in restored.outside_blobs().map_err(|e| e.to_string())? {
        let bytes = restored
            .blob(vault, hash)
            .map_err(|e| format!("an attachment didn't come back: {e}"))?
            .ok_or_else(|| "an attachment didn't come back".to_string())?;
        if bytes.len() as u64 != size || lockdown_crypto::hash::hash(&bytes) != hash {
            return Err("an attachment came back changed".into());
        }
        bodies += 1;
    }
    Ok(format!(
        "restored into a scratch {}: {} account(s), {} vault(s), {} log entries (the start of this server's {}), {} attachment(s), the server key, every head chain whole",
        if restored.dialect() == Dialect::Postgres {
            "Postgres schema"
        } else {
            "database"
        },
        counts.accounts,
        counts.vaults,
        counts.log_size,
        ours.len(),
        bodies
    ))
}

/// Runs a backup or a drill on a thread of its own unless one is already
/// running; `false` when one is.
pub fn run_in_background(state: Arc<AppState>, s: Arc<Schedule>, drill: bool) -> bool {
    if s.running.swap(true, Ordering::SeqCst) {
        return false;
    }
    let spawned = std::thread::Builder::new()
        .name("lockdown-backup".into())
        .spawn({
            let s = s.clone();
            move || {
                if drill {
                    drill_now(&state, &s);
                } else {
                    let _ = backup_now(&state, &s);
                }
                s.running.store(false, Ordering::SeqCst);
            }
        });
    if spawned.is_err() {
        s.running.store(false, Ordering::SeqCst);
        return false;
    }
    true
}

/// When the last event of `kind` was, from the operator's log.
fn last_at(state: &AppState, kind: &str) -> Option<i64> {
    live_store(state)
        .ok()?
        .events(Some(kind), 1)
        .ok()?
        .first()
        .map(|e| e.at)
}

/// Runs the schedule for as long as the process lives: a backup when one is
/// due, then a drill when one is due, checking every minute.
pub fn spawn(state: Arc<AppState>, s: Arc<Schedule>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("lockdown-schedule".into())
        .spawn(move || {
            loop {
                let now = state.now();
                let backup_due = last_at(&state, "backup")
                    .is_none_or(|t| now - t >= i64::try_from(s.every_secs()).unwrap_or(i64::MAX));
                let drill_due = last_at(&state, "drill").is_none_or(|t| {
                    now - t >= i64::try_from(s.drill_every_secs()).unwrap_or(i64::MAX)
                });
                if (backup_due || drill_due) && !s.running.swap(true, Ordering::SeqCst) {
                    if backup_due {
                        let _ = backup_now(&state, &s);
                    }
                    if drill_due {
                        drill_now(&state, &s);
                    }
                    s.running.store(false, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_sort_by_time() {
        assert_eq!(file_name(1_792_316_980), "lockdown-20261018-094940.ldbk");
        assert!(file_name(1_792_316_980) < file_name(1_792_316_981));
    }
}
