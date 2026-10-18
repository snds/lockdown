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

pub mod approval;
pub mod bridges;
pub mod browser;
pub mod depth;
pub mod devices;
pub mod key_check;
pub mod live;
pub mod lock;
pub mod passkeys;
pub mod protection;
pub mod qr;
pub mod recency;
pub mod reminders;
mod revealed;
pub mod secrets;
pub mod security;
mod session;
pub mod settings;
pub mod sharing;
pub mod strength;
mod transfer;
pub mod types;
pub mod views;

#[cfg(not(target_os = "wasi"))]
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Instant, SystemTime};

pub use approval::{Approval, ApprovalKind, Approver, GateInput, PasswordApprover, Route};
pub use lock::{LockPolicy, LockReason};
pub use lockdown_crypto::generator::Recipe;
pub use lockdown_vault::{Change, FieldChange, FieldInput, FieldKind};
pub use revealed::Revealed;
pub use security::{Dismissal, Finding, SecurityReport};
pub use session::items::field_kind_named;
pub use session::{Paths, device_name};
pub use settings::{BackupSettings, Density, RevealGesture, Settings};
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
    /// The call is gated (D101) and nothing confirmed it yet: `detail`
    /// names what the shell must ask for (`password` or `window`); the
    /// shell asks, then calls again with a [`GateInput`].
    ApprovalNeeded,
}

/// An error a shell shows as is.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    /// A machine-readable hint beside the words; today only
    /// [`ErrorCode::ApprovalNeeded`] sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    fn locked() -> Self {
        Self {
            code: ErrorCode::Locked,
            message: "Lockdown is locked".into(),
            detail: None,
        }
    }

    /// The core refused; `message` is shown as is.
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Refused,
            message: message.into(),
            detail: None,
        }
    }

    fn approval_needed(route: Route, reason: &str) -> Self {
        let (detail, message) = match route {
            Route::Password => (
                "password",
                format!("your account password is needed to {reason}"),
            ),
            Route::Window => ("window", format!("confirm in this window to {reason}")),
            Route::Platform => ("platform", format!("confirm to {reason}")),
        };
        Self {
            code: ErrorCode::ApprovalNeeded,
            message,
            detail: Some(detail.into()),
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
        Self {
            code,
            message,
            detail: None,
        }
    }
}

/// The result of every call.
pub type AppResult<T> = Result<T, AppError>;

/// How often the worker checks the lock timers when no call arrives.
#[cfg(not(target_os = "wasi"))]
const TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// What lives on the worker thread.
struct Core {
    paths: Paths,
    persona: Option<String>,
    policy: LockPolicy,
    clock: LockClock,
    session: Option<Session>,
    last_reason: Option<LockReason>,
    /// The settings file as last read or written.
    settings: Settings,
    /// The platform's own check (Touch ID, Windows Hello), when the shell
    /// installed one; else approvals fall back per `approval::route`.
    approver: Option<Box<dyn Approver + Send + Sync>>,
    /// The security center's scores and alerts for this session.
    security: security::SecurityState,
    /// Password fills this core made lately, for the passkey upgrade rule
    /// (`passkeys`); dropped on lock.
    recent_fills: Vec<passkeys::RecentFill>,
    /// Joins and sign-ins waiting for a security key (`devices`).
    depth: devices::Depth,
    /// The bridges slice's state: the clock check, the extension's
    /// sign-in observations (docs/plan/23-bridges.md).
    bridges: bridges::BridgesState,
}

impl Core {
    /// `policy` applies only while no settings file exists; once one does,
    /// its lock timers win (they are what the person set).
    fn new(paths: Paths, persona: Option<String>, policy: LockPolicy) -> Self {
        let file = paths.settings_file();
        let (settings, policy) = match (file.exists(), Settings::load(&file)) {
            (true, Ok(settings)) => {
                let policy = settings.lock_policy();
                (settings, policy)
            }
            (_, Ok(settings)) => (settings, policy),
            (_, Err(_)) => (Settings::default(), policy),
        };
        Self {
            paths,
            persona,
            policy,
            clock: LockClock::new(Instant::now(), SystemTime::now()),
            session: None,
            last_reason: None,
            settings,
            approver: None,
            security: security::SecurityState::default(),
            recent_fills: Vec::new(),
            depth: devices::Depth::default(),
            bridges: bridges::BridgesState::default(),
        }
    }

    /// Runs the gate for `kind` (D101): the platform check, the password
    /// in `gate`, or the person's confirmation in the window, whichever
    /// `approval::route` names for what this device has. Declined or
    /// mismatched comes back refused; nothing yet asked for comes back
    /// [`ErrorCode::ApprovalNeeded`] with what to ask.
    fn approve(
        &mut self,
        kind: ApprovalKind,
        reason: &str,
        gate: &GateInput,
    ) -> AppResult<Approval> {
        let has_account = self.session()?.has_account();
        match approval::route(kind, self.approver.is_some(), has_account) {
            Route::Platform => {
                let approver = self.approver.as_ref().expect("routed only when installed");
                match approver.confirm(kind, reason) {
                    Ok(true) => Ok(Approval {
                        method: approver.method().into(),
                    }),
                    Ok(false) => Err(AppError::refused(format!(
                        "{} didn't confirm, so nothing happened",
                        approver.method()
                    ))),
                    Err(e) => Err(AppError::refused(e)),
                }
            }
            Route::Password => {
                let Some(password) = gate.password.as_deref() else {
                    return Err(AppError::approval_needed(Route::Password, reason));
                };
                let paths = self.paths.clone();
                let session = self.session()?;
                let approver =
                    PasswordApprover::new(zeroize::Zeroizing::new(password.to_string()), |p| {
                        session.check_password(p)
                    });
                match approver.confirm(kind, reason) {
                    Ok(true) => {
                        // A typed password counts for the reminder (`reminders`).
                        reminders::note_password(&paths);
                        Ok(Approval {
                            method: approver.method().into(),
                        })
                    }
                    Ok(false) => Err(AppError::refused("the password didn't match")),
                    Err(e) => Err(AppError::refused(e)),
                }
            }
            Route::Window => {
                if gate.confirmed {
                    Ok(Approval {
                        method: "window".into(),
                    })
                } else {
                    Err(AppError::approval_needed(Route::Window, reason))
                }
            }
        }
    }

    /// One hidden field's bytes, through the gate when the item is a
    /// crown jewel: `Reveal` to see it, `Use` to copy it (brief §3.1).
    fn reveal(
        &mut self,
        id: &str,
        field_id: &str,
        kind: ApprovalKind,
        gate: &GateInput,
    ) -> AppResult<Revealed> {
        // A fill-only item is never shown or copied (docs/plan/18-sharing.md).
        self.session()?
            .refuse_fill_only(id)
            .map_err(AppError::refused)?;
        self.reveal_for_fill(id, field_id, kind, gate)
    }

    /// [`Self::reveal`] for a fill the browser extension asked for, which a
    /// fill-only item allows: the value goes into the page, nowhere else.
    fn reveal_for_fill(
        &mut self,
        id: &str,
        field_id: &str,
        kind: ApprovalKind,
        gate: &GateInput,
    ) -> AppResult<Revealed> {
        self.clock.touch(Instant::now());
        // A linked field's value is the other item's, read through that
        // item's gates and rights (D235).
        let (target, target_field) = self
            .session()?
            .resolve_link(id, field_id)
            .map_err(AppError::refused)?;
        if target != id {
            self.session()?
                .refuse_fill_only(&target)
                .map_err(AppError::refused)?;
        }
        let original = id;
        let (id, field_id) = (target.as_str(), target_field.as_str());
        let crown = self
            .session()?
            .is_crown_jewel(id)
            .map_err(AppError::refused)?;
        if crown {
            let reason = match kind {
                ApprovalKind::Use => "copy this protected value",
                _ => "reveal this protected value",
            };
            self.approve(kind, reason, gate)?;
        } else if self
            .session()?
            .needs_reprompt(id, field_id)
            .map_err(AppError::refused)?
        {
            // The card asks before its security code is used (D238).
            self.approve(kind, "use this card's security code", gate)?;
        }
        // Ask each time (`protection`): every value out of a non-crown item asks.
        self.exposure_gate(
            id,
            match kind {
                ApprovalKind::Reveal => "show this value",
                _ => "use this value",
            },
            gate,
        )?;
        let revealed = self
            .session()?
            .reveal(id, field_id)
            .map_err(AppError::refused)?;
        self.note_use(id);
        if original != id {
            self.note_use(original);
        }
        Ok(revealed)
    }

    /// Notes a deliberate use for the ranking; a failure to write the
    /// device's record never fails the use itself.
    fn note_use(&mut self, id: &str) {
        if let (Ok(session), Ok(item)) = (self.session(), lockdown_vault::Id::parse(id)) {
            let _ = session.note_use(item);
        }
    }

    /// The gate for `kind` when the item is a crown jewel; nothing otherwise.
    fn gate_if_crown(
        &mut self,
        id: &str,
        kind: ApprovalKind,
        reason: &str,
        gate: &GateInput,
    ) -> AppResult<()> {
        self.clock.touch(Instant::now());
        if self
            .session()?
            .is_crown_jewel(id)
            .map_err(AppError::refused)?
        {
            self.approve(kind, reason, gate)?;
        }
        Ok(())
    }

    /// One code's value now (or next), through the `Use` gate when the item
    /// is a crown jewel: a code inside one is copied, never shown, and the
    /// copy is a use (brief §3.1).
    fn code_now(
        &mut self,
        id: &str,
        field_id: &str,
        next: bool,
        gate: &GateInput,
    ) -> AppResult<Revealed> {
        self.session()?
            .refuse_fill_only(id)
            .map_err(AppError::refused)?;
        self.code_for_fill(id, field_id, next, gate)
    }

    /// [`Self::code_now`] for a fill, which a fill-only item allows.
    fn code_for_fill(
        &mut self,
        id: &str,
        field_id: &str,
        next: bool,
        gate: &GateInput,
    ) -> AppResult<Revealed> {
        self.clock.touch(Instant::now());
        let (target, target_field) = self
            .session()?
            .resolve_link(id, field_id)
            .map_err(AppError::refused)?;
        if target != id {
            self.session()?
                .refuse_fill_only(&target)
                .map_err(AppError::refused)?;
        }
        let (id, field_id) = (target.as_str(), target_field.as_str());
        if self
            .session()?
            .is_crown_jewel(id)
            .map_err(AppError::refused)?
        {
            self.approve(ApprovalKind::Use, "copy this protected code", gate)?;
        }
        self.exposure_gate(id, "use this code", gate)?;
        let now = self
            .bridges
            .code_time(session::now_secs(), self.settings.code_clock_correction);
        let code = self
            .session()?
            .code_now(id, field_id, now, next)
            .map_err(AppError::refused)?;
        self.note_use(id);
        Ok(code)
    }

    /// Moves an HOTP code on by one (a new item version), after the `Use`
    /// gate when the item is a crown jewel: the step changes the item, as
    /// an edit does (D110), and hands back a code, as a copy does.
    fn advance_code(
        &mut self,
        id: &str,
        field_id: &str,
        gate: &GateInput,
    ) -> AppResult<CodeAdvanced> {
        self.clock.touch(Instant::now());
        if self
            .session()?
            .is_crown_jewel(id)
            .map_err(AppError::refused)?
        {
            self.approve(ApprovalKind::Use, "move this protected code on", gate)?;
        }
        self.exposure_gate(id, "move this code on", gate)?;
        self.session()?
            .advance_code(id, field_id)
            .map_err(AppError::refused)
    }

    /// Changes an item through the generic edit, after the `Use` gate when
    /// the item is a crown jewel: replacing or removing a recovery phrase is
    /// as weighty as copying it (D110).
    fn edit_item(
        &mut self,
        id: &str,
        title: Option<&str>,
        changes: Vec<lockdown_vault::FieldChange>,
        gate: &GateInput,
    ) -> AppResult<u64> {
        self.clock.touch(Instant::now());
        if self
            .session()?
            .is_crown_jewel(id)
            .map_err(AppError::refused)?
        {
            self.approve(ApprovalKind::Use, "change this protected item", gate)?;
        }
        self.session()?
            .edit_item(id, title, changes)
            .map_err(AppError::refused)
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
        self.recent_fills.clear();
        self.depth.forget();
        self.security_forget();
        self.bridges.forget();
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

#[cfg(not(target_os = "wasi"))]
type Job = Box<dyn FnOnce(&mut Core) + Send>;

/// The app, as a shell holds it. Clone it freely; every clone talks to the
/// same session on the same thread.
#[cfg(not(target_os = "wasi"))]
#[derive(Clone)]
pub struct App {
    tx: Sender<Job>,
}

/// The app in a build with no threads (the web vault, compiled for
/// `wasm32-wasip1` and run in a dedicated worker): the session lives
/// beside the caller and each call runs in place, one at a time. The
/// timers run when the host calls [`App::tick`] (the worker does every
/// second), as the worker thread's loop does elsewhere.
#[cfg(target_os = "wasi")]
#[derive(Clone)]
pub struct App {
    core: std::rc::Rc<std::cell::RefCell<Core>>,
}

#[cfg(target_os = "wasi")]
impl App {
    /// Makes the session's home; nothing is opened yet, as on every build.
    pub fn start(paths: Paths, persona: Option<String>, policy: LockPolicy) -> Self {
        Self {
            core: std::rc::Rc::new(std::cell::RefCell::new(Core::new(paths, persona, policy))),
        }
    }

    /// Runs the lock timers and a due backup: what the worker thread does
    /// between calls elsewhere. Returns whether the vault is locked now.
    pub fn tick(&self) -> bool {
        let mut core = self.core.borrow_mut();
        core.enforce();
        core.backup_tick();
        core.session.is_none()
    }

    /// Runs `f` in place, after the timers (so a call that arrives after
    /// the idle timeout finds the vault locked, as on the worker thread).
    fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Core) -> AppResult<T> + Send + 'static,
    {
        let Ok(mut core) = self.core.try_borrow_mut() else {
            return Err(AppError {
                code: ErrorCode::Stopped,
                message: "another call is still running".into(),
                detail: None,
            });
        };
        core.enforce();
        f(&mut core)
    }
}

#[cfg(not(target_os = "wasi"))]
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
                        Err(RecvTimeoutError::Timeout) => {
                            core.enforce();
                            core.backup_tick();
                        }
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
            detail: None,
        };
        self.tx.send(job).map_err(|_| stopped())?;
        reply_rx.recv().map_err(|_| stopped())?
    }
}

impl App {
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

    // --- settings and approvals ----------------------------------------------------

    /// The settings as last read or written (`settings.cbor`, D100).
    pub fn settings(&self) -> AppResult<Settings> {
        self.call(|core| Ok(core.settings.clone()))
    }

    /// Writes the settings (clamped into range) and applies their lock
    /// timers from now on. Needs no open vault.
    pub fn set_settings(&self, settings: Settings) -> AppResult<Settings> {
        self.call(move |core| {
            let mut settings = settings.clamped();
            // Backups change only through the backup calls (behind the gate).
            settings.backup = core.settings.backup.clone();
            // Alias monitoring changes only through its own call, which
            // tells the server (docs/plan/23-bridges.md §9).
            settings.alias_monitoring = core.settings.alias_monitoring;
            settings
                .save(&core.paths.settings_file())
                .map_err(AppError::refused)?;
            core.policy = settings.lock_policy();
            core.settings = settings;
            core.enforce();
            Ok(core.settings.clone())
        })
    }

    /// Installs the platform's own check (Touch ID, Windows Hello). Call
    /// once at startup; without it, approvals use the password (D101).
    pub fn set_approver(&self, approver: Box<dyn Approver + Send + Sync>) -> AppResult<()> {
        self.call(move |core| {
            core.approver = Some(approver);
            Ok(())
        })
    }

    /// Which check `kind` would go to right now, and its name, so a shell
    /// can show the right prompt before calling: `platform` with the
    /// method's name, `password`, or `window`.
    pub fn approval_route(&self, kind: ApprovalKind) -> AppResult<(Route, String)> {
        self.call(move |core| {
            let has_account = core.session.as_ref().is_some_and(Session::has_account);
            let route = approval::route(kind, core.approver.is_some(), has_account);
            let method = match route {
                Route::Platform => core
                    .approver
                    .as_ref()
                    .map(|a| a.method().to_string())
                    .unwrap_or_default(),
                Route::Password => "password".into(),
                Route::Window => "window".into(),
            };
            Ok((route, method))
        })
    }

    /// Runs the gate for `kind` on its own (an approval with no item
    /// behind it, such as approving another device later).
    pub fn approve(
        &self,
        kind: ApprovalKind,
        reason: &str,
        gate: GateInput,
    ) -> AppResult<Approval> {
        let reason = reason.to_string();
        self.call(move |core| core.approve(kind, &reason, &gate))
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

    /// One hidden field's bytes, to show. A crown jewel's go through the
    /// `Reveal` gate first (the password where one exists).
    pub fn reveal(&self, id: &str, field_id: &str, gate: GateInput) -> AppResult<Revealed> {
        let (id, field_id) = (id.to_string(), field_id.to_string());
        self.call(move |core| core.reveal(&id, &field_id, ApprovalKind::Reveal, &gate))
    }

    /// One hidden field's bytes, to copy or fill without showing. A crown
    /// jewel's go through the `Use` gate first (the platform's check where
    /// one exists).
    pub fn reveal_to_use(&self, id: &str, field_id: &str, gate: GateInput) -> AppResult<Revealed> {
        let (id, field_id) = (id.to_string(), field_id.to_string());
        self.call(move |core| core.reveal(&id, &field_id, ApprovalKind::Use, &gate))
    }

    /// Every one-time code at this moment: current and next value and the
    /// seconds left, never the set-up (D102).
    pub fn codes(&self) -> AppResult<Vec<CodeView>> {
        self.call(|core| {
            let now = core
                .bridges
                .code_time(session::now_secs(), core.settings.code_clock_correction);
            core.session()?.codes(now).map_err(AppError::refused)
        })
    }

    /// The code valid now for one code field (with `next`, the one after
    /// it), as bytes, for a shell's clipboard path (the same timer as
    /// `copy_secret`). A crown jewel's goes through the `Use` gate first.
    pub fn code_now(
        &self,
        id: &str,
        field_id: &str,
        next: bool,
        gate: GateInput,
    ) -> AppResult<Revealed> {
        let (id, field_id) = (id.to_string(), field_id.to_string());
        self.call(move |core| core.code_now(&id, &field_id, next, &gate))
    }

    /// Moves a counter-based (HOTP) code on by one, for the display's
    /// "Next code": the counter is saved in a new item version, so it syncs,
    /// and the new code comes back with its counter. A time-based code is
    /// refused. A crown jewel's goes through the `Use` gate first.
    pub fn advance_code(
        &self,
        id: &str,
        field_id: &str,
        gate: GateInput,
    ) -> AppResult<CodeAdvanced> {
        let (id, field_id) = (id.to_string(), field_id.to_string());
        self.call(move |core| core.advance_code(&id, &field_id, &gate))
    }

    /// Adds a code from its URI, inside `into` (a Login) or as its own item.
    pub fn add_code(
        &self,
        uri: &str,
        title: Option<&str>,
        into: Option<&str>,
    ) -> AppResult<String> {
        let uri = zeroize::Zeroizing::new(uri.to_string());
        let (title, into) = (title.map(str::to_string), into.map(str::to_string));
        self.with_session(move |s| {
            s.add_code(&uri, title.as_deref(), into.as_deref())
                .map(|id| id.to_string())
        })
    }

    /// Every persona of the identity, the open one marked.
    pub fn personas(&self) -> AppResult<Vec<PersonaInfo>> {
        self.with_session(|s| Ok(s.personas()))
    }

    /// Reads a CXF or CSV file's bytes into the vault (D103). A hostile or
    /// unreadable file is refused with the parser's reason.
    pub fn import_items(&self, bytes: Vec<u8>, format: &str) -> AppResult<ImportReport> {
        let format = format.to_string();
        let bytes = zeroize::Zeroizing::new(bytes);
        self.with_session(move |s| s.import(&bytes, &format))
    }

    /// The live vault as one CXF or CSV file, behind the `Reveal` gate
    /// (the password where one exists; D103). The bytes cross once.
    pub fn export_items(&self, format: &str, gate: GateInput) -> AppResult<Revealed> {
        let format = format.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.approve(ApprovalKind::Reveal, "export the vault", &gate)?;
            let bytes = core
                .session()?
                .export(&format, session::now_secs().max(0) as u64)
                .map_err(AppError::refused)?;
            Ok(Revealed::new(bytes))
        })
    }

    /// `lockdown audit`, for "Check now" on the Status screen: the server's
    /// log verified from the start, in the audit's own words.
    pub fn audit(&self) -> AppResult<Vec<String>> {
        self.with_session(|s| s.audit())
    }

    /// Every kind's template, for a shell's "New item" list, editor and
    /// detail. Static data: no vault needed.
    pub fn templates(&self) -> Vec<TemplateView> {
        session::items::templates()
    }

    /// How hard `candidate`, a password the person is typing in an editor,
    /// would be to guess: zxcvbn's 0–4 score ([`strength::score`]), `None`
    /// when empty. `context` is the same form's plain words (title,
    /// username, website). Static: it reads nothing from the vault and is
    /// never given a stored secret.
    pub fn password_strength(&self, candidate: &[u8], context: &[&str]) -> AppResult<Option<u8>> {
        strength::score(candidate, context).map_err(AppError::refused)
    }

    /// The kinds a person may give a field they add by hand.
    pub fn custom_kinds(&self) -> Vec<CustomKindView> {
        session::items::custom_kinds()
    }

    /// Makes an item of `kind` from a shell's fields (D110): checked
    /// against the kind's template; secrets arrive as bytes.
    pub fn add_item(
        &self,
        kind: &str,
        title: Option<&str>,
        crown_jewel: bool,
        changes: Vec<lockdown_vault::FieldChange>,
    ) -> AppResult<String> {
        let (kind, title) = (kind.to_string(), title.map(str::to_string));
        self.with_session(move |s| {
            s.add_item(&kind, title.as_deref(), crown_jewel, changes)
                .map(|id| id.to_string())
        })
    }

    /// Saves a new version of an item from an edit that names every field
    /// (Keep for a secret the person did not retype). A crown jewel's edit
    /// goes through the `Use` gate first. Returns the version number.
    pub fn edit_item(
        &self,
        id: &str,
        title: Option<&str>,
        changes: Vec<lockdown_vault::FieldChange>,
        gate: GateInput,
    ) -> AppResult<u64> {
        let (id, title) = (id.to_string(), title.map(str::to_string));
        self.call(move |core| core.edit_item(&id, title.as_deref(), changes, &gate))
    }

    /// Adds a login. `password` is bytes a shell wipes afterwards. The
    /// narrow form of [`Self::add_item`].
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
    /// given, keeping the rest; returns the new version number. The narrow
    /// form of [`Self::edit_item`], gated the same way.
    pub fn edit_login(
        &self,
        id: &str,
        title: Option<&str>,
        username: Option<&str>,
        password: Option<Vec<u8>>,
        gate: GateInput,
    ) -> AppResult<u64> {
        let (id, title, username) = (
            id.to_string(),
            title.map(str::to_string),
            username.map(str::to_string),
        );
        let password = password.map(zeroize::Zeroizing::new);
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let session = core.session()?;
            let item_id = lockdown_vault::Id::parse(&id)
                .map_err(|_| AppError::refused(format!("'{id}' isn't an item id")))?;
            let mut set = Vec::new();
            if let Some(u) = username.as_deref() {
                set.push(lockdown_vault::FieldChange::text("user", u));
            }
            if let Some(p) = password.as_deref() {
                set.push(lockdown_vault::FieldChange::secret("pw", p));
            }
            if set.is_empty() && title.is_none() {
                return Err(AppError::refused("nothing to change"));
            }
            let changes = session
                .keep_all_but(item_id, set)
                .map_err(AppError::refused)?;
            core.edit_item(&id, title.as_deref(), changes, &gate)
        })
    }

    /// Moves an item to the trash.
    pub fn trash(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.trash(&id))
    }

    /// Brings a trashed item back (the Deleted view's Restore).
    pub fn restore(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.restore(&id))
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
        if !self.probe()?.identity_present {
            // A new device: the account may want a security key (`devices`).
            return self.enrol_new_device(&server, &login, &password, secret_key.as_deref());
        }
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let mut lines = core
                .session()?
                .account_login(&server, &login, &password, secret_key.as_deref())
                .map_err(AppError::refused)?;
            reminders::note_password(&core.paths);
            // A stronger device raises the account's stretching (D49's
            // renegotiation, `client-kdf`); a failure leaves the login as is.
            match core.session()?.strengthen_account(&password) {
                Ok(Some(line)) => lines.push(line),
                Ok(None) => {}
                Err(e) => lines.push(format!("the password stretching wasn't raised: {e}")),
            }
            Ok(lines)
        })
    }

    /// [`App::account_login`] on an empty device after a sign-in the shell
    /// ran itself with this device's keystore (the web vault, which must
    /// see the export key to seal its device key, docs/plan/17-web-vault.md):
    /// enrols this device from the account with no second password round.
    pub fn account_login_signed_in(
        &self,
        server: &str,
        login: &str,
        export_key: ecosystem_identity::ExportKey,
        session: lockdown_crypto::auth::Session,
        secret_key: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let (server, login, secret_key) = (
            server.to_string(),
            login.to_string(),
            secret_key.map(|s| zeroize::Zeroizing::new(s.to_string())),
        );
        self.call(move |core| {
            session::enrol_from_sign_in(
                &core.paths,
                &server,
                &login,
                export_key,
                session,
                secret_key.as_deref().map(String::as_str),
            )
            .map_err(AppError::refused)
        })
    }

    /// Takes a fresh session token from a sign-in the shell ran itself for
    /// the open account (the web vault's unlock).
    pub fn renew_session(
        &self,
        server: &str,
        login: &str,
        session: lockdown_crypto::auth::Session,
    ) -> AppResult<()> {
        let (server, login) = (server.to_string(), login.to_string());
        self.with_session(move |s| s.renew_session(&server, &login, session))
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
        // The same wait for a security key as an enrolment (`devices`).
        self.recover_new_device(server, login, password, secret_key, recovery_code)
    }

    /// Key transparency, then pull, merge, push, head, then key
    /// transparency again (as `lockdown sync` does). A log the server
    /// can't prove honest comes back as [`ErrorCode::Refused`] with the
    /// audit's own words, before anything is written.
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

    /// Revokes another device (D73), through the `Approve` gate first (the
    /// brief's "revoke needs a fresh check").
    pub fn device_revoke(&self, selector: &str, gate: GateInput) -> AppResult<Vec<String>> {
        let selector = selector.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            core.approve(ApprovalKind::Approve, "revoke that device", &gate)?;
            core.session()?
                .device_revoke(&selector)
                .map_err(AppError::refused)
        })
    }

    // --- organization: vaults, folders, tags, favourites, archive, moves ------

    /// Every vault of the open persona, the first one first, with its
    /// folders (`session::organize`).
    pub fn vaults(&self) -> AppResult<Vec<VaultView>> {
        self.with_session(|s| s.vault_views())
    }

    /// Makes another vault (its own Vault Key) and names it.
    pub fn create_vault(&self, name: &str) -> AppResult<String> {
        let name = name.to_string();
        self.with_session(move |s| s.create_vault(&name).map(|id| id.to_string()))
    }

    /// Names a vault.
    pub fn rename_vault(&self, vault_id: &str, name: &str) -> AppResult<()> {
        let (vault_id, name) = (vault_id.to_string(), name.to_string());
        self.with_session(move |s| s.rename_vault(&vault_id, &name))
    }

    /// How many days a vault keeps trashed items before they purge.
    pub fn set_trash_days(&self, vault_id: &str, days: u32) -> AppResult<()> {
        let vault_id = vault_id.to_string();
        self.with_session(move |s| s.set_trash_days(&vault_id, days))
    }

    /// Adds a folder to a vault; returns its id.
    pub fn add_folder(
        &self,
        vault_id: &str,
        name: &str,
        parent: Option<&str>,
    ) -> AppResult<String> {
        let (vault_id, name, parent) = (
            vault_id.to_string(),
            name.to_string(),
            parent.map(str::to_string),
        );
        self.with_session(move |s| {
            s.add_folder(&vault_id, &name, parent.as_deref())
                .map(|id| id.to_string())
        })
    }

    /// Renames a folder, or moves it inside another.
    pub fn change_folder(
        &self,
        vault_id: &str,
        folder: &str,
        name: &str,
        parent: Option<&str>,
    ) -> AppResult<()> {
        let (vault_id, folder, name, parent) = (
            vault_id.to_string(),
            folder.to_string(),
            name.to_string(),
            parent.map(str::to_string),
        );
        self.with_session(move |s| s.change_folder(&vault_id, &folder, &name, parent.as_deref()))
    }

    /// Removes a folder; what was in it moves up. Returns how many items moved.
    pub fn remove_folder(&self, vault_id: &str, folder: &str) -> AppResult<usize> {
        let (vault_id, folder) = (vault_id.to_string(), folder.to_string());
        self.with_session(move |s| s.remove_folder(&vault_id, &folder))
    }

    /// Files, tags or favourites an item; `None` leaves that part alone and
    /// `folder: Some(None)` takes it out of its folder.
    pub fn organize(
        &self,
        id: &str,
        folder: Option<Option<String>>,
        tags: Option<Vec<String>>,
        favourite: Option<bool>,
    ) -> AppResult<u64> {
        let id = id.to_string();
        self.with_session(move |s| {
            s.organize(&id, folder.as_ref().map(|f| f.as_deref()), tags, favourite)
        })
    }

    /// Renames a tag everywhere (`to: None` takes it off). Returns how many items changed.
    pub fn rename_tag(&self, from: &str, to: Option<&str>) -> AppResult<usize> {
        let (from, to) = (from.to_string(), to.map(str::to_string));
        self.with_session(move |s| s.rename_tag(&from, to.as_deref()))
    }

    /// Archives an item.
    pub fn archive(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.archive(&id))
    }

    /// Brings an archived item back.
    pub fn unarchive(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.unarchive(&id))
    }

    /// Moves an item to another vault of the persona (sealed again there;
    /// the old one purged). A crown jewel's move goes through the `Use`
    /// gate. Returns the item's new id.
    pub fn move_item(&self, id: &str, vault_id: &str, gate: GateInput) -> AppResult<String> {
        let (id, vault_id) = (id.to_string(), vault_id.to_string());
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "move this protected item", &gate)?;
            core.session()?
                .move_item(&id, &vault_id)
                .map(|n| n.to_string())
                .map_err(AppError::refused)
        })
    }

    /// Puts an item into the crown-jewel class, or (through the `Reveal`
    /// gate: the account password where one exists) takes it out; a kind
    /// that is always a crown jewel can't leave. Returns the new id.
    pub fn set_crown_jewel(&self, id: &str, on: bool, gate: GateInput) -> AppResult<String> {
        let id = id.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            if !on {
                core.approve(
                    ApprovalKind::Reveal,
                    "take this item out of the crown-jewel class",
                    &gate,
                )?;
            }
            core.session()?
                .set_crown_jewel(&id, on)
                .map(|n| n.to_string())
                .map_err(AppError::refused)
        })
    }

    /// Deletes a trashed item for good now. A crown jewel's goes through
    /// the `Use` gate.
    pub fn purge(&self, id: &str, gate: GateInput) -> AppResult<()> {
        let id = id.to_string();
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "delete this protected item", &gate)?;
            core.session()?.purge(&id).map_err(AppError::refused)
        })
    }

    /// Purges what the trash's grace period has let go of, now.
    pub fn sweep_trash(&self) -> AppResult<usize> {
        self.with_session(|s| s.sweep_trash())
    }

    /// Settles a conflict by keeping `keep` and trashing `other` (D58, D108).
    pub fn resolve_conflict(&self, keep: &str, other: &str) -> AppResult<()> {
        let (keep, other) = (keep.to_string(), other.to_string());
        self.with_session(move |s| s.resolve_conflict(&keep, &other))
    }

    /// Items that look like the same thing twice.
    pub fn duplicates(&self) -> AppResult<Vec<DuplicateGroup>> {
        self.with_session(|s| s.duplicates())
    }

    /// Folds duplicate Logins into the one kept, then trashes them. Any
    /// crown jewel among them goes through the `Use` gate first.
    pub fn merge_duplicates(
        &self,
        keep: &str,
        others: Vec<String>,
        gate: GateInput,
    ) -> AppResult<u64> {
        let keep = keep.to_string();
        self.call(move |core| {
            core.clock.touch(Instant::now());
            let session = core.session()?;
            let mut crown = session.is_crown_jewel(&keep).map_err(AppError::refused)?;
            for o in &others {
                crown |= session.is_crown_jewel(o).map_err(AppError::refused)?;
            }
            if crown {
                core.approve(ApprovalKind::Use, "merge protected items", &gate)?;
            }
            core.session()?
                .merge_duplicates(&keep, &others)
                .map_err(AppError::refused)
        })
    }

    /// Every version of an item, newest first.
    pub fn history(&self, id: &str) -> AppResult<Vec<VersionView>> {
        let id = id.to_string();
        self.with_session(move |s| s.history(&id))
    }

    /// An earlier version, masked.
    pub fn show_version(&self, id: &str, n: u64) -> AppResult<ItemDetail> {
        let id = id.to_string();
        self.with_session(move |s| s.show_version(&id, n))
    }

    /// Copies an earlier version into a new one. A crown jewel's goes
    /// through the `Use` gate, as an edit does.
    pub fn restore_version(&self, id: &str, n: u64, gate: GateInput) -> AppResult<u64> {
        let id = id.to_string();
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            core.session()?
                .restore_version(&id, n)
                .map_err(AppError::refused)
        })
    }

    /// Attaches a file. To a crown jewel, through the `Use` gate.
    pub fn attach(
        &self,
        id: &str,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
        gate: GateInput,
    ) -> AppResult<String> {
        let (id, name, mime) = (id.to_string(), name.to_string(), mime.to_string());
        let bytes = zeroize::Zeroizing::new(bytes);
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            core.session()?
                .attach(&id, &name, &mime, &bytes)
                .map(|a| a.to_string())
                .map_err(AppError::refused)
        })
    }

    /// An attached file's bytes, for the person to save. A crown jewel's
    /// go through the `Reveal` gate, like showing one of its secrets.
    pub fn attachment(&self, id: &str, att_id: &str, gate: GateInput) -> AppResult<Revealed> {
        let (id, att_id) = (id.to_string(), att_id.to_string());
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Reveal, "open this protected file", &gate)?;
            let bytes = core
                .session()?
                .attachment_bytes(&id, &att_id)
                .map_err(AppError::refused)?;
            core.note_use(&id);
            Ok(bytes)
        })
    }

    /// Takes a file off an item. From a crown jewel, through the `Use` gate.
    pub fn detach(&self, id: &str, att_id: &str, gate: GateInput) -> AppResult<u64> {
        let (id, att_id) = (id.to_string(), att_id.to_string());
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            core.session()?
                .detach(&id, &att_id)
                .map_err(AppError::refused)
        })
    }

    /// Notes a deliberate use this device made of an item outside the core
    /// (its site opened in the browser), for the ranking.
    pub fn note_use(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| {
            let item =
                lockdown_vault::Id::parse(&id).map_err(|_| format!("'{id}' isn't an item id"))?;
            s.note_use(item)
        })
    }

    /// The items due to lapse within their kind's lead, soonest first.
    pub fn expiry_reminders(&self) -> AppResult<Vec<ExpiryReminder>> {
        self.with_session(|s| s.expiry_reminders())
    }

    /// [`Self::add_item`] into the vault `vault_id`.
    pub fn add_item_in(
        &self,
        vault_id: &str,
        kind: &str,
        title: Option<&str>,
        crown_jewel: bool,
        changes: Vec<lockdown_vault::FieldChange>,
    ) -> AppResult<String> {
        let (vault_id, kind, title) = (
            vault_id.to_string(),
            kind.to_string(),
            title.map(str::to_string),
        );
        self.with_session(move |s| {
            let vault = lockdown_vault::Id::parse(&vault_id)
                .map_err(|_| format!("'{vault_id}' isn't a vault id"))?;
            s.add_item_in(vault, &kind, title.as_deref(), crown_jewel, changes)
                .map(|id| id.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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

        let one = app.reveal(&id, "pw", GateInput::default()).unwrap();
        assert_eq!(one.expose(), b"correct horse battery staple");
        assert!(app.reveal(&id, "user", GateInput::default()).is_err());
        // A standard item needs no gate to copy either.
        assert_eq!(
            app.reveal_to_use(&id, "pw", GateInput::default())
                .unwrap()
                .expose(),
            b"correct horse battery staple"
        );

        app.edit_login(
            &id,
            Some("Example"),
            None,
            Some(b"hunter2".to_vec()),
            GateInput::default(),
        )
        .unwrap();
        assert_eq!(
            app.reveal(&id, "pw", GateInput::default())
                .unwrap()
                .expose(),
            b"hunter2"
        );
        let rows = app.list().unwrap();
        assert_eq!(rows[0].title, "Example");
        assert_eq!(rows[0].methods, ["password"]);
        assert!(
            rows[0].views.is_empty(),
            "a plain login is in no smart view"
        );

        app.trash(&id).unwrap();
        let rows = app.list().unwrap();
        assert_eq!(rows.len(), 1, "a trashed item still lists, as deleted");
        assert_eq!(rows[0].state, "trashed");
        assert_eq!(rows[0].views, ["deleted"]);
        let json = serde_json::to_string(&rows).unwrap();
        assert!(!json.contains("hunter2"));

        // Restore brings it back as it was, out of Deleted.
        app.restore(&id).unwrap();
        let rows = app.list().unwrap();
        assert_eq!(rows[0].state, "live");
        assert!(rows[0].views.is_empty());
        assert_eq!(app.show(&id, false).unwrap().state, "live");
        assert_eq!(
            app.reveal(&id, "pw", GateInput::default())
                .unwrap()
                .expose(),
            b"hunter2"
        );
        let err = app.restore(&id).unwrap_err();
        assert_eq!(err.code, ErrorCode::Refused);
        assert!(err.message.contains("isn't in the trash"), "{err}");
    }

    #[test]
    fn every_kind_goes_through_one_path_and_secrets_never_cross_as_json() {
        use lockdown_vault::{Change, FieldChange, FieldInput, FieldKind};
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        let templates = app.templates();
        assert!(templates.iter().any(|t| t.kind == "wi-fi" && t.creatable));
        assert!(templates.iter().all(|t| t.kind != "consent"));

        // Wi-Fi: a generated password the shell never sent or saw.
        let wifi = app
            .add_item(
                "wi-fi",
                None,
                false,
                vec![
                    FieldChange::text("wifi.ssid", "home-net"),
                    FieldChange::new("wifi.passphrase", Change::Generate),
                    FieldChange::text("wifi.security", "WPA3"),
                    FieldChange::new("wifi.hidden", Change::Set(FieldInput::Flag(false))),
                ],
            )
            .unwrap();
        let generated = app
            .reveal(&wifi, "wifi.passphrase", GateInput::default())
            .unwrap();
        assert_eq!(generated.expose().len(), 20);
        let generated = String::from_utf8(generated.expose().to_vec()).unwrap();
        let detail = app.show(&wifi, false).unwrap();
        assert_eq!(
            detail.fields[0].value.as_deref(),
            Some("home-net"),
            "the network names an empty title"
        );
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains(&generated));

        // A card: the number masked, the brand worked out, the quick copy the number.
        let card = app
            .add_item(
                "card",
                Some("Everyday card"),
                false,
                vec![
                    FieldChange::secret("card.number", b"4111 1111 1111 1111"),
                    FieldChange::text("card.holder", "A. Person"),
                    FieldChange::secret("card.cvc", b"123"),
                    FieldChange::text("card.expiry", "2027-09"),
                ],
            )
            .unwrap();
        // A code from a bare set-up key, the core makes the link.
        let code = app
            .add_item(
                "code",
                Some("Forum"),
                false,
                vec![FieldChange::secret(
                    "code.0",
                    b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
                )],
            )
            .unwrap();
        assert!(
            app.codes()
                .unwrap()
                .iter()
                .any(|c| c.item_id == code && c.issuer.as_deref() == Some("Forum"))
        );
        // A wallet: a crown jewel by default.
        let phrase =
            "abandon ability able about above absent absorb abstract absurd abuse access accident";
        let wallet = app
            .add_item(
                "crypto-wallet",
                Some("Cold wallet"),
                false,
                vec![
                    FieldChange::secret("seed", phrase.as_bytes()),
                    FieldChange::text("wallet.network", "Bitcoin"),
                ],
            )
            .unwrap();

        let rows = app.list().unwrap();
        let row = |id: &str| rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row(&wifi).subtitle, "home-net");
        assert_eq!(row(&wifi).views, ["wi-fi"]);
        assert_eq!(row(&wifi).copy.as_ref().unwrap().label, "Wi-Fi password");
        assert_eq!(row(&card).subtitle, "A. Person");
        assert_eq!(row(&card).copy.as_ref().unwrap().field_id, "card.number");
        assert_eq!(row(&code).copy.as_ref().unwrap().how, "code");
        let w = row(&wallet);
        assert!(w.crown_jewel);
        assert!(
            w.subtitle.is_empty() && w.username.is_empty() && w.site.is_empty(),
            "title only"
        );
        let json = serde_json::to_string(&rows).unwrap();
        // The number with its space: a random item id can hold "4111". The
        // CVC as a whole JSON string: a random id can also end in "123".
        for secret in [
            "4111 1111",
            "4111111111111111",
            "\"123\"",
            phrase,
            generated.as_str(),
            "GEZDGNBV",
        ] {
            assert!(!json.contains(secret), "{secret} in the list");
        }
        let detail = app.show(&card, false).unwrap();
        let brand = detail
            .fields
            .iter()
            .find(|f| f.field_id == "card.type")
            .unwrap();
        assert_eq!(brand.value.as_deref(), Some("Visa"));
        assert!(
            !serde_json::to_string(&detail)
                .unwrap()
                .contains("4111 1111")
        );

        // An edit: untouched secrets kept without being sent; a field of your own added.
        let n = app
            .edit_item(
                &card,
                None,
                vec![
                    FieldChange::keep("card.number"),
                    FieldChange::keep("card.holder"),
                    FieldChange::new("card.cvc", Change::Remove),
                    FieldChange::text("card.expiry", "2028-01"),
                    FieldChange::keep("card.type"),
                    FieldChange::custom(
                        "Bank phone",
                        FieldKind::Phone,
                        FieldInput::Text("+1 555 0100".into()),
                    ),
                ],
                GateInput::default(),
            )
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            app.reveal(&card, "card.number", GateInput::default())
                .unwrap()
                .expose(),
            b"4111 1111 1111 1111"
        );
        let detail = app.show(&card, false).unwrap();
        assert!(detail.fields.iter().all(|f| f.field_id != "card.cvc"));
        assert!(
            detail
                .fields
                .iter()
                .any(|f| f.field_id == "custom.0" && f.label == "Bank phone")
        );
        let forgot = app
            .edit_item(
                &card,
                None,
                vec![FieldChange::keep("card.number")],
                GateInput::default(),
            )
            .unwrap_err();
        assert!(
            forgot.message.contains("doesn't say what to do"),
            "{forgot}"
        );

        // Changing a crown jewel asks first; here (no platform check, no account) in the window.
        let keep_all = vec![
            FieldChange::keep("seed"),
            FieldChange::text("wallet.network", "Ethereum"),
        ];
        let err = app
            .edit_item(&wallet, None, keep_all.clone(), GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ApprovalNeeded);
        let confirmed = GateInput {
            password: None,
            confirmed: true,
        };
        assert_eq!(
            app.edit_item(&wallet, None, keep_all, confirmed.clone())
                .unwrap(),
            2
        );
        assert_eq!(
            app.reveal(&wallet, "seed", confirmed).unwrap().expose(),
            phrase.as_bytes(),
            "the phrase was kept, never resent"
        );

        // Kinds a person can't make by hand, and a kind that doesn't exist.
        assert!(app.add_item("passkey", Some("x"), false, vec![]).is_err());
        assert!(app.add_item("consent", Some("x"), false, vec![]).is_err());
        assert!(app.add_item("spaceship", Some("x"), false, vec![]).is_err());
        // A secret sent as text is refused.
        let err = app
            .add_item(
                "note",
                Some("x"),
                false,
                vec![
                    FieldChange::text("body", "fine"),
                    FieldChange::custom("pin", FieldKind::Hidden, FieldInput::Text("1".into())),
                ],
            )
            .unwrap_err();
        assert!(err.message.contains("must arrive as bytes"), "{err}");
    }

    #[test]
    fn every_kind_survives_an_export_and_an_import_and_a_wallet_stays_a_crown_jewel() {
        use lockdown_vault::{Change, FieldChange, FieldInput};
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        let confirmed = GateInput {
            password: None,
            confirmed: true,
        };
        app.add_item(
            "wi-fi",
            Some("Home"),
            false,
            vec![
                FieldChange::text("wifi.ssid", "home-net"),
                FieldChange::secret("wifi.passphrase", b"k7Hq3mWx9rTa4nPe6uYc"),
                FieldChange::new("wifi.hidden", Change::Set(FieldInput::Flag(true))),
            ],
        )
        .unwrap();
        let phrase =
            "abandon ability able about above absent absorb abstract absurd abuse access accident";
        let wallet = app
            .add_item(
                "crypto-wallet",
                Some("Cold wallet"),
                false,
                vec![FieldChange::secret("seed", phrase.as_bytes())],
            )
            .unwrap();
        // A crown jewel leaves only once the person opted it in (D121).
        app.set_export_opt_in(&wallet, true, confirmed.clone())
            .unwrap();
        let file = app.export_items("cxf", confirmed.clone()).unwrap();

        let (_other, again) = fresh();
        again.open_or_create(true).unwrap();
        let report = again.import_items(file.expose().to_vec(), "cxf").unwrap();
        assert_eq!(report.imported, 2);
        assert!(report.notes.is_empty(), "{:?}", report.notes);
        let rows = again.list().unwrap();
        let wifi = rows
            .iter()
            .find(|r| r.kind == "wi-fi")
            .expect("the kind came back");
        assert_eq!(wifi.subtitle, "home-net");
        assert_eq!(
            again
                .reveal(&wifi.id, "wifi.passphrase", GateInput::default())
                .unwrap()
                .expose(),
            b"k7Hq3mWx9rTa4nPe6uYc"
        );
        let hidden = again.show(&wifi.id, false).unwrap();
        assert!(
            hidden
                .fields
                .iter()
                .any(|f| f.field_id == "wifi.hidden" && f.value.as_deref() == Some("true"))
        );
        let wallet = rows.iter().find(|r| r.kind == "crypto-wallet").unwrap();
        assert!(
            wallet.crown_jewel,
            "the phrase lands in the class on import too"
        );
        assert_eq!(
            again
                .reveal(&wallet.id, "seed", confirmed)
                .unwrap()
                .expose(),
            phrase.as_bytes()
        );
    }

    #[test]
    fn settings_persist_beside_the_data_and_drive_the_lock_timers() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths.clone(), None, LockPolicy::default());
        let defaults = app.settings().unwrap();
        assert_eq!(defaults, Settings::default());
        let mut changed = defaults.clone();
        changed.density = Density::Compact;
        changed.reveal = RevealGesture::Toggle;
        changed.idle_lock_secs = Some(5);
        let saved = app.set_settings(changed).unwrap();
        assert_eq!(saved.idle_lock_secs, Some(60), "clamped to the floor");
        assert_eq!(
            app.policy().unwrap().idle,
            Some(Duration::from_secs(60)),
            "the lock timer follows the setting"
        );
        assert!(paths.settings_file().ends_with("lockdown/settings.cbor"));
        drop(app);
        // A second start reads the file back and uses its timers.
        let again = App::start(paths, None, LockPolicy::default());
        assert_eq!(again.settings().unwrap().density, Density::Compact);
        assert_eq!(again.settings().unwrap().reveal, RevealGesture::Toggle);
        assert_eq!(again.policy().unwrap().idle, Some(Duration::from_secs(60)));
    }

    #[test]
    fn codes_come_as_values_and_timing_never_the_seed() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let uri = format!("otpauth://totp/Example:alice?secret={SEED}&digits=8&period=30");
        let code_id = app.add_code(&uri, None, None).unwrap();
        let login = app
            .add_login("https://example.com", "me", b"pw".to_vec(), None)
            .unwrap();
        let steam = format!("otpauth://totp/Steam:me?secret={SEED}&issuer=Steam");
        app.add_code(&steam, None, Some(&login)).unwrap();
        assert!(app.add_code("otpauth://totp/x", None, None).is_err());

        let codes = app.codes().unwrap();
        assert_eq!(codes.len(), 2);
        let by_item = |id: &str| codes.iter().find(|c| c.item_id == id).unwrap();
        let standalone = by_item(&code_id);
        assert_eq!(standalone.title, "Example (alice)");
        assert_eq!(standalone.digits, 8);
        assert_eq!(standalone.current.len(), 8);
        assert_eq!(standalone.next.as_ref().unwrap().len(), 8);
        assert!(standalone.seconds_left.unwrap() <= 30);
        let embedded = by_item(&login);
        assert_eq!(embedded.field_id, "code.0");
        assert_eq!(embedded.issuer.as_deref(), Some("Steam"));
        assert_eq!(embedded.variant, "steam", "the codes crate spots Steam");
        assert_eq!(embedded.digits, 5);
        assert_eq!(embedded.current.len(), 5);

        let now = app
            .code_now(&code_id, "code.0", false, GateInput::default())
            .unwrap();
        assert_eq!(now.expose(), standalone.current.as_bytes());
        // The next code, for the display's last seconds (D102).
        let next = app
            .code_now(&code_id, "code.0", true, GateInput::default())
            .unwrap();
        assert_eq!(next.expose(), standalone.next.as_ref().unwrap().as_bytes());
        assert!(
            app.code_now(&code_id, "title", false, GateInput::default())
                .is_err()
        );

        let json = serde_json::to_string(&codes).unwrap();
        assert!(!json.contains(SEED), "the seed never crosses");
        assert!(!json.contains("secret="));
        assert!(!json.contains("otpauth"));

        let rows = app.list().unwrap();
        let login_row = rows.iter().find(|r| r.id == login).unwrap();
        assert_eq!(login_row.codes, 1);
        assert_eq!(login_row.methods, ["password", "code"]);
        assert_eq!(login_row.views, ["codes"]);
        let code_row = rows.iter().find(|r| r.id == code_id).unwrap();
        assert_eq!(code_row.kind, "code");
        assert_eq!(code_row.views, ["codes"]);
        // The code's own set-up is a masked field in the detail, not a value.
        let detail = app.show(&code_id, false).unwrap();
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains(SEED), "{json}");
    }

    #[test]
    fn an_hotp_code_moves_on_when_asked_and_keeps_its_counter_through_a_lock() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let hotp = app
            .add_code(
                &format!("otpauth://hotp/Example:alice?secret={SEED}&counter=0"),
                None,
                None,
            )
            .unwrap();
        let totp = app
            .add_code(&format!("otpauth://totp/Ex:me?secret={SEED}"), None, None)
            .unwrap();
        let steam = app
            .add_code(
                &format!("otpauth://totp/Steam:me?secret={SEED}&issuer=Steam"),
                None,
                None,
            )
            .unwrap();
        let view = |id: &str| {
            app.codes()
                .unwrap()
                .into_iter()
                .find(|c| c.item_id == id)
                .unwrap()
        };
        // RFC 4226 counters 0 and 1.
        assert_eq!(
            (view(&hotp).current, view(&hotp).counter),
            ("755224".into(), Some(0))
        );
        let step = app
            .advance_code(&hotp, "code.0", GateInput::default())
            .unwrap();
        assert_eq!((step.code.as_str(), step.counter), ("287082", 1));
        assert_eq!(view(&hotp).current, "287082");

        // Locked and opened again: the counter is the item's, not the session's.
        app.lock().unwrap();
        assert_eq!(
            app.advance_code(&hotp, "code.0", GateInput::default())
                .unwrap_err()
                .code,
            ErrorCode::Locked
        );
        app.unlock().unwrap();
        assert_eq!(view(&hotp).counter, Some(1));
        let step = app
            .advance_code(&hotp, "code.0", GateInput::default())
            .unwrap();
        assert_eq!((step.code.as_str(), step.counter), ("359152", 2));
        let copied = app
            .code_now(&hotp, "code.0", false, GateInput::default())
            .unwrap();
        assert_eq!(copied.expose(), b"359152");

        // Time-based codes have no counter and refuse.
        for id in [&totp, &steam] {
            assert_eq!(view(id).counter, None);
            let err = app
                .advance_code(id, "code.0", GateInput::default())
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::Refused);
        }

        // Inside a crown jewel the step asks first, like an edit.
        let jewel = app
            .add_item(
                "login",
                Some("Bank"),
                true,
                vec![FieldChange::secret(
                    "code.0",
                    format!("otpauth://hotp/Bank:me?secret={SEED}&counter=7").as_bytes(),
                )],
            )
            .unwrap();
        let err = app
            .advance_code(&jewel, "code.0", GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ApprovalNeeded);
        assert_eq!(
            view(&jewel).counter,
            Some(7),
            "nothing moved before the ask"
        );
        let confirmed = GateInput {
            password: None,
            confirmed: true,
        };
        assert_eq!(
            app.advance_code(&jewel, "code.0", confirmed)
                .unwrap()
                .counter,
            8
        );
    }

    /// A platform check for tests: answers as told, counts the asks.
    struct FakePlatform {
        answer: bool,
        asked: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Approver for FakePlatform {
        fn confirm(&self, _kind: ApprovalKind, _reason: &str) -> Result<bool, String> {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.answer)
        }

        fn method(&self) -> &'static str {
            "Test ID"
        }
    }

    #[test]
    fn without_a_platform_check_or_an_account_the_gate_is_the_window() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        app.add_login("https://example.com", "me", b"pw".to_vec(), None)
            .unwrap();
        let (route, method) = app.approval_route(ApprovalKind::Reveal).unwrap();
        assert_eq!((route, method.as_str()), (Route::Window, "window"));

        let err = app.export_items("csv", GateInput::default()).unwrap_err();
        assert_eq!(err.code, ErrorCode::ApprovalNeeded);
        assert_eq!(err.detail.as_deref(), Some("window"));
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"detail\":\"window\""));

        let csv = app
            .export_items(
                "csv",
                GateInput {
                    password: None,
                    confirmed: true,
                },
            )
            .unwrap();
        let text = std::str::from_utf8(csv.expose()).unwrap();
        assert!(text.starts_with("name,url,username,password,notes,totp"));
        assert!(text.contains("example.com"));
        assert!(
            app.export_items(
                "xml",
                GateInput {
                    password: None,
                    confirmed: true
                }
            )
            .is_err()
        );

        // Revoking goes through the same gate.
        let err = app
            .device_revoke("nobody", GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ApprovalNeeded);
    }

    #[test]
    fn a_platform_check_takes_use_and_approve_and_stands_in_for_the_password() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        app.set_approver(Box::new(FakePlatform {
            answer: true,
            asked: asked.clone(),
        }))
        .unwrap();
        let (route, method) = app.approval_route(ApprovalKind::Use).unwrap();
        assert_eq!((route, method.as_str()), (Route::Platform, "Test ID"));
        // No account: Reveal has no password to go to, so the platform check stands in.
        let (route, _) = app.approval_route(ApprovalKind::Reveal).unwrap();
        assert_eq!(route, Route::Platform);
        let ok = app
            .approve(ApprovalKind::Use, "copy", GateInput::default())
            .unwrap();
        assert_eq!(ok.method, "Test ID");
        app.export_items("cxf", GateInput::default()).unwrap();
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2);

        // A declined check is a refusal with the method's name, not a crash.
        app.set_approver(Box::new(FakePlatform {
            answer: false,
            asked: asked.clone(),
        }))
        .unwrap();
        let err = app
            .approve(ApprovalKind::Approve, "revoke", GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Refused);
        assert!(err.message.contains("Test ID didn't confirm"));
    }

    #[test]
    fn import_takes_a_csv_and_a_cxf_file_and_refuses_a_hostile_one() {
        let (_dir, app) = fresh();
        app.open_or_create(true).unwrap();
        let csv = b"name,url,username,password,totp\nShop,https://shop.example,me,pw1,\n".to_vec();
        let report = app.import_items(csv, "csv").unwrap();
        assert_eq!(report.imported, 1);
        let rows = app.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Shop");
        assert_eq!(rows[0].username, "me");

        let err = app.import_items(b"{not json".to_vec(), "cxf").unwrap_err();
        assert_eq!(err.code, ErrorCode::Refused);
        assert!(
            err.message.contains("didn't read as CXF"),
            "{}",
            err.message
        );
        assert!(app.import_items(Vec::new(), "xml").is_err());
        assert_eq!(
            app.list().unwrap().len(),
            1,
            "a refused file writes nothing"
        );

        // Round trip: what was exported reads back in.
        let cxf = app
            .export_items(
                "cxf",
                GateInput {
                    password: None,
                    confirmed: true,
                },
            )
            .unwrap();
        let report = app.import_items(cxf.expose().to_vec(), "cxf").unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(app.list().unwrap().len(), 2);
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
