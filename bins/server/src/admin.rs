//! The operator's admin page at `/admin` (05-self-host §10;
//! docs/plan/25-self-host-2.md §4): accounts, storage and quota use,
//! devices, invites, backups and restore drills, behind an operator key
//! that is not an account.

use crate::db::{Conn, DbError};

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
