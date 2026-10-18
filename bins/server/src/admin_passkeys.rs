//! Passkeys on the admin page (`official-self-host`; 05-self-host §10;
//! docs/plan/27-web-server-finish.md §3).
//!
//! Signed in with the operator key, the operator adds passkeys under
//! **Passkeys**; from then on the sign-in page also offers **Sign in with a
//! passkey**. The operator key stays: it is how the first passkey is added
//! and the way back in when every passkey is lost, and clearing it
//! (`admin clear-key`) still closes the page to passkeys too.
//!
//! The ceremonies are WebAuthn's, checked by the same relying-party code as
//! account security keys (`lockdown_passkey::verify`): the RP ID is the
//! request's host, the origin `https://` it (or `http://` on this machine);
//! user verification (a PIN or a biometric on the authenticator) is
//! required both when a passkey is added and at every sign-in, so a passkey
//! stands in for the key rather than beside it; a counter that goes
//! backwards is refused. Challenges live in memory for five minutes, once.
//! Sign-ins with a passkey are slowed by the same brake as the operator
//! key and recorded in the operator's log, as are passkeys added and
//! removed.
//!
//! The pages stay script-free but for [`SCRIPT`], allowed by its hash with
//! `connect-src 'self'`, which runs the browser's half of the ceremonies.
//! What is kept: each passkey's credential id, COSE public key, counter,
//! name and dates, in `operator_settings` (so backups carry them).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use lockdown_passkey::encode::{b64url, from_b64url};
use lockdown_passkey::verify::{Expected, verify_assertion, verify_registration};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::admin::{COOKIE, SESSION_SECS, form_ok, hidden, shell_page, signed_in, unavailable};
use crate::api::Peer;
use crate::db::params;
use crate::pages::{esc, see_other};
use crate::storage::{Result, SqlStorage, StorageError};

/// How long a ceremony waits for the browser.
pub const CEREMONY_SECS: i64 = 5 * 60;
/// The most passkeys the operator keeps.
pub const MAX_PASSKEYS: usize = 10;
const TIMEOUT_MS: u32 = 120_000;
const SETTING: &str = "operator_passkeys";

/// The browser's half of both ceremonies: no inline handlers, requests to
/// this origin only (the page's policy says so too).
pub const SCRIPT: &str = r#"(()=>{const b64=b=>btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g,"-").replace(/\//g,"_").replace(/=+$/,"");const un=s=>Uint8Array.from(atob(s.replace(/-/g,"+").replace(/_/g,"/")+"===".slice((s.length+3)%4)),c=>c.charCodeAt(0));const say=t=>{const e=document.getElementById("pk-status");if(e)e.textContent=t};async function post(u,b){const r=await fetch(u,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(b),credentials:"same-origin",redirect:"error"});const j=await r.json().catch(()=>({error:"the server's answer wasn't readable"}));if(!r.ok||j.error)throw new Error(j.error||("HTTP "+r.status));return j}const go=document.getElementById("pk-sign-in");if(go&&window.PublicKeyCredential){go.hidden=false;go.addEventListener("click",async()=>{try{say("Waiting for your passkey…");const o=await post("/admin/passkey/options",{});const c=await navigator.credentials.get({publicKey:{challenge:un(o.challenge),rpId:o.rp_id,allowCredentials:o.allow.map(id=>({type:"public-key",id:un(id)})),userVerification:"required",timeout:o.timeout}});const r=c.response;await post("/admin/passkey/sign-in",{pending:o.pending,credential_id:b64(c.rawId),client_data_json:b64(r.clientDataJSON),authenticator_data:b64(r.authenticatorData),signature:b64(r.signature)});location.assign("/admin")}catch(e){say(e.message||String(e))}})}const add=document.getElementById("pk-add");if(add&&window.PublicKeyCredential){add.disabled=false;add.addEventListener("click",async()=>{try{const name=document.getElementById("pk-name").value;say("Waiting for your passkey…");const o=await post("/admin/passkeys/options",{csrf:add.dataset.csrf});const c=await navigator.credentials.create({publicKey:{challenge:un(o.challenge),rp:{id:o.rp_id,name:"Lockdown server admin"},user:{id:un(o.user_id),name:"operator",displayName:"Lockdown server operator"},pubKeyCredParams:[{type:"public-key",alg:-7},{type:"public-key",alg:-8}],excludeCredentials:o.exclude.map(id=>({type:"public-key",id:un(id)})),authenticatorSelection:{residentKey:"preferred",userVerification:"required"},attestation:"none",timeout:o.timeout}});const r=c.response;await post("/admin/passkeys/add",{csrf:add.dataset.csrf,pending:o.pending,name,client_data_json:b64(r.clientDataJSON),attestation_object:b64(r.attestationObject)});location.reload()}catch(e){say(e.message||String(e))}})}})();"#;

/// One passkey the operator added.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorPasskey {
    /// The credential id.
    #[serde(with = "serde_bytes")]
    pub credential_id: Vec<u8>,
    /// The COSE public key, as the authenticator wrote it.
    #[serde(with = "serde_bytes")]
    pub cose_key: Vec<u8>,
    /// The counter last seen.
    pub sign_count: u32,
    /// The operator's name for it.
    pub name: String,
    /// When it was added.
    pub added_at: i64,
    /// When it last signed in.
    pub last_used: Option<i64>,
}

impl SqlStorage {
    /// The operator's passkeys, oldest first.
    pub fn operator_passkeys(&self) -> Result<Vec<OperatorPasskey>> {
        let bytes: Option<Vec<u8>> = self.conn().query_opt(
            "SELECT value FROM operator_settings WHERE name = ?1",
            params![SETTING],
            |r| r.get(0),
        )?;
        match bytes {
            None => Ok(Vec::new()),
            Some(b) => lockdown_crypto::cbor::from_slice(&b).map_err(|_| {
                StorageError::Backend("the stored operator passkeys didn't decode".into())
            }),
        }
    }

    /// Replaces the operator's passkeys.
    pub fn set_operator_passkeys(&self, keys: &[OperatorPasskey]) -> Result<()> {
        if keys.is_empty() {
            self.conn().execute(
                "DELETE FROM operator_settings WHERE name = ?1",
                params![SETTING],
            )?;
            return Ok(());
        }
        let bytes = lockdown_crypto::cbor::to_canonical(&keys.to_vec())
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        self.conn().execute(
            "INSERT INTO operator_settings (name, value) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET value = excluded.value",
            params![SETTING, bytes],
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct Ceremony {
    adding: bool,
    challenge: [u8; 32],
    rp_id: String,
    origins: Vec<String>,
    expires_at: i64,
}

/// Ceremonies waiting for the browser.
#[derive(Debug, Default)]
pub struct Ceremonies(Mutex<BTreeMap<[u8; 16], Ceremony>>);

impl Ceremonies {
    fn start(&self, c: Ceremony, now: i64) -> std::result::Result<String, String> {
        let id = lockdown_crypto::rand::random_16().map_err(|e| e.to_string())?;
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, w| w.expires_at > now);
        if map.len() >= 64 {
            return Err(
                "too many passkey ceremonies are waiting; try again in a few minutes".into(),
            );
        }
        map.insert(id, c);
        Ok(b64url(&id))
    }

    /// Takes the ceremony `pending` names, once.
    fn take(&self, pending: &str, adding: bool, now: i64) -> Option<Ceremony> {
        let id: [u8; 16] = from_b64url(pending)?.try_into().ok()?;
        let mut map = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let c = map.remove(&id)?;
        (c.adding == adding && c.expires_at > now).then_some(c)
    }
}

/// The admin page's passkey routes.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/passkeys", get(page))
        .route("/admin/passkeys/options", post(add_options))
        .route("/admin/passkeys/add", post(add))
        .route("/admin/passkeys/remove", post(remove))
        .route("/admin/passkey/options", post(sign_in_options))
        .route("/admin/passkey/sign-in", post(sign_in))
}

fn json(status: StatusCode, value: serde_json::Value) -> Response {
    let mut res = (status, value.to_string()).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn refuse(status: StatusCode, why: &str) -> Response {
    json(status, serde_json::json!({ "error": why }))
}

fn store(state: &AppState) -> Option<&SqlStorage> {
    state.storage.sql()
}

fn random32() -> std::result::Result<[u8; 32], String> {
    lockdown_crypto::rand::random_32().map_err(|e| e.to_string())
}

/// The relying party for this request: the RP ID and the origins accepted.
fn relying_party(headers: &HeaderMap) -> Option<(String, Vec<String>)> {
    crate::second_factor::relying_party(headers).ok()
}

fn no_host() -> Response {
    refuse(StatusCode::BAD_REQUEST, "the request names no host")
}

fn when(at: i64) -> String {
    crate::mail::when(at)
}

/// `GET /admin/passkeys`.
pub async fn page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    passkeys_page(&state, &csrf, None)
}

fn passkeys_page(state: &AppState, csrf: &[u8; 32], note: Option<&str>) -> Response {
    let Some(store) = store(state) else {
        return unavailable();
    };
    let Ok(keys) = store.operator_passkeys() else {
        return unavailable();
    };
    let mut out = String::new();
    if let Some(n) = note {
        out.push_str(&format!("<p class=\"card\" role=\"status\">{}</p>", esc(n)));
    }
    out.push_str("<p class=\"muted\">A passkey signs you in here instead of typing the operator key. It needs your PIN or biometric each time. The operator key keeps working, and is the way back in if every passkey is lost.</p>");
    if keys.is_empty() {
        out.push_str("<div class=\"card\"><p>No passkeys yet.</p></div>");
    } else {
        out.push_str("<div class=\"card scroll\"><table><thead><tr><th>Name</th><th>Added</th><th>Last used</th><th></th></tr></thead><tbody>");
        for k in &keys {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><form method=\"post\" action=\"/admin/passkeys/remove\" class=\"inline\">{}<input type=\"hidden\" name=\"id\" value=\"{}\"><button class=\"danger\">Remove</button></form></td></tr>",
                esc(&k.name),
                esc(&when(k.added_at)),
                esc(&k.last_used.map(when).unwrap_or_else(|| "never".into())),
                hidden(csrf),
                b64url(&k.credential_id),
            ));
        }
        out.push_str("</tbody></table></div>");
    }
    if keys.len() < MAX_PASSKEYS {
        out.push_str(&format!(
            "<h2>Add a passkey</h2><div class=\"card\"><label for=\"pk-name\">Name</label><input id=\"pk-name\" type=\"text\" maxlength=\"64\" value=\"Operator passkey\">\
             <p><button id=\"pk-add\" type=\"button\" data-csrf=\"{}\" disabled>Add a passkey</button></p><p id=\"pk-status\" class=\"muted\" role=\"status\"></p></div>",
            data_encoding::HEXLOWER.encode(csrf)
        ));
    }
    shell_page(csrf, "/admin/passkeys", "Passkeys", &out, Some(SCRIPT))
}

#[derive(Deserialize)]
struct CsrfBody {
    csrf: String,
}

fn csrf_ok(given: &str, csrf: &[u8; 32]) -> bool {
    let mut fields = BTreeMap::new();
    fields.insert("csrf".to_string(), given.to_string());
    form_ok(&fields, csrf)
}

/// `POST /admin/passkeys/options` (signed in): what the browser makes a
/// passkey with.
pub async fn add_options(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(csrf) = signed_in(&state, &headers) else {
        return refuse(StatusCode::UNAUTHORIZED, "sign in to the admin page first");
    };
    let Ok(b) = serde_json::from_slice::<CsrfBody>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "that isn't what this call takes");
    };
    if !csrf_ok(&b.csrf, &csrf) {
        return refuse(StatusCode::FORBIDDEN, "the page was out of date; reload it");
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let Ok(keys) = store.operator_passkeys() else {
        return unavailable();
    };
    if keys.len() >= MAX_PASSKEYS {
        return refuse(
            StatusCode::CONFLICT,
            "the operator already has ten passkeys; remove one first",
        );
    }
    let Some((rp_id, origins)) = relying_party(&headers) else {
        return no_host();
    };
    let (Ok(challenge), Ok(user)) = (random32(), random32()) else {
        return unavailable();
    };
    let now = state.now();
    let pending = match state.admin.passkeys.start(
        Ceremony {
            adding: true,
            challenge,
            rp_id: rp_id.clone(),
            origins,
            expires_at: now + CEREMONY_SECS,
        },
        now,
    ) {
        Ok(p) => p,
        Err(e) => return refuse(StatusCode::TOO_MANY_REQUESTS, &e),
    };
    json(
        StatusCode::OK,
        serde_json::json!({
            "pending": pending,
            "challenge": b64url(&challenge),
            "rp_id": rp_id,
            "user_id": b64url(&user[..16]),
            "exclude": keys.iter().map(|k| b64url(&k.credential_id)).collect::<Vec<_>>(),
            "timeout": TIMEOUT_MS,
        }),
    )
}

#[derive(Deserialize)]
struct AddBody {
    csrf: String,
    pending: String,
    name: String,
    client_data_json: String,
    attestation_object: String,
}

/// `POST /admin/passkeys/add` (signed in): the browser's new passkey.
pub async fn add(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let Ok(csrf) = signed_in(&state, &headers) else {
        return refuse(StatusCode::UNAUTHORIZED, "sign in to the admin page first");
    };
    let Ok(b) = serde_json::from_slice::<AddBody>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "that isn't what this call takes");
    };
    if !csrf_ok(&b.csrf, &csrf) {
        return refuse(StatusCode::FORBIDDEN, "the page was out of date; reload it");
    }
    let now = state.now();
    let Some(c) = state.admin.passkeys.take(&b.pending, true, now) else {
        return refuse(StatusCode::GONE, "that request ran out; try again");
    };
    let (Some(client_data), Some(attestation)) = (
        from_b64url(&b.client_data_json),
        from_b64url(&b.attestation_object),
    ) else {
        return refuse(StatusCode::BAD_REQUEST, "the passkey's answer is malformed");
    };
    let mut last = String::from("the passkey's answer didn't check out");
    let mut made = None;
    for origin in &c.origins {
        match verify_registration(
            &client_data,
            &attestation,
            &Expected {
                rp_id: &c.rp_id,
                origin,
                challenge: &c.challenge,
            },
        ) {
            Ok(r) => {
                made = Some(r);
                break;
            }
            Err(e) => last = e,
        }
    }
    let Some(made) = made else {
        return refuse(StatusCode::BAD_REQUEST, &last);
    };
    if !made.user_verified {
        return refuse(
            StatusCode::BAD_REQUEST,
            "the passkey didn't check it was you (a PIN or biometric); the admin page needs that",
        );
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let Ok(mut keys) = store.operator_passkeys() else {
        return unavailable();
    };
    if keys.len() >= MAX_PASSKEYS {
        return refuse(
            StatusCode::CONFLICT,
            "the operator already has ten passkeys; remove one first",
        );
    }
    if keys.iter().any(|k| k.credential_id == made.credential_id) {
        return refuse(StatusCode::CONFLICT, "that passkey is already added");
    }
    let name: String = b
        .name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let name = if name.is_empty() {
        "Operator passkey".to_string()
    } else {
        name
    };
    keys.push(OperatorPasskey {
        credential_id: made.credential_id,
        cose_key: made.cose_key,
        sign_count: made.sign_count,
        name: name.clone(),
        added_at: now,
        last_used: None,
    });
    if store.set_operator_passkeys(&keys).is_err() {
        return unavailable();
    }
    let _ = store.record_event(
        "passkey",
        true,
        &format!("passkey \u{201c}{name}\u{201d} added"),
        now,
    );
    json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

/// `POST /admin/passkeys/remove` (a form: `csrf`, `id`).
pub async fn remove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let csrf = match signed_in(&state, &headers) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fields = crate::pages::form_fields(&body);
    if !form_ok(&fields, &csrf) {
        return passkeys_page(
            &state,
            &csrf,
            Some("The page was out of date; nothing changed."),
        );
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    let Ok(mut keys) = store.operator_passkeys() else {
        return unavailable();
    };
    let id = fields
        .get("id")
        .and_then(|i| from_b64url(i))
        .unwrap_or_default();
    let before = keys.len();
    let name = keys
        .iter()
        .find(|k| k.credential_id == id)
        .map(|k| k.name.clone());
    keys.retain(|k| k.credential_id != id);
    if keys.len() == before {
        return passkeys_page(&state, &csrf, Some("That passkey isn't here."));
    }
    if store.set_operator_passkeys(&keys).is_err() {
        return unavailable();
    }
    let _ = store.record_event(
        "passkey",
        true,
        &format!(
            "passkey \u{201c}{}\u{201d} removed",
            name.unwrap_or_default()
        ),
        state.now(),
    );
    see_other("/admin/passkeys")
}

/// `POST /admin/passkey/options`: what the browser signs in with.
pub async fn sign_in_options(
    State(state): State<Arc<AppState>>,
    peer: Peer,
    headers: HeaderMap,
) -> Response {
    if crate::api::brake(&state, &peer, Some("admin:operator")).is_err() {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many tries. Wait a minute and try again.",
        );
    }
    let Some(store) = store(&state) else {
        return unavailable();
    };
    match store.has_operator_key() {
        Ok(true) => {}
        _ => {
            return refuse(
                StatusCode::FORBIDDEN,
                "the admin page is closed: no operator key is set",
            );
        }
    }
    let Ok(keys) = store.operator_passkeys() else {
        return unavailable();
    };
    if keys.is_empty() {
        return refuse(
            StatusCode::NOT_FOUND,
            "no passkey has been added yet; sign in with the operator key and add one under Passkeys",
        );
    }
    let Some((rp_id, origins)) = relying_party(&headers) else {
        return no_host();
    };
    let Ok(challenge) = random32() else {
        return unavailable();
    };
    let now = state.now();
    let pending = match state.admin.passkeys.start(
        Ceremony {
            adding: false,
            challenge,
            rp_id: rp_id.clone(),
            origins,
            expires_at: now + CEREMONY_SECS,
        },
        now,
    ) {
        Ok(p) => p,
        Err(e) => return refuse(StatusCode::TOO_MANY_REQUESTS, &e),
    };
    json(
        StatusCode::OK,
        serde_json::json!({
            "pending": pending,
            "challenge": b64url(&challenge),
            "rp_id": rp_id,
            "allow": keys.iter().map(|k| b64url(&k.credential_id)).collect::<Vec<_>>(),
            "timeout": TIMEOUT_MS,
        }),
    )
}

#[derive(Deserialize)]
struct SignInBody {
    pending: String,
    credential_id: String,
    client_data_json: String,
    authenticator_data: String,
    signature: String,
}

/// `POST /admin/passkey/sign-in`: a passkey's answer; a right one sets the
/// admin cookie, exactly as the operator key does.
pub async fn sign_in(State(state): State<Arc<AppState>>, peer: Peer, body: Bytes) -> Response {
    if crate::api::brake(&state, &peer, Some("admin:operator")).is_err() {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many tries. Wait a minute and try again.",
        );
    }
    let Ok(b) = serde_json::from_slice::<SignInBody>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "that isn't what this call takes");
    };
    let now = state.now();
    let Some(c) = state.admin.passkeys.take(&b.pending, false, now) else {
        return refuse(StatusCode::GONE, "that request ran out; try again");
    };
    let Some(store) = store(&state) else {
        return unavailable();
    };
    match store.has_operator_key() {
        Ok(true) => {}
        _ => {
            return refuse(
                StatusCode::FORBIDDEN,
                "the admin page is closed: no operator key is set",
            );
        }
    }
    let Ok(mut keys) = store.operator_passkeys() else {
        return unavailable();
    };
    let decoded = (
        from_b64url(&b.credential_id),
        from_b64url(&b.client_data_json),
        from_b64url(&b.authenticator_data),
        from_b64url(&b.signature),
    );
    let (Some(id), Some(client_data), Some(auth_data), Some(sig)) = decoded else {
        return refuse(StatusCode::BAD_REQUEST, "the passkey's answer is malformed");
    };
    let failed = |why: &str| {
        let _ = store.record_event(
            "sign_in",
            false,
            &format!("a passkey sign-in failed from {}: {why}", peer.0),
            now,
        );
        eprintln!("lockdown-server: a passkey sign-in to the admin page failed");
        refuse(StatusCode::UNAUTHORIZED, "That passkey didn't sign in.")
    };
    let Some(key) = keys.iter_mut().find(|k| k.credential_id == id) else {
        return failed("a passkey this server doesn't hold");
    };
    let mut asserted = None;
    let mut last = String::new();
    for origin in &c.origins {
        match verify_assertion(
            &client_data,
            &auth_data,
            &sig,
            &key.cose_key,
            key.sign_count,
            &Expected {
                rp_id: &c.rp_id,
                origin,
                challenge: &c.challenge,
            },
        ) {
            Ok(a) => {
                asserted = Some(a);
                break;
            }
            Err(e) => last = e,
        }
    }
    let Some(a) = asserted else {
        return failed(&last);
    };
    if !a.user_verified {
        return failed("the passkey didn't check it was the operator (no PIN or biometric)");
    }
    key.sign_count = a.sign_count;
    key.last_used = Some(now);
    let name = key.name.clone();
    if store.set_operator_passkeys(&keys).is_err() {
        return unavailable();
    }
    let Ok((cookie, _)) = state.admin.open(now) else {
        return unavailable();
    };
    let _ = store.record_event(
        "sign_in",
        true,
        &format!(
            "the operator signed in with the passkey \u{201c}{name}\u{201d} from {}",
            peer.0
        ),
        now,
    );
    let mut response = json(StatusCode::OK, serde_json::json!({ "ok": true }));
    if let Ok(v) = HeaderValue::from_str(&format!(
        "{COOKIE}={cookie}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={SESSION_SECS}"
    )) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ceremony_is_taken_once_for_its_own_kind_and_runs_out() {
        let c = Ceremonies::default();
        let make = |adding| Ceremony {
            adding,
            challenge: [1; 32],
            rp_id: "vault.example".into(),
            origins: vec!["https://vault.example".into()],
            expires_at: 100,
        };
        let id = c.start(make(false), 0).unwrap();
        assert!(
            c.take(&id, true, 0).is_none(),
            "the wrong kind takes nothing"
        );
        let id = c.start(make(false), 0).unwrap();
        assert!(c.take(&id, false, 0).is_some());
        assert!(c.take(&id, false, 0).is_none(), "once");
        let id = c.start(make(true), 0).unwrap();
        assert!(c.take(&id, true, 100).is_none(), "ran out");
        assert!(c.take("not an id", true, 0).is_none());
    }

    #[test]
    fn the_script_is_allowed_by_its_hash_and_nothing_else() {
        let csp = crate::pages::csp_with_script(SCRIPT);
        assert!(csp.contains("script-src 'sha256-"));
        assert!(csp.contains("connect-src 'self'"));
        assert!(!csp.contains("unsafe-inline"));
        assert!(!SCRIPT.contains("onclick") && !SCRIPT.contains("eval("));
    }
}
