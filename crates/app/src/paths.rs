//! Where the app keeps its files (docs/plan/41-mac-paths.md): the rule in
//! `lockdown-paths`, given as the app's [`Paths`]. The desktop, the
//! developer tools and the command line ask here; the browser host, which
//! links no vault code, asks `lockdown-paths` directly, so all four agree.
//!
//! - [`default_paths`]: this computer's places, with no side effects.
//! - [`at_start`]: what a desktop process uses (`LOCKDOWN_ROOT`,
//!   `LOCKDOWN_DATA`, else the defaults, after the one-time move).
//! - [`settle_defaults`]: the one-time move out of the Mac's Group
//!   Containers, logged when it did something.

use std::path::PathBuf;

pub use lockdown_paths::{
    Dirs, MARKER, MacCheck, MacStore, Os, Settled, beside, cant_make, dirs_for, mac_check,
    make_dir, old_mac_dirs, settle_from,
};

use crate::Paths;

/// This computer's identity root and data directory: D15 and D47 on
/// Windows and Linux, `~/Library/Application Support/snds/` on a Mac
/// (doc 41). Reads the environment and, on a Mac, this program's
/// signature; writes nothing.
pub fn default_paths() -> Result<Paths, String> {
    let dirs = lockdown_paths::default_dirs()?;
    Ok(Paths {
        root: dirs.root,
        data: dirs.data,
    })
}

/// The places named by `LOCKDOWN_ROOT` and `LOCKDOWN_DATA` (a second
/// profile, for testing), else the defaults. An empty value counts as unset.
pub fn from_env() -> Result<Paths, String> {
    let (root, data) = env_overrides();
    Paths::resolve(root, data)
}

/// What a desktop process uses at start: [`from_env`], with the one-time
/// move done first when the defaults are in use.
pub fn at_start() -> Result<Paths, String> {
    let (root, data) = env_overrides();
    if root.is_none() && data.is_none() {
        settle_defaults();
    }
    Paths::resolve(root, data)
}

/// The one-time move out of `~/Library/Group Containers` on a Mac
/// ([`lockdown_paths::settle`]); a line on standard error when it did
/// something. Call it only when the default places are in use. Never
/// fails start-up.
pub fn settle_defaults() -> Settled {
    let settled = lockdown_paths::settle();
    if let (Some(home), Ok(new)) = (
        std::env::var_os("HOME").filter(|h| !h.is_empty()),
        lockdown_paths::default_dirs(),
    ) && let Some(line) = settled.describe(&old_mac_dirs(std::path::Path::new(&home)), &new)
    {
        eprintln!("lockdown: {line}");
    }
    settled
}

fn env_overrides() -> (Option<PathBuf>, Option<PathBuf>) {
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    (env("LOCKDOWN_ROOT"), env("LOCKDOWN_DATA"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_s_defaults_are_the_one_rule_s() {
        let ours = default_paths().unwrap();
        let rule = lockdown_paths::default_dirs().unwrap();
        assert_eq!((ours.root, ours.data), (rule.root, rule.data));
    }

    #[test]
    fn resolve_keeps_its_three_cases() {
        let both = Paths::resolve(Some("/r/identity".into()), Some("/d".into())).unwrap();
        assert_eq!(
            (both.root, both.data),
            (PathBuf::from("/r/identity"), PathBuf::from("/d"))
        );
        let root_only = Paths::resolve(Some("/x/snds/identity".into()), None).unwrap();
        assert_eq!(root_only.data, PathBuf::from("/x/snds/lockdown"));
        let defaults = default_paths().unwrap();
        let data_only = Paths::resolve(None, Some("/d".into())).unwrap();
        assert_eq!(
            (data_only.root, data_only.data),
            (defaults.root.clone(), PathBuf::from("/d"))
        );
        let neither = Paths::resolve(None, None).unwrap();
        assert_eq!((neither.root, neither.data), (defaults.root, defaults.data));
    }

    #[test]
    fn a_mac_never_lands_in_group_containers_today() {
        if Os::current() != Os::Mac {
            return;
        }
        let p = default_paths().unwrap();
        for dir in [&p.root, &p.data] {
            let shown = dir.to_string_lossy();
            assert!(
                shown.contains("Library/Application Support/snds/"),
                "{shown}"
            );
            assert!(!shown.contains("Group Containers"), "{shown}");
        }
    }

    #[cfg(feature = "native")]
    #[test]
    fn the_probe_names_a_folder_it_can_t_make_in_words() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("snds");
        std::fs::write(&file, b"a file where a folder should be").unwrap();
        let paths = Paths::resolve(Some(file.join("identity")), None).unwrap();
        let e = paths.probe().unwrap_err();
        assert!(e.starts_with("Lockdown couldn't create its folder"), "{e}");
        assert!(e.contains("then try again"), "{e}");
        assert!(!e.contains("os error"), "{e}");
    }
}
