//! Update checks for the server (the unified updates proposal, U5;
//! docs/plan/61-server-updates.md, D690).
//!
//! The server can't swap its own image while it runs, so it only checks
//! and says. Every merge to `main` publishes the image
//! `ghcr.io/snds/lockdown-server` tagged with the main-channel version
//! (`x.y.z-main.<commit time>`) and `main` (release.yml, `server-image`),
//! and the desktop app's manifest (`latest.json` on the `main-build`
//! pre-release) names the same version. The server reads that manifest:
//!
//! - at start and every six hours while "Check for updates automatically"
//!   is on (the admin page; on by default, kept in `operator_settings`),
//!   and whenever the operator presses Check for updates;
//! - with the shared kit's rules (`snds-app-updates`): the channel is read
//!   from the running version, the manifest and every redirect must be
//!   https on GitHub's hosts ([`app_updates::allowed`]), and only a
//!   strictly newer version counts ([`app_updates::is_newer`]);
//! - sending nothing about the server: one `GET` with `accept` and the
//!   user agent `lockdown-server` (no version, no address, no account).
//!
//! When a newer build is out, the admin page shows its version. The server
//! has no Docker socket and no way to restart itself, so who takes it
//! depends on the host:
//!
//! - **with the host helper** (its `host-helper.env` set; [`crate::helper`],
//!   D691): **Update** asks the helper to pull that version's image and
//!   recreate the container, and **Reload** asks it to restart the
//!   container. The page then waits for the server to come back;
//! - **without it:** the page shows the command that updates a compose
//!   setup by hand, and points to the helper's install steps.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use app_updates::{Channel, Feed, GITHUB_HOSTS, UpdateState};
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use semver::Version;
use serde::Deserialize;

use crate::AppState;
use crate::api::Peer;
use crate::db::params;
use crate::helper::{Action, HostHelper, Kind, Progress, Setting};
use crate::pages::{esc, see_other};
use crate::storage::{Result, SqlStorage};

/// Where the server's builds are announced: the same feed as the desktop
/// app's main channel.
pub const FEED: Feed = Feed {
    app: "Lockdown server",
    main_endpoint: "https://github.com/snds/lockdown/releases/download/main-build/latest.json",
    main_page: "https://github.com/snds/lockdown/releases/tag/main-build",
    stable_endpoint: None,
    stable_page: None,
    hosts: &GITHUB_HOSTS,
};

/// The published image (release.yml, `server-image`).
pub const IMAGE: &str = "ghcr.io/snds/lockdown-server";

/// The command that updates a compose setup by hand, run beside its
/// `compose.yaml`.
pub const UPDATE_COMMAND: &str = "docker compose pull && docker compose up -d";

/// How long one check may take.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// The largest manifest read (today's is about 2 KB).
pub const MANIFEST_LIMIT: usize = 256 * 1024;

/// The most redirects one check follows.
const MAX_REDIRECTS: usize = 5;

/// The `operator_settings` row that holds the switch.
const AUTO_SETTING: &str = "update_auto_check";

/// The version this binary was built as (stamped by the release workflow
/// on every build but a tag's).
pub fn running() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// Reads a channel's manifest. The real one is [`GitHub`]; tests give
/// their own.
pub trait ManifestSource: Send + Sync {
    /// The manifest's bytes, or what went wrong in plain words.
    fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, String>;
}

/// Whether one hop of a check (the manifest's address, or a redirect)
/// may be followed: the kit's rule, and at most [`MAX_REDIRECTS`] hops.
pub fn hop_allowed(url: &str, hops_before: usize) -> std::result::Result<(), String> {
    if hops_before > MAX_REDIRECTS {
        return Err("too many redirects".into());
    }
    let hosts: Vec<String> = FEED.hosts.iter().map(|h| h.to_string()).collect();
    app_updates::allowed(url, &hosts)
}

/// The manifest over https from GitHub, every redirect checked by
/// [`hop_allowed`].
pub struct GitHub {
    client: reqwest::blocking::Client,
}

impl GitHub {
    /// A client with the check's timeout, redirect rule and plain headers.
    pub fn new() -> std::result::Result<Self, String> {
        lockdown_tls::install_default_provider();
        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::custom(
                |attempt| match hop_allowed(attempt.url().as_str(), attempt.previous().len()) {
                    Ok(()) => attempt.follow(),
                    Err(why) => attempt.error(why),
                },
            ))
            .user_agent("lockdown-server")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }
}

impl ManifestSource for GitHub {
    fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, String> {
        hop_allowed(url, 0).map_err(|e| refused(&e))?;
        let res = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .map_err(|e| {
                // A refused redirect carries the kit's words.
                let mut source: Option<&dyn std::error::Error> = Some(&e);
                while let Some(s) = source {
                    let text = s.to_string();
                    if text.starts_with("updates ") || text == "too many redirects" {
                        return refused(&text);
                    }
                    source = s.source();
                }
                UNREACHABLE.to_string()
            })?;
        if !res.status().is_success() {
            return Err(if res.status() == reqwest::StatusCode::NOT_FOUND {
                NOTHING_PUBLISHED.to_string()
            } else {
                UNREACHABLE.to_string()
            });
        }
        let mut body = Vec::new();
        let mut limited = std::io::Read::take(res, MANIFEST_LIMIT as u64 + 1);
        std::io::Read::read_to_end(&mut limited, &mut body).map_err(|_| UNREACHABLE.to_string())?;
        Ok(body)
    }
}

/// A refused address in the desktop app's words.
fn refused(why: &str) -> String {
    if why == "updates come only over https" {
        "Updates come only over https.".into()
    } else {
        format!("The update was refused: {why}.")
    }
}

const UNREACHABLE: &str = "The server couldn't reach GitHub. Check its connection and try again.";
const NOTHING_PUBLISHED: &str = "No main-channel build is published yet.";
const UNREADABLE: &str = "The update's description couldn't be read.";

/// What a manifest says about the running build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The manifest's version is this one or older.
    UpToDate,
    /// A strictly newer version is out.
    Newer(Version),
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
}

/// Reads a manifest (`latest.json`, the desktop app's: only its `version`
/// is used) against the running version.
pub fn judge(current: &Version, body: &[u8]) -> std::result::Result<Outcome, String> {
    if body.len() > MANIFEST_LIMIT {
        return Err(UNREADABLE.into());
    }
    let manifest: Manifest = serde_json::from_slice(body).map_err(|_| UNREADABLE.to_string())?;
    let offered = Version::parse(manifest.version.trim().trim_start_matches('v'))
        .map_err(|_| UNREADABLE.to_string())?;
    Ok(if app_updates::is_newer(current, &offered) {
        Outcome::Newer(offered)
    } else {
        Outcome::UpToDate
    })
}

/// Where the server's update check stands, as the admin page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The running version.
    pub version: String,
    /// The channel it follows.
    pub channel: Channel,
    /// Why this build doesn't check (a tag build, or one from source).
    pub off: Option<&'static str>,
    /// `Idle`, `Checking`, `UpToDate`, `Available` or `Failed`.
    pub state: UpdateState,
    /// The newer version (`Available`).
    pub offered: Option<String>,
    /// What went wrong (`Failed`).
    pub error: Option<String>,
    /// When the last check finished.
    pub checked_at: Option<i64>,
}

/// Why Update or Reload can't start: the host helper isn't set up.
pub const HELPER_OFF: &str = "One-click Update and Reload need the host helper, and this server has no helper settings (host-helper.env).";

/// Why Update or Reload can't start: one is already running.
pub const ALREADY_RUNNING: &str = "An update or reload is already running. Wait for it to finish.";

/// Why Update can't start: the page asked for another version than the
/// one on offer now.
pub const OFFER_CHANGED: &str =
    "The version on offer has changed since the page was loaded. Look at the Updates card again.";

/// Why a build that isn't on the main channel doesn't check.
pub const OFF_NOT_MAIN: &str = "Update checks are off for this build: it was built from source or from a tagged release, and only main-channel builds (the published image) check for now.";

/// The server's update check: its status and where it reads the manifest.
pub struct Updates {
    version: Version,
    status: Mutex<Status>,
    source: Mutex<Option<Arc<dyn ManifestSource>>>,
    /// The host helper, when it is set up (`main` reads it at start).
    helper: Mutex<Setting>,
    /// The last Update or Reload asked for.
    action: Mutex<Option<Action>>,
}

impl std::fmt::Debug for Updates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Updates({})", self.version)
    }
}

impl Default for Updates {
    fn default() -> Self {
        Self::new(running(), None)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Updates {
    /// Checks for `version`, reading through `source` ([`GitHub`] when
    /// `None`, made at the first check).
    pub fn new(version: Version, source: Option<Arc<dyn ManifestSource>>) -> Self {
        let channel = Channel::of(&version);
        let status = Status {
            version: version.to_string(),
            channel,
            off: channel.endpoint(&FEED).is_none().then_some(OFF_NOT_MAIN),
            state: UpdateState::Idle,
            offered: None,
            error: None,
            checked_at: None,
        };
        Self {
            version,
            status: Mutex::new(status),
            source: Mutex::new(source),
            helper: Mutex::new(Setting::Off),
            action: Mutex::new(None),
        }
    }

    /// Sets how Update and Reload reach the host helper.
    pub fn set_helper(&self, setting: Setting) {
        *lock(&self.helper) = setting;
    }

    /// How Update and Reload reach the host helper.
    pub fn helper(&self) -> Setting {
        lock(&self.helper).clone()
    }

    /// The last Update or Reload asked for, if any.
    pub fn action(&self) -> Option<Action> {
        lock(&self.action).clone()
    }

    /// Starts an Update or Reload: the helper to ask, or why it can't
    /// start (no helper, a broken setting, one already running). Either
    /// way the action is recorded for the page.
    pub fn begin(
        &self,
        kind: Kind,
        tag: Option<String>,
        now: i64,
    ) -> std::result::Result<Arc<dyn HostHelper>, String> {
        let mut action = lock(&self.action);
        if let Some(a) = action.as_ref()
            && a.progress == Progress::Working
            && now - a.at < crate::helper::WORKING_FOR
        {
            return Err(ALREADY_RUNNING.to_string());
        }
        let helper = match self.helper() {
            Setting::Ready(h) => Ok(h),
            Setting::Broken(why) => Err(why),
            Setting::Off => Err(HELPER_OFF.to_string()),
        };
        *action = Some(Action {
            kind,
            tag,
            at: now,
            progress: match &helper {
                Ok(_) => Progress::Working,
                Err(why) => Progress::Failed(why.clone()),
            },
        });
        helper
    }

    /// Records how a started Update or Reload ended, when this process is
    /// still here to see it.
    pub fn finish(&self, outcome: std::result::Result<String, String>) {
        if let Some(a) = lock(&self.action).as_mut() {
            a.progress = match outcome {
                Ok(done) => Progress::Done(done),
                Err(why) => Progress::Failed(why),
            };
        }
    }

    /// Records an Update or Reload that was refused before it started.
    pub fn refuse(&self, kind: Kind, tag: Option<String>, why: &str, now: i64) {
        *lock(&self.action) = Some(Action {
            kind,
            tag,
            at: now,
            progress: Progress::Failed(why.to_string()),
        });
    }

    /// The status now.
    pub fn status(&self) -> Status {
        lock(&self.status).clone()
    }

    /// Whether this build checks at all.
    pub fn enabled(&self) -> bool {
        lock(&self.status).off.is_none()
    }

    /// One check now, on this thread (it blocks on the network). A build
    /// that doesn't check, or a check already running, returns the status
    /// as it is.
    pub fn check(&self, now: i64) -> Status {
        let Some(endpoint) = Channel::of(&self.version).endpoint(&FEED) else {
            return self.status();
        };
        {
            let mut s = lock(&self.status);
            if s.state == UpdateState::Checking {
                return s.clone();
            }
            s.state = UpdateState::Checking;
            s.error = None;
        }
        let source = {
            let mut source = lock(&self.source);
            if source.is_none() {
                match GitHub::new() {
                    Ok(g) => *source = Some(Arc::new(g)),
                    Err(e) => eprintln!("lockdown-server: the update check couldn't start: {e}"),
                }
            }
            source.clone()
        };
        let result = match source {
            Some(source) => source
                .fetch(endpoint)
                .and_then(|body| judge(&self.version, &body)),
            None => Err(UNREACHABLE.to_string()),
        };
        let mut s = lock(&self.status);
        s.checked_at = Some(now);
        match result {
            Ok(Outcome::UpToDate) => {
                s.state = UpdateState::UpToDate;
                s.offered = None;
                eprintln!("lockdown-server: update check: up to date");
            }
            Ok(Outcome::Newer(v)) => {
                s.state = UpdateState::Available;
                eprintln!("lockdown-server: update check: version {v} is out");
                s.offered = Some(v.to_string());
            }
            Err(e) => {
                s.state = UpdateState::Failed;
                s.offered = None;
                eprintln!("lockdown-server: update check failed: {e}");
                s.error = Some(e);
            }
        }
        s.clone()
    }
}

impl SqlStorage {
    /// The "Check for updates automatically" switch: on unless the
    /// operator turned it off.
    pub fn update_auto_check(&self) -> Result<bool> {
        let value: Option<Vec<u8>> = self.conn().query_opt(
            "SELECT value FROM operator_settings WHERE name = ?1",
            params![AUTO_SETTING],
            |r| r.get(0),
        )?;
        Ok(value.as_deref() != Some(b"off"))
    }

    /// Sets the switch.
    pub fn set_update_auto_check(&self, on: bool) -> Result<()> {
        let value: &[u8] = if on { b"on" } else { b"off" };
        self.conn().execute(
            "INSERT INTO operator_settings (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![AUTO_SETTING, value.to_vec()],
        )?;
        Ok(())
    }
}

/// Whether the automatic check is on for `state` (on when the store keeps
/// no settings).
pub fn auto_check(state: &AppState) -> bool {
    state
        .storage
        .sql()
        .is_none_or(|s| s.update_auto_check().unwrap_or(true))
}

/// Checks at start and then every six hours while the switch is on, on a
/// thread of its own, for as long as the process lives. A build that
/// doesn't check starts no thread.
pub fn spawn(state: Arc<AppState>) -> std::io::Result<Option<std::thread::JoinHandle<()>>> {
    if !state.updates.enabled() {
        return Ok(None);
    }
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            loop {
                if auto_check(&state) {
                    state.updates.check(state.now());
                }
                std::thread::sleep(app_updates::CHECK_EVERY);
            }
        })
        .map(Some)
}

// --- the admin page's section ----------------------------------------------------------

/// The routes the section's forms post to, and the page that waits for
/// an Update or Reload (merged into the admin page's).
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/updates/check", post(check_now))
        .route("/admin/updates/auto", post(set_auto))
        .route("/admin/updates/update", post(update_now))
        .route("/admin/updates/reload", post(reload_now))
        .route("/admin/updates/progress", get(progress))
        .route("/admin/updates/state", get(progress_state))
}

/// The status line, in the desktop app's words (docs/plan/60-auto-update.md §7).
pub fn status_line(status: &Status, auto: bool) -> String {
    if let Some(off) = status.off {
        return off.to_string();
    }
    match status.state {
        UpdateState::Checking => "Checking for updates…".into(),
        UpdateState::UpToDate => "Lockdown server is up to date.".into(),
        UpdateState::Available => format!(
            "Version {} is out.",
            status.offered.as_deref().unwrap_or("?")
        ),
        UpdateState::Failed => status
            .error
            .clone()
            .unwrap_or_else(|| "The update check didn't work.".into()),
        _ if auto => {
            "Lockdown server checks for updates when it starts and every six hours.".into()
        }
        _ => "Automatic checks are off. Check whenever you like.".into(),
    }
}

/// The Updates section of the admin overview (`csrf_field` is the form
/// token's hidden input).
pub fn section(state: &AppState, csrf_field: &str) -> String {
    let status = state.updates.status();
    let auto = auto_check(state);
    let channel = match status.channel {
        Channel::Main => "main channel (every merged change)",
        Channel::Stable => "stable channel",
    };
    let mut out = format!(
        "<h2 id=\"updates\">Updates</h2><div class=\"card\">\
         <p><b>Lockdown server {}</b> · {channel}</p>\
         <p role=\"status\">{}</p>",
        esc(&status.version),
        esc(&status_line(&status, auto)),
    );
    let helper = state.updates.helper();
    if let Some(action) = state.updates.action() {
        out.push_str(&action_line(&action));
    }
    if helper.ready() {
        if status.state == UpdateState::Available {
            let offered = esc(status.offered.as_deref().unwrap_or(""));
            out.push_str(&format!(
                "<div class=\"row\"><form class=\"inline\" method=\"post\" action=\"/admin/updates/update\">{csrf_field}<input type=\"hidden\" name=\"tag\" value=\"{offered}\"><button>Update to {offered}</button></form>\
                 <form class=\"inline\" method=\"post\" action=\"/admin/updates/reload\">{csrf_field}<button class=\"quiet\">Reload</button></form></div>\
                 <p class=\"muted\">Update asks the host helper to pull <code>{IMAGE}:{offered}</code> and start the server on it, on the same data and settings. Take a backup first. Reload restarts the server on the build it runs. Either way the server is gone for a moment and you sign in again.</p>\
                 <p><a href=\"{}\">See the build on GitHub</a></p>",
                FEED.main_page,
            ));
        } else {
            out.push_str(&format!(
                "<form method=\"post\" action=\"/admin/updates/reload\">{csrf_field}<p><button class=\"quiet\">Reload</button></p></form>\
                 <p class=\"muted\">Reload asks the host helper to restart the server on the build it runs. The server is gone for a moment and you sign in again.</p>",
            ));
        }
    } else {
        if status.state == UpdateState::Available {
            out.push_str(&format!(
                "<p>To update, run this in the folder with your <code>compose.yaml</code>:</p>\
                 <pre><code>{}</code></pre>\
                 <p class=\"muted\">It pulls <code>{IMAGE}:main</code> (the same build as <code>{IMAGE}:{}</code>) and starts the new container on the same data and settings. Take a backup first.</p>\
                 <p><a href=\"{}\">See the build on GitHub</a></p>",
                esc(UPDATE_COMMAND),
                esc(status.offered.as_deref().unwrap_or("")),
                FEED.main_page,
            ));
        }
        if let Setting::Broken(why) = &helper {
            out.push_str(&format!(
                "<p class=\"bad\">The host helper's settings aren't usable: {}</p>",
                esc(why)
            ));
        }
        out.push_str(&format!(
            "<p class=\"muted\">One-click Update and Reload need the host helper beside the server: <a href=\"{}\">its install steps</a>.</p>",
            crate::helper::INSTALL_STEPS
        ));
    }
    if let Some(at) = status.checked_at {
        out.push_str(&format!(
            "<p class=\"muted\">Last checked {}.</p>",
            crate::mail::when(at)
        ));
    }
    if status.off.is_none() {
        out.push_str(&format!(
            "<form method=\"post\" action=\"/admin/updates/check\">{csrf_field}<p><button>Check for updates</button></p></form>\
             <form method=\"post\" action=\"/admin/updates/auto\">{csrf_field}<label class=\"check\"><input type=\"checkbox\" name=\"auto\" value=\"on\"{}> Check for updates automatically</label><p><button class=\"quiet\">Save</button></p></form>",
            if auto { " checked" } else { "" },
        ));
    }
    out.push_str("</div>");
    out
}

/// `POST /admin/updates/check`: one check now; back to the overview.
pub async fn check_now(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match crate::admin::signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if crate::admin::form_ok(&crate::pages::form_fields(&body), &csrf) {
        let st = state.clone();
        let _ = tokio::task::spawn_blocking(move || st.updates.check(st.now())).await;
    }
    see_other("/admin#updates")
}

/// `POST /admin/updates/auto` (a form: `auto=on`, or no `auto` for off).
pub async fn set_auto(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match crate::admin::signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fields = crate::pages::form_fields(&body);
    if crate::admin::form_ok(&fields, &csrf)
        && let Some(store) = state.storage.sql()
    {
        let on = fields.get("auto").is_some_and(|v| v == "on");
        if store.set_update_auto_check(on).is_ok() {
            let _ = store.record_event(
                "updates",
                true,
                if on {
                    "automatic update checks turned on"
                } else {
                    "automatic update checks turned off"
                },
                state.now(),
            );
        }
    }
    see_other("/admin#updates")
}

/// The last Update or Reload, as one line on the card.
fn action_line(action: &Action) -> String {
    let what = match (action.kind, action.tag.as_deref()) {
        (Kind::Update, Some(tag)) => format!("The update to {tag}"),
        (Kind::Update, None) => "The update".into(),
        (Kind::Reload, _) => "The reload".into(),
    };
    match &action.progress {
        Progress::Working => format!(
            "<p role=\"status\">{} is running. <a href=\"/admin/updates/progress\">Follow it</a></p>",
            esc(&what)
        ),
        Progress::Done(line) => format!("<p class=\"ok\" role=\"status\">{}</p>", esc(line)),
        Progress::Failed(why) => format!(
            "<p class=\"bad\" role=\"alert\">{} didn't happen. {}</p>",
            esc(&what),
            esc(why)
        ),
    }
}

/// `POST /admin/updates/update` (a form: `csrf`, `tag`): asks the host
/// helper for the version on offer, then the page that waits for it.
pub async fn update_now(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match crate::admin::signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fields = crate::pages::form_fields(&body);
    if !crate::admin::form_ok(&fields, &csrf) {
        return see_other("/admin#updates");
    }
    let status = state.updates.status();
    let asked: String = fields
        .get("tag")
        .map(|t| t.chars().take(128).collect())
        .unwrap_or_default();
    let now = state.now();
    let on_offer =
        status.state == UpdateState::Available && status.offered.as_deref() == Some(asked.as_str());
    if !on_offer {
        state
            .updates
            .refuse(Kind::Update, Some(asked.clone()), OFFER_CHANGED, now);
        log_action(
            &state,
            &peer,
            Kind::Update,
            Some(&asked),
            Err(OFFER_CHANGED),
        );
        return see_other("/admin#updates");
    }
    // Lockdown's manifest carries no image digest yet, so the helper
    // checks the tag only (docs/plan/61-server-updates.md §6).
    let request = host_helper::UpdateRequest {
        tag: Some(asked.clone()),
        expect_digest: None,
    };
    start(state, peer, Kind::Update, Some(asked), move |h| {
        h.update(&request)
            .map(|r| crate::helper::done_line(Kind::Update, Some(&r), None))
    })
}

/// `POST /admin/updates/reload` (a form: `csrf`): asks the host helper to
/// restart the server, then the page that waits for it.
pub async fn reload_now(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match crate::admin::signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !crate::admin::form_ok(&crate::pages::form_fields(&body), &csrf) {
        return see_other("/admin#updates");
    }
    start(state, peer, Kind::Reload, None, |h| {
        h.restart()
            .map(|r| crate::helper::done_line(Kind::Reload, None, Some(&r)))
    })
}

/// Asks the helper on a thread of its own and answers at once: the
/// helper's work replaces this process, so the answer would never come
/// back on this request. A plain thread, not the runtime's blocking pool,
/// so shutting down never waits for it.
fn start(
    state: Arc<AppState>,
    peer: Peer,
    kind: Kind,
    tag: Option<String>,
    call: impl FnOnce(&dyn HostHelper) -> std::result::Result<String, host_helper::ClientError>
    + Send
    + 'static,
) -> Response {
    let now = state.now();
    let helper = match state.updates.begin(kind, tag.clone(), now) {
        Ok(h) => h,
        Err(why) => {
            log_action(&state, &peer, kind, tag.as_deref(), Err(&why));
            return see_other("/admin#updates");
        }
    };
    log_action(&state, &peer, kind, tag.as_deref(), Ok(None));
    let st = state.clone();
    let spawned = std::thread::Builder::new()
        .name("host-helper".into())
        .spawn(move || {
            let outcome = call(helper.as_ref()).map_err(|e| crate::helper::explain(kind, &e));
            log_action(
                &st,
                &peer,
                kind,
                tag.as_deref(),
                outcome
                    .as_ref()
                    .map(|d| Some(d.as_str()))
                    .map_err(String::as_str),
            );
            st.updates.finish(outcome);
        });
    if spawned.is_err() {
        state.updates.finish(Err(
            "The server couldn't start the request to the host helper. Try again.".into(),
        ));
        return see_other("/admin#updates");
    }
    see_other("/admin/updates/progress")
}

/// One line in the operator's log and the server's own log: who (the
/// operator, from where), what, which version, and how it went. Never the
/// key: it isn't in anything this sees.
fn log_action(
    state: &AppState,
    peer: &Peer,
    kind: Kind,
    tag: Option<&str>,
    outcome: std::result::Result<Option<&str>, &str>,
) {
    let what = match (kind, tag) {
        (Kind::Update, Some(tag)) => format!("an update to {tag}"),
        (Kind::Update, None) => "an update".into(),
        (Kind::Reload, _) => "a reload".into(),
    };
    let (ok, detail) = match outcome {
        Ok(None) => (
            true,
            format!(
                "the operator, from {}, asked the host helper for {what}",
                peer.0
            ),
        ),
        Ok(Some(done)) => (true, format!("the host helper finished {what}: {done}")),
        Err(why) => (false, format!("{what} didn't happen: {why}")),
    };
    eprintln!("lockdown-server: {detail}");
    if let Some(store) = state.storage.sql() {
        let _ = store.record_event("updates", ok, &detail, state.now());
    }
}

/// The waiting page's one script (allowed by its hash): it asks
/// `/admin/updates/state` every two seconds and says what it finds. A
/// failed request means the server is restarting; a sign-in asked for
/// means it came back (sign-ins live in memory, so a restart ends them).
pub const PROGRESS_SCRIPT: &str = r#"(()=>{const el=document.getElementById("hh-status");if(!el)return;const from=el.dataset.from;let tries=0,away=false;const say=t=>{el.textContent=t};const again=()=>{if(++tries>450){say("This is taking longer than it should. Look at the host helper's log on the host (docker compose logs, in its folder).");return}setTimeout(tick,2000)};async function tick(){let r;try{r=await fetch("/admin/updates/state",{credentials:"same-origin",cache:"no-store",redirect:"error"})}catch(e){away=true;say("Reconnecting… the server is restarting.");return again()}if(r.status===401){say("The server restarted. Sign in again to see the version it runs now.");return}if(!r.ok){away=true;say("Reconnecting… the server is restarting.");return again()}const j=await r.json().catch(()=>null);if(!j)return again();if(j.version!==from){say("Lockdown server "+j.version+" is running.");return}if(j.state==="failed"||j.state==="done"){say(j.message);return}say(away?"Reconnecting… the server is restarting.":j.message);again()}setTimeout(tick,1000)})();"#;

/// `GET /admin/updates/progress`: the page that waits for an Update or
/// Reload and reconnects on its own.
pub async fn progress(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match crate::admin::signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let status = state.updates.status();
    let Some(action) = state.updates.action() else {
        return see_other("/admin#updates");
    };
    let (title, line) = progress_words(&action);
    let body = format!(
        "<h1>{}</h1><div class=\"card\"><p id=\"hh-status\" role=\"status\" data-from=\"{}\">{}</p>\
         <p class=\"muted\">The server goes away while the host helper works and comes back on its own. This page waits for it. Sign-ins don't survive a restart, so you sign in again afterwards.</p>\
         <p><a href=\"/admin#updates\">Back to the admin page</a></p></div>",
        esc(title),
        esc(&status.version),
        esc(&line),
    );
    crate::admin::shell_page(&csrf, "/admin", title, &body, Some(PROGRESS_SCRIPT))
}

/// The waiting page's title and first line.
fn progress_words(action: &Action) -> (&'static str, String) {
    let title = match action.kind {
        Kind::Update => "Updating",
        Kind::Reload => "Reloading",
    };
    let line = match &action.progress {
        Progress::Working => match (action.kind, action.tag.as_deref()) {
            (Kind::Update, Some(tag)) => format!(
                "Asking the host helper to update to {tag}. Downloading the image can take a few minutes."
            ),
            (Kind::Update, None) => "Asking the host helper to update.".into(),
            (Kind::Reload, _) => "Asking the host helper to restart the server.".into(),
        },
        Progress::Done(line) | Progress::Failed(line) => line.clone(),
    };
    (title, line)
}

/// `GET /admin/updates/state`: what the waiting page reads: the running
/// version, and where the last Update or Reload stands.
pub async fn progress_state(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(r) = crate::admin::signed_in(&state, &headers) {
        return *r;
    }
    let version = state.updates.status().version;
    let (kind, step, message) = match state.updates.action() {
        None => ("none", "none", String::new()),
        Some(a) => {
            let (_, line) = progress_words(&a);
            let step = match a.progress {
                Progress::Working => "working",
                Progress::Done(_) => "done",
                Progress::Failed(_) => "failed",
            };
            (a.kind.noun(), step, line)
        }
    };
    let body = serde_json::json!({
        "version": version,
        "kind": kind,
        "state": step,
        "message": message,
    });
    let mut res = axum::response::IntoResponse::into_response((
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        )],
        body.to_string(),
    ));
    crate::pages::secure(&mut res);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn only_a_strictly_newer_manifest_is_an_update() {
        let at = v("0.1.0-main.20261108090000");
        let newer = br#"{"version":"0.1.0-main.20261108120000","platforms":{}}"#;
        assert_eq!(
            judge(&at, newer),
            Ok(Outcome::Newer(v("0.1.0-main.20261108120000")))
        );
        let same = br#"{"version":"0.1.0-main.20261108090000"}"#;
        assert_eq!(judge(&at, same), Ok(Outcome::UpToDate));
        let older = br#"{"version":"0.1.0-main.20261107090000"}"#;
        assert_eq!(judge(&at, older), Ok(Outcome::UpToDate));
        // The release sorts after its own main builds.
        assert_eq!(
            judge(&at, br#"{"version":"0.1.0"}"#),
            Ok(Outcome::Newer(v("0.1.0")))
        );
        for bad in [
            &b"not json"[..],
            br#"{"version":"soon"}"#,
            br#"{"notes":"no version"}"#,
        ] {
            assert_eq!(judge(&at, bad), Err(UNREADABLE.into()));
        }
        let huge = vec![b' '; MANIFEST_LIMIT + 1];
        assert_eq!(judge(&at, &huge), Err(UNREADABLE.into()));
    }

    #[test]
    fn checks_go_only_to_githubs_hosts_over_https() {
        assert!(hop_allowed(FEED.main_endpoint, 0).is_ok());
        assert!(
            hop_allowed(
                "https://release-assets.githubusercontent.com/github-production-release-asset/1/2",
                1
            )
            .is_ok()
        );
        assert_eq!(
            hop_allowed("https://evil.example/latest.json", 1),
            Err("updates don't come from evil.example".into())
        );
        assert!(hop_allowed("http://github.com/snds/lockdown", 0).is_err());
        assert!(hop_allowed("https://user@github.com/x", 0).is_err());
        assert!(hop_allowed("https://github.com:8443/x", 0).is_err());
        assert_eq!(
            hop_allowed(FEED.main_endpoint, MAX_REDIRECTS + 1),
            Err("too many redirects".into())
        );
        assert_eq!(
            refused("updates don't come from evil.example"),
            "The update was refused: updates don't come from evil.example."
        );
        assert_eq!(
            refused("updates come only over https"),
            "Updates come only over https."
        );
    }

    struct Fixed(std::result::Result<Vec<u8>, String>, Mutex<Vec<String>>);

    impl ManifestSource for Fixed {
        fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, String> {
            lock(&self.1).push(url.to_string());
            self.0.clone()
        }
    }

    #[test]
    fn a_main_build_checks_the_main_manifest_and_says_what_it_found() {
        let source = Arc::new(Fixed(
            Ok(br#"{"version":"0.1.0-main.20261109000000"}"#.to_vec()),
            Mutex::default(),
        ));
        let u = Updates::new(v("0.1.0-main.20261108000000"), Some(source.clone()));
        assert!(u.enabled());
        assert_eq!(
            status_line(&u.status(), true),
            "Lockdown server checks for updates when it starts and every six hours."
        );
        assert_eq!(
            status_line(&u.status(), false),
            "Automatic checks are off. Check whenever you like."
        );
        let s = u.check(1_000);
        assert_eq!(s.state, UpdateState::Available);
        assert_eq!(s.offered.as_deref(), Some("0.1.0-main.20261109000000"));
        assert_eq!(s.checked_at, Some(1_000));
        assert_eq!(
            status_line(&s, true),
            "Version 0.1.0-main.20261109000000 is out."
        );
        assert_eq!(*lock(&source.1), vec![FEED.main_endpoint.to_string()]);

        let failing = Arc::new(Fixed(Err(UNREACHABLE.into()), Mutex::default()));
        let u = Updates::new(v("0.1.0-main.20261108000000"), Some(failing));
        let s = u.check(2_000);
        assert_eq!(s.state, UpdateState::Failed);
        assert_eq!(status_line(&s, true), UNREACHABLE);
    }

    #[test]
    fn a_tag_or_source_build_never_checks() {
        let source = Arc::new(Fixed(Ok(b"{}".to_vec()), Mutex::default()));
        let u = Updates::new(v("0.1.0"), Some(source.clone()));
        assert!(!u.enabled());
        let s = u.check(1);
        assert_eq!(s.state, UpdateState::Idle);
        assert_eq!(status_line(&s, true), OFF_NOT_MAIN);
        assert!(lock(&source.1).is_empty());
    }

    /// A host helper that answers as told and remembers what it was asked.
    struct FakeHelper {
        refuse: Option<host_helper::ClientError>,
        asked: Mutex<Vec<String>>,
    }

    impl HostHelper for FakeHelper {
        fn update(
            &self,
            request: &host_helper::UpdateRequest,
        ) -> std::result::Result<host_helper::UpdateReply, host_helper::ClientError> {
            lock(&self.asked).push(format!("update {request:?}"));
            match &self.refuse {
                Some(e) => Err(e.clone()),
                None => Ok(host_helper::UpdateReply {
                    stack: "lockdown".into(),
                    pulled: format!("{IMAGE}:{}", request.tag.clone().unwrap_or_default()),
                    digest: format!("sha256:{}", "0".repeat(64)),
                    services: vec!["lockdown".into()],
                }),
            }
        }

        fn restart(
            &self,
        ) -> std::result::Result<host_helper::RestartReply, host_helper::ClientError> {
            lock(&self.asked).push("restart".into());
            match &self.refuse {
                Some(e) => Err(e.clone()),
                None => Ok(host_helper::RestartReply {
                    stack: "lockdown".into(),
                    services: vec!["lockdown".into()],
                }),
            }
        }
    }

    fn state_with(offered: Option<&str>, helper: Setting) -> AppState {
        let mut state = AppState::in_memory(crate::Config::default()).unwrap();
        let manifest = match offered {
            Some(v) => format!(r#"{{"version":"{v}"}}"#),
            None => r#"{"version":"0.1.0-main.20261108000000"}"#.into(),
        };
        let source = Arc::new(Fixed(Ok(manifest.into_bytes()), Mutex::default()));
        state.updates = Updates::new(v("0.1.0-main.20261108000000"), Some(source));
        state.updates.check(1);
        state.updates.set_helper(helper);
        state
    }

    #[test]
    fn without_the_helper_the_card_shows_the_command_and_where_to_install_it() {
        let state = state_with(Some("0.1.0-main.20261109000000"), Setting::Off);
        let html = section(&state, "<csrf>");
        assert!(html.contains(&esc(UPDATE_COMMAND)));
        assert!(html.contains(crate::helper::INSTALL_STEPS));
        assert!(!html.contains("/admin/updates/update"));
        assert!(!html.contains("/admin/updates/reload"));

        // Up to date: no command, the pointer still.
        let state = state_with(None, Setting::Off);
        let html = section(&state, "<csrf>");
        assert!(!html.contains(&esc(UPDATE_COMMAND)));
        assert!(html.contains(crate::helper::INSTALL_STEPS));

        // Half set up: what's wrong, and the command as before.
        let state = state_with(
            Some("0.1.0-main.20261109000000"),
            Setting::Broken("HOST_HELPER_KEY isn't set.".into()),
        );
        let html = section(&state, "<csrf>");
        assert!(html.contains("The host helper's settings aren't usable: "));
        assert!(html.contains(&esc("HOST_HELPER_KEY isn't set.")));
        assert!(html.contains(&esc(UPDATE_COMMAND)));
        assert!(!html.contains("/admin/updates/reload"));
    }

    #[test]
    fn with_the_helper_the_card_offers_update_for_a_newer_build_and_always_reload() {
        let fake = Arc::new(FakeHelper {
            refuse: None,
            asked: Mutex::default(),
        });
        let state = state_with(
            Some("0.1.0-main.20261109000000"),
            Setting::Ready(fake.clone()),
        );
        let html = section(&state, "<csrf>");
        assert!(html.contains("action=\"/admin/updates/update\"><csrf><input type=\"hidden\" name=\"tag\" value=\"0.1.0-main.20261109000000\"><button>Update to 0.1.0-main.20261109000000</button>"));
        assert!(html.contains("action=\"/admin/updates/reload\"><csrf>"));
        assert!(!html.contains(&esc(UPDATE_COMMAND)));
        assert!(!html.contains(crate::helper::INSTALL_STEPS));

        // Up to date: Reload alone.
        let state = state_with(None, Setting::Ready(fake.clone()));
        let html = section(&state, "<csrf>");
        assert!(!html.contains("/admin/updates/update"));
        assert!(html.contains("/admin/updates/reload"));
        assert!(
            lock(&fake.asked).is_empty(),
            "showing the card asks nothing"
        );
    }

    #[test]
    fn an_action_runs_once_at_a_time_and_its_outcome_shows_on_the_card() {
        let fake = Arc::new(FakeHelper {
            refuse: None,
            asked: Mutex::default(),
        });
        let state = state_with(
            Some("0.1.0-main.20261109000000"),
            Setting::Ready(fake.clone()),
        );
        let u = &state.updates;
        assert!(
            u.begin(Kind::Update, Some("0.1.0-main.20261109000000".into()), 10)
                .is_ok()
        );
        assert_eq!(
            u.begin(Kind::Reload, None, 11).err().as_deref(),
            Some(ALREADY_RUNNING)
        );
        assert!(
            section(&state, "").contains("The update to 0.1.0-main.20261109000000 is running.")
        );
        // Long after, a stuck one no longer blocks.
        assert!(
            u.begin(Kind::Reload, None, 10 + crate::helper::WORKING_FOR)
                .is_ok()
        );
        u.finish(Err(crate::helper::explain(
            Kind::Reload,
            &host_helper::ClientError::Refused {
                status: 409,
                code: "busy".into(),
                message: "…".into(),
            },
        )));
        let html = section(&state, "");
        assert!(html.contains("role=\"alert\">The reload didn't happen. An update or reload is already running on the host."));
        u.begin(Kind::Reload, None, 5_000).unwrap();
        u.finish(Ok("The host helper restarted lockdown.".into()));
        assert!(
            section(&state, "")
                .contains("class=\"ok\" role=\"status\">The host helper restarted lockdown.")
        );

        // No helper: the action is refused with the reason, and nothing is asked.
        let off = state_with(None, Setting::Off);
        assert_eq!(
            off.updates.begin(Kind::Reload, None, 1).err().as_deref(),
            Some(HELPER_OFF)
        );
        assert!(section(&off, "").contains(HELPER_OFF));
        assert!(lock(&fake.asked).is_empty());
    }

    #[test]
    fn the_waiting_page_script_is_plain() {
        let csp = crate::pages::csp_with_script(PROGRESS_SCRIPT);
        assert!(csp.contains("connect-src 'self'"));
        for banned in ["innerHTML", "eval(", "onclick", "http://", "https://"] {
            assert!(!PROGRESS_SCRIPT.contains(banned), "{banned}");
        }
        assert!(PROGRESS_SCRIPT.contains("/admin/updates/state"));
    }

    #[test]
    fn the_switch_is_on_until_turned_off() {
        let s = SqlStorage::open_scratch().unwrap();
        assert!(s.update_auto_check().unwrap());
        s.set_update_auto_check(false).unwrap();
        assert!(!s.update_auto_check().unwrap());
        s.set_update_auto_check(true).unwrap();
        assert!(s.update_auto_check().unwrap());
    }
}
