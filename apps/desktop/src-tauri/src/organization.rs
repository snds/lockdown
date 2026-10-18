//! The webview's commands for organization and item depth
//! (docs/plan/15-organization.md): vaults, folders, tags, favourites, the
//! archive, moves, the duplicate finder, history, attachments and the
//! expiry reminders. Each is one call into `lockdown-app` on a worker
//! thread; the gated ones take a `gate` like `copy_secret` does.
//!
//! A file to attach arrives as the raw request body (an `ArrayBuffer`,
//! never a JSON array of numbers), with its item, name and type in
//! headers; its bytes go back out only when the person asks to save it,
//! as a raw response. Nothing here is kept: the core seals the file and
//! the webview's copy is the one the person picked.
//!
//! The header parsing is outside the `shell` feature so it is unit-tested
//! without Tauri.

use lockdown_app::{AppError, AppResult, GateInput};

/// The headers an `attach` request carries.
pub const ITEM_HEADER: &str = "x-lockdown-item";
/// The file's name, percent-encoded (`encodeURIComponent`).
pub const NAME_HEADER: &str = "x-lockdown-name";
/// The file's media type.
pub const TYPE_HEADER: &str = "x-lockdown-type";
/// The gate, as the JSON a `gate` argument takes, when the item asked for one.
pub const GATE_HEADER: &str = "x-lockdown-gate";

/// Undoes `encodeURIComponent`: `%XX` escapes as UTF-8, everything else as
/// it is. A malformed escape or text that isn't UTF-8 is refused.
pub fn percent_decode(text: &str) -> AppResult<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| AppError::refused("the file's name didn't arrive whole"))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| AppError::refused("the file's name isn't valid text"))
}

/// What an `attach` request names, from its headers (looked up by `get`).
pub struct AttachHeaders {
    pub item: String,
    pub name: String,
    pub mime: String,
    pub gate: GateInput,
}

/// Reads an `attach` request's headers.
pub fn attach_headers(get: impl Fn(&str) -> Option<String>) -> AppResult<AttachHeaders> {
    let item = get(ITEM_HEADER).ok_or_else(|| AppError::refused("which item to attach to?"))?;
    let name = percent_decode(
        &get(NAME_HEADER).ok_or_else(|| AppError::refused("the file needs a name"))?,
    )?;
    let mime = get(TYPE_HEADER).unwrap_or_default();
    let gate = match get(GATE_HEADER) {
        Some(json) => serde_json::from_str(&json)
            .map_err(|_| AppError::refused("the confirmation didn't arrive whole"))?,
        None => GateInput::default(),
    };
    Ok(AttachHeaders {
        item,
        name,
        mime,
        gate,
    })
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::{
        App, AppError, AppResult, DuplicateGroup, ExpiryReminder, GateInput, ItemDetail, VaultView,
        VersionView,
    };
    use tauri::State;
    use tauri::ipc::{InvokeBody, Request, Response};

    /// Runs `f` off the main thread and waits for it.
    async fn blocking<T, F>(app: &App, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(App) -> AppResult<T> + Send + 'static,
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || f(app))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))?
    }

    #[tauri::command]
    pub async fn vaults(app: State<'_, App>) -> AppResult<Vec<VaultView>> {
        blocking(&app, |a| a.vaults()).await
    }

    #[tauri::command]
    pub async fn create_vault(app: State<'_, App>, name: String) -> AppResult<String> {
        blocking(&app, move |a| a.create_vault(&name)).await
    }

    #[tauri::command]
    pub async fn rename_vault(
        app: State<'_, App>,
        vault_id: String,
        name: String,
    ) -> AppResult<()> {
        blocking(&app, move |a| a.rename_vault(&vault_id, &name)).await
    }

    #[tauri::command]
    pub async fn set_trash_days(app: State<'_, App>, vault_id: String, days: u32) -> AppResult<()> {
        blocking(&app, move |a| a.set_trash_days(&vault_id, days)).await
    }

    #[tauri::command]
    pub async fn add_folder(
        app: State<'_, App>,
        vault_id: String,
        name: String,
        parent: Option<String>,
    ) -> AppResult<String> {
        blocking(&app, move |a| {
            a.add_folder(&vault_id, &name, parent.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub async fn change_folder(
        app: State<'_, App>,
        vault_id: String,
        folder: String,
        name: String,
        parent: Option<String>,
    ) -> AppResult<()> {
        blocking(&app, move |a| {
            a.change_folder(&vault_id, &folder, &name, parent.as_deref())
        })
        .await
    }

    #[tauri::command]
    pub async fn remove_folder(
        app: State<'_, App>,
        vault_id: String,
        folder: String,
    ) -> AppResult<usize> {
        blocking(&app, move |a| a.remove_folder(&vault_id, &folder)).await
    }

    /// `folder`: an id to file it there, `""` to take it out of its folder,
    /// left out to leave it; `tags` and `favourite` likewise left out to
    /// leave them.
    #[tauri::command]
    pub async fn organize(
        app: State<'_, App>,
        id: String,
        folder: Option<String>,
        tags: Option<Vec<String>>,
        favourite: Option<bool>,
    ) -> AppResult<u64> {
        let folder = folder.map(|f| (!f.is_empty()).then_some(f));
        blocking(&app, move |a| a.organize(&id, folder, tags, favourite)).await
    }

    #[tauri::command]
    pub async fn rename_tag(
        app: State<'_, App>,
        from: String,
        to: Option<String>,
    ) -> AppResult<usize> {
        blocking(&app, move |a| a.rename_tag(&from, to.as_deref())).await
    }

    #[tauri::command]
    pub async fn archive(app: State<'_, App>, id: String) -> AppResult<()> {
        blocking(&app, move |a| a.archive(&id)).await
    }

    #[tauri::command]
    pub async fn unarchive(app: State<'_, App>, id: String) -> AppResult<()> {
        blocking(&app, move |a| a.unarchive(&id)).await
    }

    #[tauri::command]
    pub async fn move_item(
        app: State<'_, App>,
        id: String,
        vault_id: String,
        gate: Option<GateInput>,
    ) -> AppResult<String> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.move_item(&id, &vault_id, gate)).await
    }

    #[tauri::command]
    pub async fn set_crown_jewel(
        app: State<'_, App>,
        id: String,
        on: bool,
        gate: Option<GateInput>,
    ) -> AppResult<String> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.set_crown_jewel(&id, on, gate)).await
    }

    #[tauri::command]
    pub async fn purge(app: State<'_, App>, id: String, gate: Option<GateInput>) -> AppResult<()> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.purge(&id, gate)).await
    }

    #[tauri::command]
    pub async fn resolve_conflict(
        app: State<'_, App>,
        keep: String,
        other: String,
    ) -> AppResult<()> {
        blocking(&app, move |a| a.resolve_conflict(&keep, &other)).await
    }

    #[tauri::command]
    pub async fn duplicates(app: State<'_, App>) -> AppResult<Vec<DuplicateGroup>> {
        blocking(&app, |a| a.duplicates()).await
    }

    #[tauri::command]
    pub async fn merge_duplicates(
        app: State<'_, App>,
        keep: String,
        others: Vec<String>,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.merge_duplicates(&keep, others, gate)).await
    }

    #[tauri::command]
    pub async fn history(app: State<'_, App>, id: String) -> AppResult<Vec<VersionView>> {
        blocking(&app, move |a| a.history(&id)).await
    }

    /// An earlier version, masked like `show`.
    #[tauri::command]
    pub async fn show_version(app: State<'_, App>, id: String, n: u64) -> AppResult<ItemDetail> {
        blocking(&app, move |a| a.show_version(&id, n)).await
    }

    #[tauri::command]
    pub async fn restore_version(
        app: State<'_, App>,
        id: String,
        n: u64,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.restore_version(&id, n, gate)).await
    }

    /// The file is the raw body; the item, name, type and gate are headers
    /// (`super::attach_headers`).
    #[tauri::command]
    pub async fn attach(app: State<'_, App>, request: Request<'_>) -> AppResult<String> {
        let InvokeBody::Raw(bytes) = request.body() else {
            return Err(AppError::refused("the file has to arrive as bytes"));
        };
        let bytes = bytes.clone();
        let headers = request.headers();
        let h = super::attach_headers(|name| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        })?;
        blocking(&app, move |a| {
            a.attach(&h.item, &h.name, &h.mime, bytes, h.gate)
        })
        .await
    }

    /// An attached file's bytes, raw, for the person to save. A crown
    /// jewel's go through the `Reveal` gate.
    #[tauri::command]
    pub async fn attachment(
        app: State<'_, App>,
        id: String,
        att_id: String,
        gate: Option<GateInput>,
    ) -> AppResult<Response> {
        let gate = gate.unwrap_or_default();
        let bytes = blocking(&app, move |a| a.attachment(&id, &att_id, gate)).await?;
        Ok(Response::new(bytes.into_bytes().to_vec()))
    }

    #[tauri::command]
    pub async fn detach(
        app: State<'_, App>,
        id: String,
        att_id: String,
        gate: Option<GateInput>,
    ) -> AppResult<u64> {
        let gate = gate.unwrap_or_default();
        blocking(&app, move |a| a.detach(&id, &att_id, gate)).await
    }

    /// Notes that the person used an item from outside the core (its site
    /// opened), for the ranking.
    #[tauri::command]
    pub async fn note_use(app: State<'_, App>, id: String) -> AppResult<()> {
        blocking(&app, move |a| a.note_use(&id)).await
    }

    #[tauri::command]
    pub async fn expiry_reminders(app: State<'_, App>) -> AppResult<Vec<ExpiryReminder>> {
        blocking(&app, |a| a.expiry_reminders()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_file_name_comes_back_from_its_percent_escapes() {
        assert_eq!(
            percent_decode("tax%20return%202026.pdf").unwrap(),
            "tax return 2026.pdf"
        );
        assert_eq!(percent_decode("caf%C3%A9.txt").unwrap(), "café.txt");
        assert_eq!(percent_decode("plain").unwrap(), "plain");
        assert!(percent_decode("bad%2").is_err());
        assert!(percent_decode("bad%zz").is_err());
        assert!(percent_decode("%FF").is_err(), "not UTF-8");
    }

    #[test]
    fn attach_headers_name_the_item_and_the_gate() {
        let mut h = BTreeMap::new();
        h.insert(ITEM_HEADER, "abc".to_string());
        h.insert(NAME_HEADER, "a%20b.txt".to_string());
        let got = |h: &BTreeMap<&str, String>| {
            let h = h.clone();
            attach_headers(move |n| h.get(n).cloned())
        };
        let parsed = got(&h).unwrap();
        assert_eq!(
            (parsed.item.as_str(), parsed.name.as_str()),
            ("abc", "a b.txt")
        );
        assert!(!parsed.gate.confirmed);
        h.insert(GATE_HEADER, r#"{"confirmed":true}"#.to_string());
        assert!(got(&h).unwrap().gate.confirmed);
        h.insert(GATE_HEADER, "{".to_string());
        assert!(got(&h).is_err());
        h.remove(ITEM_HEADER);
        assert!(got(&h).is_err());
    }
}
