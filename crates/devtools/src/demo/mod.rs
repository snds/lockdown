//! The demo (docs/plan/40-devtools.md §3): a full, real session over a
//! throwaway vault, filled from a data pack. The app moves into
//! `<data>/demo/` with [`App::rehome`] (the vault open before is locked
//! first), confined: nothing reaches the network, exports and backups are
//! refused, and the browser endpoint reads as locked. Leaving moves the app
//! back, locked, and wipes `<data>/demo/`. A demo left open when the app
//! quit is wiped at the next launch ([`Demo::new`]).
//!
//! The demo has its own identity root and data directory under
//! `<data>/demo/`, its own device key in a file there, no account, and no
//! entry in the OS keystore.

pub mod fresh;
pub mod load;
pub mod pack;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lockdown_app::{App, Paths, Status};
use serde::Serialize;

pub use load::{Loaded, load};
pub use pack::{Pack, default_pack};

/// The sentence the confined demo refuses with.
pub const WHY: &str = "the demo never syncs, shares, exports, backs up or goes online";

/// The demo's folder inside Lockdown's data directory.
pub const DEMO_DIR: &str = "demo";

/// Where the person was when the demo opened, so leaving goes back there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CameFrom {
    /// Nothing set up yet: the first run.
    FirstRun,
    /// Their own vault, which is locked while the demo runs.
    Vault,
}

/// The demo as the UI sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DemoState {
    /// Whether the demo is open.
    pub open: bool,
    /// Where leaving goes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub came_from: Option<CameFrom>,
    /// The pack's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    /// How many items it loaded.
    pub items: usize,
}

/// What opening (or rebuilding) the demo gives back.
#[derive(Clone, Debug, Serialize)]
pub struct Opened {
    /// The demo session, open.
    pub status: Status,
    /// Where leaving goes.
    pub came_from: CameFrom,
    /// What the pack made.
    pub loaded: Loaded,
}

struct Open {
    left: Paths,
    came_from: CameFrom,
    pack: Pack,
    loaded: Loaded,
}

/// The demo for one app.
pub struct Demo {
    app: App,
    dir: PathBuf,
    open: Mutex<Option<Open>>,
}

/// `<data>/demo` for a data directory.
pub fn demo_dir(data: &Path) -> PathBuf {
    data.join(DEMO_DIR)
}

fn wipe(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("couldn't clear {}: {e}", dir.display())),
    }
}

impl Demo {
    /// The demo for `app`, whose own home is `home`. A demo folder left
    /// from a run that quit inside the demo is wiped now.
    pub fn new(app: App, home: &Paths) -> Self {
        let dir = demo_dir(&home.data);
        let _ = wipe(&dir);
        Self {
            app,
            dir,
            open: Mutex::new(None),
        }
    }

    /// The demo's own paths: `<data>/demo/identity` and `<data>/demo/lockdown`.
    pub fn paths(&self) -> Paths {
        Paths {
            root: self.dir.join("identity"),
            data: self.dir.join("lockdown"),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Open>> {
        match self.open.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Where things stand.
    pub fn state(&self) -> DemoState {
        match self.lock().as_ref() {
            Some(o) => DemoState {
                open: true,
                came_from: Some(o.came_from),
                pack: Some(o.pack.name.clone()),
                items: o.loaded.items,
            },
            None => DemoState {
                open: false,
                came_from: None,
                pack: None,
                items: 0,
            },
        }
    }

    /// Opens the demo from `pack`, or rebuilds it from `pack` when it is
    /// open already. On any failure the app goes back where it was.
    pub fn open(&self, pack: Pack) -> Result<Opened, String> {
        pack.check()?;
        let mut open = self.lock();
        let err = |e: lockdown_app::AppError| e.message;
        let (left, came_from) = match open.take() {
            Some(o) => {
                // Drop the demo's store before its folder goes.
                self.app
                    .rehome(self.paths(), Some(WHY.into()))
                    .map_err(err)?;
                (o.left, o.came_from)
            }
            None => {
                let probe = self.app.probe().map_err(err)?;
                let came_from = if probe.identity_present {
                    CameFrom::Vault
                } else {
                    CameFrom::FirstRun
                };
                let left = self
                    .app
                    .rehome(self.paths(), Some(WHY.into()))
                    .map_err(err)?;
                (left, came_from)
            }
        };
        let built = (|| {
            wipe(&self.dir)?;
            // Again over the clean folder, so nothing from before is read.
            self.app
                .rehome(self.paths(), Some(WHY.into()))
                .map_err(err)?;
            self.app.open_or_create(true).map_err(err)?;
            let mut settings = self.app.settings().map_err(err)?;
            settings.breach_check = false;
            settings.change_password_check = false;
            settings.backup.enabled = false;
            self.app.set_settings(settings).map_err(err)?;
            let loaded = load(&self.app, &pack)?;
            let status = self.app.status().map_err(err)?;
            Ok::<_, String>((status, loaded))
        })();
        match built {
            Ok((status, loaded)) => {
                *open = Some(Open {
                    left,
                    came_from,
                    pack,
                    loaded: loaded.clone(),
                });
                Ok(Opened {
                    status,
                    came_from,
                    loaded,
                })
            }
            Err(e) => {
                let _ = self.app.rehome(left, None);
                let _ = wipe(&self.dir);
                Err(e)
            }
        }
    }

    /// The demo as it stands, when it is open.
    pub fn opened(&self) -> Result<Option<Opened>, String> {
        let open = self.lock();
        let Some(o) = open.as_ref() else {
            return Ok(None);
        };
        Ok(Some(Opened {
            status: self.app.status().map_err(|e| e.message)?,
            came_from: o.came_from,
            loaded: o.loaded.clone(),
        }))
    }

    /// Builds the demo again from the pack it was opened with.
    pub fn reset(&self) -> Result<Opened, String> {
        let pack = self
            .lock()
            .as_ref()
            .map(|o| o.pack.clone())
            .ok_or("The demo isn't open.")?;
        self.open(pack)
    }

    /// Leaves the demo: the app goes back to its own home, locked, and the
    /// demo's folder is wiped. Says where the person came from.
    pub fn leave(&self) -> Result<CameFrom, String> {
        let mut open = self.lock();
        let o = open.take().ok_or("The demo isn't open.")?;
        if let Err(e) = self.app.rehome(o.left.clone(), None) {
            *open = Some(o);
            return Err(e.message);
        }
        wipe(&self.dir)?;
        Ok(o.came_from)
    }
}
