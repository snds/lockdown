//! Crown jewels in emergency access (`high-risk-class`;
//! docs/plan/22-security-depth.md §8): the Item Keys of the crown jewels an
//! owner opted in, sealed to the emergency contact and held back with the
//! grant exactly like its vault keys, handed over only once the grant was
//! released. A crown jewel not opted in has no envelope here, so it never
//! leaves its owner, whatever happens to the grant.
//!
//! The owner replaces the whole set whenever an opt-in changes (or a vault
//! moves to new keys). Every envelope must be the owner's device's, for this
//! grant's contact, for an item of a vault the grant covers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use lockdown_sync::account_security::EmergencyItems;
use lockdown_sync::wire::ErrorCode;
use lockdown_vault::Id;
use lockdown_vault::sharing::{EmergencyGrant, ShareEnvelope, SharePurpose};
use serde_bytes::ByteBuf;

use crate::AppState;
use crate::api::{ApiError, ApiResult, Cbor, Session, device_key};
use crate::db::{Conn, DbError, params};
use crate::storage::{Result, SqlStorage};

/// The most crown jewels one grant carries.
pub const MAX_ITEMS: usize = 500;

pub(crate) fn create_tables(conn: &Conn<'_>) -> std::result::Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS emergency_items (
             grant_id TEXT NOT NULL,
             item_id  TEXT NOT NULL,
             body     BLOB NOT NULL,
             PRIMARY KEY (grant_id, item_id)
         );",
    )
}

impl SqlStorage {
    pub(crate) fn put_emergency_items(&self, grant: Id, items: &[(Id, Vec<u8>)]) -> Result<()> {
        let tx = self.begin()?;
        tx.execute(
            "DELETE FROM emergency_items WHERE grant_id = ?1",
            params![grant.to_string()],
        )?;
        for (item, body) in items {
            tx.execute(
                "INSERT INTO emergency_items (grant_id, item_id, body) VALUES (?1, ?2, ?3)",
                params![grant.to_string(), item.to_string(), body],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub(crate) fn emergency_items(&self, grant: Id) -> Result<Vec<Vec<u8>>> {
        Ok(self.conn().query_map(
            "SELECT body FROM emergency_items WHERE grant_id = ?1 ORDER BY item_id",
            params![grant.to_string()],
            |r| r.get(0),
        )?)
    }
}

fn store(state: &AppState) -> ApiResult<&dyn crate::second_factor::AccountSecurityStore> {
    state
        .storage
        .account_security()
        .ok_or_else(|| ApiError::internal("this server's store keeps no emergency items"))
}

fn grant_row(state: &AppState, grant_id: Id) -> ApiResult<crate::sharing::GrantRow> {
    state
        .storage
        .sharing()
        .ok_or_else(|| ApiError::internal("this server keeps no sharing"))?
        .grant(grant_id)?
        .ok_or_else(|| ApiError::not_found("that emergency grant"))
}

/// `POST /v1/emergency/{grant}/items`: the owner's whole set, replacing the last.
pub async fn put_items(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(grant_id): Path<Id>,
    Cbor(body): Cbor<EmergencyItems>,
) -> ApiResult<StatusCode> {
    let row = grant_row(&state, grant_id)?;
    if row.owner_account != session.account_id {
        return Err(ApiError::forbidden("that grant is someone else's"));
    }
    if matches!(row.state.as_str(), "declined" | "revoked") {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::Stale,
            "this grant has ended",
        ));
    }
    if body.envelopes.len() > MAX_ITEMS {
        return Err(ApiError::bad_request("too many crown jewels on one grant"));
    }
    let grant = EmergencyGrant::from_bytes(&row.body)
        .map_err(|_| ApiError::internal("a stored grant didn't decode"))?;
    let key = device_key(&state, &session, session.device_id)?;
    let mut items = Vec::new();
    for bytes in &body.envelopes {
        let env = ShareEnvelope::from_bytes(bytes)
            .map_err(|_| ApiError::bad_request("that isn't a share envelope"))?;
        let p = &env.place;
        let Some(item) = p.item_id else {
            return Err(ApiError::bad_request(
                "a crown jewel's envelope names its item",
            ));
        };
        if p.purpose != SharePurpose::Emergency
            || p.grant_id != Some(grant_id)
            || p.to != grant.contact
            || p.key_id != item
            || !grant.vaults.contains(&p.vault_id)
            || p.from_account != session.account_id
            || env.by_device != session.device_id
        {
            return Err(ApiError::bad_request(
                "a crown jewel's envelope is for another grant, person or vault",
            ));
        }
        env.verify(&key)
            .map_err(|_| ApiError::forbidden("a share envelope's signature didn't check out"))?;
        items.push((item, bytes.to_vec()));
    }
    store(&state)?.put_emergency_items(grant_id, &items)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/emergency/{grant}/items`: the contact's, once the grant was released.
pub async fn get_items(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(grant_id): Path<Id>,
) -> ApiResult<Cbor<EmergencyItems>> {
    let row = grant_row(&state, grant_id)?;
    if row.contact_account != session.account_id {
        return Err(ApiError::forbidden("that grant names someone else"));
    }
    if row.state != "released" {
        return Err(ApiError::forbidden(
            "the keys come only after the wait ran out and the release was taken",
        ));
    }
    Ok(Cbor(EmergencyItems {
        envelopes: store(&state)?
            .emergency_items(grant_id)?
            .into_iter()
            .map(ByteBuf::from)
            .collect(),
    }))
}
