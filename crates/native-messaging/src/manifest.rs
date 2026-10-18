//! The native-messaging host manifest, and where each browser looks for it.
//! The desktop app writes these (`browser_install`); store packaging and
//! signing are out of scope for now (D122).
//!
//! | Browser | Linux | macOS | Windows |
//! |---|---|---|---|
//! | Chrome | `~/.config/google-chrome/NativeMessagingHosts/` | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` | registry `HKCU\Software\Google\Chrome\NativeMessagingHosts\<name>` → the file |
//! | Chromium | `~/.config/chromium/NativeMessagingHosts/` | `~/Library/Application Support/Chromium/NativeMessagingHosts/` | `HKCU\Software\Chromium\NativeMessagingHosts\<name>` |
//! | Edge | `~/.config/microsoft-edge/NativeMessagingHosts/` | `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/` | `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\<name>` |
//! | Brave | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` | `~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/` | `HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\<name>` |
//! | Firefox | `~/.mozilla/native-messaging-hosts/` | `~/Library/Application Support/Mozilla/NativeMessagingHosts/` | `HKCU\Software\Mozilla\NativeMessagingHosts\<name>` |
//!
//! On Linux the Chromium family reads `$XDG_CONFIG_HOME` in place of
//! `~/.config` when it is set. On Windows the manifest file can live
//! anywhere and the registry key's default value points at it; the files go
//! under `%LOCALAPPDATA%\snds\lockdown\native-messaging\<browser>\` and the
//! keys are returned for the caller to set (this crate has no registry
//! access).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::caller::Family;
use crate::endpoint::write_private;
use crate::{CHROMIUM_IDS, FIREFOX_IDS, HOST_NAME, Result};

/// A browser the desktop can connect to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Browser {
    Chrome,
    Chromium,
    Edge,
    Brave,
    Firefox,
}

impl Browser {
    /// Every browser, in the order a settings screen lists them.
    pub const ALL: [Browser; 5] = [
        Self::Chrome,
        Self::Edge,
        Self::Brave,
        Self::Chromium,
        Self::Firefox,
    ];

    /// Chromium or Firefox.
    pub fn family(self) -> Family {
        match self {
            Self::Firefox => Family::Firefox,
            _ => Family::Chromium,
        }
    }

    /// Its name, for people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Chromium => "Chromium",
            Self::Edge => "Edge",
            Self::Brave => "Brave",
            Self::Firefox => "Firefox",
        }
    }
}

/// An operating system's manifest rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    /// The one this build runs on (anything not macOS or Windows reads as
    /// Linux, which is what the BSDs' browsers follow too).
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The directories manifests are found from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Homes {
    /// The home directory.
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME`, when set (Linux).
    pub config_home: Option<PathBuf>,
    /// `%LOCALAPPDATA%` (Windows).
    pub local_app_data: Option<PathBuf>,
}

impl Homes {
    /// From this process's environment.
    pub fn from_env() -> Option<Self> {
        let var = |n: &str| {
            std::env::var_os(n)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var("HOME").or_else(|| var("USERPROFILE"))?;
        Some(Self {
            home,
            config_home: var("XDG_CONFIG_HOME"),
            local_app_data: var("LOCALAPPDATA"),
        })
    }
}

/// Where one browser's manifest goes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Target {
    pub browser: Browser,
    /// The manifest file.
    pub manifest: PathBuf,
    /// On Windows, the registry key whose default value must name
    /// `manifest`; `None` elsewhere.
    pub registry_key: Option<String>,
}

/// The manifest's JSON for `browser`, naming the host at `host_path`.
pub fn manifest(browser: Browser, host_path: &Path) -> Value {
    let base = json!({
        "name": HOST_NAME,
        "description": "Lockdown: connects the browser extension to the Lockdown app on this computer",
        "path": host_path.display().to_string(),
        "type": "stdio",
    });
    let mut m = base;
    match browser.family() {
        Family::Chromium => {
            let origins: Vec<String> = CHROMIUM_IDS
                .iter()
                .map(|id| format!("chrome-extension://{id}/"))
                .collect();
            m["allowed_origins"] = json!(origins);
        }
        Family::Firefox => {
            m["allowed_extensions"] = json!(FIREFOX_IDS);
        }
    }
    m
}

/// The browser's own folder (the one that exists when it is installed and
/// has run once), whose `NativeMessagingHosts` child holds manifests.
fn profile_dir(os: Os, browser: Browser, homes: &Homes) -> PathBuf {
    let home = &homes.home;
    match os {
        Os::Linux => {
            let config = homes
                .config_home
                .clone()
                .unwrap_or_else(|| home.join(".config"));
            match browser {
                Browser::Chrome => config.join("google-chrome"),
                Browser::Chromium => config.join("chromium"),
                Browser::Edge => config.join("microsoft-edge"),
                Browser::Brave => config.join("BraveSoftware/Brave-Browser"),
                Browser::Firefox => home.join(".mozilla"),
            }
        }
        Os::MacOs => {
            let support = home.join("Library/Application Support");
            match browser {
                Browser::Chrome => support.join("Google/Chrome"),
                Browser::Chromium => support.join("Chromium"),
                Browser::Edge => support.join("Microsoft Edge"),
                Browser::Brave => support.join("BraveSoftware/Brave-Browser"),
                Browser::Firefox => support.join("Mozilla"),
            }
        }
        Os::Windows => {
            let local = homes
                .local_app_data
                .clone()
                .unwrap_or_else(|| home.join("AppData/Local"));
            local
                .join("snds/lockdown/native-messaging")
                .join(browser.name().to_ascii_lowercase())
        }
    }
}

/// Where `browser`'s manifest goes on `os`.
pub fn target(os: Os, browser: Browser, homes: &Homes) -> Target {
    let dir = profile_dir(os, browser, homes);
    let file = format!("{HOST_NAME}.json");
    let (manifest, registry_key) = match os {
        Os::Linux if browser == Browser::Firefox => {
            (dir.join("native-messaging-hosts").join(file), None)
        }
        Os::Linux | Os::MacOs => (dir.join("NativeMessagingHosts").join(file), None),
        Os::Windows => {
            let key = match browser {
                Browser::Chrome => r"Software\Google\Chrome",
                Browser::Chromium => r"Software\Chromium",
                Browser::Edge => r"Software\Microsoft\Edge",
                Browser::Brave => r"Software\BraveSoftware\Brave-Browser",
                Browser::Firefox => r"Software\Mozilla",
            };
            (
                dir.join(file),
                Some(format!(
                    r"HKEY_CURRENT_USER\{key}\NativeMessagingHosts\{HOST_NAME}"
                )),
            )
        }
    };
    Target {
        browser,
        manifest,
        registry_key,
    }
}

/// Whether `browser` looks installed for this user: its own folder exists.
/// Always true on Windows, where the files live under Lockdown's folder.
pub fn detected(os: Os, browser: Browser, homes: &Homes) -> bool {
    os == Os::Windows || profile_dir(os, browser, homes).is_dir()
}

/// Writes `browser`'s manifest naming the host at `host_path`, and returns
/// where it went.
pub fn install(os: Os, browser: Browser, homes: &Homes, host_path: &Path) -> Result<Target> {
    let t = target(os, browser, homes);
    let mut body = serde_json::to_vec_pretty(&manifest(browser, host_path))
        .map_err(|e| crate::Error::BadMessage(e.to_string()))?;
    body.push(b'\n');
    write_private(&t.manifest, &body)?;
    Ok(t)
}

/// Removes `browser`'s manifest, if one is there.
pub fn uninstall(os: Os, browser: Browser, homes: &Homes) -> Result<()> {
    match std::fs::remove_file(target(os, browser, homes).manifest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEV_CHROMIUM_ID, FIREFOX_ID};

    fn homes(home: &Path) -> Homes {
        Homes {
            home: home.to_path_buf(),
            config_home: None,
            local_app_data: None,
        }
    }

    #[test]
    fn manifests_name_the_host_and_the_extension_ids() {
        let host = Path::new("/opt/lockdown/lockdown-native-host");
        let chrome = manifest(Browser::Chrome, host);
        assert_eq!(chrome["name"], HOST_NAME);
        assert_eq!(chrome["type"], "stdio");
        assert_eq!(chrome["path"], "/opt/lockdown/lockdown-native-host");
        assert_eq!(
            chrome["allowed_origins"][0],
            format!("chrome-extension://{DEV_CHROMIUM_ID}/")
        );
        assert!(chrome.get("allowed_extensions").is_none());
        let firefox = manifest(Browser::Firefox, host);
        assert_eq!(firefox["allowed_extensions"][0], FIREFOX_ID);
        assert!(firefox.get("allowed_origins").is_none());
    }

    #[test]
    fn every_browser_has_its_place_on_every_os() {
        let h = homes(Path::new("/home/me"));
        let linux: Vec<PathBuf> = Browser::ALL
            .iter()
            .map(|b| target(Os::Linux, *b, &h).manifest)
            .collect();
        assert_eq!(
            linux,
            [
                "/home/me/.config/google-chrome/NativeMessagingHosts/design.snds.lockdown.json",
                "/home/me/.config/microsoft-edge/NativeMessagingHosts/design.snds.lockdown.json",
                "/home/me/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/design.snds.lockdown.json",
                "/home/me/.config/chromium/NativeMessagingHosts/design.snds.lockdown.json",
                "/home/me/.mozilla/native-messaging-hosts/design.snds.lockdown.json",
            ]
            .map(PathBuf::from)
        );
        let xdg = Homes {
            config_home: Some(PathBuf::from("/xdg")),
            ..h.clone()
        };
        assert_eq!(
            target(Os::Linux, Browser::Chromium, &xdg).manifest,
            PathBuf::from("/xdg/chromium/NativeMessagingHosts/design.snds.lockdown.json")
        );
        assert_eq!(
            target(Os::MacOs, Browser::Chrome, &h).manifest,
            PathBuf::from(
                "/home/me/Library/Application Support/Google/Chrome/NativeMessagingHosts/design.snds.lockdown.json"
            )
        );
        assert_eq!(
            target(Os::MacOs, Browser::Firefox, &h).manifest,
            PathBuf::from(
                "/home/me/Library/Application Support/Mozilla/NativeMessagingHosts/design.snds.lockdown.json"
            )
        );
        let win = Homes {
            local_app_data: Some(PathBuf::from("C:/Users/me/AppData/Local")),
            ..h
        };
        let edge = target(Os::Windows, Browser::Edge, &win);
        assert_eq!(
            edge.manifest,
            PathBuf::from(
                "C:/Users/me/AppData/Local/snds/lockdown/native-messaging/edge/design.snds.lockdown.json"
            )
        );
        assert_eq!(
            edge.registry_key.as_deref(),
            Some(
                r"HKEY_CURRENT_USER\Software\Microsoft\Edge\NativeMessagingHosts\design.snds.lockdown"
            )
        );
        assert!(
            target(Os::Linux, Browser::Edge, &win)
                .registry_key
                .is_none()
        );
    }

    #[test]
    fn install_writes_the_file_where_the_browser_reads_it_and_uninstall_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let h = homes(dir.path());
        assert!(!detected(Os::Linux, Browser::Chromium, &h));
        std::fs::create_dir_all(dir.path().join(".config/chromium")).unwrap();
        assert!(detected(Os::Linux, Browser::Chromium, &h));
        let host = dir.path().join("bin/lockdown-native-host");
        let t = install(Os::Linux, Browser::Chromium, &h, &host).unwrap();
        let written: Value = serde_json::from_slice(&std::fs::read(&t.manifest).unwrap()).unwrap();
        assert_eq!(written, manifest(Browser::Chromium, &host));
        let f = install(Os::Linux, Browser::Firefox, &h, &host).unwrap();
        assert!(
            f.manifest
                .ends_with(".mozilla/native-messaging-hosts/design.snds.lockdown.json")
        );
        uninstall(Os::Linux, Browser::Chromium, &h).unwrap();
        assert!(!t.manifest.exists());
        uninstall(Os::Linux, Browser::Chromium, &h).unwrap();
    }
}
