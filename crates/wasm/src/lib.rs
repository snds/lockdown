//! `lockdown-wasm`: the facade the extension, Safari Web Extension, web vault
//! and Tauri web view call (03-architecture §3, §4 "extension modes", §8).
//!
//! **What is real:** the matcher boundary. `decide`, `fill_check` and
//! `related_origins` take and return plain JSON objects in the shape of
//! [`lockdown_matcher::wire`] (schema files under `extension/schema/`), and
//! `WasmSession` holds one-time-code secrets behind numeric handles so the
//! page never sees a seed. The rest of [`lockdown_ffi`]'s surface is
//! re-exported so a feature exists on every platform or on none (D9).
//!
//! **What crosses the boundary:** addresses, rules, verdicts and their
//! plain-language reasons, data-file versions, and handle numbers. Never a
//! key, a password or a code seed. The JS side gets `Uint8Array` views over
//! linear memory that the core zeroes on release; the offscreen document is
//! torn down on lock, not kept warm.
//!
//! The exported functions are thin: each parses its argument with
//! `serde-wasm-bindgen`, calls the Rust function, and serialises the result
//! with the JSON-compatible serialiser (plain objects, no `Map`). A failed
//! parse or a matcher error comes back as a thrown `Error` whose message is
//! the wire error's JSON, so the page can read `code` and `message`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use lockdown_codes::Code;
pub use lockdown_ffi::{FfiError, RedactedView, SecretHandle, SecretRegistry};
use lockdown_matcher::wire::{
    self, DataStatus, DecideRequest, DecideResponse, FillRequest, FillResponse,
    RelatedOriginsRequest, RelatedOriginsResponse, SCHEMA_VERSION, WireError,
};
use lockdown_matcher::{DataError, DataState, MatcherData, TrustedKeys};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

thread_local! {
    /// The matcher data in force for this module instance: the built-in
    /// signed file until the host loads a newer one. The downgrade floor
    /// lives here for the life of the instance; the host persists it
    /// (`matcher_data_status().newest_loaded`) and hands it back through
    /// `set_matcher_data_floor` after a restart.
    static DATA: RefCell<DataState> = RefCell::new(
        DataState::builtin().unwrap_or_else(|_| DataState::new(MatcherData::empty()))
    );
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    value
        .serialize(&ser)
        .map_err(|e| JsError::new(&e.to_string()))
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|e| {
        let err = WireError {
            v: SCHEMA_VERSION,
            code: "bad_request".into(),
            message: e.to_string(),
        };
        JsError::new(&serde_json::to_string(&err).unwrap_or_else(|_| e.to_string()))
    })
}

fn wire_err(err: WireError) -> JsError {
    JsError::new(&serde_json::to_string(&err).unwrap_or(err.message))
}

/// The schema version every response carries (`v`).
#[wasm_bindgen]
pub fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// The Rust side of `decide`, for native tests and callers.
pub fn decide_wire(req: &DecideRequest) -> Result<DecideResponse, WireError> {
    DATA.with(|d| wire::decide(req, d.borrow().data()))
}

/// The Rust side of `load_matcher_data`: verifies `bytes` against the
/// build's trusted keys, refuses a downgrade, and makes it the data in force.
pub fn load_matcher_data_wire(bytes: &[u8]) -> Result<DataStatus, DataError> {
    DATA.with(|d| {
        let mut d = d.borrow_mut();
        d.load(bytes, &TrustedKeys::builtin())?;
        Ok(DataStatus::of(&d))
    })
}

/// `load_matcher_data(bytes)` → `{v, version, newest_loaded, key_id,
/// published, groups, confusables}`. Throws a wire error with code
/// `bad_signature`, `unknown_key`, `downgrade` or `bad_data`; the data in
/// force is unchanged then.
#[wasm_bindgen]
pub fn load_matcher_data(bytes: &[u8]) -> Result<JsValue, JsError> {
    let status = load_matcher_data_wire(bytes).map_err(|e| wire_err(e.into()))?;
    to_js(&status)
}

/// The Rust side of `matcher_data_status`.
pub fn matcher_data_status_wire() -> DataStatus {
    DATA.with(|d| DataStatus::of(&d.borrow()))
}

/// `matcher_data_status()` → the same object `load_matcher_data` returns,
/// for the data in force now. `newest_loaded` is what the host persists.
#[wasm_bindgen]
pub fn matcher_data_status() -> Result<JsValue, JsError> {
    to_js(&matcher_data_status_wire())
}

/// `set_matcher_data_floor(version)`: the host's remembered newest version
/// from an earlier run. Never lowers the floor. Returns the floor in force.
#[wasm_bindgen]
pub fn set_matcher_data_floor(version: f64) -> Result<f64, JsError> {
    if !version.is_finite() || version < 0.0 {
        return Err(JsError::new("version must be a non-negative number"));
    }
    DATA.with(|d| {
        let mut d = d.borrow_mut();
        d.raise_floor(version as u64);
        Ok(d.newest_loaded() as f64)
    })
}

/// `decide({saved, current, rule?})` → `{v, decision, saved, current}`.
#[wasm_bindgen]
pub fn decide(request: JsValue) -> Result<JsValue, JsError> {
    let req: DecideRequest = from_js(request)?;
    to_js(&decide_wire(&req).map_err(wire_err)?)
}

/// The Rust side of `fill_check`.
pub fn fill_check_wire(req: &FillRequest) -> Result<FillResponse, WireError> {
    wire::fill_check(req)
}

/// `fill_check({top, frame, field_visible, user_confirmed, kind, decision})`
/// → `{v, verdict, reason?}`.
#[wasm_bindgen]
pub fn fill_check(request: JsValue) -> Result<JsValue, JsError> {
    let req: FillRequest = from_js(request)?;
    to_js(&fill_check_wire(&req).map_err(wire_err)?)
}

/// The Rust side of `related_origins`.
pub fn related_origins_wire(
    req: &RelatedOriginsRequest,
) -> Result<RelatedOriginsResponse, WireError> {
    wire::related_origins(req)
}

/// `related_origins({rp_id, origins, fetched_at, now, max_age_secs, origin})`
/// → `{v, allowed, effective}`.
#[wasm_bindgen]
pub fn related_origins(request: JsValue) -> Result<JsValue, JsError> {
    let req: RelatedOriginsRequest = from_js(request)?;
    to_js(&related_origins_wire(&req).map_err(wire_err)?)
}

/// What `code` returns: the digits to fill and how long they stay valid.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CodeResponse {
    /// Schema version.
    pub v: u32,
    /// The current code.
    pub code: String,
    /// Seconds until the next code; absent for counter-based codes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds_left: Option<u64>,
    /// The issuer named in the URI, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// The account named in the URI, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// The session the offscreen document holds while unlocked. Dropping it
/// wipes every secret.
#[wasm_bindgen]
#[derive(Default, Debug)]
pub struct WasmSession {
    secrets: SecretRegistry,
    next_code: u32,
    codes: BTreeMap<u32, Code>,
}

#[wasm_bindgen]
impl WasmSession {
    /// A fresh, empty session.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes an `otpauth://` or `steam://` URI into Rust memory and returns a
    /// handle. The URI is the one time the seed crosses; the page should
    /// drop its copy. Fails with the parser's message for a bad URI.
    pub fn import_code_uri(&mut self, uri: &str) -> Result<u32, JsError> {
        let code = lockdown_codes::parse_uri(uri).map_err(|e| JsError::new(&e.to_string()))?;
        code.validate().map_err(|e| JsError::new(&e.to_string()))?;
        self.next_code += 1;
        self.codes.insert(self.next_code, code);
        Ok(self.next_code)
    }

    /// `code(handle, now_secs)` → `{v, code, seconds_left?, issuer?, account?}`.
    /// `now_secs` is a JS number (seconds since the epoch), not a BigInt.
    pub fn code(&self, handle: u32, now_secs: f64) -> Result<JsValue, JsError> {
        if !now_secs.is_finite() || now_secs < 0.0 {
            return Err(JsError::new("now_secs must be a non-negative number"));
        }
        let res = self
            .code_at(handle, now_secs as u64)
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&res)
    }

    /// Forgets one code; its seed is wiped.
    pub fn forget_code(&mut self, handle: u32) -> bool {
        self.codes.remove(&handle).is_some()
    }

    /// How many secrets and codes are held.
    pub fn len(&self) -> usize {
        self.secrets.len() + self.codes.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lock: wipe everything. The page keeps no copy.
    pub fn lock(&mut self) {
        self.secrets.lock();
        self.codes.clear();
    }
}

impl WasmSession {
    /// The secret registry, for the Rust callers that share this session.
    pub fn secrets(&mut self) -> &mut SecretRegistry {
        &mut self.secrets
    }

    /// The Rust side of `code`.
    pub fn code_at(&self, handle: u32, now_secs: u64) -> Result<CodeResponse, FfiError> {
        let code = self.codes.get(&handle).ok_or(FfiError::InvalidHandle)?;
        let digits = code
            .generate_at(now_secs)
            .map_err(|e| FfiError::Unsupported(e.to_string()))?;
        Ok(CodeResponse {
            v: SCHEMA_VERSION,
            code: digits,
            seconds_left: code.next_code_in(now_secs).map(|d| d.as_secs()),
            issuer: code.issuer.clone(),
            account: code.account.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_matcher::{FillKind, MatchDecision, MatchRule};

    #[test]
    fn locking_wipes_the_session() {
        let mut s = WasmSession::new();
        let h = s.secrets().insert(b"k".to_vec());
        let c = s
            .import_code_uri("otpauth://totp/Example:me?secret=JBSWY3DPEHPK3PXP&issuer=Example")
            .unwrap();
        assert_eq!(s.len(), 2);
        let res = s.code_at(c, 59).unwrap();
        assert_eq!(res.code.len(), 6);
        assert_eq!(res.issuer.as_deref(), Some("Example"));
        s.lock();
        assert!(s.is_empty());
        assert_eq!(s.secrets().redacted(h), Err(FfiError::InvalidHandle));
        assert_eq!(s.code_at(c, 59), Err(FfiError::InvalidHandle));
    }

    #[test]
    fn loading_data_refuses_a_downgrade_and_keeps_the_floor() {
        let before = matcher_data_status_wire();
        assert_eq!(before.key_id, lockdown_matcher::signed::DEV_KEY_ID);
        // The built-in file loads again (same version).
        let again = load_matcher_data_wire(lockdown_matcher::signed::BUILTIN_SIGNED).unwrap();
        assert_eq!(again.version, before.version);
        // A tampered copy is refused and nothing changes.
        let mut bad = lockdown_matcher::signed::BUILTIN_SIGNED.to_vec();
        let mid = bad.len() / 2;
        bad[mid] ^= 0x01;
        let err = load_matcher_data_wire(&bad).unwrap_err();
        assert!(matches!(err.code(), "bad_signature" | "bad_data"));
        assert_eq!(matcher_data_status_wire().version, before.version);
        // A remembered floor above the built-in version makes the built-in
        // file a downgrade.
        DATA.with(|d| d.borrow_mut().raise_floor(before.version + 10));
        let err = load_matcher_data_wire(lockdown_matcher::signed::BUILTIN_SIGNED).unwrap_err();
        assert!(matches!(err, DataError::Downgrade { .. }));
        assert_eq!(
            matcher_data_status_wire().newest_loaded,
            before.version + 10
        );
    }

    #[test]
    fn the_wire_functions_answer_without_a_browser() {
        let res = decide_wire(&DecideRequest {
            saved: "https://www.paypal.com".into(),
            current: "https://paypal.com.evil.net".into(),
            rule: MatchRule::ExactHost,
        })
        .unwrap();
        assert_eq!(res.decision, MatchDecision::Lookalike);
        let fill = fill_check_wire(&FillRequest {
            top: "https://news.example".into(),
            frame: "https://login.bank.example".into(),
            field_visible: true,
            user_confirmed: false,
            kind: FillKind::Login,
            decision: MatchDecision::Exact,
        })
        .unwrap();
        assert_eq!(fill.verdict, wire::Verdict::Confirm);
        let err = decide_wire(&DecideRequest {
            saved: "".into(),
            current: "https://example.com".into(),
            rule: MatchRule::ExactHost,
        })
        .unwrap_err();
        assert_eq!(err.code, "not_a_url");
    }
}
