//! Automatic updates for the desktop app (docs/plan/60-auto-update.md).
//!
//! CI publishes every merge to `main` as one rolling GitHub pre-release,
//! `main-build`, with a signed bundle per platform and the updater manifest
//! `latest.json`. The app follows it: it checks at launch and every six
//! hours (or when asked), downloads a newer build in the background, checks
//! its minisign signature against the public key built in from
//! `updater.pub`, and offers "Restart to update". An update not taken
//! installs when the app quits.
//!
//! This file is the part that needs no window, tested on every platform:
//!
//! - [`Channel`]: which feed a build follows, read from its own version
//!   (`0.1.0-main.<commit time>` follows "main"; a plain version will
//!   follow the stable channel, which isn't built yet).
//! - [`plan`]: whether this build checks at all, and where: the channel's
//!   endpoint, the key, and the hosts a download may come from.
//! - [`allowed`]: a manifest or bundle URL must be `https` on GitHub's own
//!   hosts, with no user name, password or other port.
//! - [`is_newer`]: only a strictly higher version is an update.
//! - [`UpdateStatus`]: what Settings and the menu show.
//!
//! The window's half (the plugin, the commands, the background check, the
//! menu item, installing on quit) is in `shell` below, behind the `shell`
//! feature.

use std::time::Duration;

use semver::Version;
use serde::Serialize;

/// The "main" channel's manifest: the `latest.json` asset of the rolling
/// `main-build` pre-release. No part of the URL depends on this device,
/// this person or this build.
pub const MAIN_ENDPOINT: &str =
    "https://github.com/snds/lockdown/releases/download/main-build/latest.json";

/// The page people are sent to where the app can't replace itself (Linux
/// packages, a machine-wide Windows install, a Mac app folder they can't
/// write to).
pub const MAIN_PAGE: &str = "https://github.com/snds/lockdown/releases/tag/main-build";

/// Where an update may come from: the manifest and the release page on
/// `github.com`, and the asset hosts a release download is redirected to.
/// Every redirect is checked against the same list.
pub const GITHUB_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

/// How often a running app checks, with the automatic check on.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// How long a check or a download may take before it counts as failed.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The marker a build carries only with the test-only endpoint and key
/// (the `update-test` feature): `packaging/devtools/check.sh` fails a
/// tagged release that has it.
#[cfg(any(test, feature = "update-test"))]
pub const TEST_MARKER: &str = "lockdown-update-test-3b9d";

/// The feed a build follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Every merge to `main`: versions `x.y.z-main.<YYYYMMDDHHMMSS>`.
    Main,
    /// Tagged releases (`v*`): designed for, not built yet.
    Stable,
}

impl Channel {
    /// The channel a build of `version` follows.
    pub fn of(version: &Version) -> Self {
        let pre = version.pre.as_str();
        if pre == "main" || pre.starts_with("main.") {
            Self::Main
        } else {
            Self::Stable
        }
    }

    /// Where the channel's manifest is; `None` while the channel isn't built.
    pub fn endpoint(self) -> Option<&'static str> {
        match self {
            Self::Main => Some(MAIN_ENDPOINT),
            Self::Stable => None,
        }
    }

    /// The release page for the channel.
    pub fn page(self) -> Option<&'static str> {
        match self {
            Self::Main => Some(MAIN_PAGE),
            Self::Stable => None,
        }
    }
}

/// Whether `offered` is an update for a build at `current`: strictly
/// higher in semver order, where a pre-release's numeric fields compare as
/// numbers (`main.20261107093000` < `main.20261107120000`) and a release
/// sorts after its own pre-releases (`0.2.0-main.…` < `0.2.0`). The same
/// version, an older one, or a garbled one is never an update.
pub fn is_newer(current: &Version, offered: &Version) -> bool {
    offered > current
}

/// The public key built in from `updater.pub` (build.rs), if the file was
/// there when this build was made.
pub fn built_in_key() -> Option<&'static str> {
    Some(env!("LOCKDOWN_UPDATER_PUBKEY")).filter(|k| !k.is_empty())
}

/// Whether `key` is a minisign public key in the form the updater reads:
/// base64 of the two-line text `tauri signer generate` writes.
pub fn key_is_valid(key: &str) -> bool {
    let Ok(text) = data_encoding::BASE64.decode(key.trim().as_bytes()) else {
        return false;
    };
    let Ok(text) = String::from_utf8(text) else {
        return false;
    };
    minisign_verify::PublicKey::decode(&text).is_ok()
}

/// The key's minisign ID, as `minisign` and `tauri signer` print it
/// (sixteen hex digits), read from the key itself rather than its comment.
pub fn key_id(key: &str) -> Option<String> {
    let text = data_encoding::BASE64.decode(key.trim().as_bytes()).ok()?;
    let text = String::from_utf8(text).ok()?;
    minisign_verify::PublicKey::decode(&text).ok()?;
    let raw = data_encoding::BASE64
        .decode(text.lines().nth(1)?.trim().as_bytes())
        .ok()?;
    // Two bytes of algorithm, eight of key ID (little-endian), 32 of key.
    let id = raw.get(2..10)?;
    Some(id.iter().rev().map(|b| format!("{b:02X}")).collect())
}

/// The ID of the key Sean made for the "main" channel (2026-11-07), whose
/// public half is `updater.pub`. A test holds the committed file to it.
pub const RELEASE_KEY_ID: &str = "303459ECC0739D7D";

/// Checks that `url` is somewhere an update may come from: `https` on one
/// of `hosts` with the default port and no user name or password. A test
/// source (the `update-test` feature) may also be plain `http` on this
/// computer's loopback address.
pub fn allowed(url: &str, hosts: &[String]) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "the update address isn't a web address")?;
    let host = parsed
        .host_str()
        .ok_or("the update address has no host")?
        .to_ascii_lowercase();
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("the update address carries a user name".into());
    }
    if !hosts.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
        return Err(format!("updates don't come from {host}"));
    }
    let loopback = matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]");
    match parsed.scheme() {
        "https" if parsed.port().is_none() || loopback => Ok(()),
        "http" if loopback && cfg!(any(test, feature = "update-test")) => Ok(()),
        "https" => Err(format!("updates from {host} use its usual port")),
        _ => Err("updates come only over https".into()),
    }
}

/// Why a build doesn't check for updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Off {
    /// Built without `updater.pub`.
    NoKey,
    /// `updater.pub` was there but isn't a minisign public key.
    BadKey,
    /// A tagged release: the stable channel isn't built yet.
    StableNotBuilt,
}

impl Off {
    /// The one line Settings shows.
    pub fn words(self) -> &'static str {
        match self {
            Self::NoKey => "Updates are off in this build: it was made without the update key.",
            Self::BadKey => "Updates are off in this build: its update key isn't valid.",
            Self::StableNotBuilt => {
                "Updates are off in this build: automatic updates for tagged releases come later."
            }
        }
    }
}

/// Where a build checks, and what it trusts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The manifest.
    pub endpoint: String,
    /// The minisign public key (base64), as the updater reads it.
    pub pubkey: String,
    /// The hosts the manifest and the bundle may come from.
    pub hosts: Vec<String>,
    /// The page to send people to where the app can't replace itself.
    pub page: String,
}

/// The test-only endpoint and key (`update-test` builds): an end-to-end
/// run serves build B from this computer, signed with a throwaway key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSource {
    pub endpoint: String,
    pub pubkey: String,
}

impl TestSource {
    /// `LOCKDOWN_UPDATE_TEST_ENDPOINT` and `LOCKDOWN_UPDATE_TEST_PUBKEY`,
    /// both set, in a build with the `update-test` feature; never otherwise.
    pub fn from_env() -> Option<Self> {
        #[cfg(feature = "update-test")]
        {
            let endpoint = std::env::var("LOCKDOWN_UPDATE_TEST_ENDPOINT").ok()?;
            let pubkey = std::env::var("LOCKDOWN_UPDATE_TEST_PUBKEY").ok()?;
            // The marker, kept in the binary where the override is.
            log_marker(TEST_MARKER);
            Some(Self { endpoint, pubkey })
        }
        #[cfg(not(feature = "update-test"))]
        {
            None
        }
    }
}

#[cfg(feature = "update-test")]
fn log_marker(marker: &'static str) {
    std::hint::black_box(marker);
}

/// Whether a build of `version` checks for updates, and where. A test
/// source (only ever `Some` in an `update-test` build) replaces the
/// channel's endpoint, its key and its hosts.
pub fn plan(version: &Version, key: Option<&str>, test: Option<TestSource>) -> Result<Source, Off> {
    if let Some(test) = test {
        let host = url::Url::parse(&test.endpoint)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        if !key_is_valid(&test.pubkey) {
            return Err(Off::BadKey);
        }
        return Ok(Source {
            page: test.endpoint.clone(),
            endpoint: test.endpoint,
            pubkey: test.pubkey,
            hosts: vec![host],
        });
    }
    let key = key.ok_or(Off::NoKey)?;
    if !key_is_valid(key) {
        return Err(Off::BadKey);
    }
    let channel = Channel::of(version);
    let endpoint = channel.endpoint().ok_or(Off::StableNotBuilt)?;
    Ok(Source {
        endpoint: endpoint.to_string(),
        pubkey: key.to_string(),
        hosts: GITHUB_HOSTS.iter().map(|h| h.to_string()).collect(),
        page: channel.page().unwrap_or(MAIN_PAGE).to_string(),
    })
}

/// Whether this copy of the app can replace itself, or can only point to
/// the newer build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Install {
    /// The update installs in place and the app relaunches.
    InPlace,
    /// A Linux package (deb, rpm, Flatpak): the package manager owns it.
    LinuxPackage,
    /// Windows, installed for every user (Program Files): replacing it
    /// would need an administrator, so the app doesn't try.
    MachineWide,
    /// macOS, in a folder this account can't write to: replacing it would
    /// ask for an administrator's password, so the app doesn't try.
    NotWritable,
}

impl Install {
    /// Why the app points to the build instead, in one line.
    pub fn words(self) -> Option<&'static str> {
        match self {
            Self::InPlace => None,
            Self::LinuxPackage => {
                Some("Lockdown can't update itself when installed from a Linux package.")
            }
            Self::MachineWide => Some(
                "Lockdown is installed for everyone on this computer, so it can't update itself.",
            ),
            Self::NotWritable => {
                Some("Lockdown can't update itself from a folder your account can't change.")
            }
        }
    }
}

/// Whether a Windows install at `exe` is the per-user one the NSIS
/// installer makes (under `%LOCALAPPDATA%`), which updates without an
/// administrator; anything else (Program Files, an MSI) is machine-wide.
pub fn windows_per_user(exe: &std::path::Path, local_app_data: Option<&std::path::Path>) -> bool {
    let Some(base) = local_app_data else {
        return false;
    };
    let lower = |p: &std::path::Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    let exe = lower(exe);
    let base = lower(base);
    let base = base.trim_end_matches('\\');
    !base.is_empty() && exe.starts_with(&format!("{base}\\"))
}

/// Where an update stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    /// No check since launch.
    Idle,
    Checking,
    UpToDate,
    Downloading,
    /// Downloaded and checked: "Restart to update", or it installs on quit.
    Ready,
    /// A newer build exists, but this copy can't replace itself.
    Available,
    Failed,
}

/// What Settings → About, the sidebar and the menu show. Plain data and
/// plain words; nothing here is secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpdateStatus {
    /// The running version.
    pub version: String,
    pub channel: Channel,
    /// Whether this build checks at all.
    pub enabled: bool,
    /// Why it doesn't, in one line (`enabled` false).
    pub off: Option<String>,
    /// The "Check automatically" switch (Settings, on by default).
    pub auto_check: bool,
    pub state: UpdateState,
    /// The download's progress, 0 to 100 (`downloading`).
    pub percent: Option<u8>,
    /// The newer version (`downloading`, `ready`, `available`).
    pub offered: Option<String>,
    /// What went wrong, in plain words (`failed`).
    pub error: Option<String>,
    /// The release page (`available`).
    pub link: Option<String>,
    /// Why the app points there instead of updating itself (`available`).
    pub link_why: Option<String>,
    /// Whether the last check was asked for (the button or the menu),
    /// rather than the automatic one.
    pub asked: bool,
}

impl UpdateStatus {
    /// The status at launch, before any check.
    pub fn at_start(version: &Version, plan: &Result<Source, Off>, auto_check: bool) -> Self {
        Self {
            version: version.to_string(),
            channel: Channel::of(version),
            enabled: plan.is_ok(),
            off: plan.as_ref().err().map(|o| o.words().to_string()),
            auto_check,
            state: UpdateState::Idle,
            percent: None,
            offered: None,
            error: None,
            link: None,
            link_why: None,
            asked: false,
        }
    }

    /// Moves to `state`, clearing what belonged to the one before.
    pub fn to(&mut self, state: UpdateState) {
        self.state = state;
        if state != UpdateState::Downloading {
            self.percent = None;
        }
        if state != UpdateState::Failed {
            self.error = None;
        }
        if state != UpdateState::Available {
            self.link = None;
            self.link_why = None;
        }
        if matches!(
            state,
            UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate
        ) {
            self.offered = None;
        }
    }
}

/// A download's progress as a whole percentage, never past 99 until the
/// signature has been checked (`ready` says the rest).
pub fn percent(received: u64, total: Option<u64>) -> Option<u8> {
    let total = total.filter(|t| *t > 0)?;
    Some(((received.min(total) * 100) / total).min(99) as u8)
}

#[cfg(feature = "shell")]
pub mod shell;

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    /// A minisign public key in the updater's form, made for these tests
    /// only; its secret half was never kept.
    const TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQyOUE3MTU4MUNBMTA3RkQKUldUOUI2RWNXSEdhMHBLeWVGS0xVTEZFZy83Q0xiL3VjN1NDdWxLQ25MSUxOYTJLZDNaUUhtSHkK";

    #[test]
    fn main_builds_order_by_commit_time_and_before_the_release() {
        let a = v("0.1.0-main.20261107093000");
        let b = v("0.1.0-main.20261107120000");
        let c = v("0.1.0-main.20261201000000");
        assert!(is_newer(&a, &b));
        assert!(is_newer(&b, &c));
        assert!(!is_newer(&b, &a));
        assert!(!is_newer(&b, &b), "the same build is never an update");
        // Numeric fields compare as numbers, not as text: a longer stamp
        // (a later year with more digits) still sorts after.
        assert!(is_newer(&v("0.1.0-main.9"), &v("0.1.0-main.10")));
        // A later stable version is newer than every main build before it,
        // and the release of the same version sorts after its main builds.
        assert!(is_newer(&c, &v("0.1.0")));
        assert!(is_newer(&c, &v("0.2.0")));
        // A main build of the next version is newer than the release.
        assert!(is_newer(&v("0.1.0"), &v("0.1.1-main.20270101000000")));
        assert!(!is_newer(&v("0.2.0"), &v("0.1.0-main.20991231235959")));
    }

    #[test]
    fn the_channel_comes_from_the_version() {
        assert_eq!(Channel::of(&v("0.1.0-main.20261107093000")), Channel::Main);
        assert_eq!(Channel::of(&v("0.1.0")), Channel::Stable);
        assert_eq!(Channel::of(&v("0.1.0-beta.1")), Channel::Stable);
        assert_eq!(Channel::of(&v("0.1.0-mainline.1")), Channel::Stable);
        assert_eq!(Channel::Main.endpoint(), Some(MAIN_ENDPOINT));
        assert_eq!(Channel::Stable.endpoint(), None);
    }

    #[test]
    fn the_plan_needs_a_valid_key_and_a_built_channel() {
        let main = v("0.1.0-main.20261107093000");
        assert_eq!(plan(&main, None, None), Err(Off::NoKey));
        assert_eq!(plan(&main, Some("not a key"), None), Err(Off::BadKey));
        assert_eq!(
            plan(
                &main,
                Some(&data_encoding::BASE64.encode(b"untrusted comment: x\nRWQ")),
                None
            ),
            Err(Off::BadKey)
        );
        assert_eq!(
            plan(&v("0.1.0"), Some(TEST_KEY), None),
            Err(Off::StableNotBuilt)
        );
        let source = plan(&main, Some(TEST_KEY), None).unwrap();
        assert_eq!(source.endpoint, MAIN_ENDPOINT);
        assert_eq!(source.pubkey, TEST_KEY);
        assert_eq!(source.page, MAIN_PAGE);
        assert!(allowed(&source.endpoint, &source.hosts).is_ok());
        assert!(allowed(&source.page, &source.hosts).is_ok());
        for words in [Off::NoKey, Off::BadKey, Off::StableNotBuilt].map(Off::words) {
            assert!(words.starts_with("Updates are off in this build"));
        }
    }

    #[test]
    fn a_test_source_replaces_the_channel_only_with_a_valid_key() {
        let stable = v("0.1.0");
        let test = TestSource {
            endpoint: "http://127.0.0.1:8765/latest.json".into(),
            pubkey: TEST_KEY.into(),
        };
        let source = plan(&stable, None, Some(test.clone())).unwrap();
        assert_eq!(source.hosts, vec!["127.0.0.1".to_string()]);
        assert!(allowed("http://127.0.0.1:8765/b.app.tar.gz", &source.hosts).is_ok());
        assert!(allowed(MAIN_ENDPOINT, &source.hosts).is_err());
        let bad = TestSource {
            pubkey: "nope".into(),
            ..test
        };
        assert_eq!(plan(&stable, None, Some(bad)), Err(Off::BadKey));
        // Outside an update-test build the environment is never read.
        #[cfg(not(feature = "update-test"))]
        assert_eq!(TestSource::from_env(), None);
        assert!(!TEST_MARKER.is_empty());
    }

    #[test]
    fn updates_come_only_from_github_over_https() {
        let hosts: Vec<String> = GITHUB_HOSTS.iter().map(|h| h.to_string()).collect();
        for ok in [
            MAIN_ENDPOINT,
            "https://objects.githubusercontent.com/github-production-release-asset/1/2?x=y",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/2",
            "https://GitHub.com/snds/lockdown/releases/download/main-build/Lockdown.app.tar.gz",
        ] {
            assert_eq!(allowed(ok, &hosts), Ok(()), "{ok}");
        }
        for bad in [
            "http://github.com/snds/lockdown/releases/download/main-build/latest.json",
            "https://github.com.evil.example/latest.json",
            "https://evil.example/github.com/latest.json",
            "https://github.com@evil.example/latest.json",
            "https://user:pass@github.com/latest.json",
            "https://github.com:8443/latest.json",
            "https://raw.githubusercontent.com/snds/lockdown/main/latest.json",
            "ftp://github.com/latest.json",
            "file:///etc/passwd",
            "not a url",
            "http://127.0.0.1:8765/latest.json",
        ] {
            assert!(allowed(bad, &hosts).is_err(), "{bad} was let through");
        }
    }

    #[test]
    fn plain_http_is_only_for_a_test_source_on_this_computer() {
        let hosts = vec!["127.0.0.1".to_string(), "example.org".to_string()];
        assert!(allowed("http://127.0.0.1:9/latest.json", &hosts).is_ok());
        assert!(allowed("http://example.org/latest.json", &hosts).is_err());
    }

    #[test]
    fn windows_updates_itself_only_from_a_per_user_install() {
        use std::path::Path;
        let local = Path::new(r"C:\Users\sean\AppData\Local");
        assert!(windows_per_user(
            Path::new(r"C:\Users\sean\AppData\Local\Lockdown\lockdown-desktop.exe"),
            Some(local)
        ));
        assert!(windows_per_user(
            Path::new(r"c:\users\SEAN\appdata\local\Programs\Lockdown\lockdown-desktop.exe"),
            Some(Path::new(r"C:\Users\sean\AppData\Local\"))
        ));
        assert!(!windows_per_user(
            Path::new(r"C:\Program Files\Lockdown\lockdown-desktop.exe"),
            Some(local)
        ));
        assert!(!windows_per_user(
            Path::new(r"C:\Users\sean\AppData\LocalLow\Lockdown\lockdown-desktop.exe"),
            Some(local)
        ));
        assert!(!windows_per_user(Path::new(r"C:\x.exe"), None));
        assert!(!windows_per_user(
            Path::new(r"C:\x.exe"),
            Some(Path::new(""))
        ));
    }

    #[test]
    fn the_status_moves_cleanly_between_states() {
        let version = v("0.1.0-main.20261107093000");
        let source = plan(&version, Some(TEST_KEY), None);
        let mut s = UpdateStatus::at_start(&version, &source, true);
        assert!(s.enabled);
        assert_eq!(s.state, UpdateState::Idle);
        assert_eq!(s.channel, Channel::Main);
        s.to(UpdateState::Downloading);
        s.offered = Some("0.1.0-main.20261107120000".into());
        s.percent = percent(50, Some(100));
        assert_eq!(s.percent, Some(50));
        s.to(UpdateState::Ready);
        assert_eq!(s.percent, None);
        assert_eq!(s.offered.as_deref(), Some("0.1.0-main.20261107120000"));
        s.to(UpdateState::Failed);
        s.error = Some("no connection".into());
        s.to(UpdateState::Checking);
        assert_eq!((s.error.as_deref(), s.offered.as_deref()), (None, None));
        let off = UpdateStatus::at_start(&version, &Err(Off::NoKey), true);
        assert!(!off.enabled);
        assert_eq!(off.off.as_deref(), Some(Off::NoKey.words()));
        let json = serde_json::to_value(&off).unwrap();
        assert_eq!(json["state"], "idle");
        assert_eq!(json["channel"], "main");
    }

    #[test]
    fn progress_never_reads_done_before_the_check() {
        assert_eq!(percent(0, Some(10)), Some(0));
        assert_eq!(percent(10, Some(10)), Some(99));
        assert_eq!(percent(20, Some(10)), Some(99));
        assert_eq!(percent(5, None), None);
        assert_eq!(percent(5, Some(0)), None);
    }

    #[test]
    fn the_test_key_is_a_valid_key_and_the_built_in_one_matches_the_file() {
        assert!(key_is_valid(TEST_KEY));
        let file = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("updater.pub"),
        )
        .ok();
        assert_eq!(
            built_in_key(),
            file.as_deref().map(str::trim).filter(|k| !k.is_empty())
        );
        assert_eq!(key_id(TEST_KEY).as_deref(), Some("D29A71581CA107FD"));
        assert_eq!(key_id("bm90IGEga2V5"), None);
    }

    /// The committed `updater.pub` is Sean's release key, whole: a file
    /// changed, cut short or swapped fails here before any build ships it.
    #[test]
    fn the_committed_key_is_the_release_key() {
        let file = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("updater.pub"),
        )
        .expect("apps/desktop/src-tauri/updater.pub is committed");
        assert!(
            key_is_valid(&file),
            "updater.pub doesn't parse as a minisign public key"
        );
        assert_eq!(key_id(&file).as_deref(), Some(RELEASE_KEY_ID));
        assert_eq!(
            built_in_key().and_then(key_id).as_deref(),
            Some(RELEASE_KEY_ID)
        );
    }
}
