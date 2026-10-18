//! The editor's field changes as the page sends them: the same shape and
//! the same rules as the desktop's (`apps/desktop/src-tauri/src/items.rs`),
//! copied rather than shared because that file lives in the Tauri crate,
//! which the web build cannot depend on. A secret arrives as bytes and is
//! wiped when its input drops.

use lockdown_app::{AppError, AppResult, Change, FieldChange, FieldInput, field_kind_named};
use serde::Deserialize;
use zeroize::{Zeroize, Zeroizing};

/// One field change as the webview sends it.
#[derive(Deserialize)]
pub struct FieldChangeInput {
    field_id: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    action: String,
    text: Option<String>,
    secret: Option<Vec<u8>>,
    flag: Option<bool>,
    #[serde(default)]
    match_rule: Option<String>,
}

impl Drop for FieldChangeInput {
    fn drop(&mut self) {
        if let Some(s) = self.secret.as_mut() {
            s.zeroize();
        }
    }
}

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

impl FieldChangeInput {
    /// The change, checked for shape: the core checks the meaning.
    pub fn into_change(mut self) -> AppResult<FieldChange> {
        let name = self
            .field_id
            .clone()
            .or_else(|| self.label.clone())
            .unwrap_or_else(|| "a field".into());
        let values = [
            self.text.is_some(),
            self.secret.is_some(),
            self.flag.is_some(),
        ]
        .iter()
        .filter(|v| **v)
        .count();
        let change = match self.action.as_str() {
            _ if self.match_rule.is_some() && self.text.is_none() => {
                return Err(refused(format!(
                    "{name}: a match rule goes with a web address's text"
                )));
            }
            "keep" | "generate" | "remove" if values > 0 => {
                return Err(refused(format!(
                    "{name}: '{}' carries no value",
                    self.action
                )));
            }
            "keep" => Change::Keep,
            "generate" => Change::Generate,
            "remove" => Change::Remove,
            "set" => {
                if values != 1 {
                    return Err(refused(format!(
                        "{name}: 'set' carries exactly one of text, secret or flag"
                    )));
                }
                if let Some(bytes) = self.secret.take() {
                    Change::Set(FieldInput::Secret(Zeroizing::new(bytes)))
                } else if let Some(text) = self.text.take() {
                    match self.match_rule.take() {
                        Some(match_rule) => Change::Set(FieldInput::Url {
                            value: text,
                            match_rule,
                        }),
                        None => Change::Set(FieldInput::Text(text)),
                    }
                } else {
                    Change::Set(FieldInput::Flag(self.flag.unwrap_or(false)))
                }
            }
            other => return Err(refused(format!("{name}: '{other}' isn't an action"))),
        };
        let kind = match self.kind.take() {
            Some(k) => Some(field_kind_named(&k).map_err(refused)?),
            None => None,
        };
        if self.field_id.is_none() && kind.is_none() {
            return Err(refused(format!("{name}: a field you add needs a kind")));
        }
        Ok(FieldChange {
            field_id: self.field_id.take(),
            label: self.label.take(),
            kind,
            change,
        })
    }
}

/// Every change of one add or edit, or the first refusal.
pub fn into_changes(inputs: Vec<FieldChangeInput>) -> AppResult<Vec<FieldChange>> {
    inputs
        .into_iter()
        .map(FieldChangeInput::into_change)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_app::{ErrorCode, FieldInput, FieldKind};

    fn parse(json: &str) -> AppResult<Vec<FieldChange>> {
        let inputs: Vec<FieldChangeInput> = serde_json::from_str(json).unwrap();
        into_changes(inputs)
    }

    #[test]
    fn each_action_becomes_its_change() {
        let changes = parse(
            r#"[
                {"field_id": "pw", "action": "keep"},
                {"field_id": "wifi.passphrase", "action": "generate"},
                {"field_id": "notes", "action": "remove"},
                {"field_id": "user", "action": "set", "text": "me"},
                {"field_id": "card.number", "action": "set", "secret": [52, 49, 49, 49]},
                {"field_id": "wifi.hidden", "action": "set", "flag": true},
                {"label": "Door code", "kind": "hidden", "action": "set", "secret": [49]}
            ]"#,
        )
        .unwrap();
        assert_eq!(changes[0].change, Change::Keep);
        assert_eq!(changes[1].change, Change::Generate);
        assert_eq!(changes[2].change, Change::Remove);
        assert_eq!(
            changes[3].change,
            Change::Set(FieldInput::Text("me".into()))
        );
        assert!(
            matches!(&changes[4].change, Change::Set(FieldInput::Secret(b)) if b.as_slice() == b"4111")
        );
        assert_eq!(changes[5].change, Change::Set(FieldInput::Flag(true)));
        assert_eq!(changes[6].field_id, None);
        assert_eq!(changes[6].label.as_deref(), Some("Door code"));
        assert_eq!(changes[6].kind, Some(FieldKind::Hidden));
    }

    #[test]
    fn a_web_address_carries_its_match_rule() {
        let changes = parse(
            r#"[{"field_id": "url.1", "action": "set", "text": "shop.example", "match_rule": "base_domain"},
                {"field_id": "url.0", "action": "set", "text": "a.example"}]"#,
        )
        .unwrap();
        assert_eq!(
            changes[0].change,
            Change::Set(FieldInput::Url {
                value: "shop.example".into(),
                match_rule: "base_domain".into()
            })
        );
        assert_eq!(
            changes[1].change,
            Change::Set(FieldInput::Text("a.example".into()))
        );
    }

    #[test]
    fn a_malformed_change_is_refused_with_its_name() {
        for (json, words) in [
            (r#"[{"field_id": "pw", "action": "set"}]"#, "exactly one"),
            (
                r#"[{"field_id": "pw", "action": "set", "text": "a", "secret": [1]}]"#,
                "exactly one",
            ),
            (
                r#"[{"field_id": "pw", "action": "keep", "secret": [1]}]"#,
                "carries no value",
            ),
            (
                r#"[{"field_id": "pw", "action": "copy"}]"#,
                "isn't an action",
            ),
            (
                r#"[{"label": "x", "action": "set", "text": "a"}]"#,
                "needs a kind",
            ),
            (
                r#"[{"label": "x", "kind": "passkey", "action": "set", "text": "a"}]"#,
                "can add",
            ),
            (
                r#"[{"field_id": "url.0", "action": "keep", "match_rule": "never"}]"#,
                "match rule",
            ),
        ] {
            let err = parse(json).unwrap_err();
            assert_eq!(err.code, ErrorCode::Refused);
            assert!(err.message.contains(words), "{json}: {}", err.message);
        }
    }
}
