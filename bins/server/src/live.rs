//! Live sync (`self-host-push`, desktop and web half;
//! docs/plan/20-self-host-server.md §5): a server-sent-events channel at
//! `GET /v1/events` that tells an account's signed-in devices *that*
//! something changed, so a change on one device shows on another without
//! polling.
//!
//! - **Content-free.** An event is `{v, kind}` and nothing else: no vault,
//!   item, count or device. The device then syncs (or reads its alerts or
//!   sessions) exactly as if it had polled, with every manifest and
//!   key-transparency check that implies; a lying event can at most make a
//!   device sync for nothing.
//! - **Who hears what.** Only the account's own sessions; the session that
//!   made a change doesn't hear about its own write; a "session ended"
//!   event goes to that one session, and its stream closes after it.
//! - **Where events come from.** Every successful write under
//!   `/v1/vaults` and `/v1/identity` ([`notify_writes`], a layer over the
//!   router, so routes added later are covered too), to the writer's
//!   account and, for a shared vault, to its owner and every member still
//!   in it; alerts recorded or dismissed; sessions started or ended.
//! - **Bounds.** At most [`PER_ACCOUNT`] open streams per account and
//!   [`TOTAL`] in all; a stream re-checks its session every minute and
//!   closes when the session ends or the token runs out.
//!
//! The mobile push relay (05-self-host §5) is not here: phones get the same
//! content-free ping through Lockdown's relay later (D122).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use lockdown_sync::wire::{BEARER, LIVE_V, LiveEvent, LiveKind};
use lockdown_vault::Id;
use tokio::sync::broadcast;

use crate::AppState;
use crate::api::Session;

/// Open streams allowed per account.
pub const PER_ACCOUNT: usize = 16;

/// Open streams allowed in all.
pub const TOTAL: usize = 10_000;

/// How often an open stream re-checks its session.
pub const RECHECK: Duration = Duration::from_secs(60);

/// How often a keep-alive comment is written.
pub const KEEP_ALIVE: Duration = Duration::from_secs(20);

/// One change, as the handlers announce it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    /// Whose.
    pub account_id: Id,
    /// What kind.
    pub kind: LiveKind,
    /// The session that made it (it doesn't hear its own write).
    pub origin: Option<[u8; 16]>,
    /// Only this session hears it.
    pub only: Option<[u8; 16]>,
}

impl Change {
    /// A change of `kind` for the whole account.
    pub fn account(account_id: Id, kind: LiveKind) -> Self {
        Self {
            account_id,
            kind,
            origin: None,
            only: None,
        }
    }
}

/// Fans changes out to the open streams.
pub struct Notifier {
    tx: broadcast::Sender<Change>,
    open: Mutex<HashMap<Id, usize>>,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifier")
            .field("open", &self.open_streams())
            .finish()
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier {
    /// A notifier with nobody listening.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// Announces `change`. Never blocks; with nobody listening it is
    /// dropped.
    pub fn notify(&self, change: Change) {
        let _ = self.tx.send(change);
    }

    /// How many streams are open.
    pub fn open_streams(&self) -> usize {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .sum()
    }

    fn admit(self: &Arc<Self>, account_id: Id) -> Option<Admitted> {
        let mut open = self
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total: usize = open.values().sum();
        let mine = open.entry(account_id).or_insert(0);
        if *mine >= PER_ACCOUNT || total >= TOTAL {
            return None;
        }
        *mine += 1;
        Some(Admitted {
            notifier: self.clone(),
            account_id,
        })
    }
}

/// A place in the open-stream count, given back on drop.
struct Admitted {
    notifier: Arc<Notifier>,
    account_id: Id,
}

impl Drop for Admitted {
    fn drop(&mut self) {
        let mut open = self
            .notifier
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(n) = open.get_mut(&self.account_id) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                open.remove(&self.account_id);
            }
        }
    }
}

fn event(kind: LiveKind) -> Event {
    let body = serde_json::to_string(&LiveEvent { v: LIVE_V, kind })
        .unwrap_or_else(|_| String::from("{\"v\":1,\"kind\":\"sync\"}"));
    Event::default().event("lockdown").data(body)
}

/// Whether this stream's session still stands at `now`.
fn session_stands(state: &AppState, session: &Session, now: i64) -> SessionNow {
    if session.expires_at <= now {
        return SessionNow::RanOut;
    }
    match state.storage.sessions() {
        Some(store) => match store.session(&session.session_id) {
            Ok(Some(row)) if row.ended_at.is_some() => SessionNow::Ended,
            _ => SessionNow::Stands,
        },
        None => SessionNow::Stands,
    }
}

enum SessionNow {
    Stands,
    Ended,
    RanOut,
}

struct Stream {
    rx: broadcast::Receiver<Change>,
    state: Arc<AppState>,
    session: Session,
    done: bool,
    _admitted: Admitted,
}

impl Stream {
    /// The event this change means for this stream, if any; whether the
    /// stream ends after it.
    fn wants(&self, change: &Change) -> Option<(LiveKind, bool)> {
        if change.account_id != self.session.account_id {
            return None;
        }
        if change.only.is_some_and(|id| id != self.session.session_id) {
            return None;
        }
        if change.kind == LiveKind::Sync && change.origin == Some(self.session.session_id) {
            return None;
        }
        Some((change.kind, change.kind == LiveKind::SessionEnded))
    }

    async fn next(&mut self) -> Option<Event> {
        if self.done {
            return None;
        }
        loop {
            let now = self.state.now();
            let left = u64::try_from(self.session.expires_at.saturating_sub(now)).unwrap_or(0);
            let wait = RECHECK.min(Duration::from_secs(left.max(1)));
            tokio::select! {
                got = self.rx.recv() => match got {
                    Ok(change) => {
                        if let Some((kind, last)) = self.wants(&change) {
                            self.done = last;
                            return Some(event(kind));
                        }
                    }
                    // Missed some: something changed; sync once.
                    Err(broadcast::error::RecvError::Lagged(_)) => return Some(event(LiveKind::Sync)),
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
                () = tokio::time::sleep(wait) => {
                    match session_stands(&self.state, &self.session, self.state.now()) {
                        SessionNow::Stands => {}
                        SessionNow::Ended => {
                            self.done = true;
                            return Some(event(LiveKind::SessionEnded));
                        }
                        SessionNow::RanOut => return None,
                    }
                }
            }
        }
    }
}

/// `GET /v1/events`: the live channel for this session.
pub async fn events(State(state): State<Arc<AppState>>, session: Session) -> Response {
    let Some(admitted) = state.live.admit(session.account_id) else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, HeaderValue::from_static("60"))],
            "too many live connections for this account",
        )
            .into_response();
    };
    let stream = Stream {
        rx: state.live.tx.subscribe(),
        state: state.clone(),
        session,
        done: false,
        _admitted: admitted,
    };
    let events = futures_util::stream::unfold(stream, |mut s| async move {
        s.next().await.map(|e| (Ok::<Event, Infallible>(e), s))
    });
    let sse = Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE).text("keep-alive"));
    (
        // Reverse proxies that buffer responses (nginx) pass this through
        // as it comes.
        [(
            header::HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        )],
        sse,
    )
        .into_response()
}

/// What a write to `path` changes, for the live channel.
fn kind_of_write(method: &Method, path: &str) -> Option<LiveKind> {
    if !matches!(*method, Method::PUT | Method::POST | Method::DELETE) {
        return None;
    }
    if path.starts_with("/v1/vaults") || path.starts_with("/v1/identity") {
        Some(LiveKind::Sync)
    } else if path.starts_with("/v1/account/alerts") {
        Some(LiveKind::Alerts)
    } else {
        None
    }
}

/// The layer that announces every successful write under `/v1/vaults`,
/// `/v1/identity` and `/v1/account/alerts` to the writer's account.
pub async fn notify_writes(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let kind = kind_of_write(req.method(), req.uri().path());
    let who = kind.and_then(|_| {
        let token = req
            .headers()
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix(BEARER)?
            .trim()
            .to_string();
        state.tokens.verify(&token, state.now()).ok()
    });
    let vault = vault_in_path(req.uri().path());
    let response = next.run(req).await;
    if let (Some(kind), Some(claims)) = (kind, who)
        && response.status().is_success()
    {
        state.live.notify(Change {
            account_id: claims.account_id,
            kind,
            origin: Some(claims.nonce),
            only: None,
        });
        // A shared vault: its owner and every member still in it hear too.
        if let Some(vault_id) = vault {
            for account_id in vault_audience(&state, vault_id) {
                if account_id != claims.account_id {
                    state.live.notify(Change::account(account_id, LiveKind::Sync));
                }
            }
        }
    }
    response
}

/// The vault a `/v1/vaults/{vault}/…` path names.
fn vault_in_path(path: &str) -> Option<Id> {
    let rest = path.strip_prefix("/v1/vaults/")?;
    Id::parse(rest.split('/').next()?).ok()
}

/// Everyone who reads a shared vault: its owner and its active members,
/// sorted and without repeats. Empty for a vault that isn't shared.
pub fn vault_audience(state: &AppState, vault_id: Id) -> Vec<Id> {
    let Some(sharing) = state.storage.sharing() else {
        return Vec::new();
    };
    let Ok(Some(shared)) = sharing.shared_vault(vault_id) else {
        return Vec::new();
    };
    let mut out = vec![shared.owner_account];
    if let Ok(members) = sharing.members(vault_id) {
        out.extend(
            members
                .into_iter()
                .filter(|m| m.active)
                .map(|m| m.party.account_id),
        );
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_map_to_kinds_and_reads_to_nothing() {
        assert_eq!(
            kind_of_write(&Method::PUT, "/v1/vaults/x/heads"),
            Some(LiveKind::Sync)
        );
        assert_eq!(kind_of_write(&Method::PUT, "/v1/identity"), Some(LiveKind::Sync));
        assert_eq!(
            kind_of_write(&Method::POST, "/v1/account/alerts/3/dismiss"),
            Some(LiveKind::Alerts)
        );
        assert_eq!(kind_of_write(&Method::GET, "/v1/vaults"), None);
        assert_eq!(kind_of_write(&Method::POST, "/v1/account/login/finish"), None);
    }

    #[test]
    fn the_vault_is_read_from_the_path() {
        let id = Id::new();
        assert_eq!(vault_in_path(&format!("/v1/vaults/{id}/heads")), Some(id));
        assert_eq!(vault_in_path(&format!("/v1/vaults/{id}")), Some(id));
        assert_eq!(vault_in_path("/v1/vaults"), None);
        assert_eq!(vault_in_path("/v1/vaults/not-an-id/items"), None);
    }

    #[test]
    fn streams_are_counted_per_account_and_given_back() {
        let n = Arc::new(Notifier::new());
        let a = Id::new();
        let held: Vec<_> = (0..PER_ACCOUNT).map(|_| n.admit(a).unwrap()).collect();
        assert!(n.admit(a).is_none());
        assert!(n.admit(Id::new()).is_some(), "another account still gets in");
        assert_eq!(n.open_streams(), PER_ACCOUNT);
        drop(held);
        assert_eq!(n.open_streams(), 0);
        assert!(n.admit(a).is_some());
    }
}
