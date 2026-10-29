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
//! - An identity manifest (D71) is accepted when it is the next page of
//!   the account's identity chain, every record it lists is at hand with
//!   the hash it names, every signature checks out under the profile's
//!   Identity Signing Key (pinned by the first manifest), the profile is
//!   the account's, it was written by a device the chain has not revoked,
//!   no revocation was taken back, and every new revocation names the
//!   server's current head of every vault of the account as its boundary.
//! - Anything signed by a device the identity manifest revoked is refused
//!   with `revoked_device`: heads, items, identity manifests (D73).
//! - A blob is stored under its own hash only, within the size cap, for the
//!   account's vault; an item naming a blob the server lacks is
//!   `unknown_blob`; blobs nothing names are collected after the grace
//!   period (02-model §6).
//! - The key-transparency log (04-security §6) is public: its signed tree
//!   head, inclusion and consistency proofs need no session; an account's
//!   own entries need its session.

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
use lockdown_sync::kt::{
    AccountEntries, ConsistencyProofWire, InclusionProofWire, LoggedEntry, SthWire,
};
use lockdown_sync::wire::{
    self, BEARER, BlobAccepted, CONTENT_TYPE, ErrorBody, ErrorCode, HeadAccepted, HeadRefused,
    HeadsSince, IdentityPush, IdentitySince, ItemPush, ItemWire, ItemsSince, LoginFinish,
    LoginFinishReply, LoginStart, LoginStartReply, PushReply, RecordWire, RegisterFinish,
    RegisterStart, RegisterStartReply, VaultList, VaultSummary,
};
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};
use lockdown_vault::item::ITEM_V;
use lockdown_vault::manifest::MANIFEST_V;
use lockdown_vault::transparency::{SignedTreeHead, TreeHead};
use lockdown_vault::{Hash32, Id, Item, ManifestHead, VaultRecord};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::storage::{IdentityAccept, PushResult, RecordRow, RevocationRow, StorageError};
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
    pub(crate) fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            retry_after: None,
            head: None,
        }
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::BadRequest, message)
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            message,
        )
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, message)
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ErrorCode::Forbidden, message)
    }

    fn login_failed() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::LoginFailed,
            "the login didn't go through: check the login name and password",
        )
    }

    pub(crate) fn rate_limited(wait: std::time::Duration) -> Self {
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

    pub(crate) fn head_refused(refused: HeadRefused) -> Self {
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

pub(crate) type ApiResult<T> = Result<T, ApiError>;

// --- extractors -----------------------------------------------------------

/// The client's address as a rate-limit key; `local` when the server was
/// driven without a socket (tests). Behind a trusted reverse proxy it is
/// the address the proxy saw (`client_addr`).
pub struct Peer(pub String);

/// The client's address for `parts`, or `None` without a socket.
pub fn client_address(parts: &Parts, state: &AppState) -> Option<String> {
    let peer = parts
        .extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()?
        .0
        .ip();
    let forwarded = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    Some(crate::client_addr::client_ip(peer, forwarded, &state.config.trusted_proxies).to_string())
}

impl FromRequestParts<Arc<AppState>> for Peer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(Peer(
            client_address(parts, state).unwrap_or_else(|| "local".to_string()),
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
    /// The session's id (the token's nonce).
    pub session_id: [u8; 16],
    /// When the token runs out.
    pub expires_at: i64,
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
            Ok(claims) => {
                check_session(parts, state, &claims)?;
                Ok(Session {
                    account_id: claims.account_id,
                    device_id: claims.device_id,
                    session_id: claims.nonce,
                    expires_at: claims.exp,
                })
            }
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

/// A token that verifies still needs its session not to have been ended
/// (`sessions`); its "last used" moves on at most once a minute. A token
/// from before the sessions table gets its row here.
fn check_session(
    parts: &Parts,
    state: &AppState,
    claims: &crate::token::TokenClaims,
) -> Result<(), ApiError> {
    let Some(store) = state.storage.sessions() else {
        return Ok(());
    };
    let now = state.now();
    let address = client_address(parts, state);
    let client = crate::sessions::client_label(
        parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok()),
    );
    match store.session(&claims.nonce)? {
        // Renewed by its own device (`renewal`): that device holds the
        // newer token; this one is done, without the words of an ending.
        Some(row) if row.ended_at.is_some() && row.renewed_to.is_some() => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Unauthenticated,
            "this session was renewed; the device holds a newer one",
        )),
        Some(row) if row.ended_at.is_some() => {
            let revoked = state
                .storage
                .revoked_after(claims.account_id, claims.device_id, claims.device_id)?
                .is_some();
            // A device whose sessions ended with its revocation may still
            // read the signed identity record set, and nothing else: that
            // is how it learns, from records it can verify, that it is out
            // and shuts its own copy (D73).
            if revoked
                && parts.method == axum::http::Method::GET
                && parts.uri.path() == "/v1/identity"
            {
                return Ok(());
            }
            Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthenticated,
                if revoked {
                    "this device was revoked from the identity, and its sessions ended with it"
                } else {
                    "this session was ended from one of your devices; log in again"
                },
            ))
        }
        Some(row) => {
            if now.saturating_sub(row.last_seen) >= crate::sessions::TOUCH_EVERY_SECS
                || row.address != address
            {
                store.touch_session(&claims.nonce, now, address.as_deref(), client.as_deref())?;
            }
            Ok(())
        }
        None => {
            store.open_session(&crate::sessions::SessionRow {
                id: claims.nonce,
                account_id: claims.account_id,
                device_id: claims.device_id,
                created_at: claims.iat,
                last_seen: now,
                expires_at: claims.exp,
                address,
                client,
                ended_at: None,
                renewed_to: None,
            })?;
            Ok(())
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
    Json(crate::discovery(
        state.config.signups_open,
        &state.log_key.verifying_key(),
    ))
}

// --- the key-transparency log ----------------------------------------------

/// `?index=I&size=N`.
#[derive(Deserialize, Default)]
pub struct InclusionQuery {
    /// The leaf.
    #[serde(default)]
    pub index: u64,
    /// The tree size the proof is for.
    #[serde(default)]
    pub size: u64,
}

/// `?from=M&to=N`.
#[derive(Deserialize, Default)]
pub struct ConsistencyQuery {
    /// The older size.
    #[serde(default)]
    pub from: u64,
    /// The newer size.
    #[serde(default)]
    pub to: u64,
}

/// `?from=I`.
#[derive(Deserialize, Default)]
pub struct FromQuery {
    /// The first index wanted.
    #[serde(default)]
    pub from: u64,
}

/// The log's current tree head, signed now.
pub fn current_sth(state: &AppState) -> ApiResult<SignedTreeHead> {
    let leaves = state.storage.log_leaf_hashes()?;
    let head = TreeHead {
        size: leaves.len() as u64,
        root: lockdown_crypto::merkle::root(&leaves),
        time: state.now(),
    };
    SignedTreeHead::sign_hybrid(head, &state.log_key, &state.pq_log_key)
        .map_err(|e| ApiError::internal(format!("couldn't sign the tree head: {e}")))
}

/// `GET /v1/kt/sth`.
pub async fn kt_sth(State(state): State<Arc<AppState>>) -> ApiResult<Cbor<SthWire>> {
    Ok(Cbor(SthWire {
        sth: current_sth(&state)?,
        log_key: state.log_key.verifying_key().to_bytes(),
        pq_log_key: Some(serde_bytes::ByteBuf::from(
            state.pq_log_key.verifying_key().as_bytes().to_vec(),
        )),
    }))
}

/// `GET /v1/kt/inclusion?index=I&size=N`.
pub async fn kt_inclusion(
    State(state): State<Arc<AppState>>,
    Query(q): Query<InclusionQuery>,
) -> ApiResult<Cbor<InclusionProofWire>> {
    let leaves = state.storage.log_leaf_hashes()?;
    let size = usize::try_from(q.size).unwrap_or(usize::MAX);
    if q.size == 0 || size > leaves.len() {
        return Err(ApiError::not_found(format!(
            "the log has {} entries; no tree of size {}",
            leaves.len(),
            q.size
        )));
    }
    let path =
        lockdown_crypto::merkle::inclusion_proof(&leaves[..size], q.index).ok_or_else(|| {
            ApiError::not_found(format!("no entry {} in a tree of {}", q.index, q.size))
        })?;
    Ok(Cbor(InclusionProofWire {
        index: q.index,
        size: q.size,
        path,
    }))
}

/// `GET /v1/kt/consistency?from=M&to=N`.
pub async fn kt_consistency(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ConsistencyQuery>,
) -> ApiResult<Cbor<ConsistencyProofWire>> {
    let leaves = state.storage.log_leaf_hashes()?;
    let to = usize::try_from(q.to).unwrap_or(usize::MAX);
    if to > leaves.len() || q.from > q.to {
        return Err(ApiError::not_found(format!(
            "the log has {} entries; no consistency proof from {} to {}",
            leaves.len(),
            q.from,
            q.to
        )));
    }
    let path = lockdown_crypto::merkle::consistency_proof(&leaves[..to], q.from)
        .ok_or_else(|| ApiError::not_found("no such sizes in the log"))?;
    Ok(Cbor(ConsistencyProofWire {
        from: q.from,
        to: q.to,
        path,
    }))
}

/// `GET /v1/kt/entries?from=I`: the session's account's own leaves.
pub async fn kt_entries(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<FromQuery>,
) -> ApiResult<Cbor<AccountEntries>> {
    let entries = state
        .storage
        .log_entries(session.account_id, q.from)?
        .into_iter()
        .map(|r| LoggedEntry {
            index: r.index,
            leaf: r.body,
        })
        .collect();
    Ok(Cbor(AccountEntries {
        account_id: session.account_id,
        entries,
        size: state.storage.log_size()?,
    }))
}

// --- account ----------------------------------------------------------------

pub(crate) fn brake(state: &AppState, peer: &Peer, login: Option<&str>) -> ApiResult<()> {
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
    // Closed sign-ups still take a login name the operator invited.
    if !state.config.signups_open && !crate::admin::invited(&state, &body.login) {
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
    // Closed sign-ups still take a login name the operator invited.
    if !state.config.signups_open && !crate::admin::invited(&state, &body.login) {
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
    if !state.config.signups_open {
        crate::admin::invite_used(&state, &body.login);
    }
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
    let ksf = crate::second_factor::announced_ksf(&state, account.as_ref());
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
    Ok(Cbor(LoginStartReply {
        login_id,
        response,
        ksf,
    }))
}

/// `POST /v1/account/login/finish`.
pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: axum::http::HeaderMap,
    Cbor(body): Cbor<LoginFinish>,
) -> ApiResult<Cbor<LoginFinishReply>> {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
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
    // A device the identity revoked doesn't sign back in: not with the
    // password, and it isn't asked for a key or an email it could never use
    // (docs/plan/35-revoke-split.md §1).
    crate::revoked_sign_in::refuse_revoked(&state, account.account_id, body.device.device_id)?;
    // An account with security keys wants one from a device it has never
    // seen (docs/plan/22-security-depth.md §4); a known device's key is the
    // second factor there.
    if crate::second_factor::required(&state, account.account_id, body.device.device_id)? {
        return Ok(Cbor(crate::second_factor::begin(
            &state,
            &headers,
            account,
            body.device.device_id,
            body.device.device_pub,
            &peer,
            user_agent,
        )?));
    }
    // Without keys, a server with email approval asks the account's address
    // about a device it has never seen (docs/plan/25-self-host-2.md §5).
    if crate::second_factor::email_required(&state, &account, body.device.device_id)? {
        return Ok(Cbor(crate::second_factor::begin_email(
            &state,
            account,
            body.device.device_id,
            body.device.device_pub,
            &peer,
            user_agent,
        )?));
    }
    Ok(Cbor(issue_session(
        &state,
        &account,
        body.device.device_id,
        &body.device.device_pub,
        &peer,
        user_agent,
    )?))
}

/// The end of a sign-in: the device is bound to the account (with a
/// new-device alert to the others when it is new), a session token is
/// issued and its row opened.
pub(crate) fn issue_session(
    state: &AppState,
    account: &crate::storage::Account,
    device_id: Id,
    device_pub: &[u8; 32],
    peer: &Peer,
    user_agent: Option<String>,
) -> ApiResult<LoginFinishReply> {
    // Every sign-in ends here, a second factor's included: a device revoked
    // while it waited gets no session (docs/plan/35-revoke-split.md §1).
    crate::revoked_sign_in::refuse_revoked(state, account.account_id, device_id)?;
    // A device the account has never seen, while it already has another:
    // the other devices hear about it (account alerts).
    let new_device = state
        .storage
        .device_pub(account.account_id, device_id)?
        .is_none();
    state
        .storage
        .register_device(account.account_id, device_id, device_pub)?;
    if new_device
        && let Some(alerts) = state.storage.alerts()
        && alerts.device_count(account.account_id)? > 1
    {
        alerts.record_alert(
            account.account_id,
            lockdown_sync::wire::AlertKind::NewDevice,
            state.now(),
            Some(device_id),
            None,
        )?;
        state.live.notify(crate::live::Change::account(
            account.account_id,
            lockdown_sync::wire::LiveKind::Alerts,
        ));
        // And, with SMTP, the account's address hears too.
        if let Some(mail) = &state.mail
            && crate::breach_monitor::looks_like_email(&account.login)
        {
            mail.outbox.queue(crate::mail::new_device(
                &account.login,
                &crate::mail::SignInFacts {
                    device: crate::sessions::client_label(user_agent.as_deref())
                        .unwrap_or_else(|| "a new device".into()),
                    address: peer.0.clone(),
                    at: state.now(),
                },
                &mail.public_url,
            ));
        }
    }
    let ttl = i64::try_from(state.config.session_ttl.as_secs()).unwrap_or(i64::MAX);
    let (token, claims) = state
        .tokens
        .issue_claims(account.account_id, device_id, state.now(), ttl)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if let Some(store) = state.storage.sessions() {
        store.open_session(&crate::sessions::SessionRow {
            id: claims.nonce,
            account_id: account.account_id,
            device_id,
            created_at: claims.iat,
            last_seen: claims.iat,
            expires_at: claims.exp,
            address: (peer.0 != "local").then(|| peer.0.clone()),
            client: crate::sessions::client_label(user_agent.as_deref()),
            ended_at: None,
            renewed_to: None,
        })?;
        let _ = store.prune_sessions(state.now(), crate::renewal::idle_secs(state));
        state.live.notify(crate::live::Change::account(
            account.account_id,
            lockdown_sync::wire::LiveKind::Sessions,
        ));
    }
    Ok(LoginFinishReply {
        token,
        expires_at: claims.exp,
        profile_id: account.profile_id,
        second_factor: None,
    })
}

/// `GET /v1/identity?since=N`: the identity manifests after `N` and the
/// records the newest one lists.
pub async fn identity_since(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(since): Query<Since>,
) -> ApiResult<Cbor<IdentitySince>> {
    let st = state.storage.identity_state(session.account_id)?;
    let heads = state
        .storage
        .identity_heads_since(session.account_id, since.since)?;
    let mut records = Vec::new();
    if let Some(newest) = heads.last() {
        let manifest = IdentityManifest::from_bytes(&newest.body).map_err(|e| {
            ApiError::internal(format!("a stored identity manifest didn't decode: {e}"))
        })?;
        records = state
            .storage
            .identity_records(session.account_id, &manifest.entries)?
            .into_iter()
            .map(|r| RecordWire {
                path: r.path,
                body: r.body,
            })
            .collect();
    }
    Ok(Cbor(IdentitySince {
        manifests: heads
            .into_iter()
            .map(|h| serde_bytes::ByteBuf::from(h.body))
            .collect(),
        records,
        current_seq: st.seq,
        current_hash: st.hash,
    }))
}

/// `PUT /v1/identity`: the next identity manifest with the records the
/// server may lack (D71, D73).
pub async fn put_identity(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(push): Cbor<IdentityPush>,
) -> ApiResult<Cbor<HeadAccepted>> {
    let manifest = IdentityManifest::from_bytes(&push.manifest)
        .map_err(|_| ApiError::bad_request("the body isn't an identity manifest"))?;
    manifest
        .check_shape()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let account = state
        .storage
        .account(session.account_id)?
        .ok_or_else(|| ApiError::not_found("this account"))?;
    if manifest.profile_id != account.profile_id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::WrongVault,
            "the identity manifest is for another profile than this account's",
        ));
    }
    let st = state.storage.identity_state(session.account_id)?;
    let refused = |code: ErrorCode, message: String| {
        ApiError::head_refused(HeadRefused {
            code,
            message,
            current_seq: st.seq,
            current_hash: st.hash,
        })
    };
    // Cheap chain checks first, so a client that is merely behind learns
    // that before anything is verified; storage re-checks atomically.
    if st.seq == 0 {
        if manifest.seq != 1 {
            return Err(refused(
                ErrorCode::Gap,
                format!(
                    "the server holds no identity manifest for this account yet; the first must be seq 1, not {}",
                    manifest.seq
                ),
            ));
        }
    } else if manifest.seq <= st.seq {
        return Err(refused(
            ErrorCode::OlderSeq,
            format!(
                "the server is already at identity manifest {}; {} is not newer",
                st.seq, manifest.seq
            ),
        ));
    } else if manifest.seq > st.seq + 1 {
        return Err(refused(
            ErrorCode::Gap,
            format!(
                "the server is at identity manifest {}; {} would skip {}",
                st.seq,
                manifest.seq,
                manifest.seq - st.seq - 1
            ),
        ));
    } else if Some(manifest.prev_hash) != st.hash {
        return Err(refused(
            ErrorCode::Fork,
            format!(
                "identity manifest {} names a predecessor that isn't the server's current one ({}); another device moved first",
                manifest.seq, st.seq
            ),
        ));
    }

    // Every listed record: pushed now (hash must match) or already held.
    let mut pushed: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    for r in push.records {
        pushed.insert(r.path, r.body);
    }
    let held = state
        .storage
        .identity_records(session.account_id, &manifest.entries)?;
    let mut records = std::collections::BTreeMap::new();
    let mut new_rows = Vec::new();
    for entry in &manifest.entries {
        if let Some(body) = pushed.remove(&entry.path) {
            if lockdown_crypto::hash::hash(&body) != entry.hash {
                return Err(ApiError::bad_request(format!(
                    "the record pushed at {} doesn't match the hash the manifest names",
                    entry.path
                )));
            }
            new_rows.push(RecordRow {
                path: entry.path.clone(),
                hash: entry.hash,
                body: body.clone(),
            });
            records.insert(entry.path.clone(), body);
        } else if let Some(row) = held.iter().find(|r| r.path == entry.path) {
            records.insert(entry.path.clone(), row.body.clone());
        } else {
            return Err(refused(
                ErrorCode::UnknownRecord,
                format!(
                    "the manifest lists {} but the server doesn't hold it; push the record with the manifest",
                    entry.path
                ),
            ));
        }
    }
    if !pushed.is_empty() {
        return Err(ApiError::bad_request(
            "a pushed record isn't listed by the manifest",
        ));
    }

    // Everything signed, under the pinned key when there is one.
    let set = RecordSet { manifest, records };
    let verified = set.verify(st.isk_pub.as_ref()).map_err(|e| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::BadSignature,
            format!("the identity record set didn't check out: {e}"),
        )
    })?;
    let manifest = &verified.manifest;

    // The chain's own rules: the writer stands, revocations only grow,
    // new boundaries are the server's current vault heads.
    let previous = match state
        .storage
        .identity_heads_since(session.account_id, st.seq.saturating_sub(1))?
        .pop()
    {
        Some(h) if st.seq > 0 => Some(IdentityManifest::from_bytes(&h.body).map_err(|e| {
            ApiError::internal(format!("a stored identity manifest didn't decode: {e}"))
        })?),
        _ => None,
    };
    if let Some(prev) = &previous {
        if prev.revocation(manifest.written_by).is_some() {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::RevokedDevice,
                format!(
                    "device {} was revoked from this identity; it can't write its records",
                    manifest.written_by
                ),
            ));
        }
        manifest
            .revocations_extend(prev)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
    }
    if manifest.revocation(manifest.written_by).is_some() {
        return Err(ApiError::bad_request(
            "a device can't write the manifest that revokes it",
        ));
    }
    let vaults = state.storage.vaults_of(session.account_id)?;
    for r in &manifest.revocations {
        let is_new = previous
            .as_ref()
            .is_none_or(|p| p.revocation(r.device_id).is_none());
        if !is_new {
            continue;
        }
        for v in &vaults {
            match r.vaults.iter().find(|b| b.vault_id == v.vault_id) {
                Some(b) if b.seq == v.head_seq => {}
                Some(b) => {
                    return Err(refused(
                        ErrorCode::BoundaryMoved,
                        format!(
                            "the revocation of device {} stops at head {} of vault {}, but the server is at {}; list the vaults again and re-sign",
                            r.device_id, b.seq, v.vault_id, v.head_seq
                        ),
                    ));
                }
                None => {
                    return Err(refused(
                        ErrorCode::BoundaryMoved,
                        format!(
                            "the revocation of device {} names no boundary for vault {}; list the vaults again and re-sign",
                            r.device_id, v.vault_id
                        ),
                    ));
                }
            }
        }
    }
    let revoked: Vec<Id> = manifest.revocations.iter().map(|r| r.device_id).collect();
    // The devices this push revokes (by the manifest, or by their record):
    // their sessions end once it is accepted (D405's follow-up).
    let newly_revoked: std::collections::BTreeSet<Id> = revoked
        .iter()
        .copied()
        .chain(
            verified
                .devices
                .iter()
                .filter(|(_, d)| d.revoked_at.is_some())
                .map(|(id, _)| *id),
        )
        .filter(|d| previous.as_ref().is_none_or(|p| p.revocation(*d).is_none()))
        .collect();
    let rows: Vec<RevocationRow> = manifest
        .revocations
        .iter()
        .flat_map(|r| {
            r.vaults.iter().map(|b| RevocationRow {
                device_id: r.device_id,
                vault_id: b.vault_id,
                boundary_seq: b.seq,
            })
        })
        .collect();
    match state.storage.accept_identity(
        session.account_id,
        IdentityAccept {
            manifest,
            body: &push.manifest,
            records: &new_rows,
            isk_pub: &verified.profile.isk_pub,
            revoked: &revoked,
            revocations: &rows,
        },
    )? {
        Ok(accepted) => {
            end_revoked_sessions(&state, session.account_id, &newly_revoked)?;
            Ok(Cbor(accepted))
        }
        Err(refused) => Err(ApiError::head_refused(refused)),
    }
}

/// Ends every session of each device just revoked, at once: its token stops
/// reading too, not only writing (D73), and its live channel is told. The
/// device can't renew either (`renewal`), so the account password is what
/// it needs next, with the words that say it was revoked.
fn end_revoked_sessions(
    state: &AppState,
    account_id: Id,
    devices: &std::collections::BTreeSet<Id>,
) -> ApiResult<()> {
    let Some(store) = state.storage.sessions() else {
        return Ok(());
    };
    let now = state.now();
    let mut ended = Vec::new();
    for device in devices {
        ended.extend(store.end_device_sessions(account_id, *device, now)?);
    }
    if !ended.is_empty() {
        announce_ended(state, account_id, &ended);
    }
    Ok(())
}

// --- vaults -----------------------------------------------------------------

/// A device the identity revoked may not sign anything new (D73).
pub(crate) fn check_not_revoked(
    state: &AppState,
    session: &Session,
    device_id: Id,
    vault_id: Id,
) -> ApiResult<()> {
    match state
        .storage
        .revoked_after(session.account_id, device_id, vault_id)?
    {
        Some(after) => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::RevokedDevice,
            format!(
                "device {device_id} was revoked from this identity (its last accepted head here was {after}); it can't write any more"
            ),
        )),
        None => Ok(()),
    }
}

pub(crate) fn device_key(
    state: &AppState,
    session: &Session,
    device_id: Id,
) -> ApiResult<VerifyingKey> {
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

/// The ML-DSA-65 key the account's newest identity profile publishes, if
/// any. The records were checked when they were pushed.
pub(crate) fn account_pq_key(
    state: &AppState,
    account_id: Id,
) -> ApiResult<Option<lockdown_crypto::pq_sign::PqVerifyingKey>> {
    let st = state.storage.identity_state(account_id)?;
    if st.seq == 0 {
        return Ok(None);
    }
    let Some(head) = state
        .storage
        .identity_heads_since(account_id, st.seq - 1)?
        .into_iter()
        .find(|h| h.seq == st.seq)
    else {
        return Ok(None);
    };
    let manifest = IdentityManifest::from_bytes(&head.body)
        .map_err(|_| ApiError::internal("a stored identity manifest didn't decode"))?;
    let refs: Vec<_> = manifest
        .entries
        .iter()
        .filter(|e| e.path == lockdown_vault::identity_set::PROFILE_PATH)
        .cloned()
        .collect();
    let Some(profile) = state
        .storage
        .identity_records(account_id, &refs)?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    lockdown_vault::pq::published_key_in(&profile.body)
        .map_err(|_| ApiError::internal("the stored profile's post-quantum key is malformed"))
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
    let vault = crate::sharing_api::readable(&state, &session, vault_id)?;
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
    crate::sharing_api::writable(&state, &session, vault_id)?;
    crate::sharing_api::check_head_generation(&state, vault_id, head.vault_key_ref)?;
    check_not_revoked(&state, &session, head.signer, vault_id)?;
    let key = device_key(&state, &session, head.signer)?;
    head.verify_signature(&key).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::BadSignature,
            "the head's device signature didn't check out",
        )
    })?;
    // A post-quantum signature, when there is one, must check out under
    // the key the account's profile publishes (docs/plan/28-pq-signatures.md).
    // Whether a head needs one is the clients' rule; the server refuses
    // only what can't be right.
    if head.pq_sig.is_some() {
        let bad = |message: &str| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::BadSignature,
                message.to_string(),
            )
        };
        let pq = account_pq_key(&state, session.account_id)?.ok_or_else(|| {
            bad("the head carries a post-quantum signature but the account publishes no post-quantum key")
        })?;
        head.verify_post_quantum(&pq)
            .map_err(|_| bad("the head's post-quantum signature didn't check out"))?;
    }
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
    let vault = crate::sharing_api::readable(&state, &session, vault_id)?;
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
    crate::sharing_api::readable(&state, &session, vault_id)?;
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
    let vault = crate::sharing_api::writable(&state, &session, vault_id)?;
    crate::sharing_api::check_item_generation(&state, vault_id, &item)?;
    let latest = item
        .latest()
        .map_err(|_| ApiError::bad_request("an item carries at least one version"))?;
    let key = crate::sharing_api::writer_key(&state, &vault, latest.edited_by)?;
    latest.verify(&item.header(), &key).map_err(|_| {
        ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::BadSignature,
            "the latest version's device signature didn't check out",
        )
    })?;
    let blobs = item_blobs(&item);
    let result = state.storage.push_item(
        vault_id,
        item_id,
        &push.item,
        push.if_match,
        &blobs,
        state.now(),
    )?;
    collect_blobs(&state);
    match result {
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
        PushResult::UnknownBlob(hash) => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::UnknownBlob,
            format!(
                "the item's attachments name blob {hash}, which the server doesn't hold; push the blob first"
            ),
        )),
    }
}

/// The blobs an item's versions still hold a key for: what the server
/// must have before it takes the item, and what keeps those blobs alive.
pub(crate) fn item_blobs(item: &Item) -> Vec<Hash32> {
    let mut out: Vec<Hash32> = Vec::new();
    for a in item
        .versions
        .iter()
        .flat_map(|v| v.attachments.iter())
        .filter(|a| !a.is_shredded())
    {
        if !out.contains(&a.blob) {
            out.push(a.blob);
        }
    }
    out
}

/// Collects blobs past their grace period. Best effort after a write; a
/// failure here never fails the write.
fn collect_blobs(state: &AppState) {
    let grace = i64::try_from(state.config.blob_grace.as_secs()).unwrap_or(i64::MAX);
    let _ = state.storage.gc_blobs(state.now(), grace);
}

fn blob_name(text: &str) -> ApiResult<Hash32> {
    Hash32::from_hex(text)
        .ok_or_else(|| ApiError::bad_request("a blob is named by 64 hex characters of BLAKE3"))
}

/// `PUT /v1/vaults/{vault}/blobs/{hash}`: content-addressed, idempotent,
/// size-capped. The body must hash to the name.
pub async fn put_blob(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path((vault_id, hash)): Path<(Id, String)>,
    body: Bytes,
) -> ApiResult<(StatusCode, Cbor<BlobAccepted>)> {
    let hash = blob_name(&hash)?;
    crate::sharing_api::writable(&state, &session, vault_id)?;
    if body.len() as u64 > state.config.max_blob_bytes {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::BlobTooLarge,
            format!(
                "this server takes attachment blobs up to {} bytes; this one is {}",
                state.config.max_blob_bytes,
                body.len()
            ),
        ));
    }
    if body.is_empty() {
        return Err(ApiError::bad_request("an empty blob isn't an attachment"));
    }
    if lockdown_crypto::hash::hash(&body) != hash {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::BlobMismatch,
            "the blob's bytes don't hash to the name in the path",
        ));
    }
    if let Some(quota) = state.config.attachment_quota
        && !state.storage.has_blob(vault_id, hash)?
        && let Some(owner) = state.storage.vault_owner(vault_id)?
    {
        // Counted against the vault's owner, whoever writes into it (a
        // shared vault's member spends the owner's room, as files in a
        // shared folder do).
        let used = state.storage.account_blob_bytes(owner)?;
        if used.saturating_add(body.len() as u64) > quota {
            let whose = if owner == session.account_id {
                "your account".to_string()
            } else {
                "this vault's owner".to_string()
            };
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorCode::QuotaExceeded,
                format!(
                    "{whose} has used {} of {} for attachments on this server; this file needs {} more",
                    lockdown_sync::bytes_words(used),
                    lockdown_sync::bytes_words(quota),
                    lockdown_sync::bytes_words(body.len() as u64)
                ),
            ));
        }
    }
    let new = state.storage.put_blob(vault_id, hash, &body, state.now())?;
    collect_blobs(&state);
    Ok((
        if new {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Cbor(BlobAccepted {
            hash,
            size: body.len() as u64,
            new,
        }),
    ))
}

/// `GET /v1/account/storage`: the attachment room the vaults this
/// account owns use, and its limit (D234).
pub async fn account_storage(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<lockdown_sync::wire::StorageUsage>> {
    Ok(Cbor(lockdown_sync::wire::StorageUsage {
        v: 1,
        used: state.storage.account_blob_bytes(session.account_id)?,
        quota: state.config.attachment_quota,
    }))
}

/// `GET /v1/vaults/{vault}/blobs/{hash}` (and `HEAD`): the bytes.
pub async fn get_blob(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path((vault_id, hash)): Path<(Id, String)>,
) -> ApiResult<Response> {
    let hash = blob_name(&hash)?;
    crate::sharing_api::readable(&state, &session, vault_id)?;
    let body = state
        .storage
        .blob(vault_id, hash)?
        .ok_or_else(|| ApiError::not_found(format!("blob {hash} isn't in this vault")))?;
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )],
        body,
    )
        .into_response())
}

// --- account alerts (docs/plan/13-security-center.md) ----------------------

fn alert_store(state: &AppState) -> ApiResult<&dyn crate::alerts::AlertStore> {
    state
        .storage
        .alerts()
        .ok_or_else(|| ApiError::not_found("account alerts on this server"))
}

/// `GET /v1/account/alerts?since=N`: the account's alerts that concern the
/// asking device.
pub async fn account_alerts(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(since): Query<Since>,
) -> ApiResult<Cbor<lockdown_sync::wire::AlertsSince>> {
    let store = alert_store(&state)?;
    Ok(Cbor(lockdown_sync::wire::AlertsSince {
        alerts: store.alerts_for(session.account_id, session.device_id, since.since)?,
        current_seq: store.alert_seq(session.account_id)?,
    }))
}

/// `POST /v1/account/alerts/{seq}/dismiss`: dismissed for every device of
/// the account.
pub async fn dismiss_alert(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(seq): Path<u64>,
) -> ApiResult<StatusCode> {
    if alert_store(&state)?.dismiss_alert(session.account_id, seq)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("alert {seq}")))
    }
}

// --- monitored addresses (docs/plan/23-bridges.md §9) -------------------------

/// `GET /v1/account/monitored`: the addresses beside the login that the
/// breach job watches for this account.
pub async fn monitored(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<lockdown_sync::wire::MonitoredAddresses>> {
    Ok(Cbor(lockdown_sync::wire::MonitoredAddresses {
        addresses: alert_store(&state)?.monitored(session.account_id)?,
    }))
}

/// `PUT /v1/account/monitored`: replaces the list. Each address must look
/// like one email address; at most `MAX_MONITORED`; kept lowercase and
/// sorted, duplicates folded.
pub async fn set_monitored(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(body): Cbor<lockdown_sync::wire::MonitoredAddresses>,
) -> ApiResult<Cbor<lockdown_sync::wire::MonitoredAddresses>> {
    if body.addresses.len() > lockdown_sync::wire::MAX_MONITORED {
        return Err(ApiError::bad_request(format!(
            "at most {} addresses can be watched",
            lockdown_sync::wire::MAX_MONITORED
        )));
    }
    let mut addresses: Vec<String> = Vec::with_capacity(body.addresses.len());
    for a in &body.addresses {
        let a = a.trim().to_ascii_lowercase();
        if a.len() > 254 || !crate::breach_monitor::looks_like_email(&a) {
            return Err(ApiError::bad_request(
                "one of the addresses isn't an email address",
            ));
        }
        addresses.push(a);
    }
    addresses.sort();
    addresses.dedup();
    let store = alert_store(&state)?;
    store.set_monitored(session.account_id, &addresses)?;
    Ok(Cbor(lockdown_sync::wire::MonitoredAddresses {
        addresses: store.monitored(session.account_id)?,
    }))
}

// --- sessions (docs/plan/20-self-host-server.md §6) --------------------------

fn session_store(state: &AppState) -> ApiResult<&dyn crate::sessions::SessionStore> {
    state
        .storage
        .sessions()
        .ok_or_else(|| ApiError::not_found("session lists on this server"))
}

/// `GET /v1/account/sessions`: the account's live sessions, oldest first,
/// the asking one marked.
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<lockdown_sync::wire::SessionList>> {
    let rows = session_store(&state)?.live_sessions(
        session.account_id,
        state.now(),
        crate::renewal::idle_secs(&state),
    )?;
    Ok(Cbor(lockdown_sync::wire::SessionList {
        sessions: rows.iter().map(|r| r.info(&session.session_id)).collect(),
    }))
}

fn announce_ended(state: &AppState, account_id: Id, ended: &[[u8; 16]]) {
    for id in ended {
        state.live.notify(crate::live::Change {
            account_id,
            kind: lockdown_sync::wire::LiveKind::SessionEnded,
            origin: None,
            only: Some(*id),
        });
    }
    state.live.notify(crate::live::Change::account(
        account_id,
        lockdown_sync::wire::LiveKind::Sessions,
    ));
}

/// `POST /v1/account/sessions/{id}/end`: that session's token stops working
/// at once, everywhere. Ending the asking session is signing out.
pub async fn end_session(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = crate::sessions::parse_session_id(&id)
        .ok_or_else(|| ApiError::bad_request("a session id is 32 hex digits"))?;
    if session_store(&state)?.end_session(
        session.account_id,
        &id,
        state.now(),
        crate::renewal::idle_secs(&state),
    )? {
        announce_ended(&state, session.account_id, &[id]);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("that session"))
    }
}

/// `POST /v1/account/sessions/end-others`: every session of the account but
/// the asking one.
pub async fn end_other_sessions(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<lockdown_sync::wire::SessionsEnded>> {
    let ended = session_store(&state)?.end_other_sessions(
        session.account_id,
        &session.session_id,
        state.now(),
        crate::renewal::idle_secs(&state),
    )?;
    announce_ended(&state, session.account_id, &ended);
    Ok(Cbor(lockdown_sync::wire::SessionsEnded {
        ended: ended.len() as u64,
    }))
}
