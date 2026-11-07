//! Where a shell keeps the one secret the app holds between runs that is
//! not under the identity's keys: the session token for the sync server.
//! The desktop shell installs the OS keychain here (macOS Keychain, Windows
//! Credential Manager, Secret Service on Linux); with nothing installed the
//! token stays in `sync.cbor`, readable only by the user, as the CLI keeps it.

use std::sync::OnceLock;

/// A place for small secrets, chosen by the host.
pub trait SecretStore: Send + Sync {
    /// The value under `key`, or `None` when there is none.
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    /// Writes `value` under `key`.
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    /// Forgets `key`.
    fn delete(&self, key: &str);
    /// Whether values are kept here. A store that answers `false` is only
    /// read (a token an older build left), and the token stays in
    /// `sync.cbor` as it does with no store at all: the desktop on a Mac
    /// whose own signature changes with every build, where a keychain item
    /// written by one build would make the next ask for the person's
    /// permission (docs/plan/60-auto-update.md §3).
    fn keeps_values(&self) -> bool {
        true
    }
}

static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();

/// Sets the store once, at startup, before the app opens anything.
pub fn install(store: Box<dyn SecretStore>) {
    let _ = STORE.set(store);
}

/// The installed store, if a host installed one.
pub fn store() -> Option<&'static dyn SecretStore> {
    STORE.get().map(|s| s.as_ref())
}

/// The keychain entry for an account's session token.
pub fn session_key(server: &str, login: &str) -> String {
    format!("session:{login}@{server}")
}
