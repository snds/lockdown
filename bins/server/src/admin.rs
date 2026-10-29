//! The operator's admin page at `/admin` (`official-self-host`;
//! 05-self-host §10; docs/plan/25-self-host-2.md §4).
//!
//! **Who gets in.** The operator, with an **operator key** that is not an
//! account: `lockdown-server admin set-key` makes one (or takes one typed),
//! prints it once, and keeps only its Argon2id hash in the database. A
//! sign-in sets a random session cookie (`__Host-lockdown-admin`: `Secure`,
//! `HttpOnly`, `SameSite=Strict`, eight hours, ended by an hour unused);
//! every form carries the session's own anti-forgery token as well. Wrong
//! keys are slowed by the login brake. Nothing an account holds can open
//! this page, and nothing here opens an account.
//!
//! **What it shows.** What the server already knows and nothing more
//! (docs/self-host.md "What the operator can see"): the release, where the
//! data and attachments live, the setup checks; each account's login name,
//! devices (ids and when they joined: device names are encrypted), live
//! sessions, vaults, attachment bytes against the quota, security keys;
//! sign-up invites; backups on disk and the restore drills' results, with
//! a red banner while the newest drill failed. It never shows a vault, a
//! key or a token, because it has none to show.
//!
//! **What it changes.** Invites (one login name may register while
//! sign-ups are closed, once, before the invite runs out), an account's
//! sessions ended, a backup or a drill run now. Revoking devices, keys and
//! passwords stay with the account's own devices.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use lockdown_crypto::kdf::Argon2Params;
use lockdown_vault::Id;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::AppState;
use crate::api::Peer;
use crate::db::{Conn, DbError, params};
use crate::pages::{esc, page, see_other};
use crate::storage::{Result, SqlStorage, StorageError};

/// The session cookie's name (the `__Host-` prefix pins it to this origin,
/// `Secure` and path `/`).
pub const COOKIE: &str = "__Host-lockdown-admin";
/// How long a sign-in lasts at most.
pub const SESSION_SECS: i64 = 8 * 60 * 60;
/// How long a sign-in lasts unused.
pub const IDLE_SECS: i64 = 60 * 60;
/// How long an invite lasts unless the operator says otherwise.
pub const INVITE_DAYS: i64 = 7;

/// The admin tables, beside the main schema (additive).
pub(crate) fn create_tables(conn: &Conn<'_>) -> std::result::Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS admin_invites (
             login      TEXT PRIMARY KEY NOT NULL,
             created_at INTEGER NOT NULL,
             expires_at INTEGER NOT NULL,
             used_at    INTEGER
         );
         CREATE TABLE IF NOT EXISTS admin_events (
             seq    INTEGER PRIMARY KEY NOT NULL,
             at     INTEGER NOT NULL,
             kind   TEXT NOT NULL,
             ok     INTEGER NOT NULL,
             detail TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS operator_settings (
             name  TEXT PRIMARY KEY NOT NULL,
             value BLOB NOT NULL
         );",
    )
}

// --- the operator key -------------------------------------------------------------------

/// The stored hash of the operator key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct KeyHash {
    v: u16,
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
    #[serde(with = "serde_bytes")]
    salt: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hash: Vec<u8>,
}

/// A new operator key: `LDOP-` and 20 random bytes in Base32 groups.
pub fn generate_key() -> std::result::Result<Zeroizing<String>, String> {
    let bytes = Zeroizing::new(lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?);
    let b32 = Zeroizing::new(data_encoding::BASE32_NOPAD.encode(&bytes[..20]));
    let groups: Vec<&str> = b32
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    Ok(Zeroizing::new(format!("LDOP-{}", groups.join("-"))))
}

/// What the operator typed, the way it is hashed: outer spaces dropped.
fn normalise(key: &str) -> &str {
    key.trim()
}

fn stretch(key: &str, salt: &[u8], params: &Argon2Params) -> Result<Zeroizing<[u8; 32]>> {
    lockdown_crypto::kdf::stretch(normalise(key).as_bytes(), salt, params)
        .map_err(|e| StorageError::Backend(e.to_string()))
}

impl SqlStorage {
    /// Sets the operator key (its Argon2id hash; the key itself is never
    /// kept). A key shorter than 16 characters is refused.
    pub fn set_operator_key(&self, key: &str) -> Result<()> {
        if normalise(key).chars().count() < 16 {
            return Err(StorageError::Backend(
                "an operator key has at least 16 characters".into(),
            ));
        }
        let params = lockdown_crypto::kdf::floor();
        let salt =
            lockdown_crypto::rand::random_16().map_err(|e| StorageError::Backend(e.to_string()))?;
        let hash = stretch(key, &salt, &params)?;
        let record = KeyHash {
            v: 1,
            m_cost_kib: params.m_cost_kib,
            t_cost: params.t_cost,
            p_cost: params.p_cost,
            salt: salt.to_vec(),
            hash: hash.to_vec(),
        };
        let bytes = lockdown_crypto::cbor::to_canonical(&record)
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        self.conn().execute(
            "INSERT INTO operator_settings (name, value) VALUES ('operator_key', ?1)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![bytes],
        )?;
        Ok(())
    }

    /// Removes the operator key: the admin page is closed until a new one.
    pub fn clear_operator_key(&self) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM operator_settings WHERE name = 'operator_key'",
            params![],
        )? > 0)
    }

    fn operator_key(&self) -> Result<Option<KeyHash>> {
        let bytes: Option<Vec<u8>> = self.conn().query_opt(
            "SELECT value FROM operator_settings WHERE name = 'operator_key'",
            params![],
            |r| r.get(0),
        )?;
        bytes
            .map(|b| {
                lockdown_crypto::cbor::from_slice(&b).map_err(|_| {
                    StorageError::Backend("the stored operator key didn't decode".into())
                })
            })
            .transpose()
    }

    /// Whether an operator key is set.
    pub fn has_operator_key(&self) -> Result<bool> {
        Ok(self.operator_key()?.is_some())
    }

    /// Whether `key` is the operator key.
    pub fn check_operator_key(&self, key: &str) -> Result<bool> {
        let Some(record) = self.operator_key()? else {
            return Ok(false);
        };
        let params = Argon2Params {
            m_cost_kib: record.m_cost_kib,
            t_cost: record.t_cost,
            p_cost: record.p_cost,
        };
        let got = stretch(key, &record.salt, &params)?;
        Ok(got.len() == record.hash.len()
            && got
                .iter()
                .zip(&record.hash)
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0)
    }

    // --- invites ---------------------------------------------------------------------------

    /// Lets `login` register once before `expires_at`, even while sign-ups
    /// are closed (replacing an earlier invite for it).
    pub fn put_invite(&self, login: &str, now: i64, expires_at: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO admin_invites (login, created_at, expires_at, used_at) VALUES (?1, ?2, ?3, NULL)
             ON CONFLICT (login) DO UPDATE SET created_at = excluded.created_at, expires_at = excluded.expires_at, used_at = NULL",
            params![login.to_lowercase(), now, expires_at],
        )?;
        Ok(())
    }

    /// Withdraws an invite; `false` when there was none.
    pub fn revoke_invite(&self, login: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM admin_invites WHERE login = ?1",
            params![login.to_lowercase()],
        )? > 0)
    }

    /// Whether `login` holds an invite that is unused and still running.
    pub fn invite_open(&self, login: &str, now: i64) -> Result<bool> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM admin_invites WHERE login = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![login.to_lowercase(), now],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Marks `login`'s invite used.
    pub fn use_invite(&self, login: &str, now: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE admin_invites SET used_at = ?2 WHERE login = ?1 AND used_at IS NULL",
            params![login.to_lowercase(), now],
        )?;
        Ok(())
    }

    /// Every invite, by login name.
    pub fn invites(&self) -> Result<Vec<InviteRow>> {
        Ok(self.conn().query_map(
            "SELECT login, created_at, expires_at, used_at FROM admin_invites ORDER BY login",
            params![],
            |r| {
                Ok(InviteRow {
                    login: r.get(0)?,
                    created_at: r.get(1)?,
                    expires_at: r.get(2)?,
                    used_at: r.get(3)?,
                })
            },
        )?)
    }

    // --- the operator's log -----------------------------------------------------------

    /// Records a backup, a drill or another operator event.
    pub fn record_event(&self, kind: &str, ok: bool, detail: &str, at: i64) -> Result<u64> {
        let tx = self.begin()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM admin_events",
            params![],
            |r| r.get(0),
        )?;
        let detail: String = detail.chars().take(4000).collect();
        tx.execute(
            "INSERT INTO admin_events (seq, at, kind, ok, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![seq, at, kind, ok, detail],
        )?;
        tx.commit()?;
        Ok(u64::try_from(seq).unwrap_or(0))
    }

    /// The newest `limit` events of `kind` (or of every kind), newest first.
    pub fn events(&self, kind: Option<&str>, limit: usize) -> Result<Vec<EventRow>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let read = |r: &crate::db::Row| -> std::result::Result<EventRow, DbError> {
            Ok(EventRow {
                seq: r.get(0)?,
                at: r.get(1)?,
                kind: r.get(2)?,
                ok: r.get::<i64>(3)? != 0,
                detail: r.get(4)?,
            })
        };
        Ok(match kind {
            Some(k) => self.conn().query_map(
                "SELECT seq, at, kind, ok, detail FROM admin_events WHERE kind = ?1 ORDER BY seq DESC LIMIT ?2",
                params![k, limit],
                read,
            )?,
            None => self.conn().query_map(
                "SELECT seq, at, kind, ok, detail FROM admin_events ORDER BY seq DESC LIMIT ?1",
                params![limit],
                read,
            )?,
        })
    }

    // --- what the pages list ----------------------------------------------------------

    /// One line per account, by login name.
    pub fn account_overview(&self, now: i64) -> Result<Vec<AccountLine>> {
        let rows: Vec<OverviewColumns> = self.conn().query_map(
            "SELECT a.account_id, a.login, a.created_at,
                 (SELECT COUNT(*) FROM devices d WHERE d.account_id = a.account_id),
                 (SELECT COUNT(*) FROM vaults v WHERE v.account_id = a.account_id),
                 (SELECT CAST(COALESCE(SUM(b.size), 0) AS BIGINT) FROM blobs b JOIN vaults v ON v.vault_id = b.vault_id WHERE v.account_id = a.account_id),
                 (SELECT COUNT(*) FROM sessions s WHERE s.account_id = a.account_id AND s.ended_at IS NULL AND s.expires_at > ?1),
                 (SELECT MAX(s.last_seen) FROM sessions s WHERE s.account_id = a.account_id),
                 (SELECT COUNT(*) FROM security_keys k WHERE k.account_id = a.account_id)
             FROM accounts a ORDER BY a.login",
            params![now],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )?;
        rows.into_iter()
            .map(
                |(id, login, created, devices, vaults, bytes, sessions, last, keys)| {
                    Ok(AccountLine {
                        account_id: Id::parse(&id).map_err(|_| {
                            StorageError::Backend("a bad account id is stored".into())
                        })?,
                        login,
                        created_at: created,
                        devices: u64::try_from(devices).unwrap_or(0),
                        vaults: u64::try_from(vaults).unwrap_or(0),
                        blob_bytes: u64::try_from(bytes).unwrap_or(0),
                        live_sessions: u64::try_from(sessions).unwrap_or(0),
                        last_seen: last,
                        security_keys: u64::try_from(keys).unwrap_or(0),
                    })
                },
            )
            .collect()
    }

    /// An account's devices: id, when it joined, whether it was revoked.
    pub fn account_devices(&self, account_id: Id) -> Result<Vec<(Id, i64, bool)>> {
        let rows: Vec<(String, i64, i64)> = self.conn().query_map(
            "SELECT d.device_id, d.first_seen,
                 (SELECT COUNT(*) FROM revoked_devices r WHERE r.account_id = d.account_id AND r.device_id = d.device_id AND r.vault_id = '*')
             FROM devices d WHERE d.account_id = ?1 ORDER BY d.first_seen, d.device_id",
            params![account_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        rows.into_iter()
            .map(|(id, at, revoked)| {
                Ok((
                    Id::parse(&id)
                        .map_err(|_| StorageError::Backend("a bad device id is stored".into()))?,
                    at,
                    revoked > 0,
                ))
            })
            .collect()
    }

    /// How many devices and live sessions the server has in all.
    pub fn totals(&self, now: i64) -> Result<(u64, u64, u64)> {
        let conn = self.conn();
        let count = |sql: &str, p: &[&dyn crate::db::ToSql]| -> Result<u64> {
            Ok(u64::try_from(conn.query_row(sql, p, |r| r.get::<i64>(0))?).unwrap_or(0))
        };
        Ok((
            count("SELECT COUNT(*) FROM devices", params![])?,
            count(
                "SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL AND expires_at > ?1",
                params![now],
            )?,
            count(
                "SELECT CAST(COALESCE(SUM(size), 0) AS BIGINT) FROM blobs",
                params![],
            )?,
        ))
    }
}

/// What the account overview query returns per account.
type OverviewColumns = (String, String, i64, i64, i64, i64, i64, Option<i64>, i64);

/// One invite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteRow {
    /// The login name it lets register.
    pub login: String,
    /// When it was made.
    pub created_at: i64,
    /// When it runs out.
    pub expires_at: i64,
    /// When it was used.
    pub used_at: Option<i64>,
}

/// One operator event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    /// Its number.
    pub seq: i64,
    /// When.
    pub at: i64,
    /// `backup`, `drill`, `invite`, `sessions_ended`, `sign_in`.
    pub kind: String,
    /// Whether it went well.
    pub ok: bool,
    /// In words.
    pub detail: String,
}

/// One account, as the admin page lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountLine {
    /// The account.
    pub account_id: Id,
    /// Its login name.
    pub login: String,
    /// When it registered.
    pub created_at: i64,
    /// Devices that ever signed in.
    pub devices: u64,
    /// Vaults it owns.
    pub vaults: u64,
    /// Attachment bytes in those vaults (what the quota counts).
    pub blob_bytes: u64,
    /// Sessions neither ended nor run out.
    pub live_sessions: u64,
    /// The newest use of any of its sessions.
    pub last_seen: Option<i64>,
    /// Security keys on its sign-in.
    pub security_keys: u64,
}

// --- what the page knows about the server ---------------------------------------------

/// What the running server is set up with, in words (set once at start).
#[derive(Clone, Debug, Default)]
pub struct ServerFacts {
    /// Where the database is.
    pub database: String,
    /// Where attachment bodies live.
    pub attachments: String,
    /// Whether the server serves TLS itself.
    pub tls: bool,
    /// Whether the email breach monitor runs, and over what.
    pub breach_feed: Option<String>,
}

#[derive(Debug, Clone)]
struct AdminSession {
    csrf: [u8; 32],
    created: i64,
    last: i64,
}

/// The admin page's state: its signed-in sessions, the server's facts and
/// the backup schedule.
#[derive(Default)]
pub struct AdminState {
    /// Passkey ceremonies waiting for the browser (`admin_passkeys`).
    pub(crate) passkeys: crate::admin_passkeys::Ceremonies,
    sessions: Mutex<BTreeMap<[u8; 32], AdminSession>>,
    facts: Mutex<ServerFacts>,
    schedule: Mutex<Option<Arc<crate::drills::Schedule>>>,
}

impl std::fmt::Debug for AdminState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminState")
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl AdminState {
    /// Records how the server is set up, for the overview.
    pub fn set_facts(&self, facts: ServerFacts) {
        *lock(&self.facts) = facts;
    }

    /// The backup schedule the page can run now and shows.
    pub fn set_schedule(&self, schedule: Arc<crate::drills::Schedule>) {
        *lock(&self.schedule) = Some(schedule);
    }

    /// The schedule, when there is one.
    pub fn schedule(&self) -> Option<Arc<crate::drills::Schedule>> {
        lock(&self.schedule).clone()
    }

    fn facts(&self) -> ServerFacts {
        lock(&self.facts).clone()
    }

    pub(crate) fn open(&self, now: i64) -> std::result::Result<(String, [u8; 32]), String> {
        let id = lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?;
        let csrf = lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?;
        let mut sessions = lock(&self.sessions);
        sessions.retain(|_, s| live(s, now));
        if sessions.len() >= 64 {
            // The oldest goes: an operator has few browsers.
            if let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, s)| s.created)
                .map(|(k, _)| *k)
            {
                sessions.remove(&oldest);
            }
        }
        sessions.insert(
            *blake3::hash(&id).as_bytes(),
            AdminSession {
                csrf,
                created: now,
                last: now,
            },
        );
        Ok((data_encoding::HEXLOWER.encode(&id), csrf))
    }

    /// The session a cookie names, touched; its anti-forgery token.
    fn session(&self, cookie: &str, now: i64) -> Option<[u8; 32]> {
        let id = data_encoding::HEXLOWER_PERMISSIVE
            .decode(cookie.as_bytes())
            .ok()
            .filter(|b| b.len() == 32)?;
        let key = *blake3::hash(&id).as_bytes();
        let mut sessions = lock(&self.sessions);
        let s = sessions.get_mut(&key)?;
        if !live(s, now) {
            sessions.remove(&key);
            return None;
        }
        s.last = now;
        Some(s.csrf)
    }

    fn close(&self, cookie: &str) {
        if let Ok(id) = data_encoding::HEXLOWER_PERMISSIVE.decode(cookie.as_bytes()) {
            lock(&self.sessions).remove(blake3::hash(&id).as_bytes());
        }
    }
}

fn live(s: &AdminSession, now: i64) -> bool {
    now - s.created < SESSION_SECS && now - s.last < IDLE_SECS
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether registration of `login` may go ahead while sign-ups are closed.
pub(crate) fn invited(state: &AppState, login: &str) -> bool {
    state
        .storage
        .sql()
        .is_some_and(|s| s.invite_open(login, state.now()).unwrap_or(false))
}

/// After a registration under an invite.
pub(crate) fn invite_used(state: &AppState, login: &str) {
    if let Some(s) = state.storage.sql() {
        let _ = s.use_invite(login, state.now());
    }
}

// --- the routes ------------------------------------------------------------------------

/// The admin page's routes, merged into the server's router.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin", get(overview))
        .route("/admin/sign-in", post(sign_in))
        .route("/admin/sign-out", post(sign_out))
        .route("/admin/accounts", get(accounts))
        .route("/admin/accounts/{account}", get(account))
        .route("/admin/accounts/{account}/end-sessions", post(end_sessions))
        .route("/admin/invites", get(invites).post(invite_create))
        .route("/admin/invites/revoke", post(invite_revoke))
        .route("/admin/backups", get(backups))
        .route("/admin/backups/run", post(backup_run))
        .route("/admin/drills/run", post(drill_run))
        .merge(crate::admin_passkeys::routes())
}

fn store(state: &AppState) -> Option<&SqlStorage> {
    state.storage.sql()
}

fn cookie_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

/// The signed-in operator's anti-forgery token, or the sign-in page.
pub(crate) fn signed_in(
    state: &AppState,
    headers: &HeaderMap,
) -> std::result::Result<[u8; 32], Box<Response>> {
    let Some(store) = store(state) else {
        return Err(Box::new(unavailable()));
    };
    match store.has_operator_key() {
        Ok(true) => {}
        Ok(false) => return Err(Box::new(no_key_page())),
        Err(_) => return Err(Box::new(unavailable())),
    }
    cookie_of(headers)
        .and_then(|c| state.admin.session(&c, state.now()))
        .ok_or_else(|| Box::new(sign_in_page(StatusCode::UNAUTHORIZED, None)))
}

/// Checks a form's anti-forgery token against the session's.
pub(crate) fn form_ok(fields: &BTreeMap<String, String>, csrf: &[u8; 32]) -> bool {
    fields
        .get("csrf")
        .and_then(|t| data_encoding::HEXLOWER_PERMISSIVE.decode(t.as_bytes()).ok())
        .is_some_and(|t| same(&t, csrf))
}

pub(crate) fn unavailable() -> Response {
    page(
        StatusCode::SERVICE_UNAVAILABLE,
        "Admin",
        "<h1>Admin</h1><p>This server's store has no admin tables.</p>",
    )
}

fn no_key_page() -> Response {
    page(
        StatusCode::FORBIDDEN,
        "Admin",
        "<h1>The admin page is closed</h1><p>No operator key is set. On the server, run <code>lockdown-server admin set-key</code> (with the same <code>--data</code> or <code>--database-url</code> as the server); it prints a key once. Then sign in here with it.</p>",
    )
}

fn sign_in_page(status: StatusCode, error: Option<&str>) -> Response {
    let error = error
        .map(|e| format!("<p class=\"bad\" role=\"alert\">{}</p>", esc(e)))
        .unwrap_or_default();
    crate::pages::page_with_script(
        status,
        "Admin sign-in",
        &format!(
            "<h1>Lockdown server admin</h1><div class=\"card\"><form method=\"post\" action=\"/admin/sign-in\">\
             <label for=\"key\">Operator key</label><input id=\"key\" name=\"key\" type=\"password\" autocomplete=\"current-password\" required>\
             {error}<p><button>Sign in</button></p></form>\
             <p><button id=\"pk-sign-in\" class=\"quiet\" type=\"button\" hidden>Sign in with a passkey</button></p><p id=\"pk-status\" class=\"muted\" role=\"status\"></p></div>\
             <p class=\"muted\">The operator key is not an account and opens no vault. Lost it? On the server, <code>lockdown-server admin set-key</code> makes a new one. A passkey added under Passkeys signs in here too.</p>"
        ),
        Some(crate::admin_passkeys::SCRIPT),
    )
}

pub(crate) fn hidden(csrf: &[u8; 32]) -> String {
    format!(
        "<input type=\"hidden\" name=\"csrf\" value=\"{}\">",
        data_encoding::HEXLOWER.encode(csrf)
    )
}

fn shell(csrf: &[u8; 32], current: &str, title: &str, body: &str) -> Response {
    shell_page(csrf, current, title, body, None)
}

/// [`shell`], with the one script a page may carry (allowed by its hash).
pub(crate) fn shell_page(
    csrf: &[u8; 32],
    current: &str,
    title: &str,
    body: &str,
    script: Option<&str>,
) -> Response {
    let link = |href: &str, label: &str| {
        let cur = if href == current {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!("<a href=\"{href}\"{cur}>{label}</a>")
    };
    crate::pages::page_with_script(
        StatusCode::OK,
        title,
        &format!(
            "<nav aria-label=\"Admin\">{}{}{}{}{}<form method=\"post\" action=\"/admin/sign-out\" class=\"inline\">{}<button class=\"quiet\">Sign out</button></form></nav><h1>{}</h1>{body}",
            link("/admin", "Overview"),
            link("/admin/accounts", "Accounts"),
            link("/admin/invites", "Invites"),
            link("/admin/backups", "Backups"),
            link("/admin/passkeys", "Passkeys"),
            hidden(csrf),
            esc(title),
        ),
        script,
    )
}

fn when(at: i64) -> String {
    crate::mail::when(at)
}

/// Bytes for a person: `1.2 GB`.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// `POST /admin/sign-in` (a form: `key`).
pub async fn sign_in(State(state): State<Arc<AppState>>, peer: Peer, body: Bytes) -> Response {
    let Some(store) = store(&state) else {
        return unavailable();
    };
    if crate::api::brake(&state, &peer, Some("admin:operator")).is_err() {
        return sign_in_page(
            StatusCode::TOO_MANY_REQUESTS,
            Some("Too many tries. Wait a minute and try again."),
        );
    }
    let fields = crate::pages::form_fields(&body);
    let key = Zeroizing::new(fields.get("key").cloned().unwrap_or_default());
    let right = match store.check_operator_key(&key) {
        Ok(r) => r,
        Err(_) => return unavailable(),
    };
    let now = state.now();
    if !right {
        let _ = store.record_event(
            "sign_in",
            false,
            &format!("a wrong operator key from {}", peer.0),
            now,
        );
        eprintln!("lockdown-server: a wrong operator key was tried on the admin page");
        return sign_in_page(
            StatusCode::UNAUTHORIZED,
            Some("That isn't the operator key."),
        );
    }
    let Ok((cookie, _)) = state.admin.open(now) else {
        return unavailable();
    };
    let _ = store.record_event(
        "sign_in",
        true,
        &format!("the operator signed in from {}", peer.0),
        now,
    );
    let mut response = see_other("/admin");
    if let Ok(v) = HeaderValue::from_str(&format!(
        "{COOKIE}={cookie}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={SESSION_SECS}"
    )) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    response
}

/// `POST /admin/sign-out`.
pub async fn sign_out(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Ok(csrf) = signed_in(&state, &headers)
        && form_ok(&crate::pages::form_fields(&body), &csrf)
        && let Some(c) = cookie_of(&headers)
    {
        state.admin.close(&c);
    }
    let mut response = see_other("/admin");
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "__Host-lockdown-admin=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0",
        ),
    );
    response
}

/// `GET /admin`: the overview.
pub async fn overview(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let now = state.now();
    let facts = state.admin.facts();
    let (counts, totals) = match (store.counts(), store.totals(now)) {
        (Ok(c), Ok(t)) => (c, t),
        _ => return unavailable(),
    };
    let last_drill = store
        .events(Some("drill"), 1)
        .ok()
        .and_then(|v| v.into_iter().next());
    let last_good_drill = store
        .events(Some("drill"), 200)
        .ok()
        .and_then(|v| v.into_iter().find(|e| e.ok));
    let last_backup = store
        .events(Some("backup"), 1)
        .ok()
        .and_then(|v| v.into_iter().next());
    let schedule = state.admin.schedule();
    let mut out = String::new();
    if let Some(d) = &last_drill
        && !d.ok
    {
        out.push_str(&format!(
            "<p class=\"banner\" role=\"alert\">The last restore drill failed ({}): {}. The newest backup may not bring this server back. See Backups.</p>",
            when(d.at),
            esc(&d.detail)
        ));
    }
    out.push_str("<div class=\"grid\">");
    for (label, value) in [
        ("Accounts", counts.accounts.to_string()),
        ("Devices", totals.0.to_string()),
        ("Live sessions", totals.1.to_string()),
        ("Vaults", counts.vaults.to_string()),
        ("Attachments", size(totals.2)),
        ("Public log entries", counts.log_size.to_string()),
    ] {
        out.push_str(&format!(
            "<div class=\"card stat\"><span class=\"muted\">{label}</span><b>{}</b></div>",
            esc(&value)
        ));
    }
    out.push_str("</div>");
    let drill_line = match (&last_good_drill, &last_drill) {
        (Some(g), _) => format!("last successful restore: {}", when(g.at)),
        (None, Some(_)) => "no restore drill has passed yet".into(),
        (None, None) => "no restore drill has run yet".into(),
    };
    out.push_str(&format!(
        "<h2>This server</h2><div class=\"card\"><table><tbody>\
         <tr><th>Release</th><td>lockdown-server {}</td></tr>\
         <tr><th>Database</th><td>{}</td></tr>\
         <tr><th>Attachments</th><td>{}</td></tr>\
         <tr><th>Attachment room per account</th><td>{}</td></tr>\
         <tr><th>Sign-ups</th><td>{}</td></tr>\
         <tr><th>TLS</th><td>{}</td></tr>\
         <tr><th>Email</th><td>{}</td></tr>\
         <tr><th>Breach monitoring</th><td>{}</td></tr>\
         <tr><th>Backups</th><td>{}</td></tr>\
         <tr><th>Restore drills</th><td>{}</td></tr>\
         </tbody></table></div>",
        env!("CARGO_PKG_VERSION"),
        esc(&facts.database),
        esc(&facts.attachments),
        state
            .config
            .attachment_quota
            .map(size)
            .unwrap_or_else(|| "no limit".into()),
        if state.config.signups_open {
            "open: anyone who reaches the server can register"
        } else {
            "closed: only invited login names may register"
        },
        if facts.tls {
            "on the server itself"
        } else {
            "not on the server (a proxy in front, or this machine only)"
        },
        esc(&state
            .mail
            .as_ref()
            .map(|m| {
                format!(
                    "{}; new devices {}",
                    m.describe,
                    if m.verify_new_devices {
                        "are approved from the account's email"
                    } else {
                        "are not asked to approve by email"
                    }
                )
            })
            .unwrap_or_else(|| "not set up (no new-device emails)".into())),
        esc(facts.breach_feed.as_deref().unwrap_or("off")),
        esc(&match (&schedule, &last_backup) {
            (Some(s), Some(b)) => format!(
                "every {} into {}; last: {} ({})",
                hours(s.every_secs()),
                s.dir().display(),
                when(b.at),
                if b.ok { "made" } else { "failed" }
            ),
            (Some(s), None) => format!(
                "every {} into {}; none yet",
                hours(s.every_secs()),
                s.dir().display()
            ),
            (None, _) =>
                "not scheduled on this server (lockdown-server backup create, or --backup-dir)"
                    .into(),
        }),
        esc(&drill_line),
    ));
    out.push_str("<h2>Setup checks</h2><div class=\"card\"><ul>");
    let checks = [
        (true, "An operator key protects this page."),
        (
            !state.config.signups_open,
            "Sign-ups are closed; people join by invite.",
        ),
        (schedule.is_some(), "Backups run on a schedule."),
        (
            last_good_drill
                .as_ref()
                .is_some_and(|g| now - g.at <= 35 * 86_400)
                && last_drill.as_ref().is_some_and(|d| d.ok),
            "The newest restore drill passed within 35 days.",
        ),
        (
            state.mail.is_some(),
            "Email is set up: new devices are approved by email and alerts reach people.",
        ),
        (facts.breach_feed.is_some(), "Email breach monitoring runs."),
    ];
    for (ok, text) in checks {
        out.push_str(&format!(
            "<li><span class=\"{}\">{}</span> {}</li>",
            if ok { "ok" } else { "bad" },
            if ok { "Done:" } else { "To do:" },
            esc(text)
        ));
    }
    out.push_str("</ul></div>");
    shell(&csrf, "/admin", "Overview", &out)
}

fn hours(secs: u64) -> String {
    if secs.is_multiple_of(86_400) {
        let d = secs / 86_400;
        if d == 1 {
            "day".into()
        } else {
            format!("{d} days")
        }
    } else {
        let h = (secs / 3600).max(1);
        if h == 1 {
            "hour".into()
        } else {
            format!("{h} hours")
        }
    }
}

/// `GET /admin/accounts`.
pub async fn accounts(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let Ok(lines) = store.account_overview(state.now()) else {
        return unavailable();
    };
    let quota = state.config.attachment_quota;
    let mut out = String::from(
        "<p class=\"muted\">Login names and counts only: the server can't read a vault, a device's name or anything inside.</p>\
         <div class=\"card scroll\"><table><thead><tr><th>Login</th><th>Registered</th><th>Devices</th><th>Live sessions</th><th>Last used</th><th>Vaults</th><th>Attachments</th><th>Security keys</th></tr></thead><tbody>",
    );
    for a in &lines {
        let room = match quota {
            Some(q) => format!(
                "{} of {} ({}%)",
                size(a.blob_bytes),
                size(q),
                a.blob_bytes.saturating_mul(100) / q.max(1)
            ),
            None => size(a.blob_bytes),
        };
        out.push_str(&format!(
            "<tr><td><a href=\"/admin/accounts/{}\">{}</a>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            a.account_id,
            esc(&a.login),
            if a.login.starts_with(crate::drill_account::LOGIN_PREFIX) {
                " <span class=\"muted\">(this server's restore-drill account)</span>"
            } else {
                ""
            },
            when(a.created_at),
            a.devices,
            a.live_sessions,
            a.last_seen.map(when).unwrap_or_else(|| "never".into()),
            a.vaults,
            esc(&room),
            a.security_keys,
        ));
    }
    if lines.is_empty() {
        out.push_str("<tr><td colspan=\"8\" class=\"muted\">No accounts yet.</td></tr>");
    }
    out.push_str("</tbody></table></div>");
    shell(&csrf, "/admin/accounts", "Accounts", &out)
}

/// `GET /admin/accounts/{account}`.
pub async fn account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(account_id): Path<Id>,
) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let now = state.now();
    let Ok(Some(acc)) = crate::storage::Storage::account(store, account_id) else {
        return page(
            StatusCode::NOT_FOUND,
            "Not found",
            "<h1>No such account</h1><p><a href=\"/admin/accounts\">Accounts</a></p>",
        );
    };
    let devices = store.account_devices(account_id).unwrap_or_default();
    let sessions = crate::sessions::SessionStore::live_sessions(
        store,
        account_id,
        now,
        crate::renewal::idle_secs(&state),
    )
    .unwrap_or_default();
    let mut out = format!(
        "<p><a href=\"/admin/accounts\">All accounts</a></p><div class=\"card\"><p>Login: <b>{}</b></p><p class=\"muted\">Account id <span class=\"mono\">{account_id}</span></p></div>",
        esc(&acc.login)
    );
    out.push_str("<h2>Devices</h2><div class=\"card scroll\"><table><thead><tr><th>Device</th><th>Joined</th><th>State</th></tr></thead><tbody>");
    for (id, at, revoked) in &devices {
        out.push_str(&format!(
            "<tr><td class=\"mono\">{id}</td><td>{}</td><td>{}</td></tr>",
            when(*at),
            if *revoked {
                "revoked by the account"
            } else {
                "enrolled"
            }
        ));
    }
    out.push_str("</tbody></table></div><p class=\"muted\">Device names are encrypted; the account's own devices show them.</p>");
    out.push_str("<h2>Signed in</h2><div class=\"card scroll\"><table><thead><tr><th>Device</th><th>App</th><th>Address</th><th>Started</th><th>Last used</th></tr></thead><tbody>");
    for s in &sessions {
        out.push_str(&format!(
            "<tr><td class=\"mono\">{}</td><td>{}</td><td class=\"mono\">{}</td><td>{}</td><td>{}</td></tr>",
            s.device_id,
            esc(s.client.as_deref().unwrap_or("")),
            esc(s.address.as_deref().unwrap_or("")),
            when(s.created_at),
            when(s.last_seen),
        ));
    }
    if sessions.is_empty() {
        out.push_str("<tr><td colspan=\"5\" class=\"muted\">No live sessions.</td></tr>");
    }
    out.push_str("</tbody></table></div>");
    if !sessions.is_empty() {
        out.push_str(&format!(
            "<form method=\"post\" action=\"/admin/accounts/{account_id}/end-sessions\">{}<p><button class=\"danger\">End every session of this account</button></p></form>\
             <p class=\"muted\">Each device asks for the account password at its next sync. This revokes no device and changes no password.</p>",
            hidden(&csrf)
        ));
    }
    shell(&csrf, "/admin/accounts", &acc.login, &out)
}

/// `POST /admin/accounts/{account}/end-sessions`.
pub async fn end_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(account_id): Path<Id>,
    body: Bytes,
) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !form_ok(&crate::pages::form_fields(&body), &csrf) {
        return page(
            StatusCode::FORBIDDEN,
            "Refused",
            "<h1>Refused</h1><p>That form is from an old page; open it again.</p>",
        );
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let now = state.now();
    let ended = crate::sessions::SessionStore::end_other_sessions(
        store,
        account_id,
        &[0; 16],
        now,
        crate::renewal::idle_secs(&state),
    )
    .unwrap_or_default();
    for id in &ended {
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
    let _ = store.record_event(
        "sessions_ended",
        true,
        &format!(
            "the operator ended {} session(s) of account {account_id}",
            ended.len()
        ),
        now,
    );
    see_other(&format!("/admin/accounts/{account_id}"))
}

/// `GET /admin/invites`.
pub async fn invites(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    invites_page(&state, &csrf, None)
}

fn invites_page(state: &AppState, csrf: &[u8; 32], note: Option<&str>) -> Response {
    let Some(store) = store(state) else {
        return unavailable();
    };
    let now = state.now();
    let list = store.invites().unwrap_or_default();
    let mut out = String::new();
    if let Some(n) = note {
        out.push_str(&format!("<p role=\"status\">{}</p>", esc(n)));
    }
    out.push_str(&format!(
        "<p>{}</p><div class=\"card\"><form method=\"post\" action=\"/admin/invites\">{}\
         <label for=\"login\">Login name to let in</label><input id=\"login\" name=\"login\" type=\"text\" required autocomplete=\"off\">\
         <label for=\"days\">Days it lasts</label><input id=\"days\" name=\"days\" type=\"number\" min=\"1\" max=\"90\" value=\"{INVITE_DAYS}\">\
         <p><button>Invite</button></p></form></div>",
        if state.config.signups_open {
            "Sign-ups are open, so anyone can register; invites matter once you close them (LOCKDOWN_SIGNUPS=closed)."
        } else {
            "Sign-ups are closed. An invite lets exactly that login name register, once, before it runs out. Tell the person yourself; the invite is the name, not a code."
        },
        hidden(csrf)
    ));
    out.push_str("<div class=\"card scroll\"><table><thead><tr><th>Login</th><th>Made</th><th>Runs out</th><th>State</th><th></th></tr></thead><tbody>");
    for i in &list {
        let state_word = match i.used_at {
            Some(at) => format!("used {}", when(at)),
            None if i.expires_at <= now => "ran out".into(),
            None => "open".into(),
        };
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><form method=\"post\" action=\"/admin/invites/revoke\" class=\"inline\">{}<input type=\"hidden\" name=\"login\" value=\"{}\"><button class=\"quiet\">Remove</button></form></td></tr>",
            esc(&i.login),
            when(i.created_at),
            when(i.expires_at),
            esc(&state_word),
            hidden(csrf),
            esc(&i.login),
        ));
    }
    if list.is_empty() {
        out.push_str("<tr><td colspan=\"5\" class=\"muted\">No invites.</td></tr>");
    }
    out.push_str("</tbody></table></div>");
    shell(csrf, "/admin/invites", "Invites", &out)
}

/// `POST /admin/invites` (a form: `login`, `days`).
pub async fn invite_create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fields = crate::pages::form_fields(&body);
    if !form_ok(&fields, &csrf) {
        return invites_page(
            &state,
            &csrf,
            Some("That form was from an old page; nothing changed."),
        );
    }
    let login = fields
        .get("login")
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    let days: i64 = fields
        .get("days")
        .and_then(|d| d.trim().parse().ok())
        .unwrap_or(INVITE_DAYS)
        .clamp(1, 90);
    if login.is_empty() || login.chars().count() > 254 || login.chars().any(char::is_control) {
        return invites_page(&state, &csrf, Some("A login name is 1 to 254 characters."));
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let now = state.now();
    if let Ok(Some(_)) = crate::storage::Storage::account_by_login(store, &login) {
        return invites_page(
            &state,
            &csrf,
            Some("That login name is already an account."),
        );
    }
    if store.put_invite(&login, now, now + days * 86_400).is_err() {
        return unavailable();
    }
    let _ = store.record_event(
        "invite",
        true,
        &format!("invited {login} for {days} day(s)"),
        now,
    );
    invites_page(
        &state,
        &csrf,
        Some(&format!("{login} may register for {days} day(s).")),
    )
}

/// `POST /admin/invites/revoke` (a form: `login`).
pub async fn invite_revoke(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fields = crate::pages::form_fields(&body);
    if !form_ok(&fields, &csrf) {
        return invites_page(
            &state,
            &csrf,
            Some("That form was from an old page; nothing changed."),
        );
    }
    let login = fields
        .get("login")
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let removed = store.revoke_invite(&login).unwrap_or(false);
    invites_page(
        &state,
        &csrf,
        Some(if removed {
            "The invite was removed."
        } else {
            "There was no such invite."
        }),
    )
}

/// `GET /admin/backups`.
pub async fn backups(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    backups_page(&state, &csrf, None)
}

fn backups_page(state: &AppState, csrf: &[u8; 32], note: Option<&str>) -> Response {
    let Some(store) = store(state) else {
        return unavailable();
    };
    let mut out = String::new();
    if let Some(n) = note {
        out.push_str(&format!("<p role=\"status\">{}</p>", esc(n)));
    }
    match state.admin.schedule() {
        Some(s) => {
            out.push_str(&format!(
                "<div class=\"card\"><p>A backup every {}, the newest {} kept, in <span class=\"mono\">{}</span>. A restore drill every {}: the newest backup is opened with the Backup Key, restored into a scratch {}, and checked against this server.</p>\
                 <div class=\"row\"><form method=\"post\" action=\"/admin/backups/run\">{}<button>Back up now</button></form>\
                 <form method=\"post\" action=\"/admin/drills/run\">{}<button class=\"quiet\">Run a restore drill now</button></form></div></div>",
                hours(s.every_secs()),
                s.keep(),
                esc(&s.dir().display().to_string()),
                hours(s.drill_every_secs()),
                if store.dialect() == crate::db::Dialect::Postgres { "Postgres schema" } else { "database" },
                hidden(csrf),
                hidden(csrf),
            ));
            out.push_str("<h2>On disk</h2><div class=\"card scroll\"><table><thead><tr><th>File</th><th>Size</th><th>Made</th></tr></thead><tbody>");
            let files = crate::drills::list_backups(s.dir()).unwrap_or_default();
            for f in &files {
                out.push_str(&format!(
                    "<tr><td class=\"mono\">{}</td><td>{}</td><td>{}</td></tr>",
                    esc(&f.name),
                    size(f.size),
                    when(f.modified)
                ));
            }
            if files.is_empty() {
                out.push_str("<tr><td colspan=\"3\" class=\"muted\">None yet.</td></tr>");
            }
            out.push_str("</tbody></table></div><p class=\"muted\">Copy them somewhere else too: three copies, two kinds of storage, one away from home.</p>");
        }
        None => out.push_str(
            "<div class=\"card\"><p>Backups aren't scheduled on this server. Start it with <code>--backup-dir</code> and the Backup Key (<code>LOCKDOWN_BACKUP_KEY</code> or <code>--backup-key-file</code>) to have it back up and drill by itself, or run <code>lockdown-server backup create</code> from a scheduler of your own.</p></div>",
        ),
    }
    out.push_str("<h2>Recent backups and drills</h2><div class=\"card scroll\"><table><thead><tr><th>When</th><th>What</th><th>Result</th><th>Detail</th></tr></thead><tbody>");
    let events: Vec<EventRow> = store
        .events(None, 200)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind == "backup" || e.kind == "drill")
        .take(40)
        .collect();
    for e in &events {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>",
            when(e.at),
            if e.kind == "drill" {
                "Restore drill"
            } else {
                "Backup"
            },
            if e.ok { "ok" } else { "bad" },
            if e.ok { "passed" } else { "failed" },
            esc(&e.detail)
        ));
    }
    if events.is_empty() {
        out.push_str("<tr><td colspan=\"4\" class=\"muted\">Nothing yet.</td></tr>");
    }
    out.push_str("</tbody></table></div>");
    shell(csrf, "/admin/backups", "Backups", &out)
}

/// `POST /admin/backups/run`: a backup now, on a thread of its own.
pub async fn backup_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    run_now(state, headers, body, false)
}

/// `POST /admin/drills/run`: a restore drill now, on a thread of its own.
pub async fn drill_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    run_now(state, headers, body, true)
}

fn run_now(state: Arc<AppState>, headers: HeaderMap, body: Bytes, drill: bool) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !form_ok(&crate::pages::form_fields(&body), &csrf) {
        return backups_page(
            &state,
            &csrf,
            Some("That form was from an old page; nothing ran."),
        );
    }
    let Some(schedule) = state.admin.schedule() else {
        return backups_page(&state, &csrf, Some("Backups aren't set up on this server."));
    };
    let started = crate::drills::run_in_background(state.clone(), schedule, drill);
    backups_page(
        &state,
        &csrf,
        Some(match (started, drill) {
            (true, true) => "The restore drill started; its result shows here when it is done.",
            (true, false) => "The backup started; it shows here when it is done.",
            (false, _) => "A backup or a drill is already running.",
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operator_key_is_kept_as_a_hash_and_checked() {
        let s = SqlStorage::open_scratch().unwrap();
        assert!(!s.has_operator_key().unwrap());
        assert!(!s.check_operator_key("anything at all!").unwrap());
        assert!(s.set_operator_key("short").is_err());
        let key = generate_key().unwrap();
        assert!(key.starts_with("LDOP-"));
        s.set_operator_key(&key).unwrap();
        assert!(
            s.check_operator_key(&format!("  {} ", key.as_str()))
                .unwrap()
        );
        assert!(!s.check_operator_key("LDOP-WRONG-WRONG-WRONG").unwrap());
        // The key itself is nowhere in the store.
        let stored: Vec<u8> = s
            .conn()
            .query_row("SELECT value FROM operator_settings", params![], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            !stored
                .windows(8)
                .any(|w| key.as_bytes().windows(8).any(|k| k == w))
        );
        assert!(s.clear_operator_key().unwrap());
        assert!(!s.has_operator_key().unwrap());
    }

    #[test]
    fn invites_open_once_and_run_out() {
        let s = SqlStorage::open_scratch().unwrap();
        s.put_invite("New@Example.com", 100, 200).unwrap();
        assert!(s.invite_open("new@example.com", 150).unwrap());
        assert!(!s.invite_open("new@example.com", 200).unwrap());
        s.use_invite("new@example.com", 160).unwrap();
        assert!(!s.invite_open("new@example.com", 170).unwrap());
        s.put_invite("new@example.com", 300, 400).unwrap();
        assert!(s.invite_open("new@example.com", 350).unwrap());
        assert_eq!(s.invites().unwrap().len(), 1);
        assert!(s.revoke_invite("NEW@example.com").unwrap());
        assert!(s.invites().unwrap().is_empty());
    }

    #[test]
    fn events_list_newest_first() {
        let s = SqlStorage::open_scratch().unwrap();
        s.record_event("backup", true, "one", 1).unwrap();
        s.record_event("drill", false, "two", 2).unwrap();
        s.record_event("backup", true, "three", 3).unwrap();
        let all = s.events(None, 10).unwrap();
        assert_eq!(
            all.iter().map(|e| e.detail.as_str()).collect::<Vec<_>>(),
            ["three", "two", "one"]
        );
        let drills = s.events(Some("drill"), 10).unwrap();
        assert_eq!(drills.len(), 1);
        assert!(!drills[0].ok);
        assert_eq!(size(999), "999 bytes");
        assert_eq!(size(5_000_000_000), "5.0 GB");
    }
}
