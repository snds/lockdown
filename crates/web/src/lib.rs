//! `lockdown-web`: the core of the web vault (docs/plan/17-web-vault.md).
//!
//! The web vault runs the same `lockdown-app` the desktop runs, compiled
//! for `wasm32-wasip1` and instantiated in a dedicated worker, so every
//! rule that matters (the gates, the crown-jewel class, the lock timers,
//! sync with its rollback and log checks) is the one the desktop enforces,
//! not a second copy in TypeScript. What the browser cannot give that
//! build is stood in for here:
//!
//! - **Files.** The worker gives the module a small in-memory file system
//!   (the identity root and Lockdown's data directory); it dies with the
//!   tab. Only the rollback memory and the log memory are copied out, to
//!   the browser's IndexedDB, between visits; they hold hashes and public
//!   keys, never a key or a value.
//! - **SQLite.** None: the vault store is `MemoryStore` (`lockdown-store`
//!   without its `sqlite` feature).
//! - **HTTP.** None built in: [`abi`] installs a hook that hands every
//!   request to the worker, which makes it with a synchronous
//!   `XMLHttpRequest` to the page's own origin and nowhere else.
//! - **The device keystore.** The device key is a file in that memory
//!   while the vault is open. At sign-in it is sealed under a key derived
//!   from the OPAQUE export key; on lock the file is wiped and only the
//!   sealed copy stays. Unlocking is a fresh OPAQUE sign-in: the password
//!   never leaves the worker in any form the server could test offline,
//!   and a copy of the worker's memory taken while locked gives no
//!   password guess without the server ([`host`]).
//!
//! - [`host`] — [`WebVault`]: sign in, unlock, lock, sign out
//! - [`commands`] — the command table the page calls, named and shaped as
//!   the desktop's Tauri commands are, so one client interface serves both
//! - [`fields`] — the editor's field changes, as the desktop parses them
//! - [`links`] — share links opened and request links answered in a plain
//!   browser, with no account (D198)
//! - [`abi`] — the module's exports and the worker's imports (WASI only)

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "wasi")]
pub mod abi;
pub mod commands;
pub mod fields;
pub mod host;
pub mod links;
pub mod security;

pub use commands::{Reply, call};
pub use host::{WebPaths, WebState, WebVault};
