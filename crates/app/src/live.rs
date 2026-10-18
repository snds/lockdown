//! Live sync and the session list (docs/plan/20-self-host-server.md §5–§6).
//!
//! **Live sync.** [`App::start_live`] keeps the server's live channel
//! (`GET /v1/events`) open while the vault is unlocked and the account has
//! a session. An event says only that something changed; the app then
//! syncs exactly as **Sync now** does (every manifest and key-transparency
//! check included) and tells the shell, which redraws. Bursts are folded:
//! a device pushing ten items and a head is one sync here, two at most.
//! The channel is closed within a keep-alive (about 20 s) of a lock, and
//! reopened with a growing pause (2 s to a minute) after a drop. Nothing
//! the channel carries is trusted: a forged event costs one needless sync.
//! A sync started by the channel is not activity, so it never keeps the
//! vault from locking on its idle timer.
//!
//! **Sessions.** [`App::sessions`] lists where the account is signed in
//! (device, address, client, when), with this device's session marked;
//! [`App::end_session`] and [`App::end_other_sessions`] end them on the
//! server, which stops those tokens at once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use lockdown_sync::transport::SseItem;
use lockdown_sync::wire::LiveKind;
use serde::Serialize;

use crate::{App, AppError, AppResult, SyncOutcome};

/// What the live channel did, for the shell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveNotice {
    /// The channel is open: changes elsewhere now show here by themselves.
    Connected,
    /// The channel dropped; it is reopened by itself.
    Disconnected {
        /// What happened, in plain words.
        message: String,
    },
    /// Another device changed something and this device synced.
    Synced {
        /// Items pulled.
        pulled: usize,
        /// The sync's own lines.
        lines: Vec<String>,
    },
    /// Another device changed something and the sync that followed failed.
    SyncFailed {
        /// The sync's words.
        message: String,
    },
    /// The account's alerts changed: read them again.
    Alerts,
    /// The account's session list changed.
    Sessions,
    /// This device's session was ended from another device: log in again.
    SessionEnded,
}

/// Where notices go (the shell turns them into window events).
pub type LiveSink = Arc<dyn Fn(LiveNotice) + Send + Sync>;

/// Keeps the live channel running; dropping it stops it.
pub struct LiveHandle {
    stop: Arc<AtomicBool>,
    wake: Arc<(Mutex<bool>, Condvar)>,
}

impl std::fmt::Debug for LiveHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveHandle")
    }
}

impl Drop for LiveHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let (lock, cv) = &*self.wake;
        *lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        cv.notify_all();
    }
}

/// How often the channel thread looks whether the vault opened.
const IDLE_LOOK: Duration = Duration::from_secs(3);
/// How long bursts of events are gathered into one sync.
const GATHER: Duration = Duration::from_millis(250);

/// One session of the account, as the Devices screen shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionView {
    /// The session's id (names it to end it).
    pub id: String,
    /// The device's id.
    pub device_id: String,
    /// The device's name from the identity's records, when it is there.
    pub device_name: Option<String>,
    /// The device's platform, when known.
    pub platform: Option<String>,
    /// Where it was last used from, as the server saw it.
    pub address: Option<String>,
    /// The client's name for itself.
    pub client: Option<String>,
    /// When it started (seconds since the Unix epoch).
    pub created_at: i64,
    /// When it was last used, to the minute.
    pub last_seen: i64,
    /// When it runs out.
    pub expires_at: i64,
    /// Whether it is this device's own session.
    pub current: bool,
}

fn account_error(e: lockdown_sync::TransportError) -> AppError {
    AppError::refused(e.to_string())
}

impl App {
    /// The server and token when the vault is open and the account has a
    /// session; not activity.
    fn live_target(&self) -> Option<Result<(String, String), String>> {
        self.call(|core| Ok(core.session.as_ref().and_then(|s| s.account_session())))
            .ok()
            .flatten()
    }

    /// A sync the live channel asked for: like [`App::sync`], but it
    /// doesn't count as activity for the idle lock.
    fn quiet_sync(&self) -> AppResult<SyncOutcome> {
        self.call(|core| core.session()?.sync().map_err(AppError::refused))
    }

    /// Starts the live channel on its own threads; notices go to `sink`.
    pub fn start_live(&self, sink: LiveSink) -> LiveHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new((Mutex::new(false), Condvar::new()));
        // The sync side: waits to be woken, gathers a burst, syncs once.
        {
            let (app, stop, wake, sink) = (self.clone(), stop.clone(), wake.clone(), sink.clone());
            let _ = std::thread::Builder::new()
                .name("lockdown-live-sync".into())
                .spawn(move || sync_side(app, stop, wake, sink));
        }
        {
            let (app, stop, wake) = (self.clone(), stop.clone(), wake.clone());
            let _ = std::thread::Builder::new()
                .name("lockdown-live".into())
                .spawn(move || channel_side(app, stop, wake, sink));
        }
        LiveHandle { stop, wake }
    }

    /// The account's live sessions, oldest first, with device names.
    pub fn sessions(&self) -> AppResult<Vec<SessionView>> {
        let (server, token) = self.account_now()?;
        let list = lockdown_sync::HttpTransport::new(&server)
            .map_err(account_error)?
            .sessions(&token)
            .map_err(account_error)?;
        let devices = self.with_session(|s| Ok(s.device_list()))?;
        Ok(list
            .sessions
            .into_iter()
            .map(|s| {
                let device_id = s.device_id.to_string();
                let device = devices.iter().find(|d| d.id == device_id);
                SessionView {
                    id: s.id,
                    device_name: device.map(|d| d.name.clone()),
                    platform: device.map(|d| d.platform.clone()),
                    device_id,
                    address: s.address,
                    client: s.client,
                    created_at: s.created_at,
                    last_seen: s.last_seen,
                    expires_at: s.expires_at,
                    current: s.current,
                }
            })
            .collect())
    }

    /// Ends one session on the server; its token stops working at once.
    /// Ending this device's own session signs it out.
    pub fn end_session(&self, id: &str) -> AppResult<()> {
        let (server, token) = self.account_now()?;
        lockdown_sync::HttpTransport::new(&server)
            .map_err(account_error)?
            .end_session(&token, id)
            .map_err(account_error)?;
        Ok(())
    }

    /// Ends every session of the account but this device's; how many.
    pub fn end_other_sessions(&self) -> AppResult<u64> {
        let (server, token) = self.account_now()?;
        Ok(lockdown_sync::HttpTransport::new(&server)
            .map_err(account_error)?
            .end_other_sessions(&token)
            .map_err(account_error)?
            .ended)
    }

    fn account_now(&self) -> AppResult<(String, String)> {
        let target = self.call(|core| {
            core.clock.touch(std::time::Instant::now());
            Ok(core.session()?.account_session())
        })?;
        match target {
            Some(Ok(pair)) => Ok(pair),
            Some(Err(e)) => Err(AppError::refused(e)),
            None => Err(AppError::refused(
                "sessions need an account on a server; register or log in first",
            )),
        }
    }
}

fn wait(stop: &AtomicBool, wake: &(Mutex<bool>, Condvar), d: Duration) {
    let (lock, cv) = wake;
    let guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if stop.load(Ordering::SeqCst) {
        return;
    }
    let _ = cv.wait_timeout(guard, d);
}

fn sync_side(app: App, stop: Arc<AtomicBool>, wake: Arc<(Mutex<bool>, Condvar)>, sink: LiveSink) {
    let (lock, cv) = &*wake;
    loop {
        {
            let mut due = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*due && !stop.load(Ordering::SeqCst) {
                due = cv.wait(due).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
        }
        std::thread::sleep(GATHER);
        *lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match app.quiet_sync() {
            Ok(outcome) => sink(LiveNotice::Synced {
                pulled: outcome.pulled,
                lines: outcome.lines,
            }),
            // Locked meanwhile: the next unlock's sync covers it.
            Err(e) if e.code == crate::ErrorCode::Locked => {}
            Err(e) if e.code == crate::ErrorCode::Stopped => return,
            Err(e) => sink(LiveNotice::SyncFailed { message: e.message }),
        }
    }
}

fn channel_side(app: App, stop: Arc<AtomicBool>, wake: Arc<(Mutex<bool>, Condvar)>, sink: LiveSink) {
    let mut pause = Duration::from_secs(2);
    // A token the server said was ended: don't knock again with it.
    let mut ended: Option<String> = None;
    while !stop.load(Ordering::SeqCst) {
        let (server, token) = match app.live_target() {
            Some(Ok(pair)) if ended.as_deref() != Some(pair.1.as_str()) => pair,
            _ => {
                wait(&stop, &wake, IDLE_LOOK);
                continue;
            }
        };
        let stream = lockdown_sync::HttpTransport::new(&server).and_then(|t| t.events(&token));
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                if let lockdown_sync::TransportError::Refused { status: 401, .. } = e {
                    ended = Some(token);
                }
                wait(&stop, &wake, pause);
                pause = (pause * 2).min(Duration::from_secs(60));
                continue;
            }
        };
        pause = Duration::from_secs(2);
        sink(LiveNotice::Connected);
        let why = loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            match stream.next_item() {
                Ok(Some(SseItem::Event(e))) => match e.kind {
                    LiveKind::Sync => {
                        let (lock, cv) = &*wake;
                        *lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
                        cv.notify_all();
                    }
                    LiveKind::Alerts => sink(LiveNotice::Alerts),
                    LiveKind::Sessions => sink(LiveNotice::Sessions),
                    LiveKind::SessionEnded => {
                        let t = token.clone();
                        let _ = app.call(move |core| {
                            if let Some(s) = core.session.as_mut() {
                                s.forget_session_token(&t).map_err(AppError::refused)?;
                            }
                            Ok(())
                        });
                        ended = Some(token.clone());
                        sink(LiveNotice::SessionEnded);
                        break None;
                    }
                    LiveKind::Other => {}
                },
                Ok(Some(SseItem::KeepAlive)) => {
                    // Locked, or another session since: close this one.
                    match app.live_target() {
                        Some(Ok((s, t))) if s == server && t == token => {}
                        _ => break None,
                    }
                }
                Ok(None) => break Some("the server closed the live channel".to_string()),
                Err(e) => break Some(e.to_string()),
            }
        };
        if let Some(message) = why {
            sink(LiveNotice::Disconnected { message });
            wait(&stop, &wake, pause);
        }
    }
}
