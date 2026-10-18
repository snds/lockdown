//! The extension's bridges ops (docs/plan/23-bridges.md §4, §5, §7): the
//! provider hints and sign-in observations of the federated map, the fill
//! fixes for a page, setting a fix from the popup, the security center's
//! checked change-password link, and aliases for a sign-up form.
//!
//! What the extension sends is checked in the core like anything from a
//! page; what goes back is words, ids, addresses and links, never a key.
//! An alias needs the `Use` gate: only the platform check (Touch ID,
//! Windows Hello) answers it from the browser, else the extension is told
//! to use the app, as a crown jewel's fill is (D98).

use lockdown_app::bridges::federation::FederatedEvidence;
use lockdown_app::bridges::fixes::FillFix;
use lockdown_app::{ErrorCode, GateInput};
use serde::Deserialize;
use serde_json::{Value, json};

use super::Shared;
use super::handler::{Refusal, args};

#[derive(Deserialize)]
struct Frame {
    frame: String,
}

#[derive(Deserialize)]
struct FixSet {
    frame: String,
    fix: FillFix,
}

#[derive(Deserialize)]
struct ItemRef {
    item_id: String,
}

#[derive(Deserialize)]
struct AliasCreate {
    item_id: String,
    frame: String,
}

/// The host of a page address, for an alias's note.
fn host_of(frame: &str) -> String {
    frame
        .split("://")
        .nth(1)
        .and_then(|r| r.split(['/', '?', '#']).next())
        .map(|h| {
            h.rsplit_once(':')
                .map_or(h, |(h, _)| h)
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

/// Answers one of the ops this module owns.
pub fn answer(shared: &Shared, op: &str, frame: &[u8]) -> Result<Value, Refusal> {
    let app = &shared.app;
    match op {
        "federation_hints" => Ok(json!(app.federation_hints())),
        "federation_seen" => {
            let e: FederatedEvidence = args(frame)?;
            Ok(json!(app.federation_seen(e)?))
        }
        "site_fixes" => {
            let a: Frame = args(frame)?;
            Ok(json!(app.site_fix_for(&a.frame)?))
        }
        "fix_set" => {
            let a: FixSet = args(frame)?;
            let count = app.site_fix_set_for(&a.frame, a.fix)?;
            Ok(json!({ "logins": count }))
        }
        "change_link" => {
            let a: ItemRef = args(frame)?;
            if app.show(&a.item_id, false)?.crown_jewel {
                return Err(Refusal::new(
                    "refused",
                    "this item is protected; change its password from Lockdown",
                ));
            }
            let link = app.security_change_link(&a.item_id)?;
            if !(link.url.starts_with("https://") || link.url.starts_with("http://")) {
                return Err(Refusal::new("refused", "only web pages open from here"));
            }
            Ok(json!(link))
        }
        "alias_options" => {
            let o = app.alias_overview()?;
            let services: Vec<Value> = o
                .configured
                .iter()
                .map(
                    |c| json!({ "item_id": c.item_id, "name": c.service_name, "domain": c.domain }),
                )
                .collect();
            Ok(json!({ "services": services }))
        }
        "alias_create" => {
            let a: AliasCreate = args(frame)?;
            let made = app
                .alias_generate(&a.item_id, &host_of(&a.frame), GateInput::default())
                .map_err(|e| match e.code {
                    ErrorCode::ApprovalNeeded => Refusal::new(
                        "approval_needed",
                        "an alias needs your OK, and Lockdown can't ask for it from the browser on this computer; make one in Lockdown's editor",
                    ),
                    _ => Refusal::from(e),
                })?;
            Ok(json!({ "address": made.address, "service": made.service_name }))
        }
        other => Err(Refusal::new(
            "unknown_op",
            format!("'{other}' isn't something Lockdown answers"),
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pages_host_is_read_without_its_port_or_path() {
        assert_eq!(
            super::host_of("https://Shop.Example:8443/signup?x=1"),
            "shop.example"
        );
        assert_eq!(super::host_of("not a url"), "");
    }
}
