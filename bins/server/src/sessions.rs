//! Sessions: one row per session token, so a person can see where their
//! account is signed in and end any of those sessions from another device
//! (`account-alerts`; docs/plan/20-self-host-server.md §6).
//!
//! A token (`lk1`, D52) still verifies by its signature alone; this table
//! adds the one thing a signature can't say, that the session was ended.
//! The row is keyed by the token's random nonce, which is also the
//! session's id on the wire (hex): knowing it does not let anyone use the
//! session, the signed token is still needed. A token from before this
//! table existed gets its row the first time it is seen.
//!
//! What a row holds, and so what the server learns: the account, the
//! device, when the session started, when it was last used (to the
//! minute), when it runs out, the address it was last used from and the
//! client's own name for itself (`lockdown/0.1.0 (macos)`). The server saw
//! all of that on every request already; it now keeps the latest of each.
//! Rows are dropped a week after their token ran out (an ended session's
//! row is kept at least that long, so its token can never come back).
//!
//! The table lives in the server's database beside the rest
//! ([`create_tables`] runs with the main schema). It is additive (`IF NOT
//! EXISTS`); the schema version is unchanged.

use crate::db::{Conn, DbError, DbResult, Row, params};
use crate::storage::{Result, SqlStorage, StorageError};
use lockdown_sync::wire::SessionInfo;
use lockdown_vault::Id;

/// How long a session's row outlives the session.
pub const KEEP_AFTER_END_SECS: i64 = 7 * 24 * 60 * 60;

/// How often a session's "last used" is written, at most.
pub const TOUCH_EVERY_SECS: i64 = 60;

/// A session as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    /// The token's nonce: the session's id.
    pub id: [u8; 16],
    /// The account.
    pub account_id: Id,
    /// The device that logged in.
    pub device_id: Id,
    /// When the token was issued.
    pub created_at: i64,
    /// When it was last used.
    pub last_seen: i64,
    /// When it runs out.
    pub expires_at: i64,
    /// The address it was last used from.
    pub address: Option<String>,
    /// The client's name for itself.
    pub client: Option<String>,
    /// When it was ended, if it was.
    pub ended_at: Option<i64>,
}

impl SessionRow {
    /// The wire view; `current` marks the asking session.
    pub fn info(&self, current: &[u8; 16]) -> SessionInfo {
        SessionInfo {
            id: session_id_hex(&self.id),
            device_id: self.device_id,
            created_at: self.created_at,
            last_seen: self.last_seen,
            expires_at: self.expires_at,
            address: self.address.clone(),
            client: self.client.clone(),
            current: &self.id == current,
        }
    }
}

/// A session id as it travels: 32 lowercase hex digits.
pub fn session_id_hex(id: &[u8; 16]) -> String {
    data_encoding::HEXLOWER.encode(id)
}

/// The id from its hex form.
pub fn parse_session_id(text: &str) -> Option<[u8; 16]> {
    let bytes = data_encoding::HEXLOWER_PERMISSIVE
        .decode(text.as_bytes())
        .ok()?;
    bytes.try_into().ok()
}

/// What the server keeps for sessions.
pub trait SessionStore: Send + Sync {
    /// Records a session the first time its token is seen (a no-op when
    /// the row is there).
    fn open_session(&self, row: &SessionRow) -> Result<()>;
    /// The session's row, if any.
    fn session(&self, id: &[u8; 16]) -> Result<Option<SessionRow>>;
    /// Notes a use at `now` from `address` by `client`.
    fn touch_session(
        &self,
        id: &[u8; 16],
        now: i64,
        address: Option<&str>,
        client: Option<&str>,
    ) -> Result<()>;
    /// The account's sessions that have neither ended nor run out at
    /// `now`, oldest first.
    fn live_sessions(&self, account_id: Id, now: i64) -> Result<Vec<SessionRow>>;
    /// Ends one of the account's sessions; `false` when the account has no
    /// such live session.
    fn end_session(&self, account_id: Id, id: &[u8; 16], now: i64) -> Result<bool>;
    /// Ends every live session of the account but `keep`; the ids ended.
    fn end_other_sessions(
        &self,
        account_id: Id,
        keep: &[u8; 16],
        now: i64,
    ) -> Result<Vec<[u8; 16]>>;
    /// Drops rows whose token ran out [`KEEP_AFTER_END_SECS`] ago or more.
    fn prune_sessions(&self, now: i64) -> Result<usize>;
}

/// The sessions table, beside the main schema.
pub(crate) fn create_tables(conn: &Conn<'_>) -> std::result::Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
             session_id BLOB PRIMARY KEY NOT NULL,
             account_id TEXT NOT NULL,
             device_id  TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             last_seen  INTEGER NOT NULL,
             expires_at INTEGER NOT NULL,
             address    TEXT,
             client     TEXT,
             ended_at   INTEGER
         );
         CREATE INDEX IF NOT EXISTS sessions_by_account ON sessions (account_id, created_at);",
    )
}

type Columns = (
    Vec<u8>,
    String,
    String,
    i64,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<i64>,
);

const COLUMNS: &str = "session_id, account_id, device_id, created_at, last_seen, expires_at, address, client, ended_at";

fn read_row(r: &Row) -> DbResult<Columns> {
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
}

fn row_from(c: Columns) -> Result<SessionRow> {
    let bad = |what: &str| StorageError::Backend(format!("a stored session has a bad {what}"));
    Ok(SessionRow {
        id: c.0.try_into().map_err(|_| bad("id"))?,
        account_id: Id::parse(&c.1).map_err(|_| bad("account"))?,
        device_id: Id::parse(&c.2).map_err(|_| bad("device"))?,
        created_at: c.3,
        last_seen: c.4,
        expires_at: c.5,
        address: c.6,
        client: c.7,
        ended_at: c.8,
    })
}

impl SessionStore for SqlStorage {
    fn open_session(&self, row: &SessionRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sessions (session_id, account_id, device_id, created_at, last_seen, expires_at, address, client, ended_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT DO NOTHING",
            params![
                &row.id[..],
                row.account_id.to_string(),
                row.device_id.to_string(),
                row.created_at,
                row.last_seen,
                row.expires_at,
                row.address,
                row.client,
                row.ended_at
            ],
        )?;
        Ok(())
    }

    fn session(&self, id: &[u8; 16]) -> Result<Option<SessionRow>> {
        let found = self.conn().query_opt(
            &format!("SELECT {COLUMNS} FROM sessions WHERE session_id = ?1"),
            params![&id[..]],
            read_row,
        )?;
        found.map(row_from).transpose()
    }

    fn touch_session(
        &self,
        id: &[u8; 16],
        now: i64,
        address: Option<&str>,
        client: Option<&str>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE sessions SET last_seen = CASE WHEN last_seen > ?2 THEN last_seen ELSE ?2 END, address = COALESCE(?3, address), client = COALESCE(?4, client) WHERE session_id = ?1",
            params![&id[..], now, address, client],
        )?;
        Ok(())
    }

    fn live_sessions(&self, account_id: Id, now: i64) -> Result<Vec<SessionRow>> {
        self.conn()
            .query_map(
                &format!(
                    "SELECT {COLUMNS} FROM sessions WHERE account_id = ?1 AND ended_at IS NULL AND expires_at > ?2 ORDER BY created_at, session_id"
                ),
                params![account_id.to_string(), now],
                read_row,
            )?
            .into_iter()
            .map(row_from)
            .collect()
    }

    fn end_session(&self, account_id: Id, id: &[u8; 16], now: i64) -> Result<bool> {
        let n = self.conn().execute(
            "UPDATE sessions SET ended_at = ?3 WHERE session_id = ?1 AND account_id = ?2 AND ended_at IS NULL AND expires_at > ?3",
            params![&id[..], account_id.to_string(), now],
        )?;
        Ok(n > 0)
    }

    fn end_other_sessions(
        &self,
        account_id: Id,
        keep: &[u8; 16],
        now: i64,
    ) -> Result<Vec<[u8; 16]>> {
        let tx = self.begin()?;
        let ended = tx
            .query_map(
                "SELECT session_id FROM sessions WHERE account_id = ?1 AND session_id != ?2 AND ended_at IS NULL AND expires_at > ?3 ORDER BY created_at, session_id",
                params![account_id.to_string(), &keep[..], now],
                |r| r.get::<Vec<u8>>(0),
            )?
            .into_iter()
            .map(|id| {
                <[u8; 16]>::try_from(id)
                    .map_err(|_| StorageError::Backend("a stored session has a bad id".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        tx.execute(
            "UPDATE sessions SET ended_at = ?3 WHERE account_id = ?1 AND session_id != ?2 AND ended_at IS NULL AND expires_at > ?3",
            params![account_id.to_string(), &keep[..], now],
        )?;
        tx.commit()?;
        Ok(ended)
    }

    fn prune_sessions(&self, now: i64) -> Result<usize> {
        let cutoff = now.saturating_sub(KEEP_AFTER_END_SECS);
        Ok(self.conn().execute(
            // An ended session's row stays until its token has run out
            // as well: without the row, the token would be taken for
            // one from before this table and start a new row.
            "DELETE FROM sessions WHERE expires_at < ?1",
            params![cutoff],
        )?)
    }
}

/// The client's name for itself from a `User-Agent` header, kept short and
/// printable (it comes from the network: nothing in it is trusted).
pub fn client_label(user_agent: Option<&str>) -> Option<String> {
    let text: String = user_agent?
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(80)
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(account: Id, n: u8, created: i64, expires: i64) -> SessionRow {
        SessionRow {
            id: [n; 16],
            account_id: account,
            device_id: Id::new(),
            created_at: created,
            last_seen: created,
            expires_at: expires,
            address: Some("10.0.0.1".into()),
            client: Some("lockdown/0.1.0 (linux)".into()),
            ended_at: None,
        }
    }

    #[test]
    fn sessions_open_once_list_in_order_end_and_prune() {
        let store = SqlStorage::open_scratch().unwrap();
        let (a, b) = (Id::new(), Id::new());
        store.open_session(&row(a, 2, 200, 10_000)).unwrap();
        store.open_session(&row(a, 1, 100, 10_000)).unwrap();
        store.open_session(&row(a, 3, 300, 400)).unwrap();
        store.open_session(&row(b, 4, 100, 10_000)).unwrap();
        // A second open of the same id keeps the first row.
        let mut again = row(a, 1, 999, 99_999);
        again.client = Some("other".into());
        store.open_session(&again).unwrap();
        let live = store.live_sessions(a, 500).unwrap();
        assert_eq!(
            live.iter().map(|r| r.id[0]).collect::<Vec<_>>(),
            vec![1, 2],
            "oldest first; the one that ran out is gone"
        );
        assert_eq!(live[0].client.as_deref(), Some("lockdown/0.1.0 (linux)"));

        store
            .touch_session(&[1; 16], 600, Some("192.0.2.7"), None)
            .unwrap();
        let one = store.session(&[1; 16]).unwrap().unwrap();
        assert_eq!(
            (one.last_seen, one.address.as_deref()),
            (600, Some("192.0.2.7"))
        );

        // Another account can't end this one's sessions.
        assert!(!store.end_session(b, &[1; 16], 700).unwrap());
        assert!(store.end_session(a, &[1; 16], 700).unwrap());
        assert!(!store.end_session(a, &[1; 16], 701).unwrap(), "only once");
        assert_eq!(
            store.session(&[1; 16]).unwrap().unwrap().ended_at,
            Some(700)
        );

        store.open_session(&row(a, 5, 800, 10_000)).unwrap();
        let ended = store.end_other_sessions(a, &[5; 16], 900).unwrap();
        assert_eq!(ended, vec![[2; 16]]);
        assert_eq!(
            store
                .live_sessions(a, 900)
                .unwrap()
                .iter()
                .map(|r| r.id[0])
                .collect::<Vec<_>>(),
            vec![5]
        );
        assert_eq!(store.live_sessions(b, 900).unwrap().len(), 1);

        // A week after they stopped, ended and run-out rows go.
        assert_eq!(store.prune_sessions(10_000).unwrap(), 0);
        let pruned = store.prune_sessions(10_001 + KEEP_AFTER_END_SECS).unwrap();
        assert_eq!(pruned, 5);
    }

    #[test]
    fn ids_and_client_labels_survive_hostile_input() {
        let id = [0xab; 16];
        assert_eq!(parse_session_id(&session_id_hex(&id)), Some(id));
        assert_eq!(parse_session_id("abc"), None);
        assert_eq!(parse_session_id(&"zz".repeat(16)), None);
        assert_eq!(client_label(Some("  ")), None);
        assert_eq!(
            client_label(Some("lockdown/0.1.0 (linux)\u{1b}[31m\n")).as_deref(),
            Some("lockdown/0.1.0 (linux)[31m")
        );
        assert_eq!(client_label(Some(&"x".repeat(500))).unwrap().len(), 80);
    }
}
