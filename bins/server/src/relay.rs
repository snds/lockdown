//! The relays two ends meet through (docs/plan/22-security-depth.md §2, §9):
//!
//! - a **device link**: an enrolled device opens it with the hash of a
//!   ten-character code; a new device joins with the code itself and gets
//!   a key only it holds; the two then trade frames (the identity crate's
//!   handshake, `lockdown_vault::device_link`) until the existing device
//!   approves or either side closes it;
//! - a **phrase check**: one person opens it for another account; the two
//!   trade the frames of an OPAQUE exchange keyed by a phrase they said to
//!   each other (`lockdown_app` `verify_phrase`).
//!
//! Neither relay can read what matters. Both live in memory only, close on
//! their own after ten minutes, carry at most eight frames of 256 KiB each
//! way, and are capped per account. Joining a link is braked like a login
//! (the code is the only thing a stranger could guess), and a link takes
//! one join. A frame from the wrong side, or after the link closed, is
//! refused; a closed or expired link reads as closed, never as gone.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use lockdown_sync::account_security::{
    FramePoll, FramePush, Frames, LinkDevice, LinkJoin, LinkJoined, LinkOpen, LinkOpened,
    MAX_FRAME, MAX_FRAMES, VerifyEntry, VerifyList, VerifyOpen, VerifyOpened,
};
use lockdown_sync::wire::ErrorCode;
use lockdown_vault::Id;
use lockdown_vault::device_link::{LINK_TTL_SECS, LinkCode};
use serde_bytes::ByteBuf;

use crate::AppState;
use crate::api::{ApiError, ApiResult, Cbor, Peer, Session, brake};

/// The most links one account has open at once.
pub const LINKS_PER_ACCOUNT: usize = 3;
/// The most phrase checks one account is part of at once.
pub const CHECKS_PER_ACCOUNT: usize = 16;
/// The most of either the server keeps.
const MAX_OPEN: usize = 10_000;

/// Which end of a relay a frame comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum End {
    /// The end that opened it.
    Opener,
    /// The end that joined.
    Joiner,
}

#[derive(Debug, Default)]
struct Mailbox {
    /// Frames the opener sent, oldest first.
    from_opener: Vec<Vec<u8>>,
    /// Frames the joiner sent.
    from_joiner: Vec<Vec<u8>>,
    closed: bool,
}

impl Mailbox {
    fn push(&mut self, end: End, frame: Vec<u8>) -> ApiResult<()> {
        if self.closed {
            return Err(ApiError::new(
                StatusCode::GONE,
                ErrorCode::NotFound,
                "this was closed; start again from the other device",
            ));
        }
        if frame.is_empty() || frame.len() > MAX_FRAME {
            return Err(ApiError::bad_request("a frame is 1 byte to 256 KiB"));
        }
        let queue = match end {
            End::Opener => &mut self.from_opener,
            End::Joiner => &mut self.from_joiner,
        };
        if queue.len() >= MAX_FRAMES {
            return Err(ApiError::bad_request("too many frames on one relay"));
        }
        queue.push(frame);
        Ok(())
    }

    fn poll(&self, end: End, after: u32, expires_at: i64) -> Frames {
        let queue = match end {
            End::Opener => &self.from_joiner,
            End::Joiner => &self.from_opener,
        };
        Frames {
            frames: queue
                .iter()
                .skip(after as usize)
                .map(|f| ByteBuf::from(f.clone()))
                .collect(),
            closed: self.closed,
            expires_at,
        }
    }
}

#[derive(Debug)]
struct Link {
    account: Id,
    token_hash: [u8; 32],
    joiner_key: Option<[u8; 32]>,
    device_bound: bool,
    expires_at: i64,
    mail: Mailbox,
}

#[derive(Debug)]
struct Check {
    from: Id,
    to: Id,
    expires_at: i64,
    mail: Mailbox,
}

/// The relays' state, in memory.
#[derive(Debug, Default)]
pub struct Relays {
    links: Mutex<BTreeMap<Id, Link>>,
    checks: Mutex<BTreeMap<Id, Check>>,
    /// Sign-ins waiting for a security key, and key registrations under way
    /// (`second_factor`).
    pub(crate) second_factor: crate::second_factor::Pending,
}

fn key_hash(key: &[u8]) -> [u8; 32] {
    *blake3::hash(key).as_bytes()
}

/// Constant-time equality for two hashes.
fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn prune<T>(map: &mut BTreeMap<Id, T>, now: i64, expires: impl Fn(&T) -> i64) {
    // A closed relay is kept until it would have run out, so a late poll
    // reads "closed" rather than "not found".
    map.retain(|_, v| expires(v) > now);
}

fn gone() -> ApiError {
    ApiError::not_found("that has run out or never existed; start again from the other device")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// --- device links ---------------------------------------------------------------

/// `POST /v1/links`: an enrolled device opens a link.
pub async fn link_open(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(body): Cbor<LinkOpen>,
) -> ApiResult<Cbor<LinkOpened>> {
    let now = state.now();
    let mut links = lock(&state.relays.links);
    prune(&mut links, now, |l| l.expires_at);
    let open = links
        .values()
        .filter(|l| l.account == session.account_id && !l.mail.closed)
        .count();
    if open >= LINKS_PER_ACCOUNT || links.len() >= MAX_OPEN {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "this account already has three devices being added; finish or cancel one first",
        ));
    }
    if links
        .values()
        .any(|l| same(&l.token_hash, &body.token_hash))
    {
        return Err(ApiError::bad_request("make a new code and try again"));
    }
    let mut mail = Mailbox::default();
    mail.push(End::Opener, body.hello.into_vec())?;
    let link_id = Id::new();
    let expires_at = now + LINK_TTL_SECS;
    links.insert(
        link_id,
        Link {
            account: session.account_id,
            token_hash: body.token_hash,
            joiner_key: None,
            device_bound: false,
            expires_at,
            mail,
        },
    );
    Ok(Cbor(LinkOpened {
        link_id,
        expires_at,
    }))
}

/// `POST /v1/links/join`: a new device joins with the code. Braked per
/// address like a login, and a link takes one join.
pub async fn link_join(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    Cbor(body): Cbor<LinkJoin>,
) -> ApiResult<Cbor<LinkJoined>> {
    brake(&state, &peer, None)?;
    let code = LinkCode::parse(&body.code).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let hash = code.token_hash();
    let now = state.now();
    let mut links = lock(&state.relays.links);
    prune(&mut links, now, |l| l.expires_at);
    let Some((link_id, link)) = links
        .iter_mut()
        .find(|(_, l)| same(&l.token_hash, &hash) && !l.mail.closed)
    else {
        return Err(ApiError::not_found(
            "no device is waiting with that code; check it, or make a new one on the other device",
        ));
    };
    if link.joiner_key.is_some() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::Forbidden,
            "another device already used that code; make a new one on the other device",
        ));
    }
    let key = lockdown_crypto::rand::random_32().map_err(|e| ApiError::internal(e.to_string()))?;
    link.joiner_key = Some(key_hash(&key));
    Ok(Cbor(LinkJoined {
        link_id: *link_id,
        key: ByteBuf::from(key.to_vec()),
        expires_at: link.expires_at,
    }))
}

fn with_link<T>(
    state: &AppState,
    link_id: Id,
    end: End,
    who: impl Fn(&Link) -> bool,
    f: impl FnOnce(&mut Link) -> ApiResult<T>,
) -> ApiResult<T> {
    let now = state.now();
    let mut links = lock(&state.relays.links);
    prune(&mut links, now, |l| l.expires_at);
    let link = links.get_mut(&link_id).ok_or_else(gone)?;
    if !who(link) {
        return Err(ApiError::forbidden(match end {
            End::Opener => "that link belongs to another account",
            End::Joiner => "that key doesn't open this link",
        }));
    }
    f(link)
}

fn joiner(key: &Option<ByteBuf>) -> ApiResult<[u8; 32]> {
    key.as_ref()
        .map(|k| key_hash(k))
        .ok_or_else(|| ApiError::bad_request("the joining device's key is missing"))
}

/// `POST /v1/links/{id}/frames`: the existing device sends.
pub async fn link_frames(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(link_id): Path<Id>,
    Cbor(body): Cbor<FramePush>,
) -> ApiResult<StatusCode> {
    with_link(
        &state,
        link_id,
        End::Opener,
        |l| l.account == session.account_id,
        |l| l.mail.push(End::Opener, body.frame.into_vec()),
    )?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/links/{id}/poll`: the existing device reads.
pub async fn link_poll(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(link_id): Path<Id>,
    Cbor(body): Cbor<FramePoll>,
) -> ApiResult<Cbor<Frames>> {
    with_link(
        &state,
        link_id,
        End::Opener,
        |l| l.account == session.account_id,
        |l| Ok(Cbor(l.mail.poll(End::Opener, body.after, l.expires_at))),
    )
}

/// `POST /v1/links/{id}/joiner/frames`: the new device sends, with its key.
pub async fn link_joiner_frames(
    State(state): State<Arc<AppState>>,
    Path(link_id): Path<Id>,
    Cbor(body): Cbor<FramePush>,
) -> ApiResult<StatusCode> {
    let key = joiner(&body.key)?;
    with_link(
        &state,
        link_id,
        End::Joiner,
        |l| l.joiner_key.is_some_and(|k| same(&k, &key)),
        |l| l.mail.push(End::Joiner, body.frame.into_vec()),
    )?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/links/{id}/joiner/poll`: the new device reads, with its key.
pub async fn link_joiner_poll(
    State(state): State<Arc<AppState>>,
    Path(link_id): Path<Id>,
    Cbor(body): Cbor<FramePoll>,
) -> ApiResult<Cbor<Frames>> {
    let key = joiner(&body.key)?;
    with_link(
        &state,
        link_id,
        End::Joiner,
        |l| l.joiner_key.is_some_and(|k| same(&k, &key)),
        |l| Ok(Cbor(l.mail.poll(End::Joiner, body.after, l.expires_at))),
    )
}

/// `POST /v1/links/{id}/device`: the existing device names the device it
/// approved, so that device's first sign-in is one the account already
/// knows (no second factor, no new-device alert: a person approved it on
/// a device of the account). Once per link, only after a device joined.
pub async fn link_device(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(link_id): Path<Id>,
    Cbor(body): Cbor<LinkDevice>,
) -> ApiResult<StatusCode> {
    with_link(
        &state,
        link_id,
        End::Opener,
        |l| l.account == session.account_id,
        |l| {
            if l.joiner_key.is_none() || l.mail.from_joiner.is_empty() {
                return Err(ApiError::bad_request("no device has asked on this link"));
            }
            if l.device_bound || l.mail.closed {
                return Err(ApiError::bad_request("this link already finished"));
            }
            l.device_bound = true;
            Ok(())
        },
    )?;
    lockdown_crypto::VerifyingKey::from_bytes(&body.device_pub)
        .map_err(|_| ApiError::bad_request("that isn't an Ed25519 public key"))?;
    state
        .storage
        .register_device(session.account_id, body.device_id, &body.device_pub)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/links/{id}/close`: the existing device ends the link.
pub async fn link_close(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(link_id): Path<Id>,
) -> ApiResult<StatusCode> {
    with_link(
        &state,
        link_id,
        End::Opener,
        |l| l.account == session.account_id,
        |l| {
            l.mail.closed = true;
            Ok(())
        },
    )?;
    Ok(StatusCode::NO_CONTENT)
}

// --- phrase checks between two people ----------------------------------------------

/// `POST /v1/verifications`: one person asks another to check keys.
pub async fn verify_open(
    State(state): State<Arc<AppState>>,
    session: Session,
    Cbor(body): Cbor<VerifyOpen>,
) -> ApiResult<Cbor<VerifyOpened>> {
    if body.to_account == session.account_id {
        return Err(ApiError::bad_request("that's you"));
    }
    if state.storage.account(body.to_account)?.is_none() {
        return Err(ApiError::not_found("no such account on this server"));
    }
    let now = state.now();
    let mut checks = lock(&state.relays.checks);
    prune(&mut checks, now, |c| c.expires_at);
    let mine = |a: Id| checks.values().filter(|c| c.from == a || c.to == a).count();
    if mine(session.account_id) >= CHECKS_PER_ACCOUNT
        || mine(body.to_account) >= CHECKS_PER_ACCOUNT
        || checks.len() >= MAX_OPEN
    {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::RateLimited,
            "too many key checks are open; finish one first",
        ));
    }
    let verify_id = Id::new();
    let expires_at = now + LINK_TTL_SECS;
    checks.insert(
        verify_id,
        Check {
            from: session.account_id,
            to: body.to_account,
            expires_at,
            mail: Mailbox::default(),
        },
    );
    Ok(Cbor(VerifyOpened {
        verify_id,
        expires_at,
    }))
}

/// `GET /v1/verifications`: the checks this account is part of.
pub async fn verify_list(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> ApiResult<Cbor<VerifyList>> {
    let now = state.now();
    let mut checks = lock(&state.relays.checks);
    prune(&mut checks, now, |c| c.expires_at);
    let mut open: Vec<VerifyEntry> = checks
        .iter()
        .filter(|(_, c)| {
            !c.mail.closed && (c.from == session.account_id || c.to == session.account_id)
        })
        .map(|(id, c)| VerifyEntry {
            verify_id: *id,
            from_account: c.from,
            to_account: c.to,
            expires_at: c.expires_at,
        })
        .collect();
    open.sort_by_key(|e| (e.expires_at, e.verify_id));
    Ok(Cbor(VerifyList { open }))
}

fn with_check<T>(
    state: &AppState,
    verify_id: Id,
    account: Id,
    f: impl FnOnce(&mut Check, End) -> ApiResult<T>,
) -> ApiResult<T> {
    let now = state.now();
    let mut checks = lock(&state.relays.checks);
    prune(&mut checks, now, |c| c.expires_at);
    let check = checks.get_mut(&verify_id).ok_or_else(gone)?;
    let end = if check.from == account {
        End::Opener
    } else if check.to == account {
        End::Joiner
    } else {
        return Err(ApiError::forbidden(
            "that check is between two other people",
        ));
    };
    f(check, end)
}

/// `POST /v1/verifications/{id}/frames`.
pub async fn verify_frames(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(verify_id): Path<Id>,
    Cbor(body): Cbor<FramePush>,
) -> ApiResult<StatusCode> {
    with_check(&state, verify_id, session.account_id, |c, end| {
        c.mail.push(end, body.frame.into_vec())
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/verifications/{id}/poll`.
pub async fn verify_poll(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(verify_id): Path<Id>,
    Cbor(body): Cbor<FramePoll>,
) -> ApiResult<Cbor<Frames>> {
    with_check(&state, verify_id, session.account_id, |c, end| {
        Ok(Cbor(c.mail.poll(end, body.after, c.expires_at)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mailbox_keeps_each_ends_frames_apart_and_refuses_after_close() {
        let mut m = Mailbox::default();
        m.push(End::Opener, b"hello".to_vec()).unwrap();
        m.push(End::Joiner, b"request".to_vec()).unwrap();
        assert_eq!(
            m.poll(End::Joiner, 0, 9).frames,
            vec![ByteBuf::from(b"hello".to_vec())]
        );
        assert_eq!(m.poll(End::Opener, 0, 9).frames.len(), 1);
        assert!(m.poll(End::Opener, 1, 9).frames.is_empty());
        assert!(m.push(End::Joiner, Vec::new()).is_err());
        for _ in 1..MAX_FRAMES {
            m.push(End::Joiner, b"x".to_vec()).unwrap();
        }
        assert!(
            m.push(End::Joiner, b"x".to_vec()).is_err(),
            "eight frames each way"
        );
        m.closed = true;
        assert!(m.push(End::Opener, b"late".to_vec()).is_err());
        assert!(m.poll(End::Joiner, 0, 9).closed);
        assert!(same(&key_hash(b"a"), &key_hash(b"a")) && !same(&key_hash(b"a"), &key_hash(b"b")));
    }
}
