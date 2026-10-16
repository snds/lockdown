//! Account login, behind [`AccountAuth`] (04-security §3).
//!
//! The real protocol is OPAQUE (RFC 9807) with the `opaque-ke` crate: the
//! client proves it knows the password without sending it or its salt, and
//! ends up with a 64-byte `export_key` that mixes with the Secret Key into
//! the Account Unlock Key. The server half does not exist yet, so this
//! module gives the trait (the identity crate's, re-exported, so both
//! crates speak one shape) and [`OpaqueAccountAuth`], which answers
//! `Unsupported` with a plain message until the server lands. The identity
//! crate's `LocalStubAuth` remains the stand-in for local development.

pub use ecosystem_identity::auth::{AccountAuth, ExportKey, LocalStubAuth};
use ecosystem_identity::records::Id;

/// The OPAQUE client. Not built yet: every call returns `Unsupported`,
/// never panics, so a shell can wire the account flow today and get an
/// honest error instead of a crash.
#[derive(Debug, Default, Clone)]
pub struct OpaqueAccountAuth {
    /// The server this client will talk to, kept for the message.
    pub server: String,
}

impl OpaqueAccountAuth {
    /// A client for `server` (a URL or a name; unused until OPAQUE lands).
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
        }
    }

    fn unsupported(&self, what: &str) -> ecosystem_identity::Error {
        ecosystem_identity::Error::Unsupported(format!(
            "OPAQUE {what} against {} isn't built yet (04-security §3); use the recovery kit or the local stub",
            if self.server.is_empty() {
                "the account server"
            } else {
                self.server.as_str()
            }
        ))
    }
}

impl AccountAuth for OpaqueAccountAuth {
    fn register(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        Err(self.unsupported("registration"))
    }

    fn login(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        Err(self.unsupported("login"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_is_honestly_unsupported() {
        let mut auth = OpaqueAccountAuth::new("https://example.invalid");
        let id = Id::new();
        let err = auth.register(&id, "correct horse battery").unwrap_err();
        assert!(matches!(err, ecosystem_identity::Error::Unsupported(_)));
        assert!(err.to_string().contains("example.invalid"));
        assert!(matches!(
            auth.login(&id, "correct horse battery"),
            Err(ecosystem_identity::Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_local_stub_still_works_through_the_same_trait() {
        let mut stub = LocalStubAuth::generate().unwrap();
        let id = Id::new();
        let a = stub.register(&id, "correct horse battery").unwrap();
        let b = stub.login(&id, "correct horse battery").unwrap();
        assert_eq!(a.expose(), b.expose());
    }
}
