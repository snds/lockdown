//! The one-time move out of `~/Library/Group Containers`
//! (docs/plan/41-mac-paths.md §2.3).
//!
//! Builds before doc 41 kept the identity in
//! `~/Library/Group Containers/group.design.snds.identity` and Lockdown's
//! data beside it in `~/Library/Group Containers/lockdown`. On the first
//! start of a build that keeps them in Application Support:
//!
//! - **The new places already hold something**: nothing is read from the
//!   old ones.
//! - **The old places can be read and hold something**: each is copied
//!   in full to a `.moving` folder beside its new place, then both are
//!   renamed into place together. The identity's old copy stays (other
//!   ecosystem apps may still look there); Lockdown's own old data folder
//!   is removed when macOS allows it.
//! - **An old place can't be read** (macOS refused, or something that
//!   isn't a folder is there): nothing is copied, the old copy stays where
//!   it is, untouched, and start-up goes on as on a computer with nothing
//!   on it.
//! - **A copy fails part way** (a full disk, say): what was copied is
//!   removed, the new places are as they were, and the old ones untouched.
//!
//! Whatever happened, a note ([`MARKER`]) beside the data directory says
//! so, and its presence means the old places are never looked at again:
//! macOS may ask the person once whether Lockdown may read them, never at
//! every start, and a developer reset doesn't bring the old copy back.
//! Removing the note asks for one more look at the next start.

use std::io;
use std::path::{Path, PathBuf};

use crate::{Dirs, MacStore, Os, default_dirs, mac_check, make_dir, old_mac_dirs};

/// The note left beside Lockdown's data directory once the old places
/// have been looked at (`…/snds/lockdown-moved-from-group-containers.txt`).
pub const MARKER: &str = "lockdown-moved-from-group-containers.txt";

/// What [`settle`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settled {
    /// Not a Mac keeping the identity in Application Support, or no home
    /// folder: there is nothing older to look for.
    NotHere,
    /// Looked at before (the note is there).
    Checked,
    /// The new places already hold something, so the old ones weren't read.
    InUse,
    /// Nothing at the old places.
    NothingOld,
    /// An old place couldn't be read, so nothing was copied (why, in words).
    Unreadable(String),
    /// Copied over.
    Moved {
        /// The identity was copied (its old copy stays).
        identity: bool,
        /// Lockdown's data was copied.
        data: bool,
        /// Why the old data folder is still there, when it couldn't be removed.
        old_data_left: Option<String>,
    },
    /// The copy failed part way and was undone (why, in words).
    Failed(String),
}

impl Settled {
    /// A line for the log when something worth saying happened.
    pub fn describe(&self, old: &Dirs, new: &Dirs) -> Option<String> {
        match self {
            Settled::NotHere | Settled::Checked | Settled::InUse | Settled::NothingOld => None,
            Settled::Unreadable(why) => Some(format!(
                "an older copy in Group Containers couldn't be read ({why}); it was left where it is"
            )),
            Settled::Moved {
                identity,
                data,
                old_data_left,
            } => {
                let mut said = Vec::new();
                if *identity {
                    said.push(format!(
                        "copied the identity from {} to {}",
                        old.root.display(),
                        new.root.display()
                    ));
                }
                if *data {
                    said.push(format!(
                        "moved Lockdown's data from {} to {}",
                        old.data.display(),
                        new.data.display()
                    ));
                }
                if let Some(why) = old_data_left {
                    said.push(format!("the old data folder stayed ({why})"));
                }
                Some(said.join("; "))
            }
            Settled::Failed(why) => Some(format!(
                "couldn't move the older copy out of Group Containers ({why}); it was left where it is"
            )),
        }
    }
}

/// The one-time move on this computer: on a Mac keeping the identity in
/// Application Support, from [`old_mac_dirs`] to [`default_dirs`]. Never
/// fails; the caller may log [`Settled::describe`]. Call it at start,
/// before anything opens, and only when the default places are in use (an
/// explicit `--root`, `--data`, `LOCKDOWN_ROOT` or `LOCKDOWN_DATA` means
/// the person chose the places).
pub fn settle() -> Settled {
    if Os::current() != Os::Mac || mac_check().store != MacStore::ApplicationSupport {
        return Settled::NotHere;
    }
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return Settled::NotHere;
    };
    let Ok(new) = default_dirs() else {
        return Settled::NotHere;
    };
    settle_from(&old_mac_dirs(Path::new(&home)), &new)
}

/// The move from `old` to `new`, by the rules at the top of this module.
pub fn settle_from(old: &Dirs, new: &Dirs) -> Settled {
    let marker = marker_for(new);
    if std::fs::symlink_metadata(&marker).is_ok() {
        return Settled::Checked;
    }
    let done = move_once(old, new);
    let note = match &done {
        Settled::Moved { .. } => done
            .describe(old, new)
            .unwrap_or_else(|| "moved".to_string()),
        Settled::InUse => "the new places were in use; the old ones weren't read".to_string(),
        Settled::NothingOld => "nothing was in the old places".to_string(),
        other => other.describe(old, new).unwrap_or_default(),
    };
    // A note that can't be written only means one more look next time.
    if let Some(dir) = marker.parent()
        && make_dir(dir).is_ok()
    {
        let _ = std::fs::write(&marker, format!("{note}\n"));
    }
    done
}

/// Where the note goes: beside the data directory.
fn marker_for(new: &Dirs) -> PathBuf {
    match new.data.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(MARKER),
        _ => new.data.join(MARKER),
    }
}

/// What a place holds.
enum Place {
    Absent,
    Empty,
    Full,
    Unreadable(String),
}

fn look(path: &Path) -> Place {
    match std::fs::read_dir(path) {
        Ok(mut entries) => match entries.next() {
            None => Place::Empty,
            Some(_) => Place::Full,
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Place::Absent,
        Err(e) => Place::Unreadable(format!("{}: {}", path.display(), e.kind())),
    }
}

fn move_once(old: &Dirs, new: &Dirs) -> Settled {
    for path in [&new.root, &new.data] {
        if matches!(look(path), Place::Full | Place::Unreadable(_)) {
            return Settled::InUse;
        }
    }
    let mut wanted: Vec<(&Path, &Path)> = Vec::new();
    for (from, to) in [(&old.root, &new.root), (&old.data, &new.data)] {
        match look(from) {
            Place::Absent | Place::Empty => {}
            Place::Full => wanted.push((from.as_path(), to.as_path())),
            // Half of the pair would strand the other half (a device key
            // with no identity reads as "not enrolled"), so nothing moves.
            Place::Unreadable(why) => return Settled::Unreadable(why),
        }
    }
    if wanted.is_empty() {
        return Settled::NothingOld;
    }

    // Copy each to a `.moving` folder beside its new place.
    let mut staged: Vec<(PathBuf, &Path)> = Vec::new();
    for &(from, to) in &wanted {
        let staging = moving(to);
        let copied = (|| {
            remove_if_there(&staging)?;
            if let Some(parent) = staging.parent() {
                make_dir(parent).map_err(io::Error::other)?;
            }
            copy_tree(from, &staging)
        })();
        staged.push((staging, to));
        if let Err(e) = copied {
            undo(&staged, &[]);
            return Settled::Failed(format!("{}: {e}", from.display()));
        }
    }

    // Then rename each into place; a failure takes back what moved.
    let mut placed: Vec<&Path> = Vec::new();
    for (staging, to) in &staged {
        let put = (|| {
            if look(to).is_empty_dir() {
                std::fs::remove_dir(to)?;
            }
            std::fs::rename(staging, to)
        })();
        if let Err(e) = put {
            undo(&staged, &placed);
            return Settled::Failed(format!("{}: {e}", to.display()));
        }
        placed.push(*to);
    }

    let identity = wanted.iter().any(|(from, _)| *from == old.root.as_path());
    let data = wanted.iter().any(|(from, _)| *from == old.data.as_path());
    let old_data_left = if data {
        std::fs::remove_dir_all(&old.data)
            .err()
            .map(|e| format!("{}: {}", old.data.display(), e.kind()))
    } else {
        None
    };
    Settled::Moved {
        identity,
        data,
        old_data_left,
    }
}

impl Place {
    fn is_empty_dir(&self) -> bool {
        matches!(self, Place::Empty)
    }
}

/// `<to>.moving`, beside `to`.
fn moving(to: &Path) -> PathBuf {
    let mut name = to.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".moving");
    to.with_file_name(name)
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Removes every staging folder, and every copy already renamed into
/// place (`placed`), so the new places are as they were.
fn undo(staged: &[(PathBuf, &Path)], placed: &[&Path]) {
    for (staging, _) in staged {
        let _ = remove_if_there(staging);
    }
    for to in placed {
        let _ = remove_if_there(to);
    }
}

/// Copies the folders and plain files under `from` into a new folder
/// `to`, in name order. Links, sockets and the like are left: they aren't
/// data, and the app makes its socket again at start.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    private_dir(to)?;
    let mut entries = std::fs::read_dir(from)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            // Keeps the file's permissions (0600 for the keys).
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// A folder only this user may enter.
fn private_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        _tmp: tempfile::TempDir,
        old: Dirs,
        new: Dirs,
    }

    fn world() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let old = old_mac_dirs(&home);
        let support = home.join("Library/Application Support/snds");
        let new = Dirs {
            root: support.join("identity"),
            data: support.join("lockdown"),
        };
        World {
            _tmp: tmp,
            old,
            new,
        }
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn an_old_install(w: &World) {
        put(&w.old.root.join("FORMAT"), b"ecosystem-identity/1");
        put(&w.old.root.join("personas/p1.cbor"), b"persona");
        put(&w.old.data.join("device-key.sealed.cbor"), b"sealed keys");
        put(&w.old.data.join("local-server/store.sqlite"), b"server");
    }

    #[test]
    fn a_readable_old_install_moves_into_an_empty_new_place_once() {
        let w = world();
        an_old_install(&w);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key = w.old.data.join("device-key.sealed.cbor");
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        // An empty data folder made by an earlier start counts as empty.
        std::fs::create_dir_all(&w.new.data).unwrap();

        let done = settle_from(&w.old, &w.new);
        assert_eq!(
            done,
            Settled::Moved {
                identity: true,
                data: true,
                old_data_left: None
            }
        );
        let read = |p: PathBuf| std::fs::read(p).unwrap();
        assert_eq!(read(w.new.root.join("FORMAT")), b"ecosystem-identity/1");
        assert_eq!(read(w.new.root.join("personas/p1.cbor")), b"persona");
        assert_eq!(
            read(w.new.data.join("device-key.sealed.cbor")),
            b"sealed keys"
        );
        assert_eq!(
            read(w.new.data.join("local-server/store.sqlite")),
            b"server"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(w.new.data.join("device-key.sealed.cbor")), 0o600);
        }
        // The identity's old copy stays for any other app; Lockdown's own
        // old folder is gone; no staging folder is left.
        assert!(w.old.root.join("FORMAT").exists());
        assert!(!w.old.data.exists());
        let parent = w.new.data.parent().unwrap();
        assert!(!parent.join("lockdown.moving").exists());
        assert!(!parent.join("identity.moving").exists());
        let note = std::fs::read_to_string(parent.join(MARKER)).unwrap();
        assert!(note.contains("moved Lockdown's data"), "{note}");
        let said = done.describe(&w.old, &w.new).unwrap();
        assert!(said.contains("copied the identity"), "{said}");

        // Once only: a reset that empties the new place doesn't bring the
        // old copy back.
        put(&w.old.data.join("device-key.sealed.cbor"), b"left over");
        std::fs::remove_dir_all(&w.new.data).unwrap();
        std::fs::remove_dir_all(&w.new.root).unwrap();
        assert_eq!(settle_from(&w.old, &w.new), Settled::Checked);
        assert!(!w.new.data.exists());
    }

    #[test]
    fn a_new_place_in_use_is_left_and_the_old_one_not_read() {
        let w = world();
        an_old_install(&w);
        put(&w.new.data.join("onboarding.cbor"), b"new");
        assert_eq!(settle_from(&w.old, &w.new), Settled::InUse);
        assert!(!w.new.root.exists());
        assert!(w.old.data.join("device-key.sealed.cbor").exists());
        assert!(w.new.data.parent().unwrap().join(MARKER).exists());
    }

    #[test]
    fn an_unreadable_old_place_is_ignored_and_nothing_half_moves() {
        let w = world();
        an_old_install(&w);
        // A file where the old data folder should be reads as unreadable
        // on every platform (running as root, permissions wouldn't).
        std::fs::remove_dir_all(&w.old.data).unwrap();
        put(&w.old.data, b"not a folder");
        match settle_from(&w.old, &w.new) {
            Settled::Unreadable(why) => assert!(why.contains("lockdown"), "{why}"),
            other => panic!("{other:?}"),
        }
        // Neither half moved, the old copies are untouched, start-up goes
        // on with nothing here, and the old place isn't read again.
        assert!(!w.new.root.exists());
        assert!(!w.new.data.exists());
        assert!(w.old.root.join("FORMAT").exists());
        assert_eq!(std::fs::read(&w.old.data).unwrap(), b"not a folder");
        assert_eq!(settle_from(&w.old, &w.new), Settled::Checked);
    }

    #[test]
    fn nothing_old_is_nothing_to_do() {
        let w = world();
        assert_eq!(settle_from(&w.old, &w.new), Settled::NothingOld);
        assert!(!w.new.root.exists());
        assert!(!w.new.data.exists());
        assert_eq!(settle_from(&w.old, &w.new), Settled::Checked);
    }

    #[test]
    fn only_the_data_folder_moves_when_there_is_no_old_identity() {
        let w = world();
        put(&w.old.data.join("vault.sqlite"), b"vault");
        assert_eq!(
            settle_from(&w.old, &w.new),
            Settled::Moved {
                identity: false,
                data: true,
                old_data_left: None
            }
        );
        assert_eq!(
            std::fs::read(w.new.data.join("vault.sqlite")).unwrap(),
            b"vault"
        );
        assert!(!w.new.root.exists());
    }

    #[test]
    fn a_copy_that_fails_leaves_both_places_as_they_were() {
        let w = world();
        an_old_install(&w);
        // A file where the data's staging folder must go: the identity is
        // staged, then the data can't be, and the identity's copy goes too.
        let blocker = moving(&w.new.data);
        put(&blocker, b"in the way");
        match settle_from(&w.old, &w.new) {
            Settled::Failed(why) => assert!(why.contains("lockdown"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert!(!w.new.root.exists());
        assert!(!w.new.data.exists());
        assert!(!moving(&w.new.root).exists());
        assert!(w.old.root.join("FORMAT").exists());
        assert!(w.old.data.join("device-key.sealed.cbor").exists());
        assert_eq!(std::fs::read(&blocker).unwrap(), b"in the way");
    }

    #[cfg(unix)]
    #[test]
    fn links_and_sockets_are_not_copied() {
        let w = world();
        put(&w.old.data.join("vault.sqlite"), b"vault");
        std::os::unix::fs::symlink("/etc/hostname", w.old.data.join("link")).unwrap();
        let _sock =
            std::os::unix::net::UnixListener::bind(w.old.data.join("native-host.sock")).unwrap();
        assert!(matches!(settle_from(&w.old, &w.new), Settled::Moved { .. }));
        assert!(w.new.data.join("vault.sqlite").exists());
        assert!(std::fs::symlink_metadata(w.new.data.join("link")).is_err());
        assert!(std::fs::symlink_metadata(w.new.data.join("native-host.sock")).is_err());
    }

    #[test]
    fn off_a_signed_mac_there_is_nothing_here() {
        if Os::current() != Os::Mac {
            assert_eq!(settle(), Settled::NotHere);
        }
    }
}
