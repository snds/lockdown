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

#[cfg(all(feature = "native", not(target_os = "wasi")))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(feature = "native", not(target_os = "wasi")))]
use std::sync::{Arc, Condvar, Mutex};
#[cfg(all(feature = "native", not(target_os = "wasi")))]
use std::time::Duration;

#[cfg(all(feature = "native", not(target_os = "wasi")))]
use lockdown_sync::transport::SseItem;
#[cfg(all(feature = "native", not(target_os = "wasi")))]
use lockdown_sync::wire::LiveKind;
use serde::Serialize;

#[cfg(all(feature = "native", not(target_os = "wasi")))]
use crate::SyncOutcome;
use crate::{App, AppError, AppResult};

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
    /// Something the person should hear about even with the window in the
    /// background (a new device signed in, a security key changed): the
    /// shell shows it as an OS notification. Plain words, no value.
    Notify {
        /// The notification's title.
        title: String,
        /// Its text.
        body: String,
    },
}

#[cfg(all(feature = "native", not(target_os = "wasi")))]
/// Where notices go (the shell turns them into window events).
pub type LiveSink = Arc<dyn Fn(LiveNotice) + Send + Sync>;

#[cfg(all(feature = "native", not(target_os = "wasi")))]
/// Keeps the live channel running; dropping it stops it.
pub struct LiveHandle {
    stop: Arc<AtomicBool>,
    wake: Arc<(Mutex<bool>, Condvar)>,
}

#[cfg(all(feature = "native", not(target_os = "wasi")))]
impl std::fmt::Debug for LiveHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveHandle")
    }
}

#[cfg(all(feature = "native", not(target_os = "wasi")))]
impl Drop for LiveHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let (lock, cv) = &*self.wake;
        *lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        cv.notify_all();
    }
}

#[cfg(all(feature = "native", not(target_os = "wasi")))]
/// How often the channel thread looks whether the vault opened.
const IDLE_LOOK: Duration = Duration::from_secs(3);
#[cfg(all(feature = "native", not(target_os = "wasi")))]
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

/// The OS notifications for new account alerts: a device new to the
/// account, a security key added or removed (docs/plan/25-self-host-2.md
/// §5). Dismissed alerts, breaches (Security shows those) and kinds this
/// build doesn't know make none.
pub fn notices_for(alerts: &[lockdown_sync::wire::AccountAlert]) -> Vec<LiveNotice> {
    use lockdown_sync::wire::AlertKind;
    alerts
        .iter()
        .filter(|a| !a.dismissed)
        .filter_map(|a| {
            let name = a
                .key_name
                .as_deref()
                .map(|n| format!("\u{201c}{n}\u{201d}"))
                .unwrap_or_else(|| "A security key".into());
            let (title, body) = match a.kind {
                AlertKind::NewDevice => (
                    "New device signed in".to_string(),
                    "A device that hadn't signed in to your Lockdown account before just did. If it wasn't you, end its session and revoke it in Devices.".to_string(),
                ),
                AlertKind::SecurityKeyAdded => (
                    "Security key added".to_string(),
                    format!("{name} was added to your Lockdown account's sign-in. If it wasn't you, end the sessions you don't know in Devices and change your account password."),
                ),
                AlertKind::SecurityKeyRemoved => (
                    "Security key removed".to_string(),
                    format!("{name} was removed from your Lockdown account's sign-in. If it wasn't you, end the sessions you don't know in Devices and change your account password."),
                ),
                AlertKind::EmailBreach | AlertKind::Other => return None,
            };
            Some(LiveNotice::Notify { title, body })
        })
        .collect()
}

fn account_error(e: lockdown_sync::TransportError) -> AppError {
    AppError::refused(e.to_string())
}

impl App {
    #[cfg(all(feature = "native", not(target_os = "wasi")))]
    /// The server and token when the vault is open and the account has a
    /// session; not activity.
    fn live_target(&self) -> Option<Result<(String, String), String>> {
        self.call(|core| Ok(core.session.as_mut().and_then(|s| s.account_session())))
            .ok()
            .flatten()
    }

    #[cfg(all(feature = "native", not(target_os = "wasi")))]
    /// A sync the live channel asked for: like [`App::sync`], but it
    /// doesn't count as activity for the idle lock.
    fn quiet_sync(&self) -> AppResult<SyncOutcome> {
        self.call(|core| core.session()?.sync().map_err(AppError::refused))
    }

    #[cfg(all(feature = "native", not(target_os = "wasi")))]
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

    /// The server and session token the live channel opens with, when the
    /// vault is open and the account has a session; `None` otherwise. Not
    /// activity. For a host that holds the channel itself (the web vault's
    /// worker, docs/plan/27-web-server-finish.md §2), never for a page.
    pub fn live_account(&self) -> Option<(String, String)> {
        self.call(|core| Ok(core.session.as_mut().and_then(|s| s.account_session())))
            .ok()
            .flatten()
            .and_then(Result::ok)
    }

    /// The sync a live event asks for, run by a host that holds the channel
    /// itself: like [`App::sync`], with every check, but not activity for
    /// the idle lock.
    pub fn live_sync(&self) -> AppResult<crate::SyncOutcome> {
        self.call(|core| core.session()?.sync().map_err(AppError::refused))
    }

    /// The server said `token`'s session was ended from another device:
    /// forget it, so the next call asks for the password. A token that is
    /// no longer this device's changes nothing.
    pub fn live_session_ended(&self, token: &str) -> AppResult<()> {
        let token = token.to_string();
        self.call(move |core| {
            if let Some(s) = core.session.as_mut() {
                s.forget_session_token(&token).map_err(AppError::refused)?;
            }
            Ok(())
        })
    }

    /// The account's live sessions, oldest first, with device names.
    pub fn sessions(&self) -> AppResult<Vec<SessionView>> {
        let list = self.account_call(|t, token| t.sessions(token))?;
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
        self.account_call(|t, token| t.end_session(token, id))
    }

    /// Ends every session of the account but this device's; how many.
    pub fn end_other_sessions(&self) -> AppResult<u64> {
        Ok(self
            .account_call(|t, token| t.end_other_sessions(token))?
            .ended)
    }

    /// One call to the account's server with this device's session, which
    /// is renewed first when due; a token the server refuses anyway is
    /// renewed (or a newer one taken) and the call made once more
    /// (docs/plan/33-session-renewal.md).
    fn account_call<T>(
        &self,
        call: impl Fn(&lockdown_sync::HttpTransport, &str) -> Result<T, lockdown_sync::TransportError>,
    ) -> AppResult<T> {
        let (server, token) = self.account_now()?;
        let transport = lockdown_sync::HttpTransport::new(&server).map_err(account_error)?;
        match call(&transport, &token) {
            Err(_) if transport.session_refused() => {
                let token = self.call(|core| {
                    core.session()?
                        .token_after_refusal()
                        .map_err(AppError::refused)
                })?;
                call(&transport, &token).map_err(account_error)
            }
            done => done.map_err(account_error),
        }
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

#[cfg(all(feature = "native", not(target_os = "wasi")))]
fn wait(stop: &AtomicBool, wake: &(Mutex<bool>, Condvar), d: Duration) {
    let (lock, cv) = wake;
    let guard = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if stop.load(Ordering::SeqCst) {
        return;
    }
    let _ = cv.wait_timeout(guard, d);
}

#[cfg(all(feature = "native", not(target_os = "wasi")))]
fn sync_side(app: App, stop: Arc<AtomicBool>, wake: Arc<(Mutex<bool>, Condvar)>, sink: LiveSink) {
    let (lock, cv) = &*wake;
    loop {
        {
            let mut due = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*due && !stop.load(Ordering::SeqCst) {
                due = cv
                    .wait(due)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if stop.load(Ordering::SeqCst) {
                return;
            }
        }
        std::thread::sleep(GATHER);
        *lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
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

#[cfg(all(feature = "native", not(target_os = "wasi")))]
fn channel_side(
    app: App,
    stop: Arc<AtomicBool>,
    wake: Arc<(Mutex<bool>, Condvar)>,
    sink: LiveSink,
) {
    let mut pause = Duration::from_secs(2);
    // A token the server said was ended: don't knock again with it.
    let mut ended: Option<String> = None;
    // The newest alert already shown as an OS notification: set when the
    // channel first opens, so only what happens from then on notifies.
    let mut notified: Option<u64> = None;
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
                    // Refused: renew it (or take a newer one) and open again
                    // after the pause; only a session the server won't renew
                    // stops the channel, and the shell hears why.
                    match app.call(|core| {
                        core.session()?
                            .token_after_refusal()
                            .map_err(AppError::refused)
                    }) {
                        Ok(fresh) if fresh != token => {}
                        Err(e) if e.code == crate::ErrorCode::SignInNeeded => {
                            sink(if e.detail.as_deref() == Some("ended") {
                                LiveNotice::SessionEnded
                            } else {
                                LiveNotice::SyncFailed { message: e.message }
                            });
                            ended = Some(token);
                        }
                        _ => ended = Some(token),
                    }
                }
                wait(&stop, &wake, pause);
                pause = (pause * 2).min(Duration::from_secs(60));
                continue;
            }
        };
        pause = Duration::from_secs(2);
        if notified.is_none()
            && let Ok(t) = lockdown_sync::HttpTransport::new(&server)
            && let Ok(a) = t.account_alerts(&token, u64::MAX >> 1)
        {
            notified = Some(a.current_seq);
        }
        sink(LiveNotice::Connected);
        let why = loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            match stream.next_item() {
                Ok(Some(SseItem::Event(e))) => match e.kind {
                    LiveKind::Sync => {
                        let (lock, cv) = &*wake;
                        *lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
                        cv.notify_all();
                    }
                    LiveKind::Alerts => {
                        sink(LiveNotice::Alerts);
                        if let Some(since) = notified
                            && let Ok(t) = lockdown_sync::HttpTransport::new(&server)
                            && let Ok(a) = t.account_alerts(&token, since)
                        {
                            notified = Some(a.current_seq.max(since));
                            for n in notices_for(&a.alerts) {
                                sink(n);
                            }
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_sync::wire::{AccountAlert, AlertKind};

    fn alert(kind: AlertKind, key_name: Option<&str>, dismissed: bool) -> AccountAlert {
        AccountAlert {
            seq: 1,
            kind,
            at: 0,
            device_id: None,
            breach: None,
            dismissed,
            key_name: key_name.map(str::to_string),
        }
    }

    #[test]
    fn new_devices_and_key_changes_notify_and_nothing_else_does() {
        let n = notices_for(&[
            alert(AlertKind::NewDevice, None, false),
            alert(AlertKind::SecurityKeyRemoved, Some("Blue key"), false),
            alert(AlertKind::SecurityKeyAdded, None, true),
            alert(AlertKind::EmailBreach, None, false),
            alert(AlertKind::Other, None, false),
        ]);
        assert_eq!(n.len(), 2);
        match &n[1] {
            LiveNotice::Notify { title, body } => {
                assert_eq!(title, "Security key removed");
                assert!(body.contains("\u{201c}Blue key\u{201d}"));
            }
            other => panic!("{other:?}"),
        }
        let json = serde_json::to_value(&n[0]).unwrap();
        assert_eq!(json["kind"], "notify");
    }
}
