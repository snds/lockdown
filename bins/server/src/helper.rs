//! One-click **Update** and **Reload** through the host helper (the unified
//! updates proposal, U5; docs/plan/61-server-updates.md §6, D691).
//!
//! The server still can't swap its own image or restart itself, and it
//! never holds the Docker socket. A small helper on the host does
//! (`snds/app-updates`, `host-helper/`): the server signs a request with
//! its own key and the helper pulls an allowed tag of this server's image
//! and recreates its container, or restarts it. The helper's allow-list
//! decides what it will touch; nothing here can name a command, an image
//! or a folder.
//!
//! The settings are the three lines of the stack's `host-helper.env`
//! (`HOST_HELPER_URL`, `HOST_HELPER_STACK`, `HOST_HELPER_KEY`), which the
//! helper's `compose.override.yaml` hands the container. None of them set:
//! the admin page keeps showing the command, with a pointer to the
//! helper's install steps. The key is read once at start and never shown,
//! logged or written anywhere.

use std::sync::Arc;

use host_helper::protocol::{ENV_KEY, ENV_STACK, ENV_URL};
use host_helper::{Client, ClientError, RestartReply, UpdateReply, UpdateRequest};

/// Where the helper's install steps are, for the admin page's pointer.
pub const INSTALL_STEPS: &str =
    "https://github.com/snds/app-updates/blob/main/host-helper/README.md#install-on-unraid-sean";

/// What the admin page asks of the helper. The real one is the helper's
/// [`Client`]; tests give their own.
pub trait HostHelper: Send + Sync {
    /// Pulls the image at `request.tag` and recreates the server on it.
    fn update(&self, request: &UpdateRequest) -> Result<UpdateReply, ClientError>;
    /// Restarts the server's container.
    fn restart(&self) -> Result<RestartReply, ClientError>;
}

impl HostHelper for Client {
    fn update(&self, request: &UpdateRequest) -> Result<UpdateReply, ClientError> {
        Client::update(self, request)
    }

    fn restart(&self) -> Result<RestartReply, ClientError> {
        Client::restart(self)
    }
}

/// Whether one-click Update and Reload are set up.
#[derive(Clone, Default)]
pub enum Setting {
    /// No helper settings: the page shows the command and the pointer.
    #[default]
    Off,
    /// Some settings, but not usable: what's wrong, in words (never the key).
    Broken(String),
    /// Ready to ask.
    Ready(Arc<dyn HostHelper>),
}

impl std::fmt::Debug for Setting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Setting::Off => f.write_str("Off"),
            Setting::Broken(why) => write!(f, "Broken({why:?})"),
            Setting::Ready(_) => f.write_str("Ready"),
        }
    }
}

impl Setting {
    /// The settings from any lookup: `Off` when none of the three is set.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let set = |name: &str| var(name).is_some_and(|v| !v.trim().is_empty());
        if ![ENV_URL, ENV_STACK, ENV_KEY].into_iter().any(set) {
            return Setting::Off;
        }
        match Client::from_vars(var) {
            Ok(client) => Setting::Ready(Arc::new(client)),
            Err(e) => Setting::Broken(e.to_string()),
        }
    }

    /// The settings from this process's environment.
    pub fn from_env() -> Self {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Whether Update and Reload can be offered.
    pub fn ready(&self) -> bool {
        matches!(self, Setting::Ready(_))
    }
}

/// Update or Reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Update,
    Reload,
}

impl Kind {
    /// The word in the operator's log and the page.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Update => "update",
            Kind::Reload => "reload",
        }
    }
}

/// How the last Update or Reload went, as far as this process saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Asked; waiting for the helper (the server usually restarts first).
    Working,
    /// The helper answered and the server is still this process: it said
    /// what it did.
    Done(String),
    /// It didn't start, or the helper refused: why, in words.
    Failed(String),
}

/// The last Update or Reload the admin page asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub kind: Kind,
    /// The version asked for (Update).
    pub tag: Option<String>,
    /// When it was asked.
    pub at: i64,
    pub progress: Progress,
}

/// How long an action may stay `Working` before another may start: the
/// client's own limit for a change (a slow pull), plus a minute.
pub const WORKING_FOR: i64 = host_helper::client::CHANGE_TIMEOUT.as_secs() as i64 + 60;

/// What the helper's answer means for the operator, in plain words.
pub fn explain(kind: Kind, error: &ClientError) -> String {
    let ClientError::Refused { code, message, .. } = error else {
        // Settings, an unreachable helper or a reply that isn't the
        // helper's: the client's own sentence.
        return error.to_string();
    };
    match code.as_str() {
        "tag_not_allowed" => format!(
            "The host helper's allow-list doesn't allow that version, so nothing changed. {message}"
        ),
        "digest_mismatch" | "no_digest" | "bad_digest" => format!(
            "The image the host helper downloaded isn't the build the release names, so it wasn't used and nothing restarted. {message}"
        ),
        "busy" => "An update or reload is already running on the host. Wait for it to finish, then try again.".into(),
        "rate_limited" | "too_many_failures" => {
            "The host helper has had too many requests in the last minute. Wait a minute and try again.".into()
        }
        "stale" => "The server's clock and the host's differ by more than 90 seconds, so the host helper refused. Check the host's clock, then try again.".into(),
        "bad_signature" | "unsigned" | "unknown_stack" => format!(
            "The host helper doesn't recognise this server's key or stack name, so nothing changed. Make this server's key again (the helper's install step 2) and recreate the server. {message}"
        ),
        "replayed" => "The host helper refused a repeated request. Try again.".into(),
        "docker_failed" => format!(
            "Docker on the host couldn't finish the {}. {message}",
            kind.noun()
        ),
        _ => message.clone(),
    }
}

/// The sentence for a finished call that left this process running.
pub fn done_line(
    kind: Kind,
    update: Option<&UpdateReply>,
    restart: Option<&RestartReply>,
) -> String {
    match (kind, update, restart) {
        (Kind::Update, Some(u), _) => format!(
            "The host helper pulled {} ({}) and recreated {}. If the version above hasn't changed, the server was already running that build.",
            u.pulled,
            u.digest,
            u.services.join(", ")
        ),
        (Kind::Reload, _, Some(r)) => {
            format!("The host helper restarted {}.", r.services.join(", "))
        }
        _ => "The host helper finished.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(code: &str, message: &str) -> ClientError {
        ClientError::Refused {
            status: 403,
            code: code.into(),
            message: message.into(),
        }
    }

    #[test]
    fn no_settings_is_off_and_half_settings_say_whats_wrong() {
        assert!(matches!(Setting::from_vars(|_| None), Setting::Off));
        assert!(matches!(
            Setting::from_vars(|_| Some("  ".into())),
            Setting::Off
        ));
        let key = "ab".repeat(32);
        let vars = |url: &'static str, stack: &'static str, k: Option<String>| {
            move |name: &str| match name {
                ENV_URL => Some(url.to_string()),
                ENV_STACK => Some(stack.to_string()),
                ENV_KEY => k.clone(),
                _ => None,
            }
        };
        let ready = Setting::from_vars(vars(
            "http://host-helper:7480",
            "lockdown",
            Some(key.clone()),
        ));
        assert!(ready.ready());
        assert_eq!(format!("{ready:?}"), "Ready");
        match Setting::from_vars(vars("http://host-helper:7480", "lockdown", None)) {
            Setting::Broken(why) => assert!(why.contains("HOST_HELPER_KEY isn't set"), "{why}"),
            other => panic!("{other:?}"),
        }
        match Setting::from_vars(vars("ftp://host-helper", "lockdown", Some(key.clone()))) {
            Setting::Broken(why) => {
                assert!(why.contains("http://host-helper:7480"), "{why}");
                assert!(!why.contains(&key));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn refusals_read_as_plain_words() {
        let tag = explain(
            Kind::Update,
            &refused(
                "tag_not_allowed",
                "The tag \"x\" isn't allowed for lockdown.",
            ),
        );
        assert!(tag.starts_with("The host helper's allow-list doesn't allow that version"));
        assert!(tag.ends_with("isn't allowed for lockdown."));
        assert!(
            explain(Kind::Reload, &refused("busy", "…"))
                .starts_with("An update or reload is already running")
        );
        assert!(explain(Kind::Update, &refused("stale", "…")).contains("clock"));
        assert!(
            explain(
                Kind::Update,
                &refused("bad_signature", "Check HOST_HELPER_KEY.")
            )
            .contains("Make this server's key again")
        );
        assert!(
            explain(Kind::Update, &refused("digest_mismatch", "…")).contains("nothing restarted")
        );
        assert_eq!(
            explain(
                Kind::Reload,
                &refused("docker_failed", "Docker couldn't restart.")
            ),
            "Docker on the host couldn't finish the reload. Docker couldn't restart."
        );
        assert_eq!(
            explain(Kind::Reload, &refused("something_new", "A new sentence.")),
            "A new sentence."
        );
        let down = ClientError::Unreachable(
            "The host helper at host-helper:7480 can't be reached.".into(),
        );
        assert_eq!(explain(Kind::Update, &down), down.to_string());
    }
}
