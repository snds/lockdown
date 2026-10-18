//! Security keys and passkeys as a second factor on the Lockdown account
//! (`account-security-keys`; docs/plan/22-security-depth.md §4), and the
//! account's password stretching (`client-kdf`, §5).
//!
//! **Second factor.** A person adds WebAuthn keys to their account (from
//! the web vault, or the system browser for the desktop app). From then
//! on, a sign-in from a device the account has never seen passes the
//! password (OPAQUE) and then waits: the server answers with a pending
//! sign-in instead of a session, a key answers the challenge in a browser
//! (the server checks it with `lockdown_passkey::verify`), and the
//! signing-in device collects its session with a poll key only it holds.
//! A device the account already knows (it signed in before, or another
//! device of the account approved it) proves itself with its device key and
//! needs no key; the device key is the second factor there. With every key
//! lost and no device left, the server's operator clears the keys
//! (`lockdown-server security-keys clear`); this is access control, not
//! cryptography, and the vault never depended on it.
//!
//! **Stretching.** A device that can afford stronger Argon2id than the
//! account's password file was made under re-registers the password file
//! (two authenticated round trips) with the stronger parameters; the
//! server keeps the parameters beside the file and announces them at every
//! login start (an unknown login gets the default, so nothing leaks).
//!
//! The same store keeps the crown jewels held back with emergency grants
//! (`legacy`).
//!
//! What is stored: per account, each key's credential id, COSE public key,
//! algorithm, counter, name and dates; and the stretching parameters. What
//! is in memory only: sign-ins and registrations waiting for a key, for
//! five minutes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use lockdown_crypto::Argon2Params;
use lockdown_crypto::auth::{OpaqueServer, ksf_acceptable};
use lockdown_passkey::verify::{Expected, verify_assertion, verify_registration};
use lockdown_sync::account_security::{
    CreationRequest, KeyRegisterFinish, PasswordFinish, PasswordStart, PasswordStartReply,
    SecondFactorAnswer, SecondFactorLookup, SecondFactorPending, SecondFactorPoll,
    SecondFactorRequest, SecondFactorStatus, SecurityKeyInfo, SecurityKeyList,
};
use lockdown_sync::wire::{ErrorCode, LoginFinishReply};
use lockdown_vault::Id;
use rusqlite::{Connection, OptionalExtension, params};
use serde_bytes::ByteBuf;

use crate::AppState;
use crate::api::{ApiError, ApiResult, Cbor, Peer, Session, brake};
use crate::storage::{Account, Result, SqliteStorage, StorageError};

/// How long a sign-in or a registration waits for a key.
pub const PENDING_SECS: i64 = 5 * 60;
/// The most of either kept at once.
const PENDING_MAX: usize = 10_000;
/// The most keys one account holds.
pub const MAX_KEYS: usize = 20;
/// What the browser is told it may take, in milliseconds.
const TIMEOUT_MS: u32 = 120_000;

/// One registered key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRow {
    /// The credential id.
    pub credential_id: Vec<u8>,
    /// What the person calls it.
    pub name: String,
    /// `es256` or `ed25519`.
    pub alg: String,
    /// The COSE public key.
    pub cose_key: Vec<u8>,
    /// The counter last seen.
    pub sign_count: u32,
    /// When it was added.
    pub created_at: i64,
    /// When it last answered a sign-in.
    pub last_used: Option<i64>,
}

impl KeyRow {
    fn info(&self) -> SecurityKeyInfo {
        SecurityKeyInfo {
            id: data_encoding::BASE64URL_NOPAD.encode(&self.credential_id),
            name: self.name.clone(),
            alg: self.alg.clone(),
            created_at: self.created_at,
            last_used: self.last_used,
        }
    }
}

/// The tables, beside the main schema (additive; the schema version is
/// unchanged).
pub(crate) fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS security_keys (
             account_id    TEXT NOT NULL,
             credential_id BLOB NOT NULL,
             name          TEXT NOT NULL,
             alg           TEXT NOT NULL,
             cose_key      BLOB NOT NULL,
             sign_count    INTEGER NOT NULL,
             created_at    INTEGER NOT NULL,
             last_used     INTEGER,
             PRIMARY KEY (account_id, credential_id)
         );
         CREATE TABLE IF NOT EXISTS account_ksf (
             account_id TEXT PRIMARY KEY NOT NULL,
             m_cost_kib INTEGER NOT NULL,
             t_cost     INTEGER NOT NULL,
             p_cost     INTEGER NOT NULL
         );",
    )
}

/// What the store keeps for this module.
pub trait AccountSecurityStore: Send + Sync {
    /// The account's keys, oldest first.
    fn keys(&self, account_id: Id) -> Result<Vec<KeyRow>>;
    /// Adds a key; an id already registered is refused.
    fn add_key(&self, account_id: Id, key: &KeyRow) -> Result<()>;
    /// Removes a key; `false` when the account has no such key.
    fn remove_key(&self, account_id: Id, credential_id: &[u8]) -> Result<bool>;
    /// Records a key's use: its new counter, now.
    fn touch_key(&self, account_id: Id, credential_id: &[u8], count: u32, now: i64) -> Result<()>;
    /// Removes every key of an account (the operator's escape hatch).
    fn clear_keys(&self, account_id: Id) -> Result<usize>;
    /// The stretching the account's password file was made under, when raised.
    fn ksf(&self, account_id: Id) -> Result<Option<Argon2Params>>;
    /// Replaces the password file and the stretching it was made under, at once.
    fn replace_password(&self, account_id: Id, file: &[u8], ksf: &Argon2Params) -> Result<()>;
    /// Replaces a grant's held-back crown-jewel envelopes (`legacy`).
    fn put_emergency_items(&self, grant: Id, items: &[(Id, Vec<u8>)]) -> Result<()>;
    /// A grant's held-back crown-jewel envelopes.
    fn emergency_items(&self, grant: Id) -> Result<Vec<Vec<u8>>>;
}

fn backend(e: rusqlite::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

fn u32_of(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

impl AccountSecurityStore for SqliteStorage {
    fn keys(&self, account_id: Id) -> Result<Vec<KeyRow>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT credential_id, name, alg, cose_key, sign_count, created_at, last_used FROM security_keys WHERE account_id = ?1 ORDER BY created_at, credential_id",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![account_id.to_string()], |r| {
                Ok(KeyRow {
                    credential_id: r.get(0)?,
                    name: r.get(1)?,
                    alg: r.get(2)?,
                    cose_key: r.get(3)?,
                    sign_count: u32_of(r.get(4)?),
                    created_at: r.get(5)?,
                    last_used: r.get(6)?,
                })
            })
            .map_err(backend)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
    }

    fn add_key(&self, account_id: Id, key: &KeyRow) -> Result<()> {
        let conn = self.lock();
        let n = conn
            .execute(
                "INSERT OR IGNORE INTO security_keys (account_id, credential_id, name, alg, cose_key, sign_count, created_at, last_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    account_id.to_string(),
                    key.credential_id,
                    key.name,
                    key.alg,
                    key.cose_key,
                    i64::from(key.sign_count),
                    key.created_at,
                    key.last_used
                ],
            )
            .map_err(backend)?;
        if n == 0 {
            return Err(StorageError::Backend(
                "that key is already registered".into(),
            ));
        }
        Ok(())
    }

    fn remove_key(&self, account_id: Id, credential_id: &[u8]) -> Result<bool> {
        let n = self
            .lock()
            .execute(
                "DELETE FROM security_keys WHERE account_id = ?1 AND credential_id = ?2",
                params![account_id.to_string(), credential_id],
            )
            .map_err(backend)?;
        Ok(n > 0)
    }

    fn touch_key(&self, account_id: Id, credential_id: &[u8], count: u32, now: i64) -> Result<()> {
        self.lock()
            .execute(
                "UPDATE security_keys SET sign_count = ?3, last_used = ?4 WHERE account_id = ?1 AND credential_id = ?2",
                params![account_id.to_string(), credential_id, i64::from(count), now],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn clear_keys(&self, account_id: Id) -> Result<usize> {
        self.lock()
            .execute(
                "DELETE FROM security_keys WHERE account_id = ?1",
                params![account_id.to_string()],
            )
            .map_err(backend)
    }

    fn ksf(&self, account_id: Id) -> Result<Option<Argon2Params>> {
        self.lock()
            .query_row(
                "SELECT m_cost_kib, t_cost, p_cost FROM account_ksf WHERE account_id = ?1",
                params![account_id.to_string()],
                |r| {
                    Ok(Argon2Params {
                        m_cost_kib: u32_of(r.get(0)?),
                        t_cost: u32_of(r.get(1)?),
                        p_cost: u32_of(r.get(2)?),
                    })
                },
            )
            .optional()
            .map_err(backend)
    }

    fn replace_password(&self, account_id: Id, file: &[u8], ksf: &Argon2Params) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let n = tx
            .execute(
                "UPDATE accounts SET password_file = ?2 WHERE account_id = ?1",
                params![account_id.to_string(), file],
            )
            .map_err(backend)?;
        if n == 0 {
            return Err(StorageError::NotFound("the account".into()));
        }
        tx.execute(
            "INSERT INTO account_ksf (account_id, m_cost_kib, t_cost, p_cost) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (account_id) DO UPDATE SET m_cost_kib = excluded.m_cost_kib, t_cost = excluded.t_cost, p_cost = excluded.p_cost",
            params![
                account_id.to_string(),
                i64::from(ksf.m_cost_kib),
                i64::from(ksf.t_cost),
                i64::from(ksf.p_cost)
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)
    }

    fn put_emergency_items(&self, grant: Id, items: &[(Id, Vec<u8>)]) -> Result<()> {
        SqliteStorage::put_emergency_items(self, grant, items)
    }

    fn emergency_items(&self, grant: Id) -> Result<Vec<Vec<u8>>> {
        SqliteStorage::emergency_items(self, grant)
    }
}

// --- what waits in memory -----------------------------------------------------------

/// A sign-in that passed the password and waits for a key.
#[derive(Debug)]
struct WaitingSignIn {
    account: Account,
    device_id: Id,
    device_pub: [u8; 32],
    peer: String,
    user_agent: Option<String>,
    challenge: [u8; 32],
    poll_key: [u8; 32],
    allow: Vec<Vec<u8>>,
    rp_id: String,
    origins: Vec<String>,
    expires_at: i64,
    answered: bool,
}

/// A key registration started by a session.
#[derive(Debug)]
struct WaitingKey {
    account_id: Id,
    login: String,
    challenge: [u8; 32],
    exclude: Vec<Vec<u8>>,
    rp_id: String,
    origins: Vec<String>,
    expires_at: i64,
}

/// Sign-ins and registrations waiting for a key.
#[derive(Debug, Default)]
pub struct Pending {
    sign_ins: Mutex<BTreeMap<Id, WaitingSignIn>>,
    keys: Mutex<BTreeMap<Id, WaitingKey>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn store(state: &AppState) -> ApiResult<&dyn AccountSecurityStore> {
    state
        .storage
        .account_security()
        .ok_or_else(|| ApiError::internal("this server's store keeps no security keys"))
}

/// The relying party and the origins a key may have been used on, from the
/// request's `Host`: the host name is the RP ID; the origin is `https://`
/// it, and on this machine (`localhost`, a loopback address) `http://` too.
fn relying_party(headers: &HeaderMap) -> ApiResult<(String, Vec<String>)> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .ok_or_else(|| ApiError::bad_request("the request names no host"))?;
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default().to_string()
    } else {
        host.split(':').next().unwrap_or_default().to_string()
    };
    if name.is_empty() {
        return Err(ApiError::bad_request("the request names no host"));
    }
    let local = name == "localhost" || name == "127.0.0.1" || name == "::1";
    let mut origins = vec![format!("https://{host}")];
    if local {
        origins.push(format!("http://{host}"));
    }
    Ok((name, origins))
}

fn random() -> ApiResult<[u8; 32]> {
    lockdown_crypto::rand::random_32().map_err(|e| ApiError::internal(e.to_string()))
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether a sign-in by `device_id` must wait for a key: the account has
/// keys and has never seen the device.
pub(crate) fn required(state: &AppState, account_id: Id, device_id: Id) -> ApiResult<bool> {
    let Some(store) = state.storage.account_security() else {
        return Ok(false);
    };
    if store.keys(account_id)?.is_empty() {
        return Ok(false);
    }
    Ok(state.storage.device_pub(account_id, device_id)?.is_none())
}

/// Starts the wait for a key: the reply a login gets instead of a session.
pub(crate) fn begin(
    state: &AppState,
    headers: &HeaderMap,
    account: Account,
    device_id: Id,
    device_pub: [u8; 32],
    peer: &Peer,
    user_agent: Option<String>,
) -> ApiResult<LoginFinishReply> {
    let (rp_id, origins) = relying_party(headers)?;
    let keys = store(state)?.keys(account.account_id)?;
    let now = state.now();
    let challenge = random()?;
    let poll_key = random()?;
    let pending_id = Id::new();
    let allow: Vec<Vec<u8>> = keys.iter().map(|k| k.credential_id.clone()).collect();
    let device_label = crate::sessions::client_label(user_agent.as_deref())
        .unwrap_or_else(|| "a new device".into());
    let request = SecondFactorRequest {
        pending_id,
        challenge: ByteBuf::from(challenge.to_vec()),
        rp_id: rp_id.clone(),
        allow: allow.iter().map(|a| ByteBuf::from(a.clone())).collect(),
        timeout_ms: TIMEOUT_MS,
        expires_at: now + PENDING_SECS,
        device_label,
    };
    let profile_id = account.profile_id;
    {
        let mut waiting = lock(&state.relays.second_factor.sign_ins);
        waiting.retain(|_, w| w.expires_at > now);
        if waiting.len() >= PENDING_MAX {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::RateLimited,
                "too many sign-ins are waiting; try again in a minute",
            ));
        }
        waiting.insert(
            pending_id,
            WaitingSignIn {
                account,
                device_id,
                device_pub,
                peer: peer.0.clone(),
                user_agent,
                challenge,
                poll_key,
                allow,
                rp_id,
                origins,
                expires_at: now + PENDING_SECS,
                answered: false,
            },
        );
    }
    Ok(LoginFinishReply {
        token: String::new(),
        expires_at: 0,
        profile_id,
        second_factor: Some(SecondFactorPending {
            pending_id,
            poll_key: ByteBuf::from(poll_key.to_vec()),
            request,
        }),
    })
}

fn expected_for<'a>(
    origins: &'a [String],
    rp_id: &'a str,
    challenge: &'a [u8],
) -> impl Iterator<Item = Expected<'a>> {
    origins.iter().map(move |origin| Expected {
        rp_id,
        origin,
        challenge,
    })
}

/// `POST /v1/account/login/second-factor/request`: the WebAuthn request
/// of a waiting sign-in (the system browser's page has only its id).
pub async fn request(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<SecondFactorLookup>,
) -> ApiResult<Cbor<SecondFactorRequest>> {
    brake(&state, &peer, None)?;
    let now = state.now();
    let waiting = lock(&state.relays.second_factor.sign_ins);
    let w = waiting
        .get(&body.pending_id)
        .filter(|w| w.expires_at > now && !w.answered)
        .ok_or_else(|| ApiError::not_found("that sign-in isn't waiting any more; sign in again"))?;
    Ok(Cbor(SecondFactorRequest {
        pending_id: body.pending_id,
        challenge: ByteBuf::from(w.challenge.to_vec()),
        rp_id: w.rp_id.clone(),
        allow: w.allow.iter().map(|a| ByteBuf::from(a.clone())).collect(),
        timeout_ms: TIMEOUT_MS,
        expires_at: w.expires_at,
        device_label: crate::sessions::client_label(w.user_agent.as_deref())
            .unwrap_or_else(|| "a new device".into()),
    }))
}

/// `POST /v1/account/login/second-factor`: a key answered.
pub async fn answer(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<SecondFactorAnswer>,
) -> ApiResult<StatusCode> {
    brake(&state, &peer, None)?;
    let now = state.now();
    let mut waiting = lock(&state.relays.second_factor.sign_ins);
    let w = waiting
        .get_mut(&body.pending_id)
        .filter(|w| w.expires_at > now && !w.answered)
        .ok_or_else(|| ApiError::not_found("that sign-in isn't waiting any more; sign in again"))?;
    if !w.allow.iter().any(|a| same(a, &body.credential_id)) {
        return Err(ApiError::forbidden("that key isn't one of this account's"));
    }
    let store = store(&state)?;
    let key = store
        .keys(w.account.account_id)?
        .into_iter()
        .find(|k| same(&k.credential_id, &body.credential_id))
        .ok_or_else(|| ApiError::forbidden("that key was removed from the account"))?;
    let mut last = Err(String::from("no origin to check against"));
    for expected in expected_for(&w.origins, &w.rp_id, &w.challenge) {
        last = verify_assertion(
            &body.client_data_json,
            &body.authenticator_data,
            &body.signature,
            &key.cose_key,
            key.sign_count,
            &expected,
        );
        if last.is_ok() {
            break;
        }
    }
    let asserted =
        last.map_err(|why| ApiError::new(StatusCode::UNAUTHORIZED, ErrorCode::LoginFailed, why))?;
    store.touch_key(
        w.account.account_id,
        &key.credential_id,
        asserted.sign_count,
        now,
    )?;
    w.answered = true;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/account/login/second-factor/poll`: the signing-in device
/// collects its session once a key answered.
pub async fn poll(
    State(state): State<Arc<AppState>>,
    Cbor(body): Cbor<SecondFactorPoll>,
) -> ApiResult<Cbor<SecondFactorStatus>> {
    let now = state.now();
    let ready = {
        let mut waiting = lock(&state.relays.second_factor.sign_ins);
        waiting.retain(|_, w| w.expires_at > now);
        let w = waiting.get(&body.pending_id).ok_or_else(|| {
            ApiError::not_found("that sign-in isn't waiting any more; sign in again")
        })?;
        if !same(&w.poll_key, &body.poll_key) {
            return Err(ApiError::forbidden("that isn't this sign-in's key"));
        }
        if !w.answered {
            return Ok(Cbor(SecondFactorStatus { session: None }));
        }
        waiting.remove(&body.pending_id).expect("found above")
    };
    let reply = crate::api::issue_session(
        &state,
        &ready.account,
        ready.device_id,
        &ready.device_pub,
        &Peer(ready.peer),
        ready.user_agent,
    )?;
    Ok(Cbor(SecondFactorStatus {
        session: Some(reply),
    }))
}

// --- managing keys -------------------------------------------------------------------

fn list_of(state: &AppState, account_id: Id, headers: &HeaderMap) -> ApiResult<SecurityKeyList> {
    let rp_id = relying_party(headers).map(|(r, _)| r).unwrap_or_default();
    Ok(SecurityKeyList {
        keys: store(state)?
            .keys(account_id)?
            .iter()
            .map(KeyRow::info)
            .collect(),
        rp_id,
    })
}

/// `GET /v1/account/security-keys`.
pub async fn list(
    State(state): State<Arc<AppState>>,
    session: Session,
    headers: HeaderMap,
) -> ApiResult<Cbor<SecurityKeyList>> {
    Ok(Cbor(list_of(&state, session.account_id, &headers)?))
}

/// `POST /v1/account/security-keys/start`: what the browser needs to add one.
pub async fn start(
    State(state): State<Arc<AppState>>,
    session: Session,
    headers: HeaderMap,
) -> ApiResult<Cbor<CreationRequest>> {
    let (rp_id, origins) = relying_party(&headers)?;
    let account = state
        .storage
        .account(session.account_id)?
        .ok_or_else(|| ApiError::not_found("the account"))?;
    let keys = store(&state)?.keys(session.account_id)?;
    if keys.len() >= MAX_KEYS {
        return Err(ApiError::bad_request(
            "an account holds at most twenty keys",
        ));
    }
    let now = state.now();
    let challenge = random()?;
    let pending_id = Id::new();
    let exclude: Vec<Vec<u8>> = keys.iter().map(|k| k.credential_id.clone()).collect();
    let request = creation_request(
        pending_id,
        &challenge,
        &rp_id,
        session.account_id,
        &account.login,
        &exclude,
    );
    let mut waiting = lock(&state.relays.second_factor.keys);
    waiting.retain(|_, w| w.expires_at > now);
    if waiting.len() >= PENDING_MAX {
        return Err(ApiError::internal("too many registrations are waiting"));
    }
    waiting.insert(
        pending_id,
        WaitingKey {
            account_id: session.account_id,
            login: account.login,
            challenge,
            exclude,
            rp_id,
            origins,
            expires_at: now + PENDING_SECS,
        },
    );
    Ok(Cbor(request))
}

fn creation_request(
    pending_id: Id,
    challenge: &[u8; 32],
    rp_id: &str,
    account_id: Id,
    login: &str,
    exclude: &[Vec<u8>],
) -> CreationRequest {
    CreationRequest {
        pending_id,
        challenge: ByteBuf::from(challenge.to_vec()),
        rp_id: rp_id.to_string(),
        rp_name: "Lockdown".into(),
        user_id: ByteBuf::from(account_id.as_bytes().to_vec()),
        user_name: login.to_string(),
        exclude: exclude.iter().map(|e| ByteBuf::from(e.clone())).collect(),
        timeout_ms: TIMEOUT_MS,
    }
}

/// `POST /v1/account/security-keys/request`: a registration's request by
/// its id (the system browser's page, opened by the desktop app).
pub async fn key_request(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<SecondFactorLookup>,
) -> ApiResult<Cbor<CreationRequest>> {
    brake(&state, &peer, None)?;
    let now = state.now();
    let waiting = lock(&state.relays.second_factor.keys);
    let w = waiting
        .get(&body.pending_id)
        .filter(|w| w.expires_at > now)
        .ok_or_else(|| {
            ApiError::not_found("that isn't waiting any more; start again in the app")
        })?;
    Ok(Cbor(creation_request(
        body.pending_id,
        &w.challenge,
        &w.rp_id,
        w.account_id,
        &w.login,
        &w.exclude,
    )))
}

/// `POST /v1/account/security-keys/finish`: the browser's answer. The
/// registration's id is the capability (a session started it; the system
/// browser that finishes it holds no session).
pub async fn finish(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<KeyRegisterFinish>,
) -> ApiResult<Cbor<SecurityKeyInfo>> {
    brake(&state, &peer, None)?;
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ApiError::bad_request("name the key in 1 to 64 characters"));
    }
    let now = state.now();
    let w = {
        let mut waiting = lock(&state.relays.second_factor.keys);
        waiting.retain(|_, w| w.expires_at > now);
        waiting.remove(&body.pending_id).ok_or_else(|| {
            ApiError::not_found("that isn't waiting any more; start again in the app")
        })?
    };
    let mut last = Err(String::from("no origin to check against"));
    for expected in expected_for(&w.origins, &w.rp_id, &w.challenge) {
        last = verify_registration(&body.client_data_json, &body.attestation_object, &expected);
        if last.is_ok() {
            break;
        }
    }
    let registered = last.map_err(ApiError::bad_request)?;
    if w.exclude.iter().any(|e| same(e, &registered.credential_id)) {
        return Err(ApiError::bad_request("that key is already on the account"));
    }
    let row = KeyRow {
        credential_id: registered.credential_id,
        name,
        alg: match registered.alg {
            lockdown_passkey::Alg::Es256 => "es256".into(),
            lockdown_passkey::Alg::Ed25519 => "ed25519".into(),
        },
        cose_key: registered.cose_key,
        sign_count: registered.sign_count,
        created_at: now,
        last_used: None,
    };
    store(&state)?
        .add_key(w.account_id, &row)
        .map_err(|_| ApiError::bad_request("that key is already on the account"))?;
    Ok(Cbor(row.info()))
}

/// `POST /v1/account/security-keys/{id}/remove`.
pub async fn remove(
    State(state): State<Arc<AppState>>,
    session: Session,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Cbor<SecurityKeyList>> {
    let credential_id = data_encoding::BASE64URL_NOPAD
        .decode(id.as_bytes())
        .map_err(|_| ApiError::bad_request("that isn't a key id"))?;
    if !store(&state)?.remove_key(session.account_id, &credential_id)? {
        return Err(ApiError::not_found("that key isn't on the account"));
    }
    Ok(Cbor(list_of(&state, session.account_id, &headers)?))
}

// --- the password stretching ----------------------------------------------------------

/// The stretching to announce for `account` at login start.
pub(crate) fn announced_ksf(state: &AppState, account: Option<&Account>) -> Option<Argon2Params> {
    let account = account?;
    state
        .storage
        .account_security()?
        .ksf(account.account_id)
        .ok()
        .flatten()
}

/// `POST /v1/account/password/start`: re-registration for the session's
/// account (its OPRF key is its login name's, so the file stays bound to it).
pub async fn password_start(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(body): Cbor<PasswordStart>,
) -> ApiResult<Cbor<PasswordStartReply>> {
    let account = state
        .storage
        .account(session.account_id)?
        .ok_or_else(|| ApiError::not_found("the account"))?;
    let response = state
        .opaque
        .registration_response(account.login.as_bytes(), &body.request)
        .map_err(|_| ApiError::bad_request("the registration request didn't decode"))?;
    Ok(Cbor(PasswordStartReply {
        response: ByteBuf::from(response),
    }))
}

/// `POST /v1/account/password/finish`: the new file replaces the old one
/// with its stretching. Parameters outside the suite's floor and ceiling,
/// or below what the account already has, are refused (they only rise).
pub async fn password_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(body): Cbor<PasswordFinish>,
) -> ApiResult<StatusCode> {
    if !ksf_acceptable(&body.ksf) {
        return Err(ApiError::bad_request(
            "those stretching parameters are outside what accounts may use",
        ));
    }
    let store = store(&state)?;
    if let Some(current) = store.ksf(session.account_id)?
        && !body.ksf.meets(&current)
    {
        return Err(ApiError::bad_request(
            "an account's password stretching only ever rises",
        ));
    }
    let file = OpaqueServer::password_file(&body.upload)
        .map_err(|_| ApiError::bad_request("the registration upload didn't decode"))?;
    store.replace_password(session.account_id, &file, &body.ksf)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relying_party_is_the_host_and_loopback_takes_plain_http_too() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "vault.example:8443".parse().unwrap());
        assert_eq!(
            relying_party(&h).unwrap(),
            (
                "vault.example".into(),
                vec!["https://vault.example:8443".into()]
            )
        );
        h.insert(header::HOST, "localhost:8787".parse().unwrap());
        let (rp, origins) = relying_party(&h).unwrap();
        assert_eq!(rp, "localhost");
        assert_eq!(origins.len(), 2);
        assert!(relying_party(&HeaderMap::new()).is_err());
    }

    #[test]
    fn keys_and_stretching_are_kept_per_account() {
        let storage = SqliteStorage::open_in_memory().unwrap();
        let account =
            crate::storage::Storage::create_account(&storage, "me", Id::new(), b"file").unwrap();
        let a = account.account_id;
        assert!(storage.keys(a).unwrap().is_empty());
        let row = KeyRow {
            credential_id: vec![1, 2, 3],
            name: "Blue key".into(),
            alg: "es256".into(),
            cose_key: vec![9],
            sign_count: 0,
            created_at: 5,
            last_used: None,
        };
        storage.add_key(a, &row).unwrap();
        assert!(storage.add_key(a, &row).is_err(), "once");
        storage.touch_key(a, &[1, 2, 3], 7, 9).unwrap();
        let kept = storage.keys(a).unwrap();
        assert_eq!((kept[0].sign_count, kept[0].last_used), (7, Some(9)));
        assert!(storage.remove_key(a, &[1, 2, 3]).unwrap());
        assert!(!storage.remove_key(a, &[1, 2, 3]).unwrap());

        assert_eq!(storage.ksf(a).unwrap(), None);
        let raised = Argon2Params {
            m_cost_kib: 64 * 1024,
            t_cost: 5,
            p_cost: 4,
        };
        storage.replace_password(a, b"new file", &raised).unwrap();
        assert_eq!(storage.ksf(a).unwrap(), Some(raised));
        let again = crate::storage::Storage::account(&storage, a)
            .unwrap()
            .unwrap();
        assert_eq!(again.password_file, b"new file");
        assert!(storage.replace_password(Id::new(), b"x", &raised).is_err());
    }
}
