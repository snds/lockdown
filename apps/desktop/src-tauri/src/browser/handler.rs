//! One request from the extension, answered (docs/plan/16-browser-extension.md
//! §4, the message table). Every request is `{"id", "op", …}`; every answer
//! is `{"id", "ok": true, "result"}` or `{"id", "ok": false, "error":
//! {"code", "message"}}`. Until the connection proves a pairing, only
//! `status`, `pair_begin`, `pair_status` and `auth` are answered.
//!
//! A value to fill leaves here once: serialised straight from the core's
//! bytes into the answer frame, which the caller wipes after sending.

use lockdown_app::browser::{FillOutcome, FillWhat, PageFacts, SaveProposal};
use lockdown_app::{App, AppError, ErrorCode, GateInput, Revealed};
use lockdown_native_messaging::caller::Caller;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::{Zeroize, Zeroizing};

use super::Shared;
use super::pairing::PairState;

/// The most search results one answer carries.
pub const MAX_RESULTS: usize = 50;

/// What one connection knows.
pub struct Conn {
    pub caller: Caller,
    pub challenge: [u8; 32],
    /// The paired client this connection proved, once it has.
    pub client: Option<String>,
}

/// An error answer: a code the extension switches on and plain words.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl Refusal {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<AppError> for Refusal {
    fn from(e: AppError) -> Self {
        match e.code {
            ErrorCode::Locked => Self::new("locked", "Lockdown is locked; unlock it to fill"),
            ErrorCode::NotSetUp => {
                Self::new("not_set_up", "Lockdown isn't set up on this computer yet")
            }
            ErrorCode::ApprovalNeeded => Self::new(
                "approval_needed",
                "this item is protected; open Lockdown to use it there",
            ),
            ErrorCode::Stopped => Self::new("closed", e.message),
            ErrorCode::Refused => Self::new("refused", e.message),
        }
    }
}

#[derive(Deserialize)]
struct Head {
    id: Value,
    op: String,
}

#[derive(Deserialize)]
struct PairBegin {
    client_id: String,
    client_key: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct ClientRef {
    client_id: String,
}

#[derive(Deserialize)]
struct Auth {
    client_id: String,
    proof: String,
}

#[derive(Deserialize)]
struct Matches {
    frame: String,
}

#[derive(Deserialize)]
struct Fill {
    item_id: String,
    what: FillWhat,
    #[serde(default)]
    field: Option<String>,
    top: String,
    frame: String,
    field_visible: bool,
    #[serde(default)]
    user_confirmed: bool,
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct ItemRef {
    item_id: String,
}

#[derive(Deserialize)]
struct OfferSave {
    page: String,
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct OfferRef {
    offer_id: String,
}

#[derive(Deserialize)]
struct BasicAuth {
    url: String,
}

/// One value in a fill's answer, borrowed from the core's bytes.
#[derive(Serialize)]
struct ValueOut<'a> {
    role: &'a str,
    value: &'a str,
}

pub(crate) fn args<T: for<'de> Deserialize<'de>>(frame: &[u8]) -> Result<T, Refusal> {
    serde_json::from_slice(frame).map_err(|e| Refusal::new("bad_request", e.to_string()))
}

/// Answers one frame. The result is the answer frame; the caller sends it
/// and wipes it.
pub fn handle(shared: &Shared, conn: &mut Conn, frame: &[u8]) -> Zeroizing<Vec<u8>> {
    let head: Head = match serde_json::from_slice(frame) {
        Ok(h) => h,
        Err(e) => return error_frame(&Value::Null, &Refusal::new("bad_request", e.to_string())),
    };
    if let Some(client) = &conn.client
        && !shared.pairings().is_paired(client)
    {
        conn.client = None;
    }
    let answer = match head.op.as_str() {
        "status" => Ok(status(shared, conn)),
        "pair_begin" => pair_begin(shared, conn, frame),
        "pair_status" => args::<ClientRef>(frame)
            .map(|a| json!({ "state": shared.pairings().state(&a.client_id) })),
        "auth" => auth(shared, conn, frame),
        _ if conn.client.is_none() => Err(Refusal::new(
            "not_paired",
            "pair this browser with Lockdown first",
        )),
        "matches" => args::<Matches>(frame).and_then(|a| {
            let m = shared.app.page_matches(&a.frame)?;
            Ok(json!(m))
        }),
        "wallet" => shared
            .app
            .wallet()
            .map(|w| json!({ "entries": w }))
            .map_err(Refusal::from),
        "fill" => return fill(shared, &head.id, frame),
        "basic_auth" => return basic_auth(shared, &head.id, frame),
        "search" => args::<Search>(frame).and_then(|a| search(&shared.app, &a.query, a.limit)),
        "copy" => args::<ItemRef>(frame).and_then(|a| copy(shared, &a.item_id)),
        "offer_save" => offer_save(shared, conn, frame),
        "save" => args::<OfferRef>(frame).and_then(|a| save(shared, conn, &a.offer_id)),
        "dismiss" => args::<OfferRef>(frame).map(|a| {
            shared.offers().remove(&a.offer_id);
            json!({})
        }),
        "passkey_options" | "passkey_plan" | "passkey_create" | "passkey_get"
        | "passkey_signal" => super::passkeys::answer(shared, conn, &head.op, frame),
        // A strong password for a new-password field (docs/plan/21-vault-depth.md):
        // the rules it follows, then the value itself.
        "password_rule" => args::<Matches>(frame).and_then(|a| {
            let p = shared.app.password_suggestion(&a.frame)?;
            Ok(json!(p))
        }),
        "suggest" => return suggest(shared, &head.id, frame),
        "federation_hints" | "federation_seen" | "site_fixes" | "fix_set" | "change_link"
        | "alias_options" | "alias_create" => super::bridges::answer(shared, &head.op, frame),
        other => Err(Refusal::new(
            "unknown_op",
            format!("'{other}' isn't something Lockdown answers"),
        )),
    };
    match answer {
        Ok(result) => ok_frame(&head.id, &result),
        Err(r) => error_frame(&head.id, &r),
    }
}

fn ok_frame(id: &Value, result: &Value) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(
        serde_json::to_vec(&json!({ "id": id, "ok": true, "result": result })).unwrap_or_default(),
    )
}

/// An error answer for request `id`.
pub fn error_frame(id: &Value, r: &Refusal) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(
        serde_json::to_vec(
            &json!({ "id": id, "ok": false, "error": { "code": r.code, "message": r.message } }),
        )
        .unwrap_or_default(),
    )
}

fn status(shared: &Shared, conn: &Conn) -> Value {
    let locked = shared.app.lock_state().map(|s| s.locked).unwrap_or(true);
    json!({
        "locked": locked,
        "paired": conn.client.is_some(),
        "app": format!("Lockdown {}", env!("CARGO_PKG_VERSION")),
        // Whether the extension offers Lockdown as the passkey provider
        // (docs/plan/19-passkeys.md): the person's setting; no Lockdown OS
        // provider exists on any platform yet (D122).
        "passkeys": shared.app.passkeys_in_browser().unwrap_or(false),
    })
}

fn pair_begin(shared: &Shared, conn: &Conn, frame: &[u8]) -> Result<Value, Refusal> {
    let mut a: PairBegin = args(frame)?;
    let begun = shared
        .pairings()
        .begin(&conn.caller, &a.client_id, &a.client_key, &a.name);
    a.client_key.zeroize();
    let (request_id, code) = begun.map_err(|m| Refusal::new("refused", m))?;
    Ok(json!({ "request_id": request_id, "code": code }))
}

fn auth(shared: &Shared, conn: &mut Conn, frame: &[u8]) -> Result<Value, Refusal> {
    let a: Auth = args(frame)?;
    let mut pairings = shared.pairings();
    if pairings.verify(&conn.caller, &a.client_id, &conn.challenge, &a.proof) {
        conn.client = Some(a.client_id);
        Ok(json!({ "paired": true }))
    } else {
        conn.client = None;
        let state = pairings.state(&a.client_id);
        Err(Refusal::new(
            "not_paired",
            if state == PairState::Paired {
                "the pairing proof didn't check out; pair this browser again"
            } else {
                "this browser isn't paired with Lockdown"
            },
        ))
    }
}

fn fill_values(id: &Value, values: &[lockdown_app::browser::FillValue]) -> Zeroizing<Vec<u8>> {
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        match std::str::from_utf8(v.value.expose()) {
            Ok(text) => out.push(ValueOut {
                role: &v.role,
                value: text,
            }),
            Err(_) => {
                return error_frame(id, &Refusal::new("refused", "a value to fill isn't text"));
            }
        }
    }
    #[derive(Serialize)]
    struct Answer<'a> {
        id: &'a Value,
        ok: bool,
        result: Result_<'a>,
    }
    #[derive(Serialize)]
    struct Result_<'a> {
        verdict: &'static str,
        values: Vec<ValueOut<'a>>,
    }
    let answer = Answer {
        id,
        ok: true,
        result: Result_ {
            verdict: "fill",
            values: out,
        },
    };
    Zeroizing::new(serde_json::to_vec(&answer).unwrap_or_default())
}

fn verdict_frame(id: &Value, outcome: FillOutcome) -> Zeroizing<Vec<u8>> {
    match outcome {
        FillOutcome::Fill(values) => fill_values(id, &values),
        FillOutcome::Confirm(reason) => {
            ok_frame(id, &json!({ "verdict": "confirm", "reason": reason }))
        }
        FillOutcome::Refuse(reason) => {
            ok_frame(id, &json!({ "verdict": "refuse", "reason": reason }))
        }
    }
}

fn fill(shared: &Shared, id: &Value, frame: &[u8]) -> Zeroizing<Vec<u8>> {
    let a: Fill = match args(frame) {
        Ok(a) => a,
        Err(r) => return error_frame(id, &r),
    };
    let facts = PageFacts {
        top: a.top,
        frame: a.frame,
        field_visible: a.field_visible,
        user_confirmed: a.user_confirmed,
    };
    match shared
        .app
        .fill(&a.item_id, a.what, a.field.as_deref(), facts)
    {
        Ok(outcome) => verdict_frame(id, outcome),
        Err(e) => error_frame(id, &e.into()),
    }
}

/// HTTP Basic Auth: the one login whose saved address is this exact host
/// and that holds a password, or nothing (the browser's own prompt).
fn basic_auth(shared: &Shared, id: &Value, frame: &[u8]) -> Zeroizing<Vec<u8>> {
    let a: BasicAuth = match args(frame) {
        Ok(a) => a,
        Err(r) => return error_frame(id, &r),
    };
    let matches = match shared.app.page_matches(&a.url) {
        Ok(m) => m,
        Err(e) => return error_frame(id, &e.into()),
    };
    let exact: Vec<_> = matches
        .logins
        .iter()
        .filter(|c| c.password && c.decision == lockdown_matcher::MatchDecision::Exact)
        .collect();
    if exact.len() != 1 {
        return ok_frame(
            id,
            &json!({
                "verdict": "none",
                "reason": if exact.is_empty() {
                    "no saved login is for exactly this site"
                } else {
                    "more than one login is saved for this site; Lockdown leaves the choice to you"
                },
            }),
        );
    }
    let facts = PageFacts {
        top: a.url.clone(),
        frame: a.url,
        // The browser's own sign-in prompt, not a page: nothing can cover it.
        field_visible: true,
        user_confirmed: false,
    };
    match shared
        .app
        .fill(&exact[0].item_id, FillWhat::Login, None, facts)
    {
        Ok(outcome) => verdict_frame(id, outcome),
        Err(e) => error_frame(id, &e.into()),
    }
}

/// The search over item summaries, through the core's quick-access index
/// (`lockdown_app::quick`, docs/plan/26-desktop-finish.md §1): ranked,
/// forgiving a typo, live items only, a crown jewel by title and kind
/// (D113), the same answer the desktop's palette gets. The answer's shape
/// is the popup's, as before (D62).
pub fn search(app: &App, query: &str, limit: Option<usize>) -> Result<Value, Refusal> {
    let limit = limit.unwrap_or(MAX_RESULTS).clamp(1, MAX_RESULTS);
    let items: Vec<Value> = app
        .search(query, limit)?
        .into_iter()
        .map(|hit| {
            let row = hit.item;
            let host = lockdown_matcher::Origin::parse(&row.site)
                .map(|o| o.host)
                .unwrap_or_default();
            json!({
                "item_id": row.id,
                "kind": row.kind,
                "title": row.title,
                "username": row.username,
                "host": host,
                "subtitle": row.subtitle,
                "crown_jewel": row.crown_jewel,
                "copy": row.copy.as_ref().map(|c| json!({ "label": c.label, "how": c.how, "field_id": c.field_id })),
                "exact": hit.exact,
            })
        })
        .collect();
    Ok(json!({ "items": items }))
}

/// Copies an item's one quick copy on the desktop's clipboard path, with
/// its timer: the value never reaches the browser.
fn copy(shared: &Shared, item_id: &str) -> Result<Value, Refusal> {
    let rows = shared.app.list()?;
    let row = rows
        .into_iter()
        .find(|r| r.id == item_id && r.state == "live")
        .ok_or_else(|| Refusal::new("not_found", "that item isn't in the vault"))?;
    let target = row
        .copy
        .ok_or_else(|| Refusal::new("refused", "this item has nothing to copy"))?;
    let value: Revealed = match target.how.as_str() {
        "code" => shared
            .app
            .code_now(item_id, &target.field_id, false, GateInput::default())?,
        _ => shared
            .app
            .reveal_to_use(item_id, &target.field_id, GateInput::default())?,
    };
    let clears_in_secs = shared
        .copy
        .copy(value)
        .map_err(|m| Refusal::new("refused", m))?;
    Ok(json!({ "label": target.label, "clears_in_secs": clears_in_secs }))
}

fn offer_save(shared: &Shared, conn: &Conn, frame: &[u8]) -> Result<Value, Refusal> {
    let mut a: OfferSave = args(frame)?;
    let password = Zeroizing::new(std::mem::take(&mut a.password).into_bytes());
    if password.is_empty() {
        return Err(Refusal::new("bad_request", "there is no password to save"));
    }
    let proposal = shared.app.save_proposal(&a.page, &a.username, &password)?;
    let (kind, title, update) = match proposal {
        SaveProposal::Same { title, .. } => return Ok(json!({ "kind": "same", "title": title })),
        SaveProposal::New { title } => ("new", title, None),
        SaveProposal::Update { item_id, title } => ("update", title, Some(item_id)),
    };
    let client = conn.client.clone().unwrap_or_default();
    let offer_id = shared
        .offers()
        .put(client, a.page, a.username.clone(), password, update)
        .map_err(|m| Refusal::new("refused", m))?;
    Ok(json!({ "offer_id": offer_id, "kind": kind, "title": title, "username": a.username }))
}

fn save(shared: &Shared, conn: &Conn, offer_id: &str) -> Result<Value, Refusal> {
    let client = conn.client.clone().unwrap_or_default();
    let offer = shared
        .offers()
        .take(&client, offer_id)
        .ok_or_else(|| Refusal::new("expired", "that save offer has expired"))?;
    let kind = if offer.update.is_some() {
        "update"
    } else {
        "new"
    };
    let item_id = shared.app.save_login(
        &offer.page,
        &offer.username,
        offer.password.to_vec(),
        offer.update.as_deref(),
    )?;
    Ok(json!({ "item_id": item_id, "kind": kind }))
}

/// A new strong password for a sign-up or change-password field in
/// `frame`, under that site's rules (D236). It is made, not read from the
/// vault, and the answer frame is wiped once sent.
fn suggest(shared: &Shared, id: &Value, frame: &[u8]) -> Zeroizing<Vec<u8>> {
    let a: Matches = match args(frame) {
        Ok(a) => a,
        Err(r) => return error_frame(id, &r),
    };
    let (value, preview) = match shared.app.suggest_password(&a.frame) {
        Ok(v) => v,
        Err(e) => return error_frame(id, &e.into()),
    };
    #[derive(Serialize)]
    struct Answer<'a> {
        id: &'a Value,
        ok: bool,
        result: Suggested<'a>,
    }
    #[derive(Serialize)]
    struct Suggested<'a> {
        value: &'a str,
        words: &'a str,
        site: Option<&'a str>,
    }
    let answer = Answer {
        id,
        ok: true,
        result: Suggested {
            value: value.as_str(),
            words: &preview.words,
            site: preview.site.as_ref().map(|s| s.host.as_str()),
        },
    };
    Zeroizing::new(serde_json::to_vec(&answer).unwrap_or_default())
}
