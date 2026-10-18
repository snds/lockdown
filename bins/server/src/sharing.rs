//! What the server keeps for sharing (`docs/plan/18-sharing.md` §4): the
//! membership chain of each shared vault and the share envelopes per
//! member and key generation, items shared on their own, share links,
//! request links and emergency grants.
//!
//! Nothing here opens anything. Envelopes are wrapped to the recipient's
//! receiving key and signed by the sender's device; share links and
//! answers to requests are sealed under keys the server never sees. The
//! server does learn the sharing graph (who shares with whom, with which
//! right), because it must refuse a write from a member without `edit`
//! and must not hand a vault to someone who isn't in it.
//!
//! The tables live in the same SQLite file as everything else
//! ([`create_tables`] runs with the main schema), additive (`IF NOT
//! EXISTS`), so an older build opens a file that has them.

use lockdown_vault::Id;
use lockdown_vault::sharing::{Party, Right, VaultKind};
use rusqlite::{Connection, OptionalExtension, params};

use crate::storage::{Result, SqliteStorage, StorageError};

fn backend(e: rusqlite::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

fn id_of(text: &str) -> Result<Id> {
    Id::parse(text).map_err(|_| StorageError::Backend(format!("a bad id is stored: {text}")))
}

fn cbor<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    lockdown_crypto::cbor::to_canonical(value).map_err(|e| StorageError::Backend(e.to_string()))
}

fn uncbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    lockdown_crypto::cbor::from_slice(bytes)
        .map_err(|_| StorageError::Backend("a stored sharing row didn't decode".into()))
}

fn kind_from(text: &str) -> Result<VaultKind> {
    match text {
        "shared" => Ok(VaultKind::Shared),
        "family" => Ok(VaultKind::Family),
        other => Err(StorageError::Backend(format!(
            "a shared vault of unknown kind '{other}' is stored"
        ))),
    }
}

fn right_from(text: &str) -> Result<Right> {
    Right::parse(text).map_err(|e| StorageError::Backend(e.to_string()))
}

fn u(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

fn i(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// The sharing tables, beside the main schema.
pub(crate) fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS shared_vaults (
             vault_id      TEXT PRIMARY KEY NOT NULL,
             owner_account TEXT NOT NULL,
             kind          TEXT NOT NULL,
             key_ref       TEXT NOT NULL,
             seq           INTEGER NOT NULL,
             head          BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS membership_events (
             vault_id TEXT NOT NULL,
             seq      INTEGER NOT NULL,
             body     BLOB NOT NULL,
             PRIMARY KEY (vault_id, seq)
         );
         CREATE TABLE IF NOT EXISTS vault_members (
             vault_id   TEXT NOT NULL,
             account_id TEXT NOT NULL,
             right_name TEXT NOT NULL,
             party      BLOB NOT NULL,
             active     INTEGER NOT NULL,
             PRIMARY KEY (vault_id, account_id)
         );
         CREATE INDEX IF NOT EXISTS vault_members_by_account ON vault_members (account_id);
         CREATE TABLE IF NOT EXISTS share_envelopes (
             vault_id   TEXT NOT NULL,
             key_ref    TEXT NOT NULL,
             account_id TEXT NOT NULL,
             body       BLOB NOT NULL,
             PRIMARY KEY (vault_id, key_ref, account_id)
         );
         CREATE TABLE IF NOT EXISTS item_shares (
             share_id          TEXT PRIMARY KEY NOT NULL,
             vault_id          TEXT NOT NULL,
             item_id           TEXT NOT NULL,
             owner_account     TEXT NOT NULL,
             recipient_account TEXT NOT NULL,
             recipient         BLOB NOT NULL,
             right_name        TEXT NOT NULL,
             envelope          BLOB NOT NULL,
             created_at        INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sends (
             send_id       TEXT PRIMARY KEY NOT NULL,
             owner_account TEXT NOT NULL,
             created_at    INTEGER NOT NULL,
             expires_at    INTEGER NOT NULL,
             max_views     INTEGER,
             views         INTEGER NOT NULL DEFAULT 0,
             password      BLOB,
             body          BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS secret_requests (
             request_id    TEXT PRIMARY KEY NOT NULL,
             owner_account TEXT NOT NULL,
             created_at    INTEGER NOT NULL,
             expires_at    INTEGER NOT NULL,
             key           BLOB NOT NULL,
             answer        BLOB,
             answered_at   INTEGER
         );
         CREATE TABLE IF NOT EXISTS emergency_grants (
             grant_id        TEXT PRIMARY KEY NOT NULL,
             owner_account   TEXT NOT NULL,
             contact_account TEXT NOT NULL,
             body            BLOB NOT NULL,
             state           TEXT NOT NULL,
             requested_at    INTEGER
         );
         CREATE TABLE IF NOT EXISTS emergency_events (
             grant_id   TEXT NOT NULL,
             seq        INTEGER NOT NULL,
             at         INTEGER NOT NULL,
             kind       TEXT NOT NULL,
             by_account TEXT NOT NULL,
             step       BLOB,
             PRIMARY KEY (grant_id, seq)
         );
         CREATE TABLE IF NOT EXISTS emergency_envelopes (
             grant_id TEXT NOT NULL,
             vault_id TEXT NOT NULL,
             body     BLOB NOT NULL,
             PRIMARY KEY (grant_id, vault_id)
         );",
    )
}

/// A shared vault's position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedRow {
    /// The vault.
    pub vault_id: Id,
    /// Its owner.
    pub owner_account: Id,
    /// A group or a household.
    pub kind: VaultKind,
    /// The Vault Key generation in force.
    pub key_ref: Id,
    /// The newest event's `seq`.
    pub seq: u64,
    /// Its hash.
    pub head: lockdown_vault::Hash32,
}

/// One member, current or former.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberRow {
    /// Who.
    pub party: Party,
    /// Their right while a member.
    pub right: Right,
    /// Whether they are still in the vault.
    pub active: bool,
}

/// What accepting a membership event writes, at once.
pub struct MembershipWrite<'a> {
    /// The vault.
    pub vault_id: Id,
    /// The owner, the kind, and the generation and head after the event.
    pub shared: &'a SharedRow,
    /// The event's bytes and `seq`.
    pub event: (&'a [u8], u64),
    /// Every member after the event (current and former), replacing the rows.
    pub members: &'a [MemberRow],
    /// Envelopes: (account, key generation, bytes).
    pub envelopes: &'a [(Id, Id, Vec<u8>)],
}

/// One item shared on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemShareRow {
    /// The share.
    pub share_id: Id,
    /// The vault.
    pub vault_id: Id,
    /// The item.
    pub item_id: Id,
    /// Who shared it.
    pub owner_account: Id,
    /// Who it is for.
    pub recipient: Party,
    /// `view` or `fill-only`.
    pub right: Right,
    /// The Item Key wrapped to them.
    pub envelope: Vec<u8>,
    /// When.
    pub created_at: i64,
}

/// One share link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendRow {
    /// The link.
    pub send_id: Id,
    /// Who made it.
    pub owner_account: Id,
    /// When.
    pub created_at: i64,
    /// When it stops opening.
    pub expires_at: i64,
    /// The view limit.
    pub max_views: Option<u32>,
    /// How many times it was opened.
    pub views: u32,
    /// The password half (CBOR of `SendPassword`).
    pub password: Option<lockdown_vault::send::SendPassword>,
    /// The ciphertext.
    pub body: Vec<u8>,
}

/// One request link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestRow {
    /// The request.
    pub request_id: Id,
    /// Who asked.
    pub owner_account: Id,
    /// When.
    pub created_at: i64,
    /// When it stops taking an answer.
    pub expires_at: i64,
    /// The key to seal to, as `RequestKey` CBOR.
    pub key: Vec<u8>,
    /// The sealed answer.
    pub answer: Option<Vec<u8>>,
    /// When it was answered.
    pub answered_at: Option<i64>,
}

/// One emergency grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantRow {
    /// The grant.
    pub grant_id: Id,
    /// Whose vaults.
    pub owner_account: Id,
    /// Who may ask.
    pub contact_account: Id,
    /// The signed grant.
    pub body: Vec<u8>,
    /// Where it stands (`invited`, `accepted`, `requested`, `released`,
    /// `declined`, `revoked`).
    pub state: String,
    /// When the waiting request was made.
    pub requested_at: Option<i64>,
}

/// One recorded emergency step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmergencyEventRow {
    /// Its number.
    pub seq: u64,
    /// When the server recorded it.
    pub at: i64,
    /// What happened.
    pub kind: String,
    /// Who.
    pub by_account: Id,
    /// The signed step, when a person took it.
    pub step: Option<Vec<u8>>,
}

/// What the server keeps for sharing.
pub trait SharingStore: Send + Sync {
    /// A shared vault's position, if it is shared.
    fn shared_vault(&self, vault_id: Id) -> Result<Option<SharedRow>>;
    /// Its membership events after `since`, oldest first, as signed.
    fn membership_events(&self, vault_id: Id, since: u64) -> Result<Vec<Vec<u8>>>;
    /// Writes an accepted event, refusing it if another was accepted first.
    fn accept_membership(&self, write: MembershipWrite<'_>) -> Result<bool>;
    /// Every member of a vault, current and former, by account.
    fn members(&self, vault_id: Id) -> Result<Vec<MemberRow>>;
    /// The vaults shared with `account` (as a current member), with its right.
    fn shared_with(&self, account: Id) -> Result<Vec<(Id, Right)>>;
    /// The shared vaults `account` owns.
    fn owned_shared(&self, account: Id) -> Result<Vec<SharedRow>>;
    /// The envelope wrapped for `account` at `key_ref`.
    fn share_envelope(&self, vault_id: Id, key_ref: Id, account: Id) -> Result<Option<Vec<u8>>>;

    /// Records or replaces an item share.
    fn put_item_share(&self, row: &ItemShareRow) -> Result<()>;
    /// One share.
    fn item_share(&self, share_id: Id) -> Result<Option<ItemShareRow>>;
    /// Shares to `account`.
    fn item_shares_to(&self, account: Id) -> Result<Vec<ItemShareRow>>;
    /// Shares by `account`.
    fn item_shares_from(&self, account: Id) -> Result<Vec<ItemShareRow>>;
    /// Removes one.
    fn delete_item_share(&self, share_id: Id) -> Result<bool>;

    /// Records a share link.
    fn put_send(&self, row: &SendRow) -> Result<()>;
    /// One link.
    fn send(&self, send_id: Id) -> Result<Option<SendRow>>;
    /// Counts one view if the link is still open at `now`; `false` when it
    /// has run out (expired or every view used).
    fn count_view(&self, send_id: Id, now: i64) -> Result<bool>;
    /// Links by `account`, newest first.
    fn sends_of(&self, account: Id) -> Result<Vec<SendRow>>;
    /// Removes one of `account`'s links.
    fn delete_send(&self, account: Id, send_id: Id) -> Result<bool>;
    /// Removes links that ran out before `now`.
    fn purge_sends(&self, now: i64) -> Result<usize>;

    /// Records a request.
    fn put_request(&self, row: &RequestRow) -> Result<()>;
    /// One request.
    fn request(&self, request_id: Id) -> Result<Option<RequestRow>>;
    /// Takes the one answer if the request is still open at `now`.
    fn answer_request(&self, request_id: Id, answer: &[u8], now: i64) -> Result<bool>;
    /// Requests by `account`, newest first.
    fn requests_of(&self, account: Id) -> Result<Vec<RequestRow>>;
    /// Removes one of `account`'s requests.
    fn delete_request(&self, account: Id, request_id: Id) -> Result<bool>;

    /// Records a grant with its envelopes and its first event.
    fn put_grant(&self, row: &GrantRow, envelopes: &[(Id, Vec<u8>)], at: i64) -> Result<()>;
    /// One grant.
    fn grant(&self, grant_id: Id) -> Result<Option<GrantRow>>;
    /// Grants made by `account`.
    fn grants_by_owner(&self, account: Id) -> Result<Vec<GrantRow>>;
    /// Grants naming `account` as the contact.
    fn grants_for_contact(&self, account: Id) -> Result<Vec<GrantRow>>;
    /// Moves a grant on and records the event, at once.
    fn step_grant(
        &self,
        grant_id: Id,
        state: &str,
        requested_at: Option<i64>,
        event: (&str, Id, Option<&[u8]>),
        at: i64,
    ) -> Result<()>;
    /// Replaces a grant's envelopes and records it.
    fn replace_grant_envelopes(
        &self,
        grant_id: Id,
        envelopes: &[(Id, Vec<u8>)],
        by: Id,
        at: i64,
    ) -> Result<()>;
    /// A grant's events, oldest first.
    fn emergency_events(&self, grant_id: Id) -> Result<Vec<EmergencyEventRow>>;
    /// A grant's envelopes, by vault.
    fn emergency_envelopes(&self, grant_id: Id) -> Result<Vec<(Id, Vec<u8>)>>;
}

type SharedColumns = (String, String, String, i64, Vec<u8>);

fn shared_from(vault_id: Id, row: SharedColumns) -> Result<SharedRow> {
    let (owner, kind, key_ref, seq, head) = row;
    Ok(SharedRow {
        vault_id,
        owner_account: id_of(&owner)?,
        kind: kind_from(&kind)?,
        key_ref: id_of(&key_ref)?,
        seq: u(seq),
        head: lockdown_vault::Hash32(
            head.try_into()
                .map_err(|_| StorageError::Backend("a stored head isn't 32 bytes".into()))?,
        ),
    })
}

type ItemShareColumns = (
    String,
    String,
    String,
    String,
    Vec<u8>,
    String,
    Vec<u8>,
    i64,
);

fn item_share_from(row: ItemShareColumns) -> Result<ItemShareRow> {
    let (share, vault, item, owner, recipient, right, envelope, created_at) = row;
    Ok(ItemShareRow {
        share_id: id_of(&share)?,
        vault_id: id_of(&vault)?,
        item_id: id_of(&item)?,
        owner_account: id_of(&owner)?,
        recipient: uncbor(&recipient)?,
        right: right_from(&right)?,
        envelope,
        created_at,
    })
}

const ITEM_SHARE_COLUMNS: &str =
    "share_id, vault_id, item_id, owner_account, recipient, right_name, envelope, created_at";

type SendColumns = (
    String,
    String,
    i64,
    i64,
    Option<i64>,
    i64,
    Option<Vec<u8>>,
    Vec<u8>,
);

fn send_from(row: SendColumns) -> Result<SendRow> {
    let (send, owner, created_at, expires_at, max_views, views, password, body) = row;
    Ok(SendRow {
        send_id: id_of(&send)?,
        owner_account: id_of(&owner)?,
        created_at,
        expires_at,
        max_views: max_views.map(|m| u32::try_from(m).unwrap_or(0)),
        views: u32::try_from(views).unwrap_or(u32::MAX),
        password: password.map(|p| uncbor(&p)).transpose()?,
        body,
    })
}

const SEND_COLUMNS: &str =
    "send_id, owner_account, created_at, expires_at, max_views, views, password, body";

type RequestColumns = (
    String,
    String,
    i64,
    i64,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<i64>,
);

fn request_from(row: RequestColumns) -> Result<RequestRow> {
    let (request, owner, created_at, expires_at, key, answer, answered_at) = row;
    Ok(RequestRow {
        request_id: id_of(&request)?,
        owner_account: id_of(&owner)?,
        created_at,
        expires_at,
        key,
        answer,
        answered_at,
    })
}

const REQUEST_COLUMNS: &str =
    "request_id, owner_account, created_at, expires_at, key, answer, answered_at";

type GrantColumns = (String, String, String, Vec<u8>, String, Option<i64>);

fn grant_from(row: GrantColumns) -> Result<GrantRow> {
    let (grant, owner, contact, body, state, requested_at) = row;
    Ok(GrantRow {
        grant_id: id_of(&grant)?,
        owner_account: id_of(&owner)?,
        contact_account: id_of(&contact)?,
        body,
        state,
        requested_at,
    })
}

const GRANT_COLUMNS: &str = "grant_id, owner_account, contact_account, body, state, requested_at";

impl SqliteStorage {
    fn query_rows<T, R>(
        &self,
        sql: &str,
        args: &[&dyn rusqlite::ToSql],
        read: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<R>,
        convert: impl Fn(R) -> Result<T>,
    ) -> Result<Vec<T>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(sql).map_err(backend)?;
        let rows = stmt
            .query_map(args, read)
            .map_err(backend)?
            .collect::<std::result::Result<Vec<R>, _>>()
            .map_err(backend)?;
        rows.into_iter().map(convert).collect()
    }

    fn add_emergency_event(
        tx: &Connection,
        grant_id: Id,
        kind: &str,
        by: Id,
        step: Option<&[u8]>,
        at: i64,
    ) -> Result<()> {
        let seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM emergency_events WHERE grant_id = ?1",
                params![grant_id.to_string()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        tx.execute(
            "INSERT INTO emergency_events (grant_id, seq, at, kind, by_account, step) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![grant_id.to_string(), seq, at, kind, by.to_string(), step],
        )
        .map_err(backend)?;
        Ok(())
    }
}

#[allow(clippy::type_complexity)]
fn read8<A, B, C, D, E, F, G, H>(
    r: &rusqlite::Row<'_>,
) -> rusqlite::Result<(A, B, C, D, E, F, G, H)>
where
    A: rusqlite::types::FromSql,
    B: rusqlite::types::FromSql,
    C: rusqlite::types::FromSql,
    D: rusqlite::types::FromSql,
    E: rusqlite::types::FromSql,
    F: rusqlite::types::FromSql,
    G: rusqlite::types::FromSql,
    H: rusqlite::types::FromSql,
{
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
    ))
}

#[allow(clippy::type_complexity)]
fn read7<A, B, C, D, E, F, G>(r: &rusqlite::Row<'_>) -> rusqlite::Result<(A, B, C, D, E, F, G)>
where
    A: rusqlite::types::FromSql,
    B: rusqlite::types::FromSql,
    C: rusqlite::types::FromSql,
    D: rusqlite::types::FromSql,
    E: rusqlite::types::FromSql,
    F: rusqlite::types::FromSql,
    G: rusqlite::types::FromSql,
{
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
    ))
}

#[allow(clippy::type_complexity)]
fn read6<A, B, C, D, E, F>(r: &rusqlite::Row<'_>) -> rusqlite::Result<(A, B, C, D, E, F)>
where
    A: rusqlite::types::FromSql,
    B: rusqlite::types::FromSql,
    C: rusqlite::types::FromSql,
    D: rusqlite::types::FromSql,
    E: rusqlite::types::FromSql,
    F: rusqlite::types::FromSql,
{
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

impl SharingStore for SqliteStorage {
    fn shared_vault(&self, vault_id: Id) -> Result<Option<SharedRow>> {
        let row: Option<SharedColumns> = self
            .lock()
            .query_row(
                "SELECT owner_account, kind, key_ref, seq, head FROM shared_vaults WHERE vault_id = ?1",
                params![vault_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(backend)?;
        row.map(|r| shared_from(vault_id, r)).transpose()
    }

    fn membership_events(&self, vault_id: Id, since: u64) -> Result<Vec<Vec<u8>>> {
        self.query_rows(
            "SELECT body FROM membership_events WHERE vault_id = ?1 AND seq > ?2 ORDER BY seq",
            &[&vault_id.to_string(), &i(since)],
            |r| r.get::<_, Vec<u8>>(0),
            Ok,
        )
    }

    fn accept_membership(&self, write: MembershipWrite<'_>) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let vault = write.vault_id.to_string();
        let current: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM membership_events WHERE vault_id = ?1",
                params![vault],
                |r| r.get(0),
            )
            .map_err(backend)?;
        if u(current) + 1 != write.event.1 {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO membership_events (vault_id, seq, body) VALUES (?1, ?2, ?3)",
            params![vault, i(write.event.1), write.event.0],
        )
        .map_err(backend)?;
        let s = write.shared;
        tx.execute(
            "INSERT OR REPLACE INTO shared_vaults (vault_id, owner_account, kind, key_ref, seq, head) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                vault,
                s.owner_account.to_string(),
                s.kind.as_str(),
                s.key_ref.to_string(),
                i(s.seq),
                &s.head.0[..]
            ],
        )
        .map_err(backend)?;
        tx.execute(
            "DELETE FROM vault_members WHERE vault_id = ?1",
            params![vault],
        )
        .map_err(backend)?;
        for m in write.members {
            tx.execute(
                "INSERT INTO vault_members (vault_id, account_id, right_name, party, active) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    vault,
                    m.party.account_id.to_string(),
                    m.right.as_str(),
                    cbor(&m.party)?,
                    i64::from(m.active)
                ],
            )
            .map_err(backend)?;
        }
        for (account, key_ref, body) in write.envelopes {
            tx.execute(
                "INSERT OR REPLACE INTO share_envelopes (vault_id, key_ref, account_id, body) VALUES (?1, ?2, ?3, ?4)",
                params![vault, key_ref.to_string(), account.to_string(), body],
            )
            .map_err(backend)?;
        }
        // Envelopes of older generations are dead once a rotation lands.
        tx.execute(
            "DELETE FROM share_envelopes WHERE vault_id = ?1 AND key_ref != ?2",
            params![vault, s.key_ref.to_string()],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(true)
    }

    fn members(&self, vault_id: Id) -> Result<Vec<MemberRow>> {
        self.query_rows(
            "SELECT right_name, party, active FROM vault_members WHERE vault_id = ?1 ORDER BY account_id",
            &[&vault_id.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, i64>(2)?)),
            |(right, party, active)| {
                Ok(MemberRow {
                    party: uncbor(&party)?,
                    right: right_from(&right)?,
                    active: active != 0,
                })
            },
        )
    }

    fn shared_with(&self, account: Id) -> Result<Vec<(Id, Right)>> {
        self.query_rows(
            "SELECT vault_id, right_name FROM vault_members WHERE account_id = ?1 AND active = 1 ORDER BY vault_id",
            &[&account.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            |(v, right)| Ok((id_of(&v)?, right_from(&right)?)),
        )
    }

    fn owned_shared(&self, account: Id) -> Result<Vec<SharedRow>> {
        self.query_rows(
            "SELECT vault_id, owner_account, kind, key_ref, seq, head FROM shared_vaults WHERE owner_account = ?1 ORDER BY vault_id",
            &[&account.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
                ))
            },
            |(v, cols)| shared_from(id_of(&v)?, cols),
        )
    }

    fn share_envelope(&self, vault_id: Id, key_ref: Id, account: Id) -> Result<Option<Vec<u8>>> {
        self.lock()
            .query_row(
                "SELECT body FROM share_envelopes WHERE vault_id = ?1 AND key_ref = ?2 AND account_id = ?3",
                params![vault_id.to_string(), key_ref.to_string(), account.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)
    }

    fn put_item_share(&self, row: &ItemShareRow) -> Result<()> {
        self.lock()
            .execute(
                &format!(
                    "INSERT OR REPLACE INTO item_shares ({ITEM_SHARE_COLUMNS}, recipient_account) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
                ),
                params![
                    row.share_id.to_string(),
                    row.vault_id.to_string(),
                    row.item_id.to_string(),
                    row.owner_account.to_string(),
                    cbor(&row.recipient)?,
                    row.right.as_str(),
                    row.envelope,
                    row.created_at,
                    row.recipient.account_id.to_string()
                ],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn item_share(&self, share_id: Id) -> Result<Option<ItemShareRow>> {
        Ok(self
            .query_rows(
                &format!("SELECT {ITEM_SHARE_COLUMNS} FROM item_shares WHERE share_id = ?1"),
                &[&share_id.to_string()],
                read8,
                item_share_from,
            )?
            .pop())
    }

    fn item_shares_to(&self, account: Id) -> Result<Vec<ItemShareRow>> {
        self.query_rows(
            &format!(
                "SELECT {ITEM_SHARE_COLUMNS} FROM item_shares WHERE recipient_account = ?1 ORDER BY share_id"
            ),
            &[&account.to_string()],
            read8,
            item_share_from,
        )
    }

    fn item_shares_from(&self, account: Id) -> Result<Vec<ItemShareRow>> {
        self.query_rows(
            &format!(
                "SELECT {ITEM_SHARE_COLUMNS} FROM item_shares WHERE owner_account = ?1 ORDER BY share_id"
            ),
            &[&account.to_string()],
            read8,
            item_share_from,
        )
    }

    fn delete_item_share(&self, share_id: Id) -> Result<bool> {
        Ok(self
            .lock()
            .execute(
                "DELETE FROM item_shares WHERE share_id = ?1",
                params![share_id.to_string()],
            )
            .map_err(backend)?
            > 0)
    }

    fn put_send(&self, row: &SendRow) -> Result<()> {
        let password = row.password.as_ref().map(cbor).transpose()?;
        self.lock()
            .execute(
                &format!(
                    "INSERT INTO sends ({SEND_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
                ),
                params![
                    row.send_id.to_string(),
                    row.owner_account.to_string(),
                    row.created_at,
                    row.expires_at,
                    row.max_views.map(i64::from),
                    i64::from(row.views),
                    password,
                    row.body
                ],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn send(&self, send_id: Id) -> Result<Option<SendRow>> {
        Ok(self
            .query_rows(
                &format!("SELECT {SEND_COLUMNS} FROM sends WHERE send_id = ?1"),
                &[&send_id.to_string()],
                read8,
                send_from,
            )?
            .pop())
    }

    fn count_view(&self, send_id: Id, now: i64) -> Result<bool> {
        Ok(self
            .lock()
            .execute(
                "UPDATE sends SET views = views + 1 WHERE send_id = ?1 AND expires_at > ?2 AND (max_views IS NULL OR views < max_views)",
                params![send_id.to_string(), now],
            )
            .map_err(backend)?
            > 0)
    }

    fn sends_of(&self, account: Id) -> Result<Vec<SendRow>> {
        self.query_rows(
            &format!(
                "SELECT {SEND_COLUMNS} FROM sends WHERE owner_account = ?1 ORDER BY created_at DESC, send_id"
            ),
            &[&account.to_string()],
            read8,
            send_from,
        )
    }

    fn delete_send(&self, account: Id, send_id: Id) -> Result<bool> {
        Ok(self
            .lock()
            .execute(
                "DELETE FROM sends WHERE send_id = ?1 AND owner_account = ?2",
                params![send_id.to_string(), account.to_string()],
            )
            .map_err(backend)?
            > 0)
    }

    fn purge_sends(&self, now: i64) -> Result<usize> {
        self.lock()
            .execute(
                "DELETE FROM sends WHERE expires_at <= ?1 OR (max_views IS NOT NULL AND views >= max_views)",
                params![now],
            )
            .map_err(backend)
    }

    fn put_request(&self, row: &RequestRow) -> Result<()> {
        self.lock()
            .execute(
                &format!(
                    "INSERT INTO secret_requests ({REQUEST_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
                ),
                params![
                    row.request_id.to_string(),
                    row.owner_account.to_string(),
                    row.created_at,
                    row.expires_at,
                    row.key,
                    row.answer,
                    row.answered_at
                ],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn request(&self, request_id: Id) -> Result<Option<RequestRow>> {
        Ok(self
            .query_rows(
                &format!("SELECT {REQUEST_COLUMNS} FROM secret_requests WHERE request_id = ?1"),
                &[&request_id.to_string()],
                read7,
                request_from,
            )?
            .pop())
    }

    fn answer_request(&self, request_id: Id, answer: &[u8], now: i64) -> Result<bool> {
        Ok(self
            .lock()
            .execute(
                "UPDATE secret_requests SET answer = ?2, answered_at = ?3 WHERE request_id = ?1 AND answer IS NULL AND expires_at > ?3",
                params![request_id.to_string(), answer, now],
            )
            .map_err(backend)?
            > 0)
    }

    fn requests_of(&self, account: Id) -> Result<Vec<RequestRow>> {
        self.query_rows(
            &format!(
                "SELECT {REQUEST_COLUMNS} FROM secret_requests WHERE owner_account = ?1 ORDER BY created_at DESC, request_id"
            ),
            &[&account.to_string()],
            read7,
            request_from,
        )
    }

    fn delete_request(&self, account: Id, request_id: Id) -> Result<bool> {
        Ok(self
            .lock()
            .execute(
                "DELETE FROM secret_requests WHERE request_id = ?1 AND owner_account = ?2",
                params![request_id.to_string(), account.to_string()],
            )
            .map_err(backend)?
            > 0)
    }

    fn put_grant(&self, row: &GrantRow, envelopes: &[(Id, Vec<u8>)], at: i64) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        tx.execute(
            &format!(
                "INSERT INTO emergency_grants ({GRANT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            ),
            params![
                row.grant_id.to_string(),
                row.owner_account.to_string(),
                row.contact_account.to_string(),
                row.body,
                row.state,
                row.requested_at
            ],
        )
        .map_err(backend)?;
        for (vault, body) in envelopes {
            tx.execute(
                "INSERT INTO emergency_envelopes (grant_id, vault_id, body) VALUES (?1, ?2, ?3)",
                params![row.grant_id.to_string(), vault.to_string(), body],
            )
            .map_err(backend)?;
        }
        Self::add_emergency_event(&tx, row.grant_id, "created", row.owner_account, None, at)?;
        tx.commit().map_err(backend)
    }

    fn grant(&self, grant_id: Id) -> Result<Option<GrantRow>> {
        Ok(self
            .query_rows(
                &format!("SELECT {GRANT_COLUMNS} FROM emergency_grants WHERE grant_id = ?1"),
                &[&grant_id.to_string()],
                read6,
                grant_from,
            )?
            .pop())
    }

    fn grants_by_owner(&self, account: Id) -> Result<Vec<GrantRow>> {
        self.query_rows(
            &format!(
                "SELECT {GRANT_COLUMNS} FROM emergency_grants WHERE owner_account = ?1 ORDER BY grant_id"
            ),
            &[&account.to_string()],
            read6,
            grant_from,
        )
    }

    fn grants_for_contact(&self, account: Id) -> Result<Vec<GrantRow>> {
        self.query_rows(
            &format!(
                "SELECT {GRANT_COLUMNS} FROM emergency_grants WHERE contact_account = ?1 ORDER BY grant_id"
            ),
            &[&account.to_string()],
            read6,
            grant_from,
        )
    }

    fn step_grant(
        &self,
        grant_id: Id,
        state: &str,
        requested_at: Option<i64>,
        event: (&str, Id, Option<&[u8]>),
        at: i64,
    ) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        tx.execute(
            "UPDATE emergency_grants SET state = ?2, requested_at = ?3 WHERE grant_id = ?1",
            params![grant_id.to_string(), state, requested_at],
        )
        .map_err(backend)?;
        if state == "revoked" || state == "declined" {
            tx.execute(
                "DELETE FROM emergency_envelopes WHERE grant_id = ?1",
                params![grant_id.to_string()],
            )
            .map_err(backend)?;
        }
        Self::add_emergency_event(&tx, grant_id, event.0, event.1, event.2, at)?;
        tx.commit().map_err(backend)
    }

    fn replace_grant_envelopes(
        &self,
        grant_id: Id,
        envelopes: &[(Id, Vec<u8>)],
        by: Id,
        at: i64,
    ) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        for (vault, body) in envelopes {
            tx.execute(
                "INSERT OR REPLACE INTO emergency_envelopes (grant_id, vault_id, body) VALUES (?1, ?2, ?3)",
                params![grant_id.to_string(), vault.to_string(), body],
            )
            .map_err(backend)?;
        }
        Self::add_emergency_event(&tx, grant_id, "envelopes", by, None, at)?;
        tx.commit().map_err(backend)
    }

    fn emergency_events(&self, grant_id: Id) -> Result<Vec<EmergencyEventRow>> {
        self.query_rows(
            "SELECT seq, at, kind, by_account, step FROM emergency_events WHERE grant_id = ?1 ORDER BY seq",
            &[&grant_id.to_string()],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
            |(seq, at, kind, by, step)| {
                Ok(EmergencyEventRow {
                    seq: u(seq),
                    at,
                    kind,
                    by_account: id_of(&by)?,
                    step,
                })
            },
        )
    }

    fn emergency_envelopes(&self, grant_id: Id) -> Result<Vec<(Id, Vec<u8>)>> {
        self.query_rows(
            "SELECT vault_id, body FROM emergency_envelopes WHERE grant_id = ?1 ORDER BY vault_id",
            &[&grant_id.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
            |(v, b)| Ok((id_of(&v)?, b)),
        )
    }
}
