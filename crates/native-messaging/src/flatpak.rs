//! Lockdown inside a Flatpak (docs/plan/29-release-polish.md §4).
//!
//! A browser on the host starts its native-messaging host from the path in
//! its manifest, on the host, outside any sandbox. The host program inside
//! the Flatpak (`/app/bin/lockdown-native-host`) isn't there, and a host in
//! a sandbox of its own couldn't be checked by the app: the app sees the
//! connecting process's id through the socket, and a process in another
//! sandbox's process namespace has none it can see. So:
//!
//! ```text
//! browser ── starts ──▶ ~/.var/app/<id>/data/snds/lockdown/lockdown-native-host-flatpak
//!                         └─ flatpak enter <app pid> /app/bin/lockdown-native-host   (app running)
//!                         └─ flatpak run --command=lockdown-native-host <id>     (not running → "no_app")
//! ```
//!
//! The wrapper ([`WRAPPER`]) lives in the app's data folder, which the
//! sandbox and the host see at the same path, and the manifests go where
//! the host's browsers read them, which the Flatpak's `--filesystem`
//! permissions open to the app (`packaging/flatpak/design.snds.lockdown.yml`).
//! The host then runs in the app's own namespaces, so both ends see each
//! other's process and check it by user, path and SHA-256 as everywhere
//! else.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// The wrapper's file name in the data folder.
pub const WRAPPER_NAME: &str = "lockdown-native-host-flatpak";

/// The wrapper, with `@APP_ID@` for the Flatpak's app id.
pub const WRAPPER: &str = include_str!("flatpak-host.sh");

/// The Flatpak app id this process runs as, when it runs inside a Flatpak
/// sandbox: `FLATPAK_ID`, believed only when `/.flatpak-info` (which the
/// sandbox mounts and an app can't make) is there.
pub fn sandbox_app() -> Option<String> {
    if !Path::new("/.flatpak-info").is_file() {
        return None;
    }
    std::env::var("FLATPAK_ID").ok().filter(|id| plain_id(id))
}

/// An app id that is safe to write into the wrapper: reverse-DNS letters,
/// digits, dots, dashes and underscores.
pub fn plain_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// The wrapper's text for `app`.
pub fn wrapper(app: &str) -> Result<String> {
    if !plain_id(app) {
        return Err(Error::BadMessage(format!("{app:?} isn't a Flatpak app id")));
    }
    Ok(WRAPPER.replace("@APP_ID@", app))
}

/// Writes the wrapper into the data folder `data` and answers its path,
/// which the browsers' manifests name. The folder must be the app's own
/// under `<home>/.var/app/<app>/`, the one place the sandbox and the host
/// both see at the same path.
pub fn install_wrapper(data: &Path, app: &str, home: &Path) -> Result<PathBuf> {
    let body = wrapper(app)?;
    let shared = home.join(".var/app").join(app);
    if !data.starts_with(&shared) {
        return Err(Error::NoDataDir(format!(
            "Lockdown's data folder {} isn't inside {}, where a browser outside the Flatpak can reach it",
            data.display(),
            shared.display()
        )));
    }
    let path = data.join(WRAPPER_NAME);
    std::fs::create_dir_all(data)?;
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Run by this user's browser; nobody else needs to read it.
            opts.mode(0o700);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700))?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapper_enters_the_running_app_or_runs_a_new_sandbox() {
        let w = wrapper("design.snds.lockdown").unwrap();
        assert!(w.starts_with("#!/bin/sh\n"));
        assert!(!w.contains("@APP_ID@"));
        assert!(w.contains("app='design.snds.lockdown'"));
        assert!(w.contains(r#"exec "$flatpak" enter "$pid" /app/bin/lockdown-native-host "$@""#));
        assert!(w.contains(r#"grep -qsx "name=$app" "$p/root/.flatpak-info""#));
        assert!(w.contains(r#"exec "$flatpak" run --command=lockdown-native-host "$app" "$@""#));
        for bad in ["", "a'b", "a b", "x;rm -rf ~", "$(id)"] {
            assert!(wrapper(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_wrapper_goes_in_the_shared_data_folder_and_nowhere_else() {
        let home = tempfile::tempdir().unwrap();
        let app = "design.snds.lockdown";
        let data = home
            .path()
            .join(".var/app")
            .join(app)
            .join("data/snds/lockdown");
        let path = install_wrapper(&data, app, home.path()).unwrap();
        assert_eq!(path, data.join(WRAPPER_NAME));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            wrapper(app).unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let elsewhere = home.path().join(".local/share/snds/lockdown");
        assert!(matches!(
            install_wrapper(&elsewhere, app, home.path()),
            Err(Error::NoDataDir(_))
        ));
    }

    #[test]
    fn outside_a_sandbox_there_is_no_flatpak_app() {
        if !Path::new("/.flatpak-info").exists() {
            assert_eq!(sandbox_app(), None);
        }
    }
}
