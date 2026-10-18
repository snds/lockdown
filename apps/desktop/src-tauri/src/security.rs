//! The security center as the webview calls it (`security_*` commands).
//! Kept out of the `shell` feature so it is unit-tested without Tauri: a
//! command is one of these functions on a worker thread.
//!
//! What the webview gets is the core's report (findings in words, item
//! ids and titles, counts), never a password, a hash or a reuse key; and
//! "Change password" opens the page from here, so the webview never
//! launches anything itself.

use lockdown_app::security::ChangeLink;
use lockdown_app::{App, AppError, AppResult};

/// Works out the change-password page for `item_id` and hands it to `open`
/// (the shell's browser opener). Only http and https are ever opened.
pub fn change_password(
    app: &App,
    item_id: &str,
    open: impl FnOnce(&str) -> Result<(), String>,
) -> AppResult<ChangeLink> {
    let link = app.security_change_link(item_id)?;
    if !(link.url.starts_with("https://") || link.url.starts_with("http://")) {
        return Err(AppError::refused(
            "only http and https links open from here",
        ));
    }
    open(&link.url).map_err(AppError::refused)?;
    Ok(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_app::{LockPolicy, Paths};

    fn fresh() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths, None, LockPolicy::default());
        app.open_or_create(true).unwrap();
        (dir, app)
    }

    #[test]
    fn the_webview_gets_findings_in_words_and_never_a_password() {
        let (_dir, app) = fresh();
        let id = app
            .add_login(
                "https://github.com",
                "sean",
                b"hunter2".to_vec(),
                Some("GitHub"),
            )
            .unwrap();
        app.add_login(
            "https://mail.example.com",
            "sean",
            b"hunter2".to_vec(),
            None,
        )
        .unwrap();
        let json = serde_json::to_value(app.security_report().unwrap()).unwrap();
        let text = json.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        let findings = json["findings"].as_array().unwrap();
        let weak = findings
            .iter()
            .find(|f| f["kind"] == "weak" && f["item_id"] == id.as_str())
            .unwrap();
        for key in [
            "id",
            "kind",
            "severity",
            "title",
            "detail",
            "key",
            "item_id",
            "item_title",
            "item_kind",
            "field_id",
            "site",
            "related",
            "setup_url",
            "change_password",
            "playbook",
            "alert",
            "at",
            "dismissed",
        ] {
            assert!(weak.get(key).is_some(), "{key} missing from {weak}");
        }
        assert_eq!(weak["key"], "weak:pw");
        assert!(json["counts"]["important"].as_u64().unwrap() >= 3);
        let playbooks = serde_json::to_value(app.security_playbooks()).unwrap();
        assert_eq!(playbooks.as_array().unwrap().len(), 7);
        assert!(playbooks[0]["steps"].as_array().unwrap().len() >= 3);
    }

    #[test]
    fn change_password_opens_only_a_web_page_and_says_which() {
        let (_dir, app) = fresh();
        let mut settings = app.settings().unwrap();
        settings.change_password_check = false;
        app.set_settings(settings).unwrap();
        let id = app
            .add_login("https://example.com/login", "me", b"pw".to_vec(), None)
            .unwrap();
        let mut opened = None;
        let link = change_password(&app, &id, |url| {
            opened = Some(url.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            opened.as_deref(),
            Some("https://example.com/.well-known/change-password")
        );
        assert_eq!(link.url, opened.unwrap());
        let err = change_password(&app, &id, |_| Err("no browser".into())).unwrap_err();
        assert_eq!(err.message, "no browser");
    }
}
