//! Where Lockdown keeps its files on this computer
//! (docs/plan/41-mac-paths.md). This crate is the one place that decides:
//! the app (`lockdown_app::paths`), the command line, the browser host's
//! endpoint lookup and the developer reset all ask it, and none of them
//! asks `ecosystem_identity::default_root()` itself.
//!
//! ```text
//!            identity root                                  data directory
//! Linux      $XDG_DATA_HOME/snds/identity                   …/snds/lockdown        (D15, D47)
//! Windows    %LOCALAPPDATA%\snds\identity                   …\snds\lockdown        (D15, D47)
//! macOS      ~/Library/Application Support/snds/identity    …/snds/lockdown        (every build today)
//! macOS,     ~/Library/Group Containers/<TEAM>.design.snds.identity
//!  signed                                                   ~/Library/Application Support/snds/lockdown
//! ```
//!
//! Since macOS 15, `~/Library/Group Containers` is protected: a program
//! may use only the group containers its signature's
//! `com.apple.security.application-groups` entitlement names. An ad-hoc
//! signed build (D450) can't carry a usable one, so it is refused there
//! with "Operation not permitted". The identity goes to a group container
//! only when the running program is signed by a Developer ID whose team
//! prefixes the group it names ([`MacStore::from_signature`]); no build is
//! today, so Application Support always applies.
//!
//! - [`default_dirs`]: this computer's places, decided once.
//! - [`dirs_for`]: the same rule over a given platform, environment and
//!   signature, so every platform's answer is tested on any one.
//! - [`settle`]: the one-time move of an older Mac build's folders out of
//!   Group Containers.
//! - [`make_dir`] and [`cant_make`]: a folder made, or a plain sentence
//!   naming it and a fix when it can't be.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod settle;

pub use settle::{MARKER, Settled, settle, settle_from};

/// The shared folder every ecosystem app keeps its files under, beside
/// each other (`snds/identity`, `snds/lockdown`; D15).
pub const SNDS: &str = "snds";
/// The identity's folder under [`SNDS`].
pub const IDENTITY: &str = "identity";
/// Lockdown's own folder under [`SNDS`], beside the identity (D47).
pub const LOCKDOWN: &str = "lockdown";
/// What a signed Mac build's app group ends with, after its team prefix.
pub const APP_GROUP_SUFFIX: &str = "design.snds.identity";

/// An identity root and Lockdown's data directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dirs {
    /// The ecosystem identity root.
    pub root: PathBuf,
    /// Lockdown's data directory.
    pub data: PathBuf,
}

/// The platforms whose places differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    Mac,
    /// Windows.
    Windows,
    /// Linux and every other desktop Unix (the XDG places).
    Linux,
    /// iOS and Android: the shell passes the container's path.
    Mobile,
}

impl Os {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(any(target_os = "ios", target_os = "android")) {
            Os::Mobile
        } else {
            Os::Linux
        }
    }
}

/// Where the identity lives on a Mac.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacStore {
    /// `~/Library/Application Support/snds/identity`: every build until
    /// one is signed with a Developer ID that carries the app group.
    ApplicationSupport,
    /// `~/Library/Group Containers/<group>`: the running program's
    /// Developer ID signature names this group (`<TEAM>.design.snds.identity`).
    GroupContainer(String),
}

impl MacStore {
    /// The store a signature allows: a group container only when the
    /// program is validly signed by a Developer ID (`signed`, not ad hoc),
    /// has a team, and names the group `<team>.design.snds.identity`.
    /// Anything else, Application Support.
    pub fn from_signature(signed: bool, team: Option<&str>, groups: &[String]) -> Self {
        let Some(team) = team.filter(|t| signed && !t.is_empty()) else {
            return MacStore::ApplicationSupport;
        };
        let wanted = format!("{team}.{APP_GROUP_SUFFIX}");
        match groups.iter().find(|g| **g == wanted) {
            Some(group) => MacStore::GroupContainer(group.clone()),
            None => MacStore::ApplicationSupport,
        }
    }
}

/// What this program's own signature says, and the store it leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacCheck {
    /// Validly signed with a certificate (not ad hoc, not unsigned).
    pub signed: bool,
    /// The signature's team, when it has one.
    pub team: Option<String>,
    /// The app groups the signature names.
    pub groups: Vec<String>,
    /// Why the signature couldn't be read, when it couldn't.
    pub problem: Option<String>,
    /// The store this leads to.
    pub store: MacStore,
}

/// This program's signature, read once (macOS). Elsewhere: unsigned, no
/// groups, Application Support.
pub fn mac_check() -> &'static MacCheck {
    static CHECK: OnceLock<MacCheck> = OnceLock::new();
    CHECK.get_or_init(read_mac_check)
}

#[cfg(target_os = "macos")]
fn read_mac_check() -> MacCheck {
    use lockdown_codesign::SignatureKind;
    let mut problem = None;
    let (signed, team) = match lockdown_codesign::macos::own() {
        Ok(sig) => (sig.kind == SignatureKind::Signed && sig.valid, sig.team),
        Err(e) => {
            problem = Some(e);
            (false, None)
        }
    };
    let groups = match lockdown_codesign::macos::own_app_groups() {
        Ok(g) => g,
        Err(e) => {
            problem.get_or_insert(e);
            Vec::new()
        }
    };
    let store = MacStore::from_signature(signed, team.as_deref(), &groups);
    MacCheck {
        signed,
        team,
        groups,
        problem,
        store,
    }
}

#[cfg(not(target_os = "macos"))]
fn read_mac_check() -> MacCheck {
    MacCheck {
        signed: false,
        team: None,
        groups: Vec::new(),
        problem: None,
        store: MacStore::ApplicationSupport,
    }
}

/// This computer's places: [`dirs_for`] over this platform, this
/// process's environment and, on a Mac, this program's signature.
pub fn default_dirs() -> Result<Dirs, String> {
    dirs_for(
        Os::current(),
        &|name| std::env::var_os(name),
        &mac_check().store,
    )
}

/// The places on `os`, with `env` for the environment (an empty value
/// counts as unset) and `mac` for where a Mac keeps the identity.
pub fn dirs_for(
    os: Os,
    env: &dyn Fn(&str) -> Option<OsString>,
    mac: &MacStore,
) -> Result<Dirs, String> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = || {
        var("HOME").ok_or_else(|| {
            "No home folder is set (HOME), so Lockdown doesn't know where to keep its files."
                .to_string()
        })
    };
    let shared = |base: PathBuf| {
        let snds = base.join(SNDS);
        Dirs {
            root: snds.join(IDENTITY),
            data: snds.join(LOCKDOWN),
        }
    };
    match os {
        Os::Mac => {
            let library = home()?.join("Library");
            let support = shared(library.join("Application Support"));
            Ok(match mac {
                MacStore::ApplicationSupport => support,
                MacStore::GroupContainer(group) => Dirs {
                    root: library.join("Group Containers").join(group),
                    data: support.data,
                },
            })
        }
        Os::Windows => {
            let local = var("LOCALAPPDATA").ok_or_else(|| {
                "No local app data folder is set (%LOCALAPPDATA%), so Lockdown doesn't know where to keep its files.".to_string()
            })?;
            Ok(shared(local))
        }
        Os::Linux => {
            let base = match var("XDG_DATA_HOME") {
                Some(base) => base,
                None => home()?.join(".local").join("share"),
            };
            Ok(shared(base))
        }
        Os::Mobile => Err("On this platform the app passes its container's path.".into()),
    }
}

/// Where an older Mac build kept things: the identity in the app group
/// `group.design.snds.identity` and Lockdown's data beside it, both under
/// `~/Library/Group Containers` (D15, D47 before doc 41).
pub fn old_mac_dirs(home: &Path) -> Dirs {
    let containers = home.join("Library").join("Group Containers");
    Dirs {
        root: containers.join(ecosystem_identity::store::APPLE_APP_GROUP),
        data: containers.join(LOCKDOWN),
    }
}

/// The `lockdown` directory next to an identity root (D47): with a root
/// given alone, Lockdown's data goes beside it.
pub fn beside(root: &Path) -> PathBuf {
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(LOCKDOWN),
        _ => root.join(LOCKDOWN),
    }
}

/// Makes `dir` and every folder above it that is missing, or says in
/// plain words why it couldn't ([`cant_make`]).
pub fn make_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| cant_make(dir, &e))
}

/// The sentence for a folder Lockdown couldn't make: which folder, what
/// stood in the way and what to do, never a bare `os error`.
pub fn cant_make(dir: &Path, e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    let shown = dir.display();
    let above = dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(dir)
        .display();
    let system = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "the system"
    };
    let why = match e.kind() {
        ErrorKind::PermissionDenied => format!(
            "{system} didn't allow it. Check that your user account can write to {above}, then try again."
        ),
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => {
            "the disk is full. Free some space, then try again.".to_string()
        }
        ErrorKind::ReadOnlyFilesystem => {
            "the disk it's on can only be read. Use a disk Lockdown can write to, then try again."
                .to_string()
        }
        ErrorKind::NotADirectory | ErrorKind::AlreadyExists => format!(
            "a file is in the way where a folder should be, in {above}. Move that file aside, then try again."
        ),
        kind => format!(
            "{system} said \"{kind}\". Check that the disk is connected and that your user account can write to {above}, then try again."
        ),
    };
    format!("Lockdown couldn't create its folder {shown}: {why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(*v))
        }
    }

    #[test]
    fn a_mac_keeps_both_in_application_support() {
        let d = dirs_for(
            Os::Mac,
            &env(&[("HOME", "/Users/sam")]),
            &MacStore::ApplicationSupport,
        )
        .unwrap();
        assert_eq!(
            d.root,
            Path::new("/Users/sam/Library/Application Support/snds/identity")
        );
        assert_eq!(
            d.data,
            Path::new("/Users/sam/Library/Application Support/snds/lockdown")
        );
        assert!(!d.root.to_string_lossy().contains("Group Containers"));
        assert!(!d.data.to_string_lossy().contains("Group Containers"));
        assert_eq!(beside(&d.root), d.data);
    }

    #[test]
    fn a_signed_mac_build_keeps_the_identity_in_its_group_and_the_data_in_application_support() {
        let store = MacStore::GroupContainer("ABCDE12345.design.snds.identity".into());
        let d = dirs_for(Os::Mac, &env(&[("HOME", "/Users/sam")]), &store).unwrap();
        assert_eq!(
            d.root,
            Path::new("/Users/sam/Library/Group Containers/ABCDE12345.design.snds.identity")
        );
        assert_eq!(
            d.data,
            Path::new("/Users/sam/Library/Application Support/snds/lockdown")
        );
    }

    #[test]
    fn only_a_developer_id_signature_naming_its_own_team_s_group_leads_to_group_containers() {
        let group = vec!["ABCDE12345.design.snds.identity".to_string()];
        assert_eq!(
            MacStore::from_signature(true, Some("ABCDE12345"), &group),
            MacStore::GroupContainer("ABCDE12345.design.snds.identity".into())
        );
        // Ad hoc (or an invalid signature): never, whatever it names.
        assert_eq!(
            MacStore::from_signature(false, Some("ABCDE12345"), &group),
            MacStore::ApplicationSupport
        );
        // No team, another team's group, the old unprefixed group, none.
        assert_eq!(
            MacStore::from_signature(true, None, &group),
            MacStore::ApplicationSupport
        );
        assert_eq!(
            MacStore::from_signature(true, Some("ZZZZZ99999"), &group),
            MacStore::ApplicationSupport
        );
        assert_eq!(
            MacStore::from_signature(
                true,
                Some("ABCDE12345"),
                &["group.design.snds.identity".to_string()]
            ),
            MacStore::ApplicationSupport
        );
        assert_eq!(
            MacStore::from_signature(true, Some("ABCDE12345"), &[]),
            MacStore::ApplicationSupport
        );
    }

    #[test]
    fn this_build_is_not_signed_with_a_group() {
        // Tests run unsigned or linker-signed ad hoc: Application Support.
        assert_eq!(mac_check().store, MacStore::ApplicationSupport);
        assert!(!mac_check().signed);
    }

    #[test]
    fn windows_and_linux_keep_their_places() {
        let w = dirs_for(
            Os::Windows,
            &env(&[("LOCALAPPDATA", "C:/Users/sam/AppData/Local")]),
            &MacStore::ApplicationSupport,
        )
        .unwrap();
        let local = PathBuf::from("C:/Users/sam/AppData/Local");
        assert_eq!(w.root, local.join("snds").join("identity"));
        assert_eq!(w.data, local.join("snds").join("lockdown"));

        let xdg = dirs_for(
            Os::Linux,
            &env(&[("XDG_DATA_HOME", "/data"), ("HOME", "/home/sam")]),
            &MacStore::ApplicationSupport,
        )
        .unwrap();
        assert_eq!(xdg.root, Path::new("/data/snds/identity"));
        assert_eq!(xdg.data, Path::new("/data/snds/lockdown"));

        let plain = dirs_for(
            Os::Linux,
            &env(&[("XDG_DATA_HOME", ""), ("HOME", "/home/sam")]),
            &MacStore::ApplicationSupport,
        )
        .unwrap();
        assert_eq!(
            plain.root,
            Path::new("/home/sam/.local/share/snds/identity")
        );
        assert_eq!(
            plain.data,
            Path::new("/home/sam/.local/share/snds/lockdown")
        );
    }

    #[test]
    fn off_the_mac_the_places_are_the_identity_crate_s_own() {
        // Windows and Linux are unchanged by doc 41: the identity root is
        // still `ecosystem_identity::default_root()` and the data dir is
        // beside it, on the machine the tests run on.
        if Os::current() == Os::Mac {
            return;
        }
        let ours = default_dirs().unwrap();
        let theirs = ecosystem_identity::default_root().unwrap();
        assert_eq!(ours.root, theirs);
        assert_eq!(ours.data, beside(&theirs));
    }

    #[test]
    fn no_home_or_a_phone_is_said_in_words() {
        let none = |_: &str| None;
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            let e = dirs_for(os, &none, &MacStore::ApplicationSupport).unwrap_err();
            assert!(e.contains("doesn't know where to keep its files"), "{e}");
        }
        assert!(dirs_for(Os::Mobile, &none, &MacStore::ApplicationSupport).is_err());
    }

    #[test]
    fn the_old_mac_places_are_the_group_containers_ones() {
        let old = old_mac_dirs(Path::new("/Users/sam"));
        assert_eq!(
            old.root,
            Path::new("/Users/sam/Library/Group Containers/group.design.snds.identity")
        );
        assert_eq!(
            old.data,
            Path::new("/Users/sam/Library/Group Containers/lockdown")
        );
        // The old rule: the data beside the root.
        assert_eq!(beside(&old.root), old.data);
    }

    #[test]
    fn a_folder_that_can_t_be_made_is_named_with_a_fix() {
        use std::io::{Error, ErrorKind};
        let dir = Path::new("/Users/sam/Library/Application Support/snds/lockdown");
        let denied = cant_make(dir, &Error::from(ErrorKind::PermissionDenied));
        assert!(denied.starts_with(&format!(
            "Lockdown couldn't create its folder {}",
            dir.display()
        )));
        assert!(denied.contains("didn't allow it"), "{denied}");
        assert!(
            denied.contains("/Users/sam/Library/Application Support/snds"),
            "{denied}"
        );
        assert!(!denied.contains("os error"), "{denied}");
        let full = cant_make(dir, &Error::from(ErrorKind::StorageFull));
        assert!(full.contains("the disk is full"), "{full}");
        let file = cant_make(dir, &Error::from(ErrorKind::NotADirectory));
        assert!(file.contains("a file is in the way"), "{file}");
        let other = cant_make(dir, &Error::from(ErrorKind::TimedOut));
        assert!(other.contains("then try again"), "{other}");
        assert!(!other.contains("os error"), "{other}");
    }

    #[test]
    fn make_dir_makes_it_or_names_it() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a/b/c");
        make_dir(&deep).unwrap();
        assert!(deep.is_dir());
        let file = tmp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let e = make_dir(&file.join("inside")).unwrap_err();
        assert!(e.starts_with("Lockdown couldn't create its folder"), "{e}");
        assert!(!e.contains("os error"), "{e}");
    }
}
