//! `lockdown-wasm`: the facade the extension, Safari Web Extension, web vault
//! and Tauri web view call (03-architecture §3, §4 "extension modes", §8).
//!
//! **Status: skeleton.** The `wasm-bindgen` exports are added with the
//! extension build. The surface is [`lockdown_ffi`]'s, re-exported, so a
//! feature exists on every platform or on none (D9). The JS side will get
//! `Uint8Array` views over linear memory that the core zeroes on release;
//! the offscreen document is torn down on lock, not kept warm.

pub use lockdown_ffi::{FfiError, RedactedView, SecretHandle, SecretRegistry};

/// The session the offscreen document holds while unlocked. Dropping it
/// wipes every secret.
#[derive(Default, Debug)]
pub struct WasmSession {
    /// The handles the page may reference.
    pub secrets: SecretRegistry,
}

impl WasmSession {
    /// A fresh, empty session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lock: wipe everything. The page keeps no copy.
    pub fn lock(&mut self) {
        self.secrets.lock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locking_wipes_the_session() {
        let mut s = WasmSession::new();
        let h = s.secrets.insert(b"k".to_vec());
        assert_eq!(s.secrets.len(), 1);
        s.lock();
        assert_eq!(s.secrets.redacted(h), Err(FfiError::InvalidHandle));
    }
}
