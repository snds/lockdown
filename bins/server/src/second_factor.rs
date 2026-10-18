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
//! **Email approval.** On a server with SMTP (`--smtp-url`), an account
//! without keys whose login is an email address gets the same wait from a
//! device it has never seen, answered by a one-time link mailed to that
//! address instead of a key (docs/plan/25-self-host-2.md §5): the link's
//! page shows the device and asks for Approve or "This wasn't me", so a
//! mail scanner that fetches links approves nothing. The link carries a
//! random code; the server keeps its BLAKE3 hash. The account's first
//! device has nothing to be approved from and isn't asked.
//!
//! **Key changes are announced.** Adding or removing a key records an
//! account alert every device sees and, with SMTP, emails the account.
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
use serde_bytes::ByteBuf;

use crate::AppState;
use crate::api::{ApiError, ApiResult, Cbor, Peer, Session, brake};
use crate::db::{Conn, DbError, params};
use crate::storage::{Account, Result, SqlStorage, StorageError};

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
pub(crate) fn create_tables(conn: &Conn<'_>) -> std::result::Result<(), DbError> {
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

fn u32_of(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

impl AccountSecurityStore for SqlStorage {
    fn keys(&self, account_id: Id) -> Result<Vec<KeyRow>> {
        Ok(self.conn().query_map(
            "SELECT credential_id, name, alg, cose_key, sign_count, created_at, last_used FROM security_keys WHERE account_id = ?1 ORDER BY created_at, credential_id",
            params![account_id.to_string()],
            |r| {
                Ok(KeyRow {
                    credential_id: r.get(0)?,
                    name: r.get(1)?,
                    alg: r.get(2)?,
                    cose_key: r.get(3)?,
                    sign_count: u32_of(r.get(4)?),
                    created_at: r.get(5)?,
                    last_used: r.get(6)?,
                })
            },
        )?)
    }

    fn add_key(&self, account_id: Id, key: &KeyRow) -> Result<()> {
        let n = self.conn().execute(
            "INSERT INTO security_keys (account_id, credential_id, name, alg, cose_key, sign_count, created_at, last_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT DO NOTHING",
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
        )?;
        if n == 0 {
            return Err(StorageError::Backend(
                "that key is already registered".into(),
            ));
        }
        Ok(())
    }

    fn remove_key(&self, account_id: Id, credential_id: &[u8]) -> Result<bool> {
        let n = self.conn().execute(
            "DELETE FROM security_keys WHERE account_id = ?1 AND credential_id = ?2",
            params![account_id.to_string(), credential_id],
        )?;
        Ok(n > 0)
    }

    fn touch_key(&self, account_id: Id, credential_id: &[u8], count: u32, now: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE security_keys SET sign_count = ?3, last_used = ?4 WHERE account_id = ?1 AND credential_id = ?2",
            params![account_id.to_string(), credential_id, i64::from(count), now],
        )?;
        Ok(())
    }

    fn clear_keys(&self, account_id: Id) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM security_keys WHERE account_id = ?1",
            params![account_id.to_string()],
        )?)
    }

    fn ksf(&self, account_id: Id) -> Result<Option<Argon2Params>> {
        Ok(self.conn().query_opt(
            "SELECT m_cost_kib, t_cost, p_cost FROM account_ksf WHERE account_id = ?1",
            params![account_id.to_string()],
            |r| {
                Ok(Argon2Params {
                    m_cost_kib: u32_of(r.get(0)?),
                    t_cost: u32_of(r.get(1)?),
                    p_cost: u32_of(r.get(2)?),
                })
            },
        )?)
    }

    fn replace_password(&self, account_id: Id, file: &[u8], ksf: &Argon2Params) -> Result<()> {
        let tx = self.begin()?;
        let n = tx.execute(
            "UPDATE accounts SET password_file = ?2 WHERE account_id = ?1",
            params![account_id.to_string(), file],
        )?;
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
        )?;
        Ok(tx.commit()?)
    }

    fn put_emergency_items(&self, grant: Id, items: &[(Id, Vec<u8>)]) -> Result<()> {
        SqlStorage::put_emergency_items(self, grant, items)
    }

    fn emergency_items(&self, grant: Id) -> Result<Vec<Vec<u8>>> {
        SqlStorage::emergency_items(self, grant)
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
    /// For an email approval: BLAKE3 of the link's code.
    email_code: Option<[u8; 32]>,
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
pub(crate) fn relying_party(headers: &HeaderMap) -> ApiResult<(String, Vec<String>)> {
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
        email_to: None,
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
                email_code: None,
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
        email_to: w.email_code.map(|_| crate::mail::masked(&w.account.login)),
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
    if w.email_code.is_some() || !w.allow.iter().any(|a| same(a, &body.credential_id)) {
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

// --- approval by email ------------------------------------------------------------------

/// How long an emailed approval link works.
pub const EMAIL_SECS: i64 = 15 * 60;

/// Whether a sign-in by `device_id` waits for the account's email: SMTP is
/// set with approval on, the login is an email address, the account has a
/// device already and has never seen this one.
pub(crate) fn email_required(
    state: &AppState,
    account: &Account,
    device_id: Id,
) -> ApiResult<bool> {
    let Some(mail) = &state.mail else {
        return Ok(false);
    };
    if !mail.verify_new_devices || !crate::breach_monitor::looks_like_email(&account.login) {
        return Ok(false);
    }
    if state
        .storage
        .device_pub(account.account_id, device_id)?
        .is_some()
    {
        return Ok(false);
    }
    let Some(alerts) = state.storage.alerts() else {
        return Ok(false);
    };
    Ok(alerts.device_count(account.account_id)? > 0)
}

/// Starts the wait for the account's email: the link goes out, the reply
/// is a waiting sign-in the device polls like a key's.
pub(crate) fn begin_email(
    state: &AppState,
    account: Account,
    device_id: Id,
    device_pub: [u8; 32],
    peer: &Peer,
    user_agent: Option<String>,
) -> ApiResult<LoginFinishReply> {
    let mail = state
        .mail
        .as_ref()
        .ok_or_else(|| ApiError::internal("this server sends no email"))?;
    let now = state.now();
    if !mail.take_verification(account.account_id, now) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "this account was sent several approval emails in the last hour; use one of those, or try again later",
        ));
    }
    let code = random()?;
    let poll_key = random()?;
    let pending_id = Id::new();
    let device_label = crate::sessions::client_label(user_agent.as_deref())
        .unwrap_or_else(|| "a new device".into());
    let request = SecondFactorRequest {
        pending_id,
        challenge: ByteBuf::from(random()?.to_vec()),
        rp_id: String::new(),
        allow: Vec::new(),
        timeout_ms: TIMEOUT_MS,
        expires_at: now + EMAIL_SECS,
        device_label: device_label.clone(),
        email_to: Some(crate::mail::masked(&account.login)),
    };
    let link = format!(
        "{}/approve/{pending_id}?code={}",
        mail.public_url,
        data_encoding::HEXLOWER.encode(&code)
    );
    let facts = crate::mail::SignInFacts {
        device: device_label,
        address: peer.0.clone(),
        at: now,
    };
    let message = crate::mail::verify_device(
        &account.login,
        &facts,
        &link,
        &mail.public_url,
        EMAIL_SECS / 60,
    );
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
                challenge: [0; 32],
                poll_key,
                allow: Vec::new(),
                rp_id: String::new(),
                origins: Vec::new(),
                expires_at: now + EMAIL_SECS,
                answered: false,
                email_code: Some(*blake3::hash(&code).as_bytes()),
            },
        );
    }
    mail.outbox.queue(message);
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

/// `?code=…` on the approval link.
#[derive(serde::Deserialize, Default)]
pub struct ApproveQuery {
    /// The link's code, hex.
    #[serde(default)]
    pub code: String,
}

fn code_hash(code: &str) -> Option<[u8; 32]> {
    let bytes = data_encoding::HEXLOWER_PERMISSIVE
        .decode(code.trim().as_bytes())
        .ok()?;
    (bytes.len() == 32).then(|| *blake3::hash(&bytes).as_bytes())
}

fn gone_page() -> axum::response::Response {
    crate::pages::page(
        StatusCode::NOT_FOUND,
        "This link has run out",
        "<h1>This link has run out</h1><p>It was used, it ran out after fifteen minutes, or the sign-in it was for stopped. If you are signing in, start again on that device; a new link comes.</p>",
    )
}

/// `GET /approve/{pending}?code=…`: what the person sees when they open
/// the emailed link: the device, and a choice. Opening it changes nothing.
pub async fn approve_page(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(pending_id): Path<Id>,
    axum::extract::Query(q): axum::extract::Query<ApproveQuery>,
) -> axum::response::Response {
    if brake(&state, &peer, None).is_err() {
        return crate::pages::page(
            StatusCode::TOO_MANY_REQUESTS,
            "Slow down",
            "<h1>Slow down</h1><p>Too many tries from here; wait a minute.</p>",
        );
    }
    let now = state.now();
    let waiting = lock(&state.relays.second_factor.sign_ins);
    let Some(w) = waiting.get(&pending_id).filter(|w| {
        w.expires_at > now
            && !w.answered
            && w.email_code.is_some()
            && code_hash(&q.code).is_some_and(|h| w.email_code.is_some_and(|c| same(&c, &h)))
    }) else {
        return gone_page();
    };
    let device = crate::sessions::client_label(w.user_agent.as_deref())
        .unwrap_or_else(|| "a new device".into());
    let esc = crate::pages::esc;
    let body = format!(
        r#"<h1>A new device wants to sign in</h1><div class="card"><p>Account: <b>{}</b></p><p>Device: <b>{}</b></p><p>From: <span class="mono">{}</span></p><p>Asked: {}</p></div><p>Approve it only if you are signing in on that device right now.</p><form method="post" action="/approve/{pending_id}" class="row"><input type="hidden" name="code" value="{}"><button name="answer" value="approve">Approve</button><button name="answer" value="deny" class="danger">This wasn't me</button></form>"#,
        esc(&w.account.login),
        esc(&device),
        esc(&w.peer),
        crate::mail::when(w.expires_at - EMAIL_SECS),
        esc(q.code.trim()),
    );
    crate::pages::page(StatusCode::OK, "Approve a new device", &body)
}

/// `POST /approve/{pending}` (a form: `code`, `answer` = `approve` or
/// `deny`): the person's choice. Approve lets the device collect its
/// session; "This wasn't me" stops the sign-in.
pub async fn approve_answer(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Path(pending_id): Path<Id>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    if brake(&state, &peer, None).is_err() {
        return crate::pages::page(
            StatusCode::TOO_MANY_REQUESTS,
            "Slow down",
            "<h1>Slow down</h1><p>Too many tries from here; wait a minute.</p>",
        );
    }
    let fields = crate::pages::form_fields(&body);
    let code = fields.get("code").map(String::as_str).unwrap_or_default();
    let approve = match fields.get("answer").map(String::as_str) {
        Some("approve") => true,
        Some("deny") => false,
        _ => return gone_page(),
    };
    let now = state.now();
    let mut waiting = lock(&state.relays.second_factor.sign_ins);
    let ok = waiting.get(&pending_id).is_some_and(|w| {
        w.expires_at > now
            && !w.answered
            && code_hash(code).is_some_and(|h| w.email_code.is_some_and(|c| same(&c, &h)))
    });
    if !ok {
        return gone_page();
    }
    if approve {
        if let Some(w) = waiting.get_mut(&pending_id) {
            w.answered = true;
        }
        crate::pages::page(
            StatusCode::OK,
            "Approved",
            "<h1>Approved</h1><p>The device carries on signing in by itself. You can close this page.</p>",
        )
    } else {
        waiting.remove(&pending_id);
        crate::pages::page(
            StatusCode::OK,
            "Sign-in stopped",
            "<h1>Sign-in stopped</h1><p>That device won't be let in. Someone knows your account password: change it from one of your devices (Settings, then Account). Your vault stays locked without your Secret Key, which the server never had.</p>",
        )
    }
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
    key_changed(&state, w.account_id, true, &row.name)?;
    Ok(Cbor(row.info()))
}

/// A key was added or removed: an alert every device of the account sees,
/// and, with SMTP, an email to the account.
fn key_changed(state: &AppState, account_id: Id, added: bool, name: &str) -> ApiResult<()> {
    let now = state.now();
    if let Some(alerts) = state.storage.alerts() {
        let kind = if added {
            lockdown_sync::wire::AlertKind::SecurityKeyAdded
        } else {
            lockdown_sync::wire::AlertKind::SecurityKeyRemoved
        };
        alerts.record_note_alert(account_id, kind, now, name)?;
        state.live.notify(crate::live::Change::account(
            account_id,
            lockdown_sync::wire::LiveKind::Alerts,
        ));
    }
    if let Some(mail) = &state.mail
        && let Some(account) = state.storage.account(account_id)?
        && crate::breach_monitor::looks_like_email(&account.login)
    {
        mail.outbox.queue(crate::mail::security_key_changed(
            &account.login,
            added,
            name,
            now,
            &mail.public_url,
        ));
    }
    Ok(())
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
    let store = store(&state)?;
    let name = store
        .keys(session.account_id)?
        .into_iter()
        .find(|k| same(&k.credential_id, &credential_id))
        .map(|k| k.name)
        .unwrap_or_default();
    if !store.remove_key(session.account_id, &credential_id)? {
        return Err(ApiError::not_found("that key isn't on the account"));
    }
    key_changed(&state, session.account_id, false, &name)?;
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
        let storage = SqlStorage::open_scratch().unwrap();
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
