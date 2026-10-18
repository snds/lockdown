//! The extension's passkey requests (docs/plan/19-passkeys.md §4): the
//! core decides and signs; this module carries the request in and the
//! public answer out, and holds the one thing a page can't be trusted with,
//! the approval in the app's own window.
//!
//! When a site requires user verification and this machine's `Approve`
//! gate is the account password (no Touch ID or Windows Hello), the core
//! answers `approval_needed`. The request then waits here, at most two
//! minutes, while the app's window asks the person ("Sign in to github.com
//! as sean?") and runs the gate; the connection's thread blocks on it, the
//! core's worker does not. Approved, the core is asked again with the
//! method that verified; declined or unanswered, the page gets
//! `NotAllowedError`. The password never reaches the browser.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use lockdown_app::passkeys::{
    B64, CreateRequest, GetRequest, Outcome, PasskeyPage, Refusal as PasskeyRefusal,
    RefusalKind as RefusalName, Signal,
};
use lockdown_app::{AppError, AppResult, ApprovalKind, ErrorCode, GateInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::handler::{Conn, Refusal, args};
use super::{Endpoint, Shared, pairing::random};

/// How long a request waits for the person's answer in the app.
pub const APPROVAL_FOR: Duration = Duration::from_secs(120);

/// How many passkey approvals one browser may have waiting.
pub const MAX_APPROVALS: usize = 3;

struct Pending {
    id: String,
    client: String,
    browser: String,
    text: String,
    created: Instant,
    /// `Some(Some(method))` approved, `Some(None)` declined.
    answer: Option<Option<String>>,
}

/// A passkey request waiting for the person, as the app's dialog shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApprovalView {
    pub request_id: String,
    /// "Chrome on Linux".
    pub browser: String,
    /// "Sign in to github.com as sean".
    pub text: String,
    pub expires_in_secs: u64,
}

/// The passkey approvals waiting.
#[derive(Default)]
pub struct Approvals {
    list: Mutex<Vec<Pending>>,
    changed: Condvar,
}

impl Approvals {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Pending>> {
        self.list.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Holds a new request; answers its id.
    pub fn put(&self, client: &str, browser: &str, text: &str) -> Result<String, String> {
        let mut list = self.lock();
        list.retain(|p| p.created.elapsed() < APPROVAL_FOR);
        if list.iter().filter(|p| p.client == client).count() >= MAX_APPROVALS {
            return Err("too many passkey requests are waiting in Lockdown".into());
        }
        let mut id = [0u8; 16];
        random(&mut id)?;
        let id = data_encoding::HEXLOWER.encode(&id);
        list.push(Pending {
            id: id.clone(),
            client: client.into(),
            browser: browser.into(),
            text: text.into(),
            created: Instant::now(),
            answer: None,
        });
        Ok(id)
    }

    /// Waits for the answer to `id`: the method that approved it, or `None`
    /// when declined or not answered within `timeout`. The request is gone
    /// afterwards either way.
    pub fn wait(&self, id: &str, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        let mut list = self.lock();
        loop {
            let at = list.iter().position(|p| p.id == id)?;
            if let Some(answer) = list[at].answer.clone() {
                list.remove(at);
                return answer;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                list.retain(|p| p.id != id);
                return None;
            }
            list = self
                .changed
                .wait_timeout(list, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Every request still waiting, oldest first.
    pub fn pending(&self) -> Vec<ApprovalView> {
        let list = self.lock();
        list.iter()
            .filter(|p| p.answer.is_none() && p.created.elapsed() < APPROVAL_FOR)
            .map(|p| ApprovalView {
                request_id: p.id.clone(),
                browser: p.browser.clone(),
                text: p.text.clone(),
                expires_in_secs: APPROVAL_FOR.saturating_sub(p.created.elapsed()).as_secs(),
            })
            .collect()
    }

    /// The person's answer: the method that approved it, or `None`.
    pub fn answer(&self, id: &str, method: Option<String>) -> Result<String, String> {
        let mut list = self.lock();
        let p = list
            .iter_mut()
            .find(|p| p.id == id && p.answer.is_none())
            .ok_or("that request isn't waiting any more")?;
        p.answer = Some(method);
        let text = p.text.clone();
        drop(list);
        self.changed.notify_all();
        Ok(text)
    }
}

impl Endpoint {
    /// Passkey requests waiting for the person in the app.
    pub fn passkey_pending(&self) -> Vec<ApprovalView> {
        self.shared.approvals.pending()
    }

    /// The person's answer to a passkey request: an approval runs the
    /// `Approve` gate (the password typed in the app's window) first.
    pub fn passkey_answer(
        &self,
        request_id: &str,
        approve: bool,
        gate: GateInput,
    ) -> AppResult<()> {
        let text = self
            .shared
            .approvals
            .pending()
            .into_iter()
            .find(|p| p.request_id == request_id)
            .map(|p| p.text)
            .ok_or_else(|| AppError::refused("that request isn't waiting any more"))?;
        let method = if approve {
            let reason = text.to_lowercase();
            Some(
                self.shared
                    .app
                    .approve(ApprovalKind::Approve, &reason, gate)?
                    .method,
            )
        } else {
            None
        };
        self.shared
            .approvals
            .answer(request_id, method)
            .map(|_| ())
            .map_err(AppError::refused)
    }
}

#[derive(Deserialize)]
struct OptionsArgs {
    page: PasskeyPage,
    request: GetRequest,
}

#[derive(Deserialize)]
struct CreateArgs {
    page: PasskeyPage,
    request: CreateRequest,
}

#[derive(Deserialize)]
struct GetArgs {
    page: PasskeyPage,
    request: GetRequest,
    item_id: String,
    credential_id: B64,
    /// For the app's dialog: the account, as the menu showed it.
    #[serde(default)]
    user_name: String,
}

#[derive(Deserialize)]
struct SignalArgs {
    page: String,
    signal: Signal,
}

fn refused_json(r: &PasskeyRefusal) -> Value {
    json!({ "outcome": "refused", "name": r.name.dom_name(), "message": r.message })
}

fn outcome<T: Serialize>(o: Outcome<T>) -> Value {
    match o {
        Outcome::Done(t) => json!({ "outcome": "done", "result": t }),
        Outcome::Confirm(reason) => json!({ "outcome": "confirm", "reason": reason }),
        Outcome::Refused(r) => refused_json(&r),
    }
}

fn plain<T: Serialize>(r: Result<T, PasskeyRefusal>) -> Value {
    match r {
        Ok(t) => json!({ "outcome": "done", "result": t }),
        Err(r) => refused_json(&r),
    }
}

/// The page's host, for the dialog's words.
fn host_of(page: &PasskeyPage) -> String {
    lockdown_matcher::Origin::parse(&page.frame)
        .map(|o| o.host)
        .unwrap_or_default()
}

/// Runs a ceremony; when the core needs the password approved in the app,
/// waits for the person there and runs it again with what verified.
fn gated<T>(
    shared: &Shared,
    conn: &Conn,
    text: &str,
    run: impl Fn(Option<String>) -> AppResult<Outcome<T>>,
) -> Result<Outcome<T>, Refusal> {
    match run(None) {
        Err(e) if e.code == ErrorCode::ApprovalNeeded => {
            let client = conn.client.clone().unwrap_or_default();
            let browser = shared
                .pairings()
                .paired()
                .into_iter()
                .find(|p| p.client_id == client)
                .map(|p| p.name)
                .unwrap_or_else(|| "A browser".into());
            let id = shared
                .approvals
                .put(&client, &browser, text)
                .map_err(|m| Refusal::new("refused", m))?;
            match shared.approvals.wait(&id, APPROVAL_FOR) {
                Some(method) => run(Some(method)).map_err(Refusal::from),
                None => Ok(Outcome::Refused(PasskeyRefusal::new(
                    RefusalName::NotAllowed,
                    "this wasn't approved in Lockdown",
                ))),
            }
        }
        other => other.map_err(Refusal::from),
    }
}

/// Answers one passkey op.
pub fn answer(shared: &Shared, conn: &Conn, op: &str, frame: &[u8]) -> Result<Value, Refusal> {
    if !shared.app.passkeys_in_browser()? {
        return Err(Refusal::new(
            "passkeys_off",
            "Lockdown's passkeys are turned off for browsers in Settings",
        ));
    }
    match op {
        "passkey_options" => {
            let a: OptionsArgs = args(frame)?;
            Ok(plain(shared.app.passkey_options(a.page, a.request)?))
        }
        "passkey_plan" => {
            let a: CreateArgs = args(frame)?;
            Ok(plain(shared.app.passkey_plan(a.page, a.request)?))
        }
        "passkey_create" => {
            let a: CreateArgs = args(frame)?;
            let text = format!(
                "Save a passkey for {} as {}",
                a.request.rp_id.clone().unwrap_or_else(|| host_of(&a.page)),
                a.request.user_name
            );
            let o = gated(shared, conn, &text, |verified| {
                shared
                    .app
                    .passkey_create(a.page.clone(), a.request.clone(), verified)
            })?;
            Ok(outcome(o))
        }
        "passkey_get" => {
            let a: GetArgs = args(frame)?;
            let host = a.request.rp_id.clone().unwrap_or_else(|| host_of(&a.page));
            let text = if a.user_name.is_empty() {
                format!("Sign in to {host} with your passkey")
            } else {
                format!("Sign in to {host} as {}", a.user_name)
            };
            let o = gated(shared, conn, &text, |verified| {
                shared.app.passkey_get(
                    a.page.clone(),
                    a.request.clone(),
                    &a.item_id,
                    a.credential_id.clone(),
                    verified,
                )
            })?;
            Ok(outcome(o))
        }
        "passkey_signal" => {
            let a: SignalArgs = args(frame)?;
            Ok(match shared.app.passkey_signal(&a.page, a.signal)? {
                Ok(changed) => json!({ "outcome": "done", "result": { "changed": changed } }),
                Err(r) => refused_json(&r),
            })
        }
        other => Err(Refusal::new(
            "unknown_op",
            format!("'{other}' isn't something Lockdown answers"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_waits_for_the_answer_and_expires() {
        let a = std::sync::Arc::new(Approvals::default());
        let id = a
            .put("c1", "Chrome on Linux", "Sign in to example.com as me")
            .unwrap();
        assert_eq!(a.pending().len(), 1);
        assert_eq!(a.pending()[0].browser, "Chrome on Linux");
        let (b, id2) = (a.clone(), id.clone());
        let answerer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            b.answer(&id2, Some("password".into())).unwrap();
        });
        assert_eq!(
            a.wait(&id, Duration::from_secs(5)).as_deref(),
            Some("password")
        );
        answerer.join().unwrap();
        assert!(a.pending().is_empty(), "answered requests leave the list");
        assert!(a.answer(&id, None).is_err());

        // Declined, and unanswered.
        let id = a.put("c1", "b", "t").unwrap();
        a.answer(&id, None).unwrap();
        assert_eq!(a.wait(&id, Duration::from_secs(1)), None);
        let id = a.put("c1", "b", "t").unwrap();
        assert_eq!(a.wait(&id, Duration::from_millis(30)), None);
        assert!(a.pending().is_empty());

        // At most three waiting per browser.
        for _ in 0..MAX_APPROVALS {
            a.put("c2", "b", "t").unwrap();
        }
        assert!(a.put("c2", "b", "t").is_err());
        assert!(a.put("c3", "b", "t").is_ok());
    }
}
