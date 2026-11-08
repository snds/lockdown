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
//! The rules and the window's glue live in the shared update kit,
//! `snds/app-updates` (the unified updates proposal, U2), extracted from
//! this file. What stays here is Lockdown's own:
//!
//! - [`FEED`]: where Lockdown's builds are published, and its name.
//! - [`built_in_key`] and [`RELEASE_KEY_ID`]: the key every update must be
//!   signed with.
//! - [`TEST_MARKER`] and [`test_source_from_env`]: the test-only source.
//! - [`plan`]: the kit's plan for Lockdown's feed.
//!
//! The window's half (the commands, the vault locked before a restart,
//! the end-to-end hooks) is in `shell` below, behind the `shell` feature.

use semver::Version;

pub use app_updates::{
    CHECK_EVERY, Channel, Feed, GITHUB_HOSTS, Install, NETWORK_TIMEOUT, Off, Source, TestSource,
    UpdateState, UpdateStatus, allowed, is_newer, key_id, key_is_valid, percent, windows_per_user,
};

/// The "main" channel's manifest: the `latest.json` asset of the rolling
/// `main-build` pre-release. No part of the URL depends on this device,
/// this person or this build.
pub const MAIN_ENDPOINT: &str =
    "https://github.com/snds/lockdown/releases/download/main-build/latest.json";

/// The page people are sent to where the app can't replace itself (Linux
/// packages, a machine-wide Windows install, a Mac app folder they can't
/// write to).
pub const MAIN_PAGE: &str = "https://github.com/snds/lockdown/releases/tag/main-build";

/// Lockdown's feed: the main channel on its own releases, GitHub's hosts,
/// and no stable channel yet.
pub const FEED: Feed = Feed {
    app: "Lockdown",
    main_endpoint: MAIN_ENDPOINT,
    main_page: MAIN_PAGE,
    stable_endpoint: None,
    stable_page: None,
    hosts: &GITHUB_HOSTS,
};

/// The marker a build carries only with the test-only endpoint and key
/// (the `update-test` feature): `packaging/devtools/check.sh` fails a
/// tagged release that has it.
#[cfg(any(test, feature = "update-test"))]
pub const TEST_MARKER: &str = "lockdown-update-test-3b9d";

/// The public key built in from `updater.pub` (build.rs), if the file was
/// there when this build was made.
pub fn built_in_key() -> Option<&'static str> {
    Some(env!("LOCKDOWN_UPDATER_PUBKEY")).filter(|k| !k.is_empty())
}

/// The ID of the key Sean made for the "main" channel (2026-11-07), whose
/// public half is `updater.pub`. A test holds the committed file to it.
pub const RELEASE_KEY_ID: &str = "303459ECC0739D7D";

/// `LOCKDOWN_UPDATE_TEST_ENDPOINT` and `LOCKDOWN_UPDATE_TEST_PUBKEY`,
/// both set, in a build with the `update-test` feature; never otherwise.
pub fn test_source_from_env() -> Option<TestSource> {
    #[cfg(feature = "update-test")]
    {
        // The marker, kept in the binary where the override is.
        std::hint::black_box(TEST_MARKER);
        TestSource::from_env("LOCKDOWN")
    }
    #[cfg(not(feature = "update-test"))]
    {
        None
    }
}

/// Whether a build of `version` checks for updates, and where. A test
/// source (only ever `Some` in an `update-test` build) replaces the
/// channel's endpoint, its key and its hosts.
pub fn plan(version: &Version, key: Option<&str>, test: Option<TestSource>) -> Result<Source, Off> {
    app_updates::plan(&FEED, version, key, test)
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
        assert_eq!(Channel::Main.endpoint(&FEED), Some(MAIN_ENDPOINT));
        assert_eq!(Channel::Stable.endpoint(&FEED), None);
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
        assert_eq!(test_source_from_env(), None);
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
