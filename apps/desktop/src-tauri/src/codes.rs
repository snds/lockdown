//! "Next code" on a counter-based (HOTP) code, as the webview calls it
//! (`advance_code`). Kept out of the `shell` feature so it is unit-tested
//! without Tauri: the command is this function on a worker thread.
//!
//! The answer is JSON with the new code, the counter it was made from and
//! the item's new version, never the set-up:
//!
//! ```json
//! { "code": "287082", "counter": 1, "version": 2 }
//! ```

use lockdown_app::{App, AppResult, CodeAdvanced, GateInput};

/// Moves the code in `field_id` of `item_id` on by one. A missing `gate`
/// is the empty one: a crown jewel's step then comes back
/// `approval_needed`, and the webview asks and calls again, as for a copy.
pub fn advance_code(
    app: &App,
    item_id: &str,
    field_id: &str,
    gate: Option<GateInput>,
) -> AppResult<CodeAdvanced> {
    app.advance_code(item_id, field_id, gate.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_app::{ErrorCode, LockPolicy, Paths};

    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn fresh() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths, None, LockPolicy::default());
        app.open_or_create(true).unwrap();
        (dir, app)
    }

    #[test]
    fn the_webview_gets_the_new_code_and_counter_and_never_the_set_up() {
        let (_dir, app) = fresh();
        let id = app
            .add_code(
                &format!("otpauth://hotp/Example:alice?secret={SEED}&counter=0"),
                None,
                None,
            )
            .unwrap();
        let step = advance_code(&app, &id, "code.0", None).unwrap();
        let json = serde_json::to_value(&step).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "code": "287082", "counter": 1, "version": 2 })
        );
        let text = json.to_string();
        assert!(!text.contains(SEED) && !text.contains("otpauth"), "{text}");

        // The counter is stored: locked and unlocked, the next step is 2.
        app.lock().unwrap();
        app.unlock().unwrap();
        let step = advance_code(&app, &id, "code.0", None).unwrap();
        assert_eq!((step.code.as_str(), step.counter), ("359152", 2));
    }

    #[test]
    fn a_time_based_code_is_refused_in_the_cores_words() {
        let (_dir, app) = fresh();
        for uri in [
            format!("otpauth://totp/Ex:me?secret={SEED}"),
            format!("otpauth://totp/Steam:me?secret={SEED}&issuer=Steam"),
        ] {
            let id = app.add_code(&uri, None, None).unwrap();
            let err = advance_code(&app, &id, "code.0", None).unwrap_err();
            assert_eq!(err.code, ErrorCode::Refused);
            assert!(err.message.contains("follows the clock"), "{}", err.message);
        }
    }
}
