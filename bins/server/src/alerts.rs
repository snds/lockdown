//! Account alerts (docs/plan/13-security-center.md): what the server tells
//! an account's devices about the account itself.
//!
//! - **New device.** When a device the account has never seen logs in and
//!   the account already has another device, the server records it. Every
//!   other device sees it on its next look; the new device never sees its
//!   own sign-in, and a device never sees one from before it joined.
//! - **Email breach.** The breach monitor ([`crate::breach_monitor`])
//!   records one per breach it learns of for the account's address, once.
//!
//! An alert dismissed on one device is dismissed for the account, so the
//! person deals with it once. The server learns nothing new by keeping
//! these: it already knows when a device logs in and which login name the
//! account has.
//!
//! The tables live in the same SQLite file as everything else
//! ([`create_tables`] runs with the main schema). They are additive (`IF
//! NOT EXISTS`), so an older build opens a file that has them.

use lockdown_sync::wire::{AccountAlert, AlertKind, BreachNotice};
use lockdown_vault::Id;
use rusqlite::{Connection, OptionalExtension, params};

use crate::storage::{Account, Result, SqliteStorage, StorageError};

/// What the server keeps for alerts.
pub trait AlertStore: Send + Sync {
    /// Records an alert and returns its `seq` within the account.
    fn record_alert(
        &self,
        account_id: Id,
        kind: AlertKind,
        at: i64,
        device_id: Option<Id>,
        breach: Option<&BreachNotice>,
    ) -> Result<u64>;
    /// The alerts after `since` that concern `device_id`: never its own new
    /// sign-in, never a new-device alert from before it joined.
    fn alerts_for(&self, account_id: Id, device_id: Id, since: u64) -> Result<Vec<AccountAlert>>;
    /// The newest `seq` the account has (0 for none).
    fn alert_seq(&self, account_id: Id) -> Result<u64>;
    /// Marks an alert dismissed for the account; `false` when there is none.
    fn dismiss_alert(&self, account_id: Id, seq: u64) -> Result<bool>;
    /// How many devices the account has logged in with.
    fn device_count(&self, account_id: Id) -> Result<u64>;
    /// Every account, for the breach monitor.
    fn all_accounts(&self) -> Result<Vec<Account>>;
    /// Whether the monitor already alerted this account about `breach`.
    fn breach_seen(&self, account_id: Id, breach: &str) -> Result<bool>;
    /// Remembers that it did.
    fn mark_breach_seen(&self, account_id: Id, breach: &str) -> Result<()>;
    /// The addresses beside the login the account asked to have watched
    /// (its email aliases, docs/plan/23-bridges.md §9), sorted.
    fn monitored(&self, account_id: Id) -> Result<Vec<String>> {
        let _ = account_id;
        Ok(Vec::new())
    }
    /// Replaces that list.
    fn set_monitored(&self, account_id: Id, addresses: &[String]) -> Result<()> {
        let _ = (account_id, addresses);
        Err(StorageError::Backend(
            "this server keeps no monitored addresses".into(),
        ))
    }
}

fn backend(e: rusqlite::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

fn kind_name(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::NewDevice => "new_device",
        AlertKind::EmailBreach => "email_breach",
    }
}

fn kind_from(name: &str) -> Result<AlertKind> {
    match name {
        "new_device" => Ok(AlertKind::NewDevice),
        "email_breach" => Ok(AlertKind::EmailBreach),
        other => Err(StorageError::Backend(format!(
            "an alert of an unknown kind '{other}' is stored"
        ))),
    }
}

/// The alert tables, beside the main schema.
pub(crate) fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS account_alerts (
             account_id TEXT NOT NULL REFERENCES accounts(account_id),
             seq        INTEGER NOT NULL,
             kind       TEXT NOT NULL,
             at         INTEGER NOT NULL,
             device_id  TEXT,
             breach     BLOB,
             dismissed  INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (account_id, seq)
         );
         CREATE TABLE IF NOT EXISTS breach_seen (
             account_id TEXT NOT NULL REFERENCES accounts(account_id),
             name       TEXT NOT NULL,
             PRIMARY KEY (account_id, name)
         );
         CREATE TABLE IF NOT EXISTS monitored_addresses (
             account_id TEXT NOT NULL REFERENCES accounts(account_id),
             address    TEXT NOT NULL,
             PRIMARY KEY (account_id, address)
         );",
    )
}

type AlertRow = (i64, String, i64, Option<String>, Option<Vec<u8>>, i64);

fn alert_from(row: AlertRow) -> Result<AccountAlert> {
    let (seq, kind, at, device, breach, dismissed) = row;
    Ok(AccountAlert {
        seq: u64::try_from(seq).unwrap_or(0),
        kind: kind_from(&kind)?,
        at,
        device_id: device
            .map(|d| {
                Id::parse(&d).map_err(|_| StorageError::Backend("a bad device id is stored".into()))
            })
            .transpose()?,
        breach: breach
            .map(|b| {
                lockdown_crypto::cbor::from_slice::<BreachNotice>(&b)
                    .map_err(|_| StorageError::Backend("a stored breach didn't decode".into()))
            })
            .transpose()?,
        dismissed: dismissed != 0,
    })
}

impl AlertStore for SqliteStorage {
    fn record_alert(
        &self,
        account_id: Id,
        kind: AlertKind,
        at: i64,
        device_id: Option<Id>,
        breach: Option<&BreachNotice>,
    ) -> Result<u64> {
        let body = breach
            .map(lockdown_crypto::cbor::to_canonical)
            .transpose()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        let seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM account_alerts WHERE account_id = ?1",
                params![account_id.to_string()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        tx.execute(
            "INSERT INTO account_alerts (account_id, seq, kind, at, device_id, breach) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                account_id.to_string(),
                seq,
                kind_name(kind),
                at,
                device_id.map(|d| d.to_string()),
                body
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(u64::try_from(seq).unwrap_or(0))
    }

    fn alerts_for(&self, account_id: Id, device_id: Id, since: u64) -> Result<Vec<AccountAlert>> {
        let conn = self.lock();
        // When each device joined, by the same clock: a device sees a new
        // device's sign-in only when it was there first.
        let mut joined = std::collections::HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT device_id, first_seen FROM devices WHERE account_id = ?1")
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![account_id.to_string()], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(backend)?;
            for row in rows {
                let (id, at) = row.map_err(backend)?;
                joined.insert(id, at);
            }
        }
        let mine = joined
            .get(&device_id.to_string())
            .copied()
            .unwrap_or(i64::MAX);
        let mut stmt = conn
            .prepare(
                "SELECT seq, kind, at, device_id, breach, dismissed FROM account_alerts
                 WHERE account_id = ?1 AND seq > ?2 ORDER BY seq",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(
                params![
                    account_id.to_string(),
                    i64::try_from(since).unwrap_or(i64::MAX)
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<Vec<u8>>>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(backend)?
            .collect::<std::result::Result<Vec<AlertRow>, _>>()
            .map_err(backend)?;
        let mut out = Vec::new();
        for row in rows {
            let alert = alert_from(row)?;
            if alert.kind == AlertKind::NewDevice {
                let theirs = alert
                    .device_id
                    .and_then(|d| joined.get(&d.to_string()).copied())
                    .unwrap_or(i64::MIN);
                if alert.device_id == Some(device_id) || theirs < mine {
                    continue;
                }
            }
            out.push(alert);
        }
        Ok(out)
    }

    fn alert_seq(&self, account_id: Id) -> Result<u64> {
        let seq: i64 = self
            .lock()
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM account_alerts WHERE account_id = ?1",
                params![account_id.to_string()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        Ok(u64::try_from(seq).unwrap_or(0))
    }

    fn dismiss_alert(&self, account_id: Id, seq: u64) -> Result<bool> {
        let n = self
            .lock()
            .execute(
                "UPDATE account_alerts SET dismissed = 1 WHERE account_id = ?1 AND seq = ?2",
                params![account_id.to_string(), i64::try_from(seq).unwrap_or(-1)],
            )
            .map_err(backend)?;
        Ok(n > 0)
    }

    fn device_count(&self, account_id: Id) -> Result<u64> {
        let n: i64 = self
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM devices WHERE account_id = ?1",
                params![account_id.to_string()],
                |r| r.get(0),
            )
            .map_err(backend)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    fn all_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT account_id FROM accounts ORDER BY created_at")
            .map_err(backend)?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .map_err(backend)?
            .collect::<std::result::Result<_, _>>()
            .map_err(backend)?;
        drop(stmt);
        drop(conn);
        let mut out = Vec::new();
        for id in ids {
            let id = Id::parse(&id)
                .map_err(|_| StorageError::Backend("a bad account id is stored".into()))?;
            if let Some(a) = crate::storage::Storage::account(self, id)? {
                out.push(a);
            }
        }
        Ok(out)
    }

    fn breach_seen(&self, account_id: Id, breach: &str) -> Result<bool> {
        let found: Option<i64> = self
            .lock()
            .query_row(
                "SELECT 1 FROM breach_seen WHERE account_id = ?1 AND name = ?2",
                params![account_id.to_string(), breach],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)?;
        Ok(found.is_some())
    }

    fn mark_breach_seen(&self, account_id: Id, breach: &str) -> Result<()> {
        self.lock()
            .execute(
                "INSERT OR IGNORE INTO breach_seen (account_id, name) VALUES (?1, ?2)",
                params![account_id.to_string(), breach],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn monitored(&self, account_id: Id) -> Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn
            .prepare(
                "SELECT address FROM monitored_addresses WHERE account_id = ?1 ORDER BY address",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map(params![account_id.to_string()], |r| r.get(0))
            .map_err(backend)?
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(backend)?;
        Ok(rows)
    }

    fn set_monitored(&self, account_id: Id, addresses: &[String]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(backend)?;
        tx.execute(
            "DELETE FROM monitored_addresses WHERE account_id = ?1",
            params![account_id.to_string()],
        )
        .map_err(backend)?;
        for a in addresses {
            tx.execute(
                "INSERT OR IGNORE INTO monitored_addresses (account_id, address) VALUES (?1, ?2)",
                params![account_id.to_string(), a],
            )
            .map_err(backend)?;
        }
        tx.commit().map_err(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[test]
    fn a_new_device_alert_reaches_the_other_devices_only_and_dismisses_for_all() {
        let s = SqliteStorage::open_in_memory().unwrap();
        let account = s
            .create_account("sean@example.com", Id::new(), b"pf")
            .unwrap();
        let (first, second) = (Id::new(), Id::new());
        s.register_device(account.account_id, first, &[1; 32])
            .unwrap();
        s.register_device(account.account_id, second, &[2; 32])
            .unwrap();
        assert_eq!(s.device_count(account.account_id).unwrap(), 2);
        let now = lockdown_vault::Timestamp::now().0;
        let seq = s
            .record_alert(
                account.account_id,
                AlertKind::NewDevice,
                now,
                Some(second),
                None,
            )
            .unwrap();
        assert_eq!(seq, 1);
        let for_first = s.alerts_for(account.account_id, first, 0).unwrap();
        assert_eq!(for_first.len(), 1);
        assert_eq!(for_first[0].device_id, Some(second));
        assert!(
            s.alerts_for(account.account_id, second, 0)
                .unwrap()
                .is_empty()
        );
        assert!(
            s.alerts_for(account.account_id, first, 1)
                .unwrap()
                .is_empty()
        );
        // A device that joins later doesn't see a sign-in from before it.
        s.lock()
            .execute("UPDATE devices SET first_seen = first_seen - 100", [])
            .unwrap();
        let third = Id::new();
        s.register_device(account.account_id, third, &[3; 32])
            .unwrap();
        assert!(
            s.alerts_for(account.account_id, third, 0)
                .unwrap()
                .is_empty()
        );
        // And sees one after it.
        let fourth = Id::new();
        s.register_device(account.account_id, fourth, &[4; 32])
            .unwrap();
        s.record_alert(
            account.account_id,
            AlertKind::NewDevice,
            now,
            Some(fourth),
            None,
        )
        .unwrap();
        assert_eq!(s.alerts_for(account.account_id, third, 0).unwrap().len(), 1);
        assert!(s.dismiss_alert(account.account_id, 1).unwrap());
        assert!(s.alerts_for(account.account_id, first, 0).unwrap()[0].dismissed);
        assert!(!s.dismiss_alert(account.account_id, 99).unwrap());
        // Another account sees nothing of it.
        let other = s
            .create_account("other@example.com", Id::new(), b"pf")
            .unwrap();
        assert!(s.alerts_for(other.account_id, first, 0).unwrap().is_empty());
        assert_eq!(s.alert_seq(account.account_id).unwrap(), 2);
    }

    #[test]
    fn breach_alerts_carry_their_notice_and_are_remembered() {
        let s = SqliteStorage::open_in_memory().unwrap();
        let account = s
            .create_account("sean@example.com", Id::new(), b"pf")
            .unwrap();
        let device = Id::new();
        s.register_device(account.account_id, device, &[1; 32])
            .unwrap();
        let notice = BreachNotice {
            address: "sean@example.com".into(),
            name: "ExampleShop".into(),
            domain: Some("shop.example".into()),
            breach_date: Some("2026-09-01".into()),
            data: vec!["Email addresses".into(), "Passwords".into()],
        };
        assert!(!s.breach_seen(account.account_id, "ExampleShop").unwrap());
        s.record_alert(
            account.account_id,
            AlertKind::EmailBreach,
            0,
            None,
            Some(&notice),
        )
        .unwrap();
        s.mark_breach_seen(account.account_id, "ExampleShop")
            .unwrap();
        s.mark_breach_seen(account.account_id, "ExampleShop")
            .unwrap();
        assert!(s.breach_seen(account.account_id, "ExampleShop").unwrap());
        // A breach alert is the account's, whenever a device joined.
        let alerts = s.alerts_for(account.account_id, device, 0).unwrap();
        assert_eq!(alerts[0].breach.as_ref(), Some(&notice));
        assert_eq!(s.all_accounts().unwrap().len(), 1);
    }
}
