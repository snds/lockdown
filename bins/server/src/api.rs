//! The handlers. Bodies are CBOR ([`lockdown_sync::wire`]); every refusal
//! is an HTTP error status with an [`ErrorBody`] a client can act on.
//!
//! Rules enforced here, in plain words:
//! - A login attempt is rate-limited per client address and per login
//!   name; an unknown login gets a decoy answer, so the only thing a
//!   wrong guess learns is "failed".
//! - A session token names an account and a device; everything under
//!   `/v1/vaults` needs one, and only the owning account sees a vault.
//! - A manifest head is accepted when it is signed by a device the
//!   account logged in with, its `seq` is exactly one past the server's,
//!   its `prev_hash` is the server's head, and every entry matches an
//!   item the server holds at that revision. Otherwise: `unknown_device`,
//!   `bad_signature`, `gap`, `older_seq`, `fork`, `unknown_item`.
//! - An item push is accepted when `if_match` is the item's current
//!   revision (zero for a new one) and its latest version is signed by a
//!   device of the account; otherwise `stale` with the server's copy, and
//!   the client merges.

use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, FromRequest, FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::auth::{OpaqueServer, device_pop_message};
use lockdown_sync::wire::{
    self, BEARER, CONTENT_TYPE, ErrorBody, ErrorCode, HeadAccepted, HeadRefused, HeadsSince,
    ItemPush, ItemWire, ItemsSince, LoginFinish, LoginFinishReply, LoginStart, LoginStartReply,
    PushReply, RegisterFinish, RegisterStart, RegisterStartReply, VaultList, VaultSummary,
};
use lockdown_vault::item::ITEM_V;
use lockdown_vault::manifest::MANIFEST_V;
use lockdown_vault::{Id, Item, ManifestHead, VaultRecord};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::storage::{PushResult, StorageError, VaultRow};
use crate::token::TokenError;
use crate::{AppState, Discovery, PENDING_MAX, PENDING_TTL, PendingLogin};

// --- responses and errors -------------------------------------------------

/// A CBOR body, in or out.
pub struct Cbor<T>(pub T);

impl<T: serde::Serialize> IntoResponse for Cbor<T> {
    fn into_response(self) -> Response {
        match wire::encode(&self.0) {
            Ok(bytes) => (
                [(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE))],
                bytes,
            )
                .into_response(),
            Err(e) => ApiError::internal(format!("couldn't encode the reply: {e}")).into_response(),
        }
    }
}

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Cbor<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| ApiError::bad_request(format!("couldn't read the body: {e}")))?;
        wire::decode(&bytes)
            .map(Cbor)
            .map_err(|_| ApiError::bad_request("the body isn't the CBOR this endpoint expects"))
    }
}

/// A refusal: status, code, message, and for a head the server's position.
#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
    retry_after: Option<u64>,
    head: Option<HeadRefused>,
}

impl ApiError {
    fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
            head: None,
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::BadRequest, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            message,
        )
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, message)
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ErrorCode::Forbidden, message)
    }

    fn login_failed() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::LoginFailed,
            "the login didn't go through: check the login name and password",
        )
    }

    fn rate_limited(wait: std::time::Duration) -> Self {
        let secs = wait.as_secs().max(1);
        Self {
            retry_after: Some(secs),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimited,
                format!("too many attempts; try again in {secs} s"),
            )
        }
    }

    fn head_refused(refused: HeadRefused) -> Self {
        Self {
            head: Some(refused.clone()),
            ..Self::new(StatusCode::CONFLICT, refused.code, refused.message)
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::LoginTaken => Self::new(
                StatusCode::CONFLICT,
                ErrorCode::LoginTaken,
                "that login name is already an account; log in instead",
            ),
            StorageError::DeviceKeyMismatch(_) => Self::forbidden(
                "this device id is already registered to the account with another key",
            ),
            StorageError::Forbidden(_) => Self::forbidden("that vault belongs to another account"),
            StorageError::NotFound(what) => Self::not_found(format!("{what} wasn't found")),
            other => Self::internal(other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = match &self.head {
            Some(refused) => wire::encode(refused),
            None => wire::encode(&ErrorBody {
                code: self.code,
                message: self.message.clone(),
            }),
        };
        let mut response = match body {
            Ok(bytes) => (
                self.status,
                [(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE))],
                bytes,
            )
                .into_response(),
            Err(_) => (self.status, self.message).into_response(),
        };
        if let Some(secs) = self.retry_after
            && let Ok(v) = HeaderValue::from_str(&secs.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, v);
        }
        response
    }
}

type ApiResult<T> = Result<T, ApiError>;

// --- extractors -----------------------------------------------------------

/// The client's address as a rate-limit key; `local` when the server was
/// driven without a socket (tests).
pub struct Peer(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Peer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Peer(
            parts
                .extensions
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0.ip().to_string())
                .unwrap_or_else(|| "local".to_string()),
        ))
    }
}

/// A checked session: which account and device the token stands for.
#[derive(Clone, Copy, Debug)]
pub struct Session {
    /// The account.
    pub account_id: Id,
    /// The device that logged in.
    pub device_id: Id,
}

impl FromRequestParts<Arc<AppState>> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix(BEARER))
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::Unauthenticated,
                    "this call needs a session; log in first",
                )
            })?;
        match state.tokens.verify(header.trim(), state.now()) {
            Ok(claims) => Ok(Session {
                account_id: claims.account_id,
                device_id: claims.device_id,
            }),
            Err(TokenError::Expired) => Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::TokenExpired,
                "the session has run out; log in again",
            )),
            Err(TokenError::Invalid) => Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthenticated,
                "the session token didn't check out; log in again",
            )),
        }
    }
}

/// `?since=N`.
#[derive(Deserialize, Default)]
pub struct Since {
    /// Everything after this sequence or revision.
    #[serde(default)]
    pub since: u64,
}

// --- health and discovery ---------------------------------------------------

/// `GET /healthz`.
pub async fn healthz() -> &'static str {
    "ok"
}

/// `GET /.well-known/lockdown`.
pub async fn well_known(State(state): State<Arc<AppState>>) -> Json<Discovery> {
    Json(crate::discovery(state.config.signups_open))
}

// --- account ----------------------------------------------------------------

fn brake(state: &AppState, peer: &Peer, login: Option<&str>) -> ApiResult<()> {
    let now = Instant::now();
    state
        .per_address
        .check(&peer.0, now)
        .map_err(ApiError::rate_limited)?;
    if let Some(login) = login {
        state
            .per_login
            .check(login, now)
            .map_err(ApiError::rate_limited)?;
    }
    Ok(())
}

fn check_login_name(login: &str) -> ApiResult<()> {
    let ok = !login.is_empty()
        && login.chars().count() <= 254
        && !login.chars().any(char::is_control)
        && login.trim() == login;
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "a login name is 1 to 254 characters with no control characters or outer spaces",
        ))
    }
}

/// `POST /v1/account/register/start`.
pub async fn register_start(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<RegisterStart>,
) -> ApiResult<Cbor<RegisterStartReply>> {
    check_login_name(&body.login)?;
    brake(&state, &peer, Some(&body.login))?;
    if !state.config.signups_open {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::SignupsClosed,
            "this server isn't taking new accounts",
        ));
    }
    if state.storage.account_by_login(&body.login)?.is_some() {
        return Err(StorageError::LoginTaken.into());
    }
    let response = state
        .opaque
        .registration_response(body.login.as_bytes(), &body.request)
        .map_err(|_| ApiError::bad_request("the registration request didn't decode"))?;
    Ok(Cbor(RegisterStartReply { response }))
}

/// `POST /v1/account/register/finish`.
pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<RegisterFinish>,
) -> ApiResult<StatusCode> {
    check_login_name(&body.login)?;
    brake(&state, &peer, Some(&body.login))?;
    if !state.config.signups_open {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::SignupsClosed,
            "this server isn't taking new accounts",
        ));
    }
    let file = OpaqueServer::password_file(&body.upload)
        .map_err(|_| ApiError::bad_request("the registration upload didn't decode"))?;
    state
        .storage
        .create_account(&body.login, body.profile_id, &file)?;
    Ok(StatusCode::CREATED)
}

fn prune_pending(pending: &mut std::collections::HashMap<Id, PendingLogin>, now: Instant) {
    pending.retain(|_, p| now.duration_since(p.started) < PENDING_TTL);
    if pending.len() >= PENDING_MAX {
        // Under a flood, the oldest attempts go; a real client retries.
        let mut by_age: Vec<(Id, Instant)> = pending.iter().map(|(k, p)| (*k, p.started)).collect();
        by_age.sort_by_key(|(_, started)| *started);
        for (id, _) in by_age.iter().take(pending.len() - PENDING_MAX + 1) {
            pending.remove(id);
        }
    }
}

/// `POST /v1/account/login/start`.
pub async fn login_start(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<LoginStart>,
) -> ApiResult<Cbor<LoginStartReply>> {
    check_login_name(&body.login)?;
    brake(&state, &peer, Some(&body.login))?;
    let account = state.storage.account_by_login(&body.login)?;
    let (response, server_state) = state
        .opaque
        .login_response(
            body.login.as_bytes(),
            account.as_ref().map(|a| a.password_file.as_slice()),
            &body.request,
        )
        .map_err(|_| ApiError::bad_request("the login request didn't decode"))?;
    let login_id = Id::new();
    let now = Instant::now();
    let mut pending = state
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    prune_pending(&mut pending, now);
    pending.insert(
        login_id,
        PendingLogin {
            login: body.login,
            account,
            state: server_state,
            started: now,
        },
    );
    Ok(Cbor(LoginStartReply { login_id, response }))
}

/// `POST /v1/account/login/finish`.
pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<LoginFinish>,
) -> ApiResult<Cbor<LoginFinishReply>> {
    brake(&state, &peer, None)?;
    let pending = {
        let mut pending = state
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune_pending(&mut pending, Instant::now());
        pending.remove(&body.login_id)
    };
    let Some(pending) = pending else {
        return Err(ApiError::login_failed());
    };
    brake(&state, &peer, Some(&pending.login))?;
    OpaqueServer::login_finish(&pending.state, &body.finalization)
        .map_err(|_| ApiError::login_failed())?;
    // A decoy login never gets here with a valid finalization, but the
    // answer is the same either way.
    let Some(account) = pending.account else {
        return Err(ApiError::login_failed());
    };
    let key = VerifyingKey::from_bytes(&body.device.device_pub)
        .map_err(|_| ApiError::bad_request("the device key isn't an Ed25519 public key"))?;
    key.verify(
        &device_pop_message(&pending.login, &body.login_id),
        &body.device.pop,
        "device proof",
    )
    .map_err(|_| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::BadSignature,
            "the device didn't prove it holds its key",
        )
    })?;
    state.storage.register_device(
        account.account_id,
        body.device.device_id,
        &body.device.device_pub,
    )?;
    let ttl = i64::try_from(state.config.session_ttl.as_secs()).unwrap_or(i64::MAX);
    let (token, expires_at) = state
        .tokens
        .issue(account.account_id, body.device.device_id, state.now(), ttl)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Cbor(LoginFinishReply {
        token,
        expires_at,
        profile_id: account.profile_id,
    }))
}

/// `PUT /v1/account/identity`: the identity backup, opaque to the server.
pub async fn put_identity(
    State(state): State<Arc<AppState>>,
    session: Session,
    body: Bytes,
) -> ApiResult<StatusCode> {
    if body.is_empty() {
        return Err(ApiError::bad_request("an identity backup can't be empty"));
    }
    state
        .storage
        .put_identity_backup(session.account_id, &body)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/account/identity`.
pub async fn get_identity(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Response> {
    let body = state
        .storage
        .identity_backup(session.account_id)?
        .ok_or_else(|| ApiError::not_found("this account has no identity backup yet"))?;
    Ok((
        [(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE))],
        body,
    )
        .into_response())
}

// --- vaults -----------------------------------------------------------------

fn owned_vault(state: &AppState, session: &Session, vault_id: Id) -> ApiResult<VaultRow> {
    let row = state
        .storage
        .vault(vault_id)?
        .ok_or_else(|| ApiError::not_found(format!("vault {vault_id} isn't on this server")))?;
    if row.account_id != session.account_id {
        return Err(ApiError::forbidden("that vault belongs to another account"));
    }
    Ok(row)
}

fn device_key(state: &AppState, session: &Session, device_id: Id) -> ApiResult<VerifyingKey> {
    let key = state
        .storage
        .device_pub(session.account_id, device_id)?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::UnknownDevice,
                format!(
                    "device {device_id} hasn't logged in to this account; it can't sign for it"
                ),
            )
        })?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| ApiError::internal("a stored device key is unusable"))
}

/// `GET /v1/vaults`.
pub async fn list_vaults(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<VaultList>> {
    let vaults = state
        .storage
        .vaults_of(session.account_id)?
        .into_iter()
        .map(|v| VaultSummary {
            vault_id: v.vault_id,
            record: v.record,
            head_seq: v.head_seq,
            head_hash: v.head_hash,
            item_rev: v.item_rev,
        })
        .collect();
    Ok(Cbor(VaultList { vaults }))
}

/// `PUT /v1/vaults/{vault}`: claim a vault for the account, or replace
/// its record.
pub async fn put_vault(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let record = VaultRecord::from_bytes(&body)
        .map_err(|_| ApiError::bad_request("the body isn't a vault record"))?;
    if record.context.vault_id != vault_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::WrongVault,
            "the record is for another vault than the path names",
        ));
    }
    let existed = state.storage.vault(vault_id)?.is_some();
    state
        .storage
        .put_vault(session.account_id, vault_id, &body)?;
    Ok(if existed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CREATED
    })
}

/// `GET /v1/vaults/{vault}/heads?since=N`.
pub async fn heads_since(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    Query(since): Query<Since>,
) -> ApiResult<Cbor<HeadsSince>> {
    let vault = owned_vault(&state, &session, vault_id)?;
    let heads = state
        .storage
        .heads_since(vault_id, since.since)?
        .into_iter()
        .map(|h| serde_bytes::ByteBuf::from(h.body))
        .collect();
    Ok(Cbor(HeadsSince {
        heads,
        current_seq: vault.head_seq,
        current_hash: vault.head_hash,
    }))
}

/// `PUT /v1/vaults/{vault}/heads`.
pub async fn put_head(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    body: Bytes,
) -> ApiResult<Cbor<HeadAccepted>> {
    let head = ManifestHead::from_bytes(&body)
        .map_err(|_| ApiError::bad_request("the body isn't a manifest head"))?;
    if head.vault_id != vault_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::WrongVault,
            "the head is for another vault than the path names",
        ));
    }
    if head.v > MANIFEST_V {
        return Err(ApiError::bad_request(format!(
            "manifest format {} is newer than this server reads ({MANIFEST_V})",
            head.v
        )));
    }
    if head.checkpoint.is_some() {
        return Err(ApiError::bad_request("a pushed head carries no checkpoint"));
    }
    let mut ids: Vec<Id> = head.entries.iter().map(|e| e.item_id).collect();
    ids.dedup();
    if ids.len() != head.entries.len() || !ids.is_sorted() {
        return Err(ApiError::bad_request(
            "a head's entries are sorted and unique by item id",
        ));
    }
    owned_vault(&state, &session, vault_id)?;
    let key = device_key(&state, &session, head.signer)?;
    head.verify_signature(&key).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::BadSignature,
            "the head's device signature didn't check out",
        )
    })?;
    match state.storage.accept_head(vault_id, &head, &body)? {
        Ok(accepted) => Ok(Cbor(accepted)),
        Err(refused) => Err(ApiError::head_refused(refused)),
    }
}

/// `GET /v1/vaults/{vault}/items?since=R`.
pub async fn items_since(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(vault_id): Path<Id>,
    Query(since): Query<Since>,
) -> ApiResult<Cbor<ItemsSince>> {
    let vault = owned_vault(&state, &session, vault_id)?;
    let items = state
        .storage
        .items_since(vault_id, since.since)?
        .into_iter()
        .map(|i| ItemWire {
            rev: i.rev,
            item: i.body,
        })
        .collect();
    Ok(Cbor(ItemsSince {
        items,
        current_rev: vault.item_rev,
    }))
}

/// `GET /v1/vaults/{vault}/items/{item}`.
pub async fn get_item(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path((vault_id, item_id)): Path<(Id, Id)>,
) -> ApiResult<Cbor<ItemWire>> {
    owned_vault(&state, &session, vault_id)?;
    let row = state
        .storage
        .item(vault_id, item_id)?
        .ok_or_else(|| ApiError::not_found(format!("item {item_id} isn't in this vault")))?;
    Ok(Cbor(ItemWire {
        rev: row.rev,
        item: row.body,
    }))
}

/// `PUT /v1/vaults/{vault}/items/{item}`.
pub async fn push_item(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path((vault_id, item_id)): Path<(Id, Id)>,
    Cbor(push): Cbor<ItemPush>,
) -> ApiResult<Response> {
    let item = Item::from_bytes(&push.item)
        .map_err(|_| ApiError::bad_request("the body isn't an item envelope"))?;
    if item.vault_id != vault_id || item.item_id != item_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::WrongVault,
            "the envelope names another vault or item than the path",
        ));
    }
    if item.v > ITEM_V {
        return Err(ApiError::bad_request(format!(
            "item format {} is newer than this server reads ({ITEM_V})",
            item.v
        )));
    }
    owned_vault(&state, &session, vault_id)?;
    let latest = item
        .latest()
        .map_err(|_| ApiError::bad_request("an item carries at least one version"))?;
    let key = device_key(&state, &session, latest.edited_by)?;
    latest.verify(&item.header(), &key).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::BadSignature,
            "the latest version's device signature didn't check out",
        )
    })?;
    match state
        .storage
        .push_item(vault_id, item_id, &push.item, push.if_match)?
    {
        PushResult::Accepted(rev) => Ok(Cbor(PushReply::Accepted { rev }).into_response()),
        PushResult::Stale(current) => Ok((
            StatusCode::CONFLICT,
            Cbor(PushReply::Stale {
                current: ItemWire {
                    rev: current.rev,
                    item: current.body,
                },
            }),
        )
            .into_response()),
        PushResult::Unknown => Err(ApiError::bad_request(format!(
            "if_match is {} but the server holds no item {item_id}; a new item is pushed with if_match 0",
            push.if_match
        ))),
    }
}
