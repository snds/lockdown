//! The browser extension's channel to the desktop app
//! (docs/plan/16-browser-extension.md §2).
//!
//! ```text
//! extension ──stdio──▶ lockdown-native-host ──Unix socket──▶ desktop app
//!   (MV3)    native        (launched by the       framed     (the core
//!            messaging      browser, decrypts      JSON       decides and
//!                           nothing)                          answers)
//! ```
//!
//! This crate holds what both ends of that chain must agree on and nothing
//! that touches a vault:
//!
//! - [`framing`]: the browser's stdio framing (a native-endian `u32`
//!   length, then UTF-8 JSON), with the sizes this channel allows.
//! - [`caller`]: who launched the host, read from the arguments the browser
//!   passes (an extension origin on Chromium, a manifest path and an add-on
//!   id on Firefox), and the [`caller::HostHello`] the host sends first.
//! - [`endpoint`]: where the two meet: the socket path, and the endpoint
//!   file the desktop writes so the host knows which process it may talk to.
//! - [`manifest`]: the native-messaging host manifest for each browser and
//!   where each browser looks for it on Linux, macOS and Windows.

#![forbid(unsafe_code)]

pub mod caller;
pub mod endpoint;
pub mod framing;
pub mod manifest;

/// The host's name as every browser knows it: the manifest's `name` and
/// file name, and what `runtime.connectNative` asks for.
pub const HOST_NAME: &str = "design.snds.lockdown";

/// The Chromium extension id of the development build, fixed by the public
/// key in `extension/manifest.base.json` (the unpacked id otherwise changes
/// with the folder). A store build gets its id from the store; it joins
/// [`CHROMIUM_IDS`] when it exists.
pub const DEV_CHROMIUM_ID: &str = "dbdblnolpiehhlabpmhobmkbjipmebpe";

/// The Firefox add-on id (`browser_specific_settings.gecko.id`).
pub const FIREFOX_ID: &str = "lockdown@snds.design";

/// Every Chromium extension id the desktop answers.
pub const CHROMIUM_IDS: &[&str] = &[DEV_CHROMIUM_ID];

/// Every Firefox add-on id the desktop answers.
pub const FIREFOX_IDS: &[&str] = &[FIREFOX_ID];

/// What can go wrong on this channel.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a pipe, a socket or a file.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// A message larger than this channel allows.
    #[error("a message of {0} bytes is larger than the channel allows")]
    TooLarge(usize),
    /// Text that is not the JSON this channel speaks.
    #[error("the message isn't the JSON this channel speaks: {0}")]
    BadMessage(String),
    /// The browser did not say which extension launched the host.
    #[error("the browser didn't say which extension started the host")]
    UnknownCaller,
    /// No home directory or data directory could be worked out.
    #[error("{0}")]
    NoDataDir(String),
}

/// The result of every call here.
pub type Result<T> = std::result::Result<T, Error>;
