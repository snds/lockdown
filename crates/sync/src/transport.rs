//! How the client reaches the server: the [`Transport`] contract over the
//! wire types, and [`HttpTransport`], the HTTPS implementation (blocking
//! `reqwest` over rustls, which is enough for the CLI; the shells get an
//! async one later). `HttpTransport` is also the
//! [`AuthRounds`](lockdown_crypto::auth::AuthRounds) the OPAQUE client
//! drives, so one connection does login and sync.

use lockdown_crypto::auth::{AuthRounds, DeviceProof, LoginStarted, Session};
use lockdown_vault::{Hash32, Id};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::wire::{
    self, BEARER, BlobAccepted, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, HeadAccepted,
    HeadRefused, HeadsSince, IdentityPush, IdentitySince, ItemPush, ItemsSince, LoginFinish,
    LoginFinishReply, LoginStart, LoginStartReply, PushReply, RegisterFinish, RegisterStart,
    RegisterStartReply, VaultList,
};

/// What can go wrong between client and server.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// No answer, or not an HTTP one.
    #[error("couldn't reach the server: {0}")]
    Network(String),
    /// An answer that did not decode as the wire type expected.
    #[error("the server's answer didn't decode: {0}")]
    Decode(String),
    /// The server said no.
    #[error("the server refused: {message}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// What the server said to do about it.
        code: ErrorCode,
        /// For the person.
        message: String,
        /// For a refused head, where the server stands.
        head: Option<HeadRefused>,
    },
}

/// The transport result type.
pub type Result<T> = std::result::Result<T, TransportError>;

/// The authenticated calls of the protocol (`token` is the session).
pub trait Transport {
    /// `GET /v1/identity?since=N`.
    fn identity_since(&self, token: &str, since: u64) -> Result<IdentitySince>;
    /// `PUT /v1/identity`. A refusal with a chain code comes back as
    /// `Ok(Err(_))`; anything else is an error.
    fn put_identity(
        &self,
        token: &str,
        push: &IdentityPush,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// `GET /v1/vaults`.
    fn list_vaults(&self, token: &str) -> Result<VaultList>;
    /// `PUT /v1/vaults/{vault}`.
    fn put_vault(&self, token: &str, vault_id: Id, record: &[u8]) -> Result<()>;
    /// `GET /v1/vaults/{vault}/heads?since=N`.
    fn heads_since(&self, token: &str, vault_id: Id, since: u64) -> Result<HeadsSince>;
    /// `PUT /v1/vaults/{vault}/heads`. A refusal with a head code comes
    /// back as `Ok(Err(_))`; anything else is an error.
    fn put_head(
        &self,
        token: &str,
        vault_id: Id,
        head: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>>;
    /// `GET /v1/vaults/{vault}/items?since=R`.
    fn items_since(&self, token: &str, vault_id: Id, since: u64) -> Result<ItemsSince>;
    /// `PUT /v1/vaults/{vault}/items/{item}`. Stale comes back as
    /// [`PushReply::Stale`], not an error.
    fn push_item(
        &self,
        token: &str,
        vault_id: Id,
        item_id: Id,
        push: &ItemPush,
    ) -> Result<PushReply>;
    /// `PUT /v1/vaults/{vault}/blobs/{hash}`.
    fn put_blob(
        &self,
        token: &str,
        vault_id: Id,
        hash: Hash32,
        bytes: &[u8],
    ) -> Result<BlobAccepted>;
    /// `GET /v1/vaults/{vault}/blobs/{hash}`. `None` when the server
    /// doesn't hold it.
    fn get_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>>;
    /// `HEAD /v1/vaults/{vault}/blobs/{hash}`.
    fn has_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<bool>;
}

/// The HTTPS transport.
pub struct HttpTransport {
    base: String,
    client: reqwest::blocking::Client,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HttpTransport({})", self.base)
    }
}

/// Installs rustls's `ring` provider once per process. Harmless if another
/// part of the program installed a provider already.
fn ensure_tls_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

impl HttpTransport {
    /// A transport for the server at `base` (`https://host[:port]`, no
    /// trailing slash needed).
    pub fn new(base: &str) -> Result<Self> {
        ensure_tls_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("lockdown/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// The server's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE);
        if let Some(t) = token {
            req = req.header(reqwest::header::AUTHORIZATION, format!("{BEARER}{t}"));
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        let res = req
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let status = res.status().as_u16();
        let bytes = res
            .bytes()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        Ok((status, bytes.to_vec()))
    }

    /// Like `send`, with an `application/octet-stream` body: blobs.
    fn send_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream");
        if let Some(t) = token {
            req = req.header(reqwest::header::AUTHORIZATION, format!("{BEARER}{t}"));
        }
        if let Some(b) = body {
            req = req.body(b);
        }
        let res = req
            .send()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        let status = res.status().as_u16();
        let bytes = res
            .bytes()
            .map_err(|e| TransportError::Network(e.to_string()))?;
        Ok((status, bytes.to_vec()))
    }

    fn refused(status: u16, bytes: &[u8]) -> TransportError {
        match wire::decode::<ErrorBody>(bytes) {
            Ok(body) => TransportError::Refused {
                status,
                code: body.code,
                message: body.message,
                head: wire::decode::<HeadRefused>(bytes).ok(),
            },
            Err(_) => TransportError::Refused {
                status,
                code: ErrorCode::Internal,
                message: format!(
                    "HTTP {status}: {}",
                    String::from_utf8_lossy(bytes)
                        .chars()
                        .take(200)
                        .collect::<String>()
                ),
                head: None,
            },
        }
    }

    fn call<T: Serialize, R: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
    ) -> Result<R> {
        let body = match body {
            Some(b) => Some(wire::encode(b).map_err(|e| TransportError::Decode(e.to_string()))?),
            None => None,
        };
        let (status, bytes) = self.send(method, path, token, body)?;
        if (200..300).contains(&status) {
            wire::decode(&bytes).map_err(|e| TransportError::Decode(format!("{path}: {e}")))
        } else {
            Err(Self::refused(status, &bytes))
        }
    }

    fn call_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<u8>)> {
        let (status, bytes) = self.send(method, path, token, body)?;
        if (200..300).contains(&status) {
            Ok((status, bytes))
        } else {
            Err(Self::refused(status, &bytes))
        }
    }
}

fn auth_error(e: TransportError) -> lockdown_crypto::Error {
    match e {
        TransportError::Refused {
            code: ErrorCode::LoginFailed,
            ..
        } => lockdown_crypto::Error::LoginFailed,
        other => lockdown_crypto::Error::Invalid(other.to_string()),
    }
}

impl AuthRounds for HttpTransport {
    fn register_start(&mut self, login: &str, request: &[u8]) -> lockdown_crypto::Result<Vec<u8>> {
        let reply: RegisterStartReply = self
            .call(
                reqwest::Method::POST,
                "/v1/account/register/start",
                None,
                Some(&RegisterStart {
                    login: login.into(),
                    request: request.to_vec(),
                }),
            )
            .map_err(auth_error)?;
        Ok(reply.response)
    }

    fn register_finish(
        &mut self,
        login: &str,
        profile_id: &Id,
        upload: &[u8],
    ) -> lockdown_crypto::Result<()> {
        let body = wire::encode(&RegisterFinish {
            login: login.into(),
            profile_id: *profile_id,
            upload: upload.to_vec(),
        })?;
        self.call_raw(
            reqwest::Method::POST,
            "/v1/account/register/finish",
            None,
            Some(body),
        )
        .map_err(auth_error)?;
        Ok(())
    }

    fn login_start(
        &mut self,
        login: &str,
        request: &[u8],
    ) -> lockdown_crypto::Result<LoginStarted> {
        let reply: LoginStartReply = self
            .call(
                reqwest::Method::POST,
                "/v1/account/login/start",
                None,
                Some(&LoginStart {
                    login: login.into(),
                    request: request.to_vec(),
                }),
            )
            .map_err(auth_error)?;
        Ok(LoginStarted {
            login_id: reply.login_id,
            response: reply.response,
        })
    }

    fn login_finish(
        &mut self,
        login_id: &Id,
        finalization: &[u8],
        device: &DeviceProof,
    ) -> lockdown_crypto::Result<Session> {
        let reply: LoginFinishReply = self
            .call(
                reqwest::Method::POST,
                "/v1/account/login/finish",
                None,
                Some(&LoginFinish {
                    login_id: *login_id,
                    finalization: finalization.to_vec(),
                    device: DeviceProofWire {
                        device_id: device.device_id,
                        device_pub: device.device_pub,
                        pop: device.pop.clone(),
                    },
                }),
            )
            .map_err(auth_error)?;
        Ok(Session {
            token: reply.token,
            expires_at: reply.expires_at,
            profile_id: reply.profile_id,
        })
    }
}

impl Transport for HttpTransport {
    fn identity_since(&self, token: &str, since: u64) -> Result<IdentitySince> {
        self.call::<(), _>(
            reqwest::Method::GET,
            &format!("/v1/identity?since={since}"),
            Some(token),
            None,
        )
    }

    fn put_identity(
        &self,
        token: &str,
        push: &IdentityPush,
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let body = wire::encode(push).map_err(|e| TransportError::Decode(e.to_string()))?;
        let (status, bytes) = self.send(
            reqwest::Method::PUT,
            "/v1/identity",
            Some(token),
            Some(body),
        )?;
        if (200..300).contains(&status) {
            return Ok(Ok(
                wire::decode(&bytes).map_err(|e| TransportError::Decode(e.to_string()))?
            ));
        }
        if status == 409
            && let Ok(refused) = wire::decode::<HeadRefused>(&bytes)
        {
            return Ok(Err(refused));
        }
        Err(Self::refused(status, &bytes))
    }

    fn list_vaults(&self, token: &str) -> Result<VaultList> {
        self.call::<(), _>(reqwest::Method::GET, "/v1/vaults", Some(token), None)
    }

    fn put_vault(&self, token: &str, vault_id: Id, record: &[u8]) -> Result<()> {
        self.call_raw(
            reqwest::Method::PUT,
            &format!("/v1/vaults/{vault_id}"),
            Some(token),
            Some(record.to_vec()),
        )?;
        Ok(())
    }

    fn heads_since(&self, token: &str, vault_id: Id, since: u64) -> Result<HeadsSince> {
        self.call::<(), _>(
            reqwest::Method::GET,
            &format!("/v1/vaults/{vault_id}/heads?since={since}"),
            Some(token),
            None,
        )
    }

    fn put_head(
        &self,
        token: &str,
        vault_id: Id,
        head: &[u8],
    ) -> Result<std::result::Result<HeadAccepted, HeadRefused>> {
        let (status, bytes) = self.send(
            reqwest::Method::PUT,
            &format!("/v1/vaults/{vault_id}/heads"),
            Some(token),
            Some(head.to_vec()),
        )?;
        if (200..300).contains(&status) {
            return Ok(Ok(
                wire::decode(&bytes).map_err(|e| TransportError::Decode(e.to_string()))?
            ));
        }
        if status == 409
            && let Ok(refused) = wire::decode::<HeadRefused>(&bytes)
        {
            return Ok(Err(refused));
        }
        Err(Self::refused(status, &bytes))
    }

    fn items_since(&self, token: &str, vault_id: Id, since: u64) -> Result<ItemsSince> {
        self.call::<(), _>(
            reqwest::Method::GET,
            &format!("/v1/vaults/{vault_id}/items?since={since}"),
            Some(token),
            None,
        )
    }

    fn push_item(
        &self,
        token: &str,
        vault_id: Id,
        item_id: Id,
        push: &ItemPush,
    ) -> Result<PushReply> {
        let body = wire::encode(push).map_err(|e| TransportError::Decode(e.to_string()))?;
        let (status, bytes) = self.send(
            reqwest::Method::PUT,
            &format!("/v1/vaults/{vault_id}/items/{item_id}"),
            Some(token),
            Some(body),
        )?;
        if ((200..300).contains(&status) || status == 409)
            && let Ok(reply) = wire::decode::<PushReply>(&bytes)
        {
            return Ok(reply);
        }
        Err(Self::refused(status, &bytes))
    }

    fn put_blob(
        &self,
        token: &str,
        vault_id: Id,
        hash: Hash32,
        bytes: &[u8],
    ) -> Result<BlobAccepted> {
        let (status, body) = self.send_raw(
            reqwest::Method::PUT,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            Some(bytes.to_vec()),
        )?;
        if (200..300).contains(&status) {
            return wire::decode(&body).map_err(|e| TransportError::Decode(e.to_string()));
        }
        Err(Self::refused(status, &body))
    }

    fn get_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>> {
        let (status, body) = self.send_raw(
            reqwest::Method::GET,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            None,
        )?;
        if status == 404 {
            return Ok(None);
        }
        if (200..300).contains(&status) {
            return Ok(Some(body));
        }
        Err(Self::refused(status, &body))
    }

    fn has_blob(&self, token: &str, vault_id: Id, hash: Hash32) -> Result<bool> {
        let (status, body) = self.send_raw(
            reqwest::Method::HEAD,
            &format!("/v1/vaults/{vault_id}/blobs/{hash}"),
            Some(token),
            None,
        )?;
        if status == 404 {
            return Ok(false);
        }
        if (200..300).contains(&status) {
            return Ok(true);
        }
        Err(Self::refused(status, &body))
    }
}
