//! The command table the page calls (through the worker): one entry per
//! operation, named and shaped as the desktop's Tauri commands are
//! (`apps/desktop/src-tauri/src/commands.rs`, arguments in the camelCase
//! the client sends), so the desktop's `Client` interface and its views
//! work over either. The web adds its own sign-in lifecycle (`web_*`),
//! and copies differ: there is no Rust side outside the page to write the
//! clipboard, so `reveal_to_use` and `code_now` hand the value to the page,
//! which writes and clears it (docs/plan/17-web-vault.md §5).
//!
//! What crosses: JSON in, JSON or raw bytes out. A secret goes out only as
//! bytes ([`Reply::Bytes`]), only from a call that names one field, and
//! only through the same gates as on the desktop (`Reveal`, `Use`). Every
//! string in the arguments is overwritten once the call is done.

use std::time::Duration;

use lockdown_app::{AppError, AppResult, ApprovalKind, GateInput, LockPolicy, Settings};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use zeroize::{Zeroize, Zeroizing};

use crate::fields::{FieldChangeInput, into_changes};
use crate::host::WebVault;

/// What a call answers.
pub enum Reply {
    /// A JSON value.
    Json(Value),
    /// Raw bytes: a revealed value, a code, the recovery kit's page. Wiped
    /// when dropped.
    Bytes(Zeroizing<Vec<u8>>),
}

impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(v) => write!(f, "Json({v})"),
            Self::Bytes(b) => write!(f, "Bytes({} bytes)", b.len()),
        }
    }
}

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

/// A required argument.
fn req<T: DeserializeOwned>(args: &Value, key: &str) -> AppResult<T> {
    match args.get(key) {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map_err(|e| refused(format!("'{key}' isn't what this call takes: {e}"))),
        _ => Err(refused(format!("'{key}' is missing"))),
    }
}

/// An optional argument (`null` and absent are both none).
fn opt<T: DeserializeOwned>(args: &Value, key: &str) -> AppResult<Option<T>> {
    match args.get(key) {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| refused(format!("'{key}' isn't what this call takes: {e}"))),
        _ => Ok(None),
    }
}

/// A secret argument as bytes (an array of numbers), wiped on drop.
fn bytes(args: &Value, key: &str) -> AppResult<Zeroizing<Vec<u8>>> {
    req::<Vec<u8>>(args, key).map(Zeroizing::new)
}

/// A secret argument as text, wiped on drop.
fn secret_text(args: &Value, key: &str) -> AppResult<Zeroizing<String>> {
    req::<String>(args, key).map(Zeroizing::new)
}

fn gate(args: &Value) -> AppResult<GateInput> {
    Ok(opt::<GateInput>(args, "gate")?.unwrap_or_default())
}

fn json<T: Serialize>(value: T) -> AppResult<Reply> {
    serde_json::to_value(value)
        .map(Reply::Json)
        .map_err(|e| refused(e.to_string()))
}

fn done() -> AppResult<Reply> {
    Ok(Reply::Json(Value::Null))
}

/// Overwrites every string in `value` (the arguments, once a call is done:
/// they may have held a password or a field's text).
fn scrub(value: &mut Value) {
    match value {
        Value::String(s) => s.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(scrub),
        Value::Object(map) => map.values_mut().for_each(scrub),
        _ => {}
    }
}

/// The timers as the settings screen edits them; `null` means never.
#[derive(serde::Deserialize)]
struct PolicyInput {
    idle_secs: Option<u64>,
    hidden_secs: Option<u64>,
    sleep_gap_secs: Option<u64>,
}

/// Runs one command. `args` is the call's JSON object; it is scrubbed
/// afterwards, whatever happened.
pub fn call(web: &mut WebVault, cmd: &str, mut args: Value) -> AppResult<Reply> {
    let out = dispatch(web, cmd, &args);
    scrub(&mut args);
    web.after_call();
    out
}

fn dispatch(web: &mut WebVault, cmd: &str, args: &Value) -> AppResult<Reply> {
    match cmd {
        // --- the web's own lifecycle ---------------------------------------
        "web_state" => json(web.state()),
        "web_sign_in" => {
            let server: String = req(args, "server")?;
            let login: String = req(args, "login")?;
            let password = secret_text(args, "password")?;
            let secret_key = opt::<String>(args, "secretKey")?.map(Zeroizing::new);
            let carried = opt::<Vec<u8>>(args, "carried")?;
            json(web.sign_in(
                &server,
                &login,
                &password,
                secret_key.as_deref().map(String::as_str),
                carried.as_deref(),
            )?)
        }
        "web_unlock" => {
            let password = secret_text(args, "password")?;
            json(web.unlock(&password)?)
        }
        "web_sign_out" => {
            web.sign_out()?;
            done()
        }
        "web_carried" => match web.carried()? {
            Some(bytes) => Ok(Reply::Bytes(Zeroizing::new(bytes))),
            None => done(),
        },
        "tick" => json(web.tick()),

        // --- links, with no account (docs/plan/18-sharing.md §7) -----------
        "link_meta" => json(web.link_meta(&req::<String>(args, "link")?)?),
        "link_open" => {
            let link = secret_text(args, "link")?;
            let password = opt::<Vec<u8>>(args, "password")?.map(Zeroizing::new);
            json(web.link_open(&link, password.as_deref().map(Vec::as_slice))?)
        }
        "link_value" => Ok(Reply::Bytes(web.link_value(req(args, "index")?)?)),
        "link_file" => Ok(Reply::Bytes(web.link_file()?)),
        "link_close" => {
            web.link_close();
            done()
        }
        "request_label" => json(web.request_label(&secret_text(args, "link")?)?),
        // Making links and reading answers, signed in (the account's own).
        "send_text" => {
            let title: String = opt(args, "title")?.unwrap_or_default();
            let text = bytes(args, "text")?;
            let password = opt::<Vec<u8>>(args, "password")?.map(Zeroizing::new);
            let days: u32 = opt(args, "days")?.unwrap_or(7);
            let max_views: Option<u32> = opt(args, "maxViews")?;
            let link = web.app().send_text(
                &title,
                text.to_vec(),
                password.map(|p| p.to_vec()),
                lockdown_app::sharing::SendOptions { days, max_views },
            )?;
            Ok(Reply::Bytes(Zeroizing::new(link.expose().to_vec())))
        }
        "sends" => json(web.app().sends()?),
        "send_delete" => {
            web.app().delete_send(&req::<String>(args, "sendId")?)?;
            done()
        }
        "request_create" => {
            let label: String = req(args, "label")?;
            let days: u32 = opt(args, "days")?.unwrap_or(7);
            let link = web.app().create_request(&label, days)?;
            Ok(Reply::Bytes(Zeroizing::new(link.expose().to_vec())))
        }
        "requests" => json(web.app().requests()?),
        "request_open" => json(web.app().open_answer(&req::<String>(args, "requestId")?)?),
        "request_delete" => {
            web.app()
                .delete_request(&req::<String>(args, "requestId")?)?;
            done()
        }
        "opened_value" => {
            let value = web.app().opened_value(req(args, "index")?)?;
            Ok(Reply::Bytes(Zeroizing::new(value.expose().to_vec())))
        }
        "request_answer" => {
            let link = secret_text(args, "link")?;
            let title: String = opt(args, "title")?.unwrap_or_default();
            let secret = bytes(args, "secret")?;
            web.request_answer(&link, &title, secret.to_vec())?;
            done()
        }
        "lock" => {
            web.lock()?;
            done()
        }

        // --- the desktop's commands, same names and shapes ------------------
        "lock_state" => json(web.app().lock_state()?),
        "touch" => {
            web.app().touch()?;
            done()
        }
        "set_hidden" => json(web.app().set_hidden(req(args, "hidden")?)?),
        "set_policy" => {
            let p: PolicyInput = req(args, "policy")?;
            json(web.app().set_policy(LockPolicy {
                idle: p.idle_secs.map(Duration::from_secs),
                hidden: p.hidden_secs.map(Duration::from_secs),
                sleep_gap: p.sleep_gap_secs.map(Duration::from_secs),
            })?)
        }
        "get_settings" => json(web.app().settings()?),
        "set_settings" => {
            let settings: Settings = req(args, "settings")?;
            json(web.app().set_settings(settings)?)
        }
        "approval_route" => {
            let kind: ApprovalKind = req(args, "kind")?;
            let (route, method) = web.app().approval_route(kind)?;
            json(json!({ "route": route.as_str(), "method": method }))
        }
        "list" => json(web.app().list()?),
        // Masked metadata only.
        "show" => {
            let id: String = req(args, "id")?;
            json(web.app().show(&id, false)?)
        }
        "reveal" => {
            let id: String = req(args, "id")?;
            let field: String = req(args, "fieldId")?;
            let revealed = web.app().reveal(&id, &field, gate(args)?)?;
            Ok(Reply::Bytes(Zeroizing::new(revealed.expose().to_vec())))
        }
        // The web's copy: the value goes to the page, which writes the
        // clipboard; a use, through the `Use` gate for a crown jewel.
        "reveal_to_use" => {
            let id: String = req(args, "id")?;
            let field: String = req(args, "fieldId")?;
            let revealed = web.app().reveal_to_use(&id, &field, gate(args)?)?;
            Ok(Reply::Bytes(Zeroizing::new(revealed.expose().to_vec())))
        }
        "code_now" => {
            let id: String = req(args, "itemId")?;
            let field: String = req(args, "fieldId")?;
            let next = opt::<String>(args, "which")?.as_deref() == Some("next");
            let code = web.app().code_now(&id, &field, next, gate(args)?)?;
            Ok(Reply::Bytes(Zeroizing::new(code.expose().to_vec())))
        }
        "templates" => json(json!({
            "kinds": web.app().templates(),
            "custom_kinds": web.app().custom_kinds(),
        })),
        "password_strength" => {
            let candidate = bytes(args, "candidate")?;
            let context: Vec<String> = opt(args, "context")?.unwrap_or_default();
            let words: Vec<&str> = context.iter().map(String::as_str).collect();
            json(web.app().password_strength(&candidate, &words)?)
        }
        "add_item" => {
            let kind: String = req(args, "kind")?;
            let title: Option<String> = opt(args, "title")?;
            let crown: bool = opt(args, "crownJewel")?.unwrap_or(false);
            let fields: Vec<FieldChangeInput> = opt(args, "fields")?.unwrap_or_default();
            let changes = into_changes(fields)?;
            let id = match opt::<String>(args, "vault")? {
                Some(vault) => {
                    web.app()
                        .add_item_in(&vault, &kind, title.as_deref(), crown, changes)?
                }
                None => web
                    .app()
                    .add_item(&kind, title.as_deref(), crown, changes)?,
            };
            json(id)
        }
        "edit_item" => {
            let id: String = req(args, "id")?;
            let title: Option<String> = opt(args, "title")?;
            let changes: Vec<FieldChangeInput> = opt(args, "changes")?.unwrap_or_default();
            let changes = into_changes(changes)?;
            json(
                web.app()
                    .edit_item(&id, title.as_deref(), changes, gate(args)?)?,
            )
        }
        "trash" => {
            web.app().trash(&req::<String>(args, "id")?)?;
            done()
        }
        "restore" => {
            web.app().restore(&req::<String>(args, "id")?)?;
            done()
        }
        "codes" => json(web.app().codes()?),
        "advance_code" => {
            let id: String = req(args, "itemId")?;
            let field: String = req(args, "fieldId")?;
            json(web.app().advance_code(&id, &field, gate(args)?)?)
        }
        "add_code" => {
            let uri = secret_text(args, "uri")?;
            let title: Option<String> = opt(args, "title")?;
            let into: Option<String> = opt(args, "into")?;
            json(
                web.app()
                    .add_code(&uri, title.as_deref(), into.as_deref())?,
            )
        }
        "personas" => json(web.app().personas()?),
        "status" => json(web.app().status()?),
        "audit" => json(web.app().audit()?),
        "sync" => json(web.app().sync()?),
        "device_list" => json(web.app().device_list()?),
        "device_revoke" => {
            let id: String = req(args, "id")?;
            json(web.app().device_revoke(&id, gate(args)?)?)
        }
        "security_report" => json(web.app().security_report()?),
        "security_playbooks" => json(web.app().security_playbooks()),
        "vaults" => json(web.app().vaults()?),
        "create_vault" => json(web.app().create_vault(&req::<String>(args, "name")?)?),
        "rename_vault" => {
            let vault: String = req(args, "vaultId")?;
            web.app()
                .rename_vault(&vault, &req::<String>(args, "name")?)?;
            done()
        }
        "set_trash_days" => {
            let vault: String = req(args, "vaultId")?;
            web.app().set_trash_days(&vault, req(args, "days")?)?;
            done()
        }
        "add_folder" => {
            let vault: String = req(args, "vaultId")?;
            let name: String = req(args, "name")?;
            let parent: Option<String> = opt(args, "parent")?;
            json(web.app().add_folder(&vault, &name, parent.as_deref())?)
        }
        "change_folder" => {
            let vault: String = req(args, "vaultId")?;
            let folder: String = req(args, "folder")?;
            let name: String = req(args, "name")?;
            let parent: Option<String> = opt(args, "parent")?;
            web.app()
                .change_folder(&vault, &folder, &name, parent.as_deref())?;
            done()
        }
        "remove_folder" => {
            let vault: String = req(args, "vaultId")?;
            json(
                web.app()
                    .remove_folder(&vault, &req::<String>(args, "folder")?)?,
            )
        }
        "organize" => {
            let id: String = req(args, "id")?;
            // `folder: ""` takes the item out of its folder; absent leaves it.
            let folder = opt::<String>(args, "folder")?.map(|f| (!f.is_empty()).then_some(f));
            let tags: Option<Vec<String>> = opt(args, "tags")?;
            let favourite: Option<bool> = opt(args, "favourite")?;
            json(web.app().organize(&id, folder, tags, favourite)?)
        }
        "rename_tag" => {
            let from: String = req(args, "from")?;
            let to: Option<String> = opt(args, "to")?;
            json(web.app().rename_tag(&from, to.as_deref())?)
        }
        "archive" => {
            web.app().archive(&req::<String>(args, "id")?)?;
            done()
        }
        "unarchive" => {
            web.app().unarchive(&req::<String>(args, "id")?)?;
            done()
        }
        "move_item" => {
            let id: String = req(args, "id")?;
            let vault: String = req(args, "vaultId")?;
            json(web.app().move_item(&id, &vault, gate(args)?)?)
        }
        "set_crown_jewel" => {
            let id: String = req(args, "id")?;
            json(
                web.app()
                    .set_crown_jewel(&id, req(args, "on")?, gate(args)?)?,
            )
        }
        "purge" => {
            web.app().purge(&req::<String>(args, "id")?, gate(args)?)?;
            done()
        }
        "resolve_conflict" => {
            let keep: String = req(args, "keep")?;
            web.app()
                .resolve_conflict(&keep, &req::<String>(args, "other")?)?;
            done()
        }
        "duplicates" => json(web.app().duplicates()?),
        "merge_duplicates" => {
            let keep: String = req(args, "keep")?;
            let others: Vec<String> = req(args, "others")?;
            json(web.app().merge_duplicates(&keep, others, gate(args)?)?)
        }
        "history" => json(web.app().history(&req::<String>(args, "id")?)?),
        "show_version" => {
            let id: String = req(args, "id")?;
            json(web.app().show_version(&id, req(args, "n")?)?)
        }
        "restore_version" => {
            let id: String = req(args, "id")?;
            json(
                web.app()
                    .restore_version(&id, req(args, "n")?, gate(args)?)?,
            )
        }
        "note_use" => {
            web.app().note_use(&req::<String>(args, "id")?)?;
            done()
        }
        "expiry_reminders" => json(web.app().expiry_reminders()?),

        // --- vault depth (docs/plan/21-vault-depth.md) ----------------------
        "user_templates" => json(web.app().user_templates()?),
        "save_user_template" => {
            let vault: Option<String> = opt(args, "vault")?;
            let id: Option<String> = opt(args, "id")?;
            let template: lockdown_app::UserTemplateInput = req(args, "template")?;
            json(
                web.app()
                    .save_user_template(vault.as_deref(), id.as_deref(), template)?,
            )
        }
        "delete_user_template" => {
            web.app()
                .delete_user_template(&req::<String>(args, "id")?)?;
            done()
        }
        "link_targets" => {
            let vault: String = req(args, "vaultId")?;
            let except: Option<String> = opt(args, "except")?;
            json(web.app().link_targets(&vault, except.as_deref())?)
        }
        "generator_preview" => {
            let recipe: lockdown_app::Recipe = req(args, "recipe")?;
            let url: Option<String> = opt(args, "url")?;
            let rule: Option<String> = opt(args, "rule")?;
            json(
                web.app()
                    .generator_preview(&recipe, url.as_deref(), rule.as_deref())?,
            )
        }
        "site_rule" => json(web.app().site_rule(&req::<String>(args, "url")?)),
        // A secret (the network's password is in the code): bytes, gated.
        "wifi_qr" => {
            let id: String = req(args, "id")?;
            let grid = web.app().wifi_qr(&id, gate(args)?)?;
            Ok(Reply::Bytes(Zeroizing::new(grid.expose().to_vec())))
        }
        "log_away" => {
            let id: String = req(args, "id")?;
            let place: String = req(args, "place")?;
            let hours: u32 = req(args, "remindInHours")?;
            if !(1..=30 * 24).contains(&hours) {
                return Err(refused("a reminder is 1 hour to 30 days away"));
            }
            json(
                web.app()
                    .log_away(&id, &place, i64::from(hours) * 3600, gate(args)?)?,
            )
        }
        "clear_away" => {
            let id: String = req(args, "id")?;
            json(web.app().clear_away(&id, req(args, "at")?, gate(args)?)?)
        }
        "away_reminders" => json(web.app().away_reminders()?),
        "storage_usage" => json(web.app().storage_usage()?),
        other => depth::dispatch(web, other, args),
    }
}

/// The table's tail: what docs/plan/22-security-depth.md adds.
mod depth;
/// After it: what docs/plan/26-desktop-finish.md adds.
mod finish;

fn unknown(other: &str) -> AppResult<Reply> {
    Err(refused(format!("the web vault has no command '{other}'")))
}
