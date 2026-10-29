//! The developer reset (docs/plan/40-devtools.md §2): everything Lockdown
//! keeps on this device, found without opening anything, then erased.
//!
//! | Erased | How it is found |
//! |---|---|
//! | Lockdown's data directory (the vault, the device key, settings, the endpoint file, pairings, the demo, and whatever later slices put there, such as the in-process server's store) | the app's own [`lockdown_app::Paths`] |
//! | The shell's own directories (config, cache, logs, local data) | named by the shell |
//! | Every OS keystore entry under [`SERVICE`] | the keystore's own search ([`Keystore::list`]) |
//! | The shared identity root, only when Lockdown alone uses it | [`identity_plan`] |
//!
//! Nothing is taken from the page: a plan is worked out again from the
//! places each time it is shown and again when it runs. A path the
//! [`guard`] doesn't like (relative, near the top of the disk, holding the
//! home folder) is never touched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

/// The service every Lockdown keystore entry is filed under (the desktop's
/// keychain, `apps/desktop/src-tauri/src/lib.rs`).
pub const SERVICE: &str = "design.snds.lockdown";

/// The app name Lockdown writes into the shared identity's records.
pub const APP: &str = lockdown_vault::identity::APP;

/// The file that says a reset is waiting for the next launch, inside the
/// data directory (so the erase removes it too).
pub const PENDING_FILE: &str = "devtools-reset-pending";

/// A place for small secrets that can list what it holds under a service.
pub trait Keystore: Send + Sync {
    /// The account names filed under `service`, sorted.
    fn list(&self, service: &str) -> Result<Vec<String>, String>;
    /// Removes one entry; a missing one is not an error.
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

/// A keystore in memory: the tests' double, and a stand-in where a shell
/// has no keystore.
#[derive(Default)]
pub struct MemoryKeystore {
    entries: Mutex<BTreeMap<(String, String), String>>,
}

impl MemoryKeystore {
    /// Files `value` under `service` and `account`.
    pub fn put(&self, service: &str, account: &str, value: &str) {
        self.lock()
            .insert((service.into(), account.into()), value.into());
    }

    /// Every (service, account) held, sorted.
    pub fn all(&self) -> Vec<(String, String)> {
        self.lock().keys().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), String>> {
        match self.entries.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl Keystore for MemoryKeystore {
    fn list(&self, service: &str) -> Result<Vec<String>, String> {
        Ok(self
            .lock()
            .keys()
            .filter(|(s, _)| s == service)
            .map(|(_, a)| a.clone())
            .collect())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        self.lock().remove(&(service.into(), account.into()));
        Ok(())
    }
}

/// The app's own secret store over a [`MemoryKeystore`], under [`SERVICE`]:
/// what the desktop's keychain is, for tests.
pub struct ServiceStore<'a>(pub &'a MemoryKeystore);

impl lockdown_app::secrets::SecretStore for ServiceStore<'static> {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().get(&(SERVICE.into(), key.into())).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.put(SERVICE, key, value);
        Ok(())
    }

    fn delete(&self, key: &str) {
        let _ = Keystore::delete(self.0, SERVICE, key);
    }
}

/// Where Lockdown lives on this device.
#[derive(Clone, Debug)]
pub struct Places {
    /// Lockdown's data directory (`Paths::data`).
    pub data: PathBuf,
    /// The shared identity root (`Paths::root`).
    pub identity_root: PathBuf,
    /// The shell's own directories, each with what it is in words.
    pub shell_dirs: Vec<(String, PathBuf)>,
    /// The home folder, which the guard keeps.
    pub home: Option<PathBuf>,
}

/// One place on disk in a plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Place {
    /// What it is, in words.
    pub what: String,
    /// Where.
    pub path: String,
}

/// What a reset does to the shared identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IdentityPlan {
    /// Its root.
    pub root: String,
    /// Whether the reset removes it.
    pub erase: bool,
    /// Why, in words.
    pub why: String,
    /// The apps its records name, sorted.
    pub apps: Vec<String>,
}

/// The keystore's part of a plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KeystorePlan {
    /// The service searched.
    pub service: String,
    /// The account names found.
    pub entries: Vec<String>,
    /// Why the search failed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Everything a reset would do, for the dialog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Directories that exist now and will be erased.
    pub erase: Vec<Place>,
    /// Keystore entries that will be erased.
    pub keystore: KeystorePlan,
    /// The shared identity.
    pub identity: IdentityPlan,
    /// What stays, in words.
    pub keeps: Vec<String>,
    /// Places the guard refused, with why.
    pub refused: Vec<Place>,
}

/// What a run did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Erased {
    /// Paths removed.
    pub removed: Vec<String>,
    /// Keystore entries removed.
    pub keystore: Vec<String>,
    /// What couldn't be removed, in words.
    pub failed: Vec<String>,
}

/// What stays after a reset, in the dialog's words.
pub fn keeps() -> Vec<String> {
    vec![
        "Your account on a server, if you made one: a reset deletes nothing there. Sign in again to get its vaults back.".into(),
        "Exports and backups saved in folders you chose.".into(),
        "The browser connection's registration with each browser, which Lockdown writes again when it starts. Each browser has to be paired again.".into(),
        "Anything belonging to Mailbird, Synapse or another app that shares your identity.".into(),
    ]
}

/// Refuses a path a reset must never remove: not absolute, fewer than
/// three folders deep, or the home folder or one that holds it.
pub fn guard(path: &Path, home: Option<&Path>) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("it isn't a full path".into());
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("it has '..' or '.' in it".into());
    }
    let depth = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if depth < 3 {
        return Err("it is too near the top of the disk".into());
    }
    if let Some(home) = home
        && home.starts_with(path)
    {
        return Err("it holds your home folder".into());
    }
    Ok(())
}

/// What a reset does to the identity root: removes it only when every
/// record names Lockdown alone (the profile's manager, each persona's
/// maker, each key envelope's app). A store that doesn't verify is read
/// record by record without its signatures; one naming another app is
/// kept.
pub fn identity_plan(root: &Path) -> IdentityPlan {
    let shown = root.display().to_string();
    let plan = |erase: bool, why: String, apps: BTreeSet<String>| IdentityPlan {
        root: shown.clone(),
        erase,
        why,
        apps: apps.into_iter().collect(),
    };
    if !root.exists() {
        return plan(false, "There is no identity here.".into(), BTreeSet::new());
    }
    let store = ecosystem_identity::Store::at(root);
    let named = match store.load() {
        Ok(loaded) => Ok(apps_named(&loaded.contents)),
        Err(e) => match read_unverified(root) {
            Some(contents) => Err((e.to_string(), apps_named(&contents))),
            None => {
                return plan(
                    false,
                    format!(
                        "It couldn't be read ({e}), so it is left as it is. Remove {shown} by hand if nothing else uses it."
                    ),
                    BTreeSet::new(),
                );
            }
        },
    };
    let (apps, unverified) = match named {
        Ok(apps) => (apps, None),
        Err((why, apps)) => (apps, Some(why)),
    };
    let others: Vec<&String> = apps.iter().filter(|a| a.as_str() != APP).collect();
    if others.is_empty() {
        let why = match unverified {
            None => "Only Lockdown uses it, so it is erased: the next launch starts from nothing."
                .to_string(),
            Some(e) => format!(
                "It didn't verify ({e}) and names no app but Lockdown, so it is erased: the next launch starts from nothing."
            ),
        };
        plan(true, why, apps)
    } else {
        let names: Vec<&str> = others.iter().map(|s| s.as_str()).collect();
        plan(
            false,
            format!(
                "It is shared with {}, so it is left as it is. Lockdown's own keys in it stay too, sealed; the next launch says this device isn't enrolled, and joining from that app or recovering with the kit gets back in.",
                names.join(", ")
            ),
            apps,
        )
    }
}

fn apps_named(contents: &ecosystem_identity::store::Contents) -> BTreeSet<String> {
    let mut apps = BTreeSet::new();
    if let Some(manager) = contents.profile.as_ref().and_then(|p| p.manager.clone()) {
        apps.insert(manager);
    }
    for p in contents.personas.values() {
        apps.insert(p.created_by_app.clone());
    }
    for e in contents.envelopes.values() {
        apps.insert(e.app.clone());
    }
    // An identity-level record (the IRK's own envelopes) names no app.
    apps.remove("");
    apps
}

/// The records under `root`, each decoded alone without checking any
/// signature; `None` when not one decodes.
fn read_unverified(root: &Path) -> Option<ecosystem_identity::store::Contents> {
    use ecosystem_identity::cbor::from_slice;
    use ecosystem_identity::records::{KeyEnvelope, Persona, Profile};
    let mut contents = ecosystem_identity::store::Contents::default();
    let mut any = false;
    if let Some(p) = std::fs::read(root.join("profile.cbor"))
        .ok()
        .and_then(|b| from_slice::<Profile>(&b).ok())
    {
        contents.profile = Some(p);
        any = true;
    }
    for (dir, is_persona) in [("personas", true), ("envelopes", false)] {
        let Ok(read) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut names: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
        names.sort();
        for path in names {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let name = path.display().to_string();
            if is_persona {
                if let Ok(p) = from_slice::<Persona>(&bytes) {
                    contents.personas.insert(p.persona_id, p);
                    any = true;
                }
            } else if let Ok(e) = from_slice::<KeyEnvelope>(&bytes) {
                contents.envelopes.insert(name, e);
                any = true;
            }
        }
    }
    any.then_some(contents)
}

/// The plan for `places`, worked out now.
pub fn plan(places: &Places, keystore: &dyn Keystore) -> Plan {
    let mut erase = Vec::new();
    let mut refused = Vec::new();
    let mut consider = |what: String, path: &Path| {
        if !path.exists() {
            return;
        }
        let place = Place {
            what,
            path: path.display().to_string(),
        };
        match guard(path, places.home.as_deref()) {
            Ok(()) => {
                if !erase.iter().any(|p: &Place| p.path == place.path) {
                    erase.push(place);
                }
            }
            Err(why) => refused.push(Place {
                what: format!("{} (left alone: {why})", place.what),
                path: place.path,
            }),
        }
    };
    consider(
        "Lockdown's data: the vault, this device's key, settings, browser pairings, the endpoint file and the demo".into(),
        &places.data,
    );
    for (what, dir) in &places.shell_dirs {
        consider(what.clone(), dir);
    }
    let identity = identity_plan(&places.identity_root);
    if identity.erase {
        consider(
            "The shared identity (only Lockdown uses it)".into(),
            &places.identity_root,
        );
    }
    let keystore = match keystore.list(SERVICE) {
        Ok(entries) => KeystorePlan {
            service: SERVICE.into(),
            entries,
            problem: None,
        },
        Err(e) => KeystorePlan {
            service: SERVICE.into(),
            entries: Vec::new(),
            problem: Some(e),
        },
    };
    Plan {
        erase,
        keystore,
        identity,
        keeps: keeps(),
        refused,
    }
}

/// Erases what [`plan`] finds now (never a plan handed in): the keystore
/// entries first, then each directory.
pub fn erase(places: &Places, keystore: &dyn Keystore) -> Erased {
    let plan = plan(places, keystore);
    let mut out = Erased::default();
    for account in &plan.keystore.entries {
        match keystore.delete(SERVICE, account) {
            Ok(()) => out.keystore.push(account.clone()),
            Err(e) => out
                .failed
                .push(format!("the keystore entry '{account}': {e}")),
        }
    }
    if let Some(problem) = &plan.keystore.problem {
        out.failed
            .push(format!("the keystore couldn't be searched: {problem}"));
    }
    for place in &plan.erase {
        let path = Path::new(&place.path);
        // The guard again, at the moment of removal.
        if let Err(why) = guard(path, places.home.as_deref()) {
            out.failed.push(format!("{}: {why}", place.path));
            continue;
        }
        let gone = if path.is_dir() && !path.is_symlink() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match gone {
            Ok(()) => out.removed.push(place.path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => out.failed.push(format!("{}: {e}", place.path)),
        }
    }
    out
}

/// Leaves word in `data` that a reset waits for the next launch.
pub fn mark_pending(data: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    std::fs::write(data.join(PENDING_FILE), b"reset\n").map_err(|e| e.to_string())
}

/// Whether a reset is waiting in `data`.
pub fn pending(data: &Path) -> bool {
    data.join(PENDING_FILE).is_file()
}

/// At launch, before anything opens: runs a waiting reset. `None` when
/// none was waiting.
pub fn finish_pending(places: &Places, keystore: &dyn Keystore) -> Option<Erased> {
    if !pending(&places.data) {
        return None;
    }
    let erased = erase(places, keystore);
    // Never run twice for one confirmation, even if the data stayed.
    let _ = std::fs::remove_file(places.data.join(PENDING_FILE));
    Some(erased)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_keeps_the_top_of_the_disk_and_home() {
        let home = Path::new("/home/sam");
        assert!(
            guard(
                Path::new("/home/sam/.local/share/snds/lockdown"),
                Some(home)
            )
            .is_ok()
        );
        assert!(guard(Path::new("relative/a/b"), Some(home)).is_err());
        assert!(guard(Path::new("/home/sam"), Some(home)).is_err());
        assert!(guard(Path::new("/home"), Some(home)).is_err());
        assert!(guard(Path::new("/a/b"), None).is_err());
        assert!(guard(Path::new("/a/b/../c/d"), None).is_err());
        assert!(guard(Path::new("/srv/data/lockdown"), Some(home)).is_ok());
    }

    #[test]
    fn the_memory_keystore_lists_one_service() {
        let k = MemoryKeystore::default();
        k.put(SERVICE, "session:sam@https://a.test", "t");
        k.put(SERVICE, "quick-unlock:x", "q");
        k.put("design.snds.mailbird", "session:sam", "m");
        assert_eq!(
            k.list(SERVICE).unwrap(),
            vec!["quick-unlock:x", "session:sam@https://a.test"]
        );
        k.delete(SERVICE, "quick-unlock:x").unwrap();
        assert_eq!(k.all().len(), 2);
    }

    #[test]
    fn no_identity_is_nothing_to_do() {
        let dir = tempfile::tempdir().unwrap();
        let p = identity_plan(&dir.path().join("snds/identity"));
        assert!(!p.erase);
        assert!(p.apps.is_empty());
    }
}
