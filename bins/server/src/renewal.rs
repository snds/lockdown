//! Session renewal (D52 as changed by D375; docs/plan/33-session-renewal.md):
//! a signed-in device renews its own session with its device key, so the
//! password is asked only on a new device, after a session was ended, after
//! the device was revoked, or after the idle limit.
//!
//! `POST /v1/account/session/challenge` and `POST /v1/account/session/renew`
//! both take the old token as the bearer. Its signature must check out (it
//! is ours) but its age doesn't matter: renewal is for tokens that ran out.
//! What decides is the session's row and the device:
//!
//! - the row exists, belongs to the token's account and device, and was
//!   not ended (D221, D296); a row that a renewal ended may be renewed
//!   again only within [`RETRY_SECS`], for a device whose answer was lost;
//! - it was used within the idle limit (`--session-idle-limit`, 30 days by
//!   default; 0 turns renewal off), measured from its last use (D220);
//! - the device was not revoked, and the key it signs with is the one the
//!   server holds for it and the one the identity's records list;
//! - the signature is over [`session_renew_message`]: the label
//!   `lockdown/session-renew/v1`, a challenge this server gave for this
//!   very token (two minutes, used once, even when the answer fails), and
//!   the token's hash.
//!
//! Renewal rotates: a new token and session id, the old row ended and
//! pointing at the new one (`renewed_to`), in one transaction. The new row
//! keeps the old one's "signed in" time and takes this request's address
//! and client, so the Signed in list reads as before. Nothing from the
//! network is trusted: a stolen token alone gets a challenge it can't
//! sign, a signature alone names a token it doesn't have, and a used or
//! foreign challenge is refused.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::auth::session_renew_message;
use lockdown_sync::wire::{
    BEARER, ErrorCode, LiveKind, LoginFinishReply, RENEW_V, RenewChallenge, RenewFinish,
};
use lockdown_vault::identity_set::{IdentityManifest, RecordSet};
use lockdown_vault::{Hash32, Id};

use crate::AppState;
use crate::api::{ApiError, ApiResult, Cbor, Peer, brake};
use crate::sessions::{SessionRow, client_label};
use crate::token::TokenClaims;

/// How long a challenge stands, in seconds.
pub const CHALLENGE_SECS: i64 = 120;

/// The most challenges kept at once; past it the oldest go.
pub const CHALLENGES_MAX: usize = 10_000;

/// How long after a renewal the same old session may be renewed again,
/// by the same device key over a fresh challenge, when the answer to the
/// first never arrived (seconds). The unused successor is ended.
pub const RETRY_SECS: i64 = 120;

/// The default idle limit: 30 days.
pub const DEFAULT_IDLE_LIMIT: std::time::Duration =
    std::time::Duration::from_secs(30 * 24 * 60 * 60);

/// A challenge between its two round trips.
#[derive(Clone, Debug)]
struct Pending {
    session_id: [u8; 16],
    token_hash: Hash32,
    challenge: [u8; 32],
    issued_at: i64,
}

/// The challenges handed out and not yet answered.
#[derive(Default)]
pub struct Challenges {
    pending: Mutex<HashMap<Id, Pending>>,
}

impl std::fmt::Debug for Challenges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Challenges(…)")
    }
}

impl Challenges {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Id, Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Hands out a challenge for `session_id`'s token; any earlier one for
    /// that session is dropped.
    fn issue(
        &self,
        session_id: [u8; 16],
        token_hash: Hash32,
        now: i64,
    ) -> ApiResult<RenewChallenge> {
        let challenge = lockdown_crypto::rand::random_32()
            .map_err(|_| ApiError::internal("no randomness for a challenge"))?;
        let challenge_id = Id::new();
        let mut pending = self.lock();
        pending.retain(|_, p| {
            p.session_id != session_id && now.saturating_sub(p.issued_at) < CHALLENGE_SECS
        });
        if pending.len() >= CHALLENGES_MAX {
            // Under a flood the oldest go (ties by id, so the choice
            // doesn't hang on the map's order); a real device asks again.
            let mut by_age: Vec<(i64, Id)> =
                pending.iter().map(|(id, p)| (p.issued_at, *id)).collect();
            by_age.sort();
            for (_, id) in by_age.iter().take(pending.len() + 1 - CHALLENGES_MAX) {
                pending.remove(id);
            }
        }
        pending.insert(
            challenge_id,
            Pending {
                session_id,
                token_hash,
                challenge,
                issued_at: now,
            },
        );
        Ok(RenewChallenge {
            v: RENEW_V,
            challenge_id,
            challenge,
            expires_at: now.saturating_add(CHALLENGE_SECS),
        })
    }

    /// Takes a challenge out, used or not: it never answers twice.
    fn take(&self, challenge_id: Id, now: i64) -> Option<Pending> {
        let p = self.lock().remove(&challenge_id)?;
        (now.saturating_sub(p.issued_at) < CHALLENGE_SECS).then_some(p)
    }

    /// How many are waiting (tests).
    pub fn waiting(&self) -> usize {
        self.lock().len()
    }
}

fn refuse(status: StatusCode, code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(status, code, message)
}

/// The bearer token, checked as ours whatever its age.
fn old_token(state: &AppState, headers: &HeaderMap) -> ApiResult<(String, TokenClaims)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix(BEARER))
        .map(str::trim)
        .ok_or_else(|| {
            refuse(
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthenticated,
                "renewing a session needs its token",
            )
        })?;
    let claims = state.tokens.verify_any_age(token).map_err(|_| {
        refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Unauthenticated,
            "the session token didn't check out; log in again",
        )
    })?;
    Ok((token.to_string(), claims))
}

/// The server's idle limit in seconds; zero with renewal off.
pub fn idle_secs(state: &AppState) -> i64 {
    i64::try_from(state.config.session_idle.as_secs()).unwrap_or(i64::MAX)
}

fn idle_limit(state: &AppState) -> ApiResult<i64> {
    let secs = idle_secs(state);
    if secs == 0 {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            ErrorCode::RenewalOff,
            "this server doesn't renew sessions; log in again",
        ));
    }
    Ok(secs)
}

/// What a renewal of `claims`' session may stand on: its row and the key
/// its device signs with. Every refusal names the one thing that stops it.
fn standing(state: &AppState, claims: &TokenClaims) -> ApiResult<(SessionRow, VerifyingKey)> {
    let idle = idle_limit(state)?;
    let Some(store) = state.storage.sessions() else {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            ErrorCode::RenewalOff,
            "this server keeps no sessions to renew; log in again",
        ));
    };
    let now = state.now();
    let idle_refusal = || {
        refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::SessionIdle,
            "this session went unused for longer than this server allows; log in again",
        )
    };
    // A token can't be used after it runs out, so its last use was before
    // then: past the limit from there is past it from the last use too.
    if now.saturating_sub(claims.exp) > idle {
        return Err(idle_refusal());
    }
    let ended = || {
        refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::SessionEnded,
            "this session was ended; log in again",
        )
    };
    let Some(row) = store.session(&claims.nonce)? else {
        return Err(ended());
    };
    if row.account_id != claims.account_id || row.device_id != claims.device_id {
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Unauthenticated,
            "the session token doesn't match the session it names",
        ));
    }
    if let Some(at) = row.ended_at {
        // Ended by a renewal whose answer may never have arrived: the same
        // device may try once more, briefly. Ended any other way: done.
        let retry = row.renewed_to.is_some() && now.saturating_sub(at) <= RETRY_SECS;
        if !retry {
            return Err(ended());
        }
    }
    if now.saturating_sub(row.last_seen) > idle {
        return Err(idle_refusal());
    }
    if state.storage.account(claims.account_id)?.is_none() {
        return Err(ended());
    }
    let revoked = || {
        refuse(
            StatusCode::FORBIDDEN,
            ErrorCode::RevokedDevice,
            "this device was revoked from the identity; its session can't be renewed",
        )
    };
    if state
        .storage
        .revoked_after(claims.account_id, claims.device_id, claims.device_id)?
        .is_some()
    {
        return Err(revoked());
    }
    let held = state
        .storage
        .device_pub(claims.account_id, claims.device_id)?
        .ok_or_else(|| {
            refuse(
                StatusCode::FORBIDDEN,
                ErrorCode::UnknownDevice,
                "this device hasn't signed in to the account; it can't renew a session",
            )
        })?;
    // Once the identity's records list the device, its key there is the
    // key; a record marked revoked is a revocation too.
    if let Some((listed_key, listed_revoked)) =
        listed_device(state, claims.account_id, claims.device_id)?
    {
        if listed_revoked {
            return Err(revoked());
        }
        if listed_key != held {
            return Err(refuse(
                StatusCode::FORBIDDEN,
                ErrorCode::UnknownDevice,
                "the key this server holds for the device isn't the one the identity lists",
            ));
        }
    }
    let key = VerifyingKey::from_bytes(&held)
        .map_err(|_| ApiError::internal("a stored device key is unusable"))?;
    Ok((row, key))
}

/// The device's key and whether it is marked revoked, from the account's
/// newest identity record set as verified under its pinned Identity
/// Signing Key; `None` while the records don't list the device yet.
fn listed_device(
    state: &AppState,
    account_id: Id,
    device_id: Id,
) -> ApiResult<Option<([u8; 32], bool)>> {
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
    let records = state
        .storage
        .identity_records(account_id, &manifest.entries)?
        .into_iter()
        .map(|r| (r.path, r.body))
        .collect();
    let set = RecordSet { manifest, records };
    let verified = set
        .verify(st.isk_pub.as_ref())
        .map_err(|_| ApiError::internal("the stored identity record set didn't verify"))?;
    Ok(verified
        .devices
        .get(&device_id)
        .map(|d| (d.device_pub, d.revoked_at.is_some())))
}

/// `POST /v1/account/session/challenge`: a one-time challenge for the
/// bearer token's session, after the same checks the renewal makes (so a
/// device learns early that only the password helps, and why).
pub async fn challenge(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: HeaderMap,
) -> ApiResult<Cbor<RenewChallenge>> {
    brake(&state, &peer, None)?;
    let (token, claims) = old_token(&state, &headers)?;
    standing(&state, &claims)?;
    let token_hash = lockdown_crypto::hash::hash(token.as_bytes());
    Ok(Cbor(state.renewals.issue(
        claims.nonce,
        token_hash,
        state.now(),
    )?))
}

/// `POST /v1/account/session/renew`: the signed challenge in, a new
/// session out; the old one ends in the same step.
pub async fn renew(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: HeaderMap,
    Cbor(body): Cbor<RenewFinish>,
) -> ApiResult<Cbor<LoginFinishReply>> {
    brake(&state, &peer, None)?;
    let (token, claims) = old_token(&state, &headers)?;
    let now = state.now();
    let stale = || {
        ApiError::bad_request(
            "that renewal challenge isn't one this server gave this session, or it ran out; ask for a new one",
        )
    };
    if body.v != RENEW_V {
        return Err(ApiError::bad_request(format!(
            "renewal format {} isn't one this server reads",
            body.v
        )));
    }
    let pending = state
        .renewals
        .take(body.challenge_id, now)
        .ok_or_else(stale)?;
    let token_hash = lockdown_crypto::hash::hash(token.as_bytes());
    if pending.session_id != claims.nonce || pending.token_hash != token_hash {
        return Err(stale());
    }
    let (old, key) = standing(&state, &claims)?;
    key.verify(
        &session_renew_message(&body.challenge_id, &pending.challenge, &token),
        &body.signature,
        "session renewal",
    )
    .map_err(|_| {
        refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::BadSignature,
            "the renewal wasn't signed by this device's key",
        )
    })?;
    let account = state
        .storage
        .account(claims.account_id)?
        .ok_or_else(|| ApiError::not_found("this account"))?;
    let ttl = i64::try_from(state.config.session_ttl.as_secs()).unwrap_or(i64::MAX);
    let (new_token, new_claims) = state
        .tokens
        .issue_claims(claims.account_id, claims.device_id, now, ttl)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let address = (peer.0 != "local").then(|| peer.0.clone());
    let row = SessionRow {
        id: new_claims.nonce,
        account_id: claims.account_id,
        device_id: claims.device_id,
        // "Signed in" stays the moment the password was typed.
        created_at: old.created_at,
        last_seen: now,
        expires_at: new_claims.exp,
        address: address.or(old.address),
        client: client_label(
            headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok()),
        )
        .or(old.client),
        ended_at: None,
        renewed_to: None,
    };
    let Some(store) = state.storage.sessions() else {
        return Err(refuse(
            StatusCode::FORBIDDEN,
            ErrorCode::RenewalOff,
            "this server keeps no sessions to renew; log in again",
        ));
    };
    if !store.renew_session(&claims.nonce, &row, now, RETRY_SECS)? {
        // Ended, or renewed by another request, between the checks and here.
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            ErrorCode::SessionEnded,
            "this session was ended; log in again",
        ));
    }
    let _ = store.prune_sessions(now, idle_secs(&state));
    state.live.notify(crate::live::Change::account(
        claims.account_id,
        LiveKind::Sessions,
    ));
    Ok(Cbor(LoginFinishReply {
        token: new_token,
        expires_at: new_claims.exp,
        profile_id: account.profile_id,
        second_factor: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenges_answer_once_run_out_and_one_per_session() {
        let c = Challenges::default();
        let h = lockdown_crypto::hash::hash(b"token");
        let a = c.issue([1; 16], h, 1_000).unwrap();
        assert_eq!(a.expires_at, 1_000 + CHALLENGE_SECS);
        // Used once, then never again.
        assert!(c.take(a.challenge_id, 1_001).is_some());
        assert!(c.take(a.challenge_id, 1_001).is_none());
        // Run out.
        let b = c.issue([1; 16], h, 1_000).unwrap();
        assert!(c.take(b.challenge_id, 1_000 + CHALLENGE_SECS).is_none());
        // A second challenge for one session replaces the first.
        let first = c.issue([2; 16], h, 2_000).unwrap();
        let second = c.issue([2; 16], h, 2_001).unwrap();
        assert_ne!(first.challenge, second.challenge);
        assert!(c.take(first.challenge_id, 2_002).is_none());
        let p = c.take(second.challenge_id, 2_002).unwrap();
        assert_eq!((p.session_id, p.token_hash), ([2; 16], h));
        assert_eq!(c.waiting(), 0);
    }

    #[test]
    fn a_flood_of_challenges_keeps_the_newest() {
        let c = Challenges::default();
        let h = lockdown_crypto::hash::hash(b"t");
        let first = c.issue([0xff; 16], h, 4_950).unwrap();
        for i in 0..CHALLENGES_MAX {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&(i as u64).to_le_bytes());
            c.issue(id, h, 5_000).unwrap();
        }
        assert_eq!(c.waiting(), CHALLENGES_MAX);
        assert!(
            c.take(first.challenge_id, 5_000).is_none(),
            "the oldest went"
        );
    }
}
