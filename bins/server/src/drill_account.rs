//! The restore drill's client check (`official-self-host`; 05-self-host §4
//! "a test identity opening a test vault"; docs/plan/27-web-server-finish.md
//! §3).
//!
//! A drill that only compares tables proves the backup holds the rows; it
//! doesn't prove a person could sign in to the restored server and read
//! their vault. So the server keeps one **restore-drill account** of its
//! own: an identity and a vault made by the real client (`lockdown-app`,
//! the same code the desktop app runs), holding one login whose password is
//! a random marker. It is made once, on the live server, through a
//! listener on this machine's loopback address that serves the live state
//! for as long as it takes; its device files, its password and the marker
//! stay in `<data>/restore-drill/` (user-only). The account holds nothing of
//! anyone's, so a copy of that folder gives away nothing but itself.
//!
//! Each drill then starts a scratch server over the restored copy (its own
//! loopback listener, the restored key file), copies the account's device
//! files to a scratch folder, signs in with the password exactly as a
//! device that already holds the identity does (OPAQUE, then every sync
//! check: the identity's records, the log's tree head and the account's
//! audit, the vault's head chain), syncs, and reads the login's password
//! back through the core's reveal. Only the marker coming back counts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::AppState;

/// The account's login names start with this (the admin page marks it).
pub const LOGIN_PREFIX: &str = "lockdown-restore-drill-";
/// The one login it holds.
pub const ITEM_TITLE: &str = "Lockdown restore drill";
/// What a drill's detail says when the check passed (the clients read it).
pub const READ_BACK: &str = "a client signed in to the restored copy and read an item back";
const FILE: &str = "drill-account.cbor";
const SITE: &str = "https://restore-drill.invalid";

/// What the folder keeps beside the device files.
#[derive(Serialize, Deserialize)]
struct Saved {
    v: u16,
    login: String,
    password: String,
    marker: String,
    created_at: i64,
}

impl Drop for Saved {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        self.marker.zeroize();
    }
}

/// The restore-drill account's folder.
#[derive(Clone, Debug)]
pub struct DrillAccount {
    dir: PathBuf,
}

fn hex(n: usize) -> Result<String, String> {
    let bytes = lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?;
    Ok(data_encoding::HEXLOWER.encode(&bytes[..n.min(32)]))
}

fn paths(dir: &Path) -> lockdown_app::Paths {
    lockdown_app::Paths {
        root: dir.join("identity"),
        data: dir.join("data"),
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

impl DrillAccount {
    /// The account kept in `dir`.
    pub fn at(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// Its folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn saved(&self) -> Result<Option<Saved>, String> {
        match std::fs::read(self.dir.join(FILE)) {
            Ok(bytes) => {
                let bytes = Zeroizing::new(bytes);
                lockdown_sync::wire::decode(&bytes)
                    .map(Some)
                    .map_err(|e| format!("the restore-drill account's file is unreadable: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("couldn't read the restore-drill account: {e}")),
        }
    }

    /// When it was made, if it has been.
    pub fn created_at(&self) -> Option<i64> {
        self.saved().ok().flatten().map(|s| s.created_at)
    }

    /// Makes the account on the live server, once. `Ok(true)` when it was
    /// made now.
    pub fn ensure(&self, state: &Arc<AppState>) -> Result<bool, String> {
        if self.saved()?.is_some() {
            return Ok(false);
        }
        crate::keys::create_private_dir(&self.dir)
            .map_err(|e| format!("couldn't make {}: {e}", self.dir.display()))?;
        // A half-made account from an earlier try is started over.
        for sub in ["identity", "data"] {
            let _ = std::fs::remove_dir_all(self.dir.join(sub));
        }
        let now = state.now();
        let saved = Saved {
            v: 1,
            login: format!("{LOGIN_PREFIX}{}", hex(6)?),
            password: hex(24)?,
            marker: hex(16)?,
            created_at: now,
        };
        // Sign-ups closed: the server invites its own account for an hour.
        if !state.config.signups_open {
            let store = state
                .storage
                .sql()
                .ok_or("this server's store can't hold invites")?;
            store
                .put_invite(&saved.login, now, now + 3600)
                .map_err(|e| e.to_string())?;
        }
        let server = crate::run_in_background(state.clone())
            .map_err(|e| format!("couldn't listen on this machine for the drill account: {e}"))?;
        let url = format!("http://{}", server.addr);
        let app =
            lockdown_app::App::start(paths(&self.dir), None, lockdown_app::LockPolicy::default());
        let made = (|| {
            app.open_or_create(true).map_err(|e| e.message)?;
            app.add_login(
                SITE,
                "restore-drill",
                saved.marker.as_bytes().to_vec(),
                Some(ITEM_TITLE),
            )
            .map_err(|e| e.message)?;
            app.account_register(&url, &saved.login, &saved.password, None)
                .map_err(|e| e.message)?;
            app.sync().map_err(|e| e.message)?;
            Ok::<_, String>(())
        })();
        let _ = app.lock();
        drop(app);
        drop(server);
        made.map_err(|e| format!("couldn't make the restore-drill account: {e}"))?;
        let bytes = Zeroizing::new(lockdown_sync::wire::encode(&saved).map_err(|e| e.to_string())?);
        crate::keys::write_private(&self.dir.join(FILE), &bytes)
            .map_err(|e| format!("couldn't keep the restore-drill account: {e}"))?;
        Ok(true)
    }

    /// Signs a client in to the server at `url` from a scratch copy of the
    /// account's device in `work`, syncs, and reads the login back.
    pub fn read_back(&self, url: &str, work: &Path) -> Result<(), String> {
        let saved = self
            .saved()?
            .ok_or("there is no restore-drill account yet")?;
        let copy = work.join("client");
        copy_tree(&self.dir.join("identity"), &copy.join("identity"))
            .and_then(|()| copy_tree(&self.dir.join("data"), &copy.join("data")))
            .map_err(|e| format!("couldn't copy the restore-drill device: {e}"))?;
        let app = lockdown_app::App::start(paths(&copy), None, lockdown_app::LockPolicy::default());
        let result = (|| {
            app.open_or_create(false)
                .map_err(|e| format!("the drill device didn't open: {}", e.message))?;
            app.account_login(url, &saved.login, &saved.password, None)
                .map_err(|e| format!("signing in to the restored copy failed: {}", e.message))?;
            app.sync()
                .map_err(|e| format!("syncing from the restored copy failed: {}", e.message))?;
            let item = app
                .list()
                .map_err(|e| e.message)?
                .into_iter()
                .find(|i| i.title == ITEM_TITLE)
                .ok_or("the restored copy doesn't hold the drill account's login")?;
            let value = app
                .reveal(&item.id, "pw", lockdown_app::GateInput::default())
                .map_err(|e| format!("the login didn't open: {}", e.message))?;
            if value.expose() != saved.marker.as_bytes() {
                return Err("the login came back with another password".to_string());
            }
            Ok(())
        })();
        let _ = app.lock();
        result
    }
}
