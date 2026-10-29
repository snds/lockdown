//! `lockdown-devtools`: tools for Lockdown's own development builds, kept
//! in one crate so they can be removed in one go (docs/plan/40-devtools.md).
//!
//! - [`reset`] (feature `reset`): a full reset of Lockdown on this device.
//! - [`demo`] (feature `demo`): the demo, a real session over a throwaway
//!   vault filled from a replaceable data pack.
//! - [`keystore`] (feature `os-keystore`): the OS keystore searched by
//!   Lockdown's service name, for the reset.
//! - [`shell`] (feature `shell`): the Tauri plugin the desktop registers
//!   with its `dev-reset` and `demo-vault` features: the commands, the
//!   Developer menu and the relaunch.
//!
//! None of it is in a release: the desktop's tag build turns both features
//! off, and CI fails a release whose files carry [`MARKER`].

/// A string every build with the developer tools carries, which the release
/// workflow looks for in a tag build's binaries and front end and fails on
/// (docs/plan/40-devtools.md §5). The front end has its own copy in
/// `apps/desktop/src/devtools/marker.ts`.
pub const MARKER: &str = "lockdown-devtools-6f1c2a";

pub mod menu;

#[cfg(feature = "reset")]
pub mod reset;

#[cfg(feature = "os-keystore")]
pub mod keystore;

#[cfg(feature = "demo")]
pub mod demo;

#[cfg(feature = "shell")]
pub mod shell;
