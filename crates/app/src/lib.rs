//! The app on top of the core: one command surface the desktop shell uses
//! today and the phone shells will, with the rules a shell must not be
//! trusted with enforced here (03-architecture §8):
//!
//! - **Secrets cross as bytes, never strings.** A hidden value leaves this
//!   crate only inside a [`Revealed`], only when asked for by field, and
//!   is wiped when that value drops. `show` without `reveal` is masked
//!   metadata only.
//! - **Lock is the core's.** [`App::lock`] drops every key; the idle,
//!   hidden-window and sleep timers in [`lock`] decide on their own, on a
//!   worker thread, whether a shell is awake or not. A locked app answers
//!   every vault call with [`ErrorCode::Locked`].
//!
//! The open session (identity, store, keys, vault) holds types that are not
//! `Send`, so it lives on one worker thread and [`App`] is the handle: a
//! clonable, `Send + Sync` sender that runs one call at a time there. One
//! call at a time is also the mutex: two shells (or two windows) never
//! interleave on the vault.

#![forbid(unsafe_code)]

pub mod lock;
mod revealed;
pub mod secrets;
mod session;
pub mod types;

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

pub use lock::{LockPolicy, LockReason};
pub use revealed::Revealed;
pub use session::{Paths, device_name};
pub use types::*;

use lock::LockClock;
use session::{Mode, Session};

/// What went wrong, in a shape a shell can switch on.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The vault is locked; unlock first.
    Locked,
    /// Nothing is set up here yet; create or join first.
    NotSetUp,
    /// The core refused; `message` says why in plain words.
    Refused,
    /// The worker thread is gone (the app is shutting down).
    Stopped,
}

/// An error a shell shows as is.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    fn locked() -> Self {
        Self {
            code: ErrorCode::Locked,
            message: "Lockdown is locked".into(),
        }
    }

    fn refused(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Refused,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        let code = if message.starts_with("nothing is set up")
            || message.starts_with("no ecosystem identity")
        {
            ErrorCode::NotSetUp
        } else {
            ErrorCode::Refused
        };
        Self { code, message }
    }
}

/// The result of every call.
pub type AppResult<T> = Result<T, AppError>;

/// How often the worker checks the lock timers when no call arrives.
const TICK: Duration = Duration::from_secs(1);

/// What lives on the worker thread.
struct Core {
    paths: Paths,
    persona: Option<String>,
    policy: LockPolicy,
    clock: LockClock,
    session: Option<Session>,
    last_reason: Option<LockReason>,
}

impl Core {
    fn new(paths: Paths, persona: Option<String>, policy: LockPolicy) -> Self {
        Self {
            paths,
            persona,
            policy,
            clock: LockClock::new(Instant::now(), SystemTime::now()),
            session: None,
            last_reason: None,
        }
    }

    /// Runs the timers; locks when one is due.
    fn enforce(&mut self) {
        if self.session.is_none() {
            return;
        }
        if let Some(reason) = self
            .clock
            .due(&self.policy, Instant::now(), SystemTime::now())
        {
            self.lock(reason);
        }
    }

    fn lock(&mut self, reason: LockReason) {
        // Dropping the session drops the identity, the persona keys and
        // the vault: every key type zeroes itself on drop.
        self.session = None;
        self.last_reason = Some(reason);
    }

    fn open(&mut self, mode: Mode) -> AppResult<Status> {
        let session = Session::open(self.paths.clone(), self.persona.as_deref(), mode)?;
        let status = session.status()?;
        self.session = Some(session);
        self.last_reason = None;
        self.clock = LockClock::new(Instant::now(), SystemTime::now());
        Ok(status)
    }

    fn session(&mut self) -> AppResult<&mut Session> {
        self.session.as_mut().ok_or_else(AppError::locked)
    }

    fn lock_state(&self) -> LockState {
        LockState {
            locked: self.session.is_none(),
            reason: self.last_reason,
            idle_secs: self.clock.idle_for(Instant::now()).as_secs(),
            idle_timeout_secs: self.policy.idle.map(|d| d.as_secs()),
            hidden_timeout_secs: self.policy.hidden.map(|d| d.as_secs()),
        }
    }
}

type Job = Box<dyn FnOnce(&mut Core) + Send>;

/// The app, as a shell holds it. Clone it freely; every clone talks to the
/// same session on the same thread.
#[derive(Clone)]
pub struct App {
    tx: Sender<Job>,
}

impl App {
    /// Starts the worker. Nothing is opened yet: call [`App::probe`] to see
    /// what is here, then [`App::open_or_create`] or one of the join flows.
    pub fn start(paths: Paths, persona: Option<String>, policy: LockPolicy) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("lockdown-app".into())
            .spawn(move || {
                let mut core = Core::new(paths, persona, policy);
                loop {
                    match rx.recv_timeout(TICK) {
                        Ok(job) => {
                            core.enforce();
                            job(&mut core);
                        }
                        Err(RecvTimeoutError::Timeout) => core.enforce(),
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                core.lock(LockReason::Requested);
            })
            .expect("the app's worker thread starts");
        Self { tx }
    }

    /// Runs `f` on the worker and waits for its answer.
    fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Core) -> AppResult<T> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let job: Job = Box::new(move |core| {
            let _ = reply_tx.send(f(core));
        });
        let stopped = || AppError {
            code: ErrorCode::Stopped,
            message: "the app is shutting down".into(),
        };
        self.tx.send(job).map_err(|_| stopped())?;
        reply_rx.recv().map_err(|_| stopped())?
    }

    /// A call that needs the vault open; it counts as activity.
    fn with_session<T, F>(&self, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> Result<T, String> + Send + 'static,
    {
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let session = core.session()?;
            f(session).map_err(AppError::refused)
        })
    }

    // --- lifecycle -------------------------------------------------------------

    /// What is on this device before anything opens.
    pub fn probe(&self) -> AppResult<Probe> {
        self.call(|core| core.paths.probe().map_err(AppError::from))
    }

    /// First run or a later one: with `create`, the identity and the vault
    /// are made when missing; without, only what exists is opened.
    pub fn open_or_create(&self, create: bool) -> AppResult<Status> {
        let mode = if create { Mode::Init } else { Mode::Open };
        self.call(move |core| core.open(mode))
    }

    /// Opens again after a lock, through the device keystore (D11).
    pub fn unlock(&self) -> AppResult<Status> {
        self.call(|core| core.open(Mode::Open))
    }

    /// Drops every key now.
    pub fn lock(&self) -> AppResult<()> {
        self.call(|core| {
            core.lock(LockReason::Requested);
            Ok(())
        })
    }

    /// Locked or open, and the timers.
    pub fn lock_state(&self) -> AppResult<LockState> {
        self.call(|core| Ok(core.lock_state()))
    }

    /// The shell saw activity (a keystroke, a click) without calling.
    pub fn touch(&self) -> AppResult<()> {
        self.call(|core| {
            core.clock.touch(Instant::now());
            Ok(())
        })
    }

    /// The window went out of view (`true`) or came back.
    pub fn set_hidden(&self, hidden: bool) -> AppResult<LockState> {
        self.call(move |core| {
            core.clock.set_hidden(hidden, Instant::now());
            core.enforce();
            Ok(core.lock_state())
        })
    }

    /// Changes the timers from now on.
    pub fn set_policy(&self, policy: LockPolicy) -> AppResult<LockState> {
        self.call(move |core| {
            core.policy = policy;
            core.enforce();
            Ok(core.lock_state())
        })
    }

    /// The timers in force.
    pub fn policy(&self) -> AppResult<LockPolicy> {
        self.call(|core| Ok(core.policy))
    }

    // --- items -----------------------------------------------------------------

    /// Every visible item, nothing hidden in it.
    pub fn list(&self) -> AppResult<Vec<ItemSummary>> {
        self.with_session(|s| s.list())
    }

    /// One item, masked; with `reveal`, hidden fields carry their bytes.
    pub fn show(&self, id: &str, reveal: bool) -> AppResult<ItemDetail> {
        let id = id.to_string();
        self.with_session(move |s| s.show(&id, reveal))
    }

    /// One hidden field's bytes.
    pub fn reveal(&self, id: &str, field_id: &str) -> AppResult<Revealed> {
        let (id, field_id) = (id.to_string(), field_id.to_string());
        self.with_session(move |s| s.reveal(&id, &field_id))
    }

    /// Adds a login. `password` is bytes a shell wipes afterwards.
    pub fn add_login(
        &self,
        site: &str,
        username: &str,
        password: Vec<u8>,
        title: Option<&str>,
    ) -> AppResult<String> {
        let (site, username, title) = (
            site.to_string(),
            username.to_string(),
            title.map(str::to_string),
        );
        let password = zeroize::Zeroizing::new(password);
        self.with_session(move |s| {
            s.add_login(&site, &username, &password, title.as_deref())
                .map(|id| id.to_string())
        })
    }

    /// Changes whichever of a login's title, username or password are
    /// given; returns the new version number.
    pub fn edit_login(
        &self,
        id: &str,
        title: Option<&str>,
        username: Option<&str>,
        password: Option<Vec<u8>>,
    ) -> AppResult<u64> {
        let (id, title, username) = (
            id.to_string(),
            title.map(str::to_string),
            username.map(str::to_string),
        );
        let password = password.map(zeroize::Zeroizing::new);
        self.with_session(move |s| {
            s.edit_login(
                &id,
                title.as_deref(),
                username.as_deref(),
                password.as_deref().map(|p| p.as_slice()),
            )
        })
    }

    /// Moves an item to the trash.
    pub fn trash(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.trash(&id))
    }

    // --- the account, sync, kit, devices ------------------------------------------

    /// Where everything is and the vault's newest head.
    pub fn status(&self) -> AppResult<Status> {
        self.with_session(|s| s.status())
    }

    /// Makes an account for this identity (OPAQUE; the password never
    /// leaves this device).
    pub fn account_register(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> AppResult<Registered> {
        let (server, login, secret_key) = (
            server.to_string(),
            login.to_string(),
            secret_key.map(str::to_string),
        );
        let password = zeroize::Zeroizing::new(password.to_string());
        self.with_session(move |s| {
            s.account_register(&server, &login, &password, secret_key.as_deref())
        })
    }

    /// Logs in. On a device that holds the identity: a fresh session. On an
    /// empty one: enrols this device from the account (then open).
    pub fn account_login(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let (server, login, secret_key) = (
            server.to_string(),
            login.to_string(),
            secret_key.map(str::to_string),
        );
        let password = zeroize::Zeroizing::new(password.to_string());
        self.call(move |core| {
            let probe = core.paths.probe()?;
            if !probe.identity_present {
                return session::enrol_from_account(
                    &core.paths,
                    &server,
                    &login,
                    &password,
                    secret_key.as_deref(),
                )
                .map_err(AppError::refused);
            }
            core.clock.touch(Instant::now());
            core.session()?
                .account_login(&server, &login, &password, secret_key.as_deref())
                .map_err(AppError::refused)
        })
    }

    /// On a device with nothing on it: the identity back from the server
    /// with the recovery kit and the account password.
    pub fn recover(
        &self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: &str,
        recovery_code: &str,
    ) -> AppResult<Vec<String>> {
        let (server, login) = (server.to_string(), login.to_string());
        let password = zeroize::Zeroizing::new(password.to_string());
        let secret_key = zeroize::Zeroizing::new(secret_key.to_string());
        let recovery_code = zeroize::Zeroizing::new(recovery_code.to_string());
        self.call(move |core| {
            session::recover(
                &core.paths,
                &server,
                &login,
                &password,
                &secret_key,
                &recovery_code,
            )
            .map_err(AppError::refused)
        })
    }

    /// Pull, merge, push, head.
    pub fn sync(&self) -> AppResult<SyncOutcome> {
        self.with_session(|s| s.sync())
    }

    /// Makes the recovery kit; its page comes back once.
    pub fn kit_create(&self, replace: bool, password: Option<&str>) -> AppResult<KitCreated> {
        let password = password.map(|p| zeroize::Zeroizing::new(p.to_string()));
        self.with_session(move |s| s.kit_create(replace, password.as_deref().map(|p| p.as_str())))
    }

    /// Checks a kit against this identity, changing nothing unless `drill`.
    pub fn kit_verify(
        &self,
        secret_key: &str,
        recovery_code: &str,
        password: Option<&str>,
        drill: bool,
    ) -> AppResult<Vec<String>> {
        let secret_key = zeroize::Zeroizing::new(secret_key.to_string());
        let recovery_code = zeroize::Zeroizing::new(recovery_code.to_string());
        let password = password.map(|p| zeroize::Zeroizing::new(p.to_string()));
        self.with_session(move |s| {
            s.kit_verify(
                &secret_key,
                &recovery_code,
                password.as_deref().map(|p| p.as_str()),
                drill,
            )
        })
    }

    /// Every device of the identity.
    pub fn device_list(&self) -> AppResult<Vec<DeviceInfo>> {
        self.with_session(|s| Ok(s.device_list()))
    }

    /// Revokes another device (D73).
    pub fn device_revoke(&self, selector: &str) -> AppResult<Vec<String>> {
        let selector = selector.to_string();
        self.with_session(move |s| s.device_revoke(&selector))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths, None, LockPolicy::default());
        (dir, app)
    }

    #[test]
    fn probe_then_create_then_open_again() {
        let (_dir, app) = fresh();
        let probe = app.probe().unwrap();
        assert!(!probe.identity_present);
        assert!(!probe.device_key_present);
        let err = app.open_or_create(false).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotSetUp);
        let status = app.open_or_create(true).unwrap();
        assert_eq!(status.live_items, 0);
        assert!(status.device_name.starts_with("Lockdown"));
        let probe = app.probe().unwrap();
        assert!(probe.identity_present && probe.device_key_present);
        assert!(!app.lock_state().unwrap().locked);
    }

    #[test]
    fn show_is_masked_until_asked_and_reveal_comes_back_as_bytes() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        let id = app
            .add_login(
                "https://example.com/login",
                "me",
                b"correct horse battery staple".to_vec(),
                None,
            )
            .unwrap();
        let rows = app.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "example.com");
        assert_eq!(rows[0].username, "me");

        let masked = app.show(&id, false).unwrap();
        let pw = masked.fields.iter().find(|f| f.field_id == "pw").unwrap();
        assert!(pw.hidden);
        assert!(pw.value.is_none());
        assert!(pw.secret.is_none());
        assert!(pw.mask.as_deref().unwrap().starts_with('•'));
        // Nothing serialised for the shell carries the password.
        let json = serde_json::to_string(&masked).unwrap();
        assert!(!json.contains("correct horse"));

        let revealed = app.show(&id, true).unwrap();
        let pw = revealed.fields.iter().find(|f| f.field_id == "pw").unwrap();
        assert_eq!(
            pw.secret.as_ref().unwrap().expose(),
            b"correct horse battery staple"
        );
        let json = serde_json::to_string(&revealed).unwrap();
        assert!(!json.contains("correct horse"));

        let one = app.reveal(&id, "pw").unwrap();
        assert_eq!(one.expose(), b"correct horse battery staple");
        assert!(app.reveal(&id, "user").is_err());

        app.edit_login(&id, Some("Example"), None, Some(b"hunter2".to_vec()))
            .unwrap();
        assert_eq!(app.reveal(&id, "pw").unwrap().expose(), b"hunter2");
        assert_eq!(app.list().unwrap()[0].title, "Example");

        app.trash(&id).unwrap();
        assert!(app.list().unwrap().is_empty());
    }

    #[test]
    fn the_idle_timer_locks_and_unlock_opens_again() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        app.add_login("https://example.com", "me", b"pw".to_vec(), None)
            .unwrap();
        // An idle timeout of zero: due on the very next tick.
        let state = app
            .set_policy(LockPolicy {
                idle: Some(Duration::ZERO),
                hidden: None,
                sleep_gap: None,
            })
            .unwrap();
        assert!(state.locked);
        assert_eq!(state.reason, Some(LockReason::Idle));
        let err = app.list().unwrap_err();
        assert_eq!(err.code, ErrorCode::Locked);
        assert!(app.status().is_err());

        app.set_policy(LockPolicy::default()).unwrap();
        let status = app.unlock().unwrap();
        assert_eq!(status.live_items, 1);
        assert!(!app.lock_state().unwrap().locked);
        assert_eq!(app.list().unwrap().len(), 1);
    }

    #[test]
    fn a_hidden_window_locks_after_its_timeout_and_a_request_locks_at_once() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        app.set_policy(LockPolicy {
            idle: None,
            hidden: Some(Duration::ZERO),
            sleep_gap: None,
        })
        .unwrap();
        assert!(!app.lock_state().unwrap().locked);
        let state = app.set_hidden(true).unwrap();
        assert!(state.locked);
        assert_eq!(state.reason, Some(LockReason::Hidden));

        app.set_hidden(false).unwrap();
        app.unlock().unwrap();
        app.lock().unwrap();
        let state = app.lock_state().unwrap();
        assert!(state.locked);
        assert_eq!(state.reason, Some(LockReason::Requested));
    }
}
