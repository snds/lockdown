//! Paper: printing a page and saving the recovery kit as a file
//! (docs/plan/38-mac-fixes.md §3–§4).
//!
//! **Print.** `window.print()` does nothing in WKWebView under Tauri, so
//! the webview asks the shell, which opens the webview's own print panel
//! (`Webview::print`: the system's print panel on macOS, the GTK print
//! dialog on Linux, the browser engine's own on Windows). The page's print
//! stylesheet decides what is on the paper, as it did before.
//!
//! **The kit as a file.** The webview sends back the kit page it is
//! showing: a NUL-terminated JSON list of the notes, then the page's bytes
//! (the shape `kit_create` sent it in). The core checks it is a kit page
//! before anything else, the OS's save dialog picks the place (suggested
//! name `Lockdown Recovery Kit.pdf`), and the core lays it out and writes
//! it there and nowhere else. The page's bytes and the PDF are wiped once
//! written. The webview never holds the PDF and gets no file permission.
//! The split is outside the `shell` feature so it is unit-tested without
//! Tauri.

use lockdown_app::{AppError, AppResult};
use zeroize::Zeroizing;

/// The longest body a kit page and its notes take (the core refuses far
/// less; this only stops a large body being copied around first).
pub const MAX_BODY: usize = 16 * 1024;

/// The notes and the page from the webview's body.
pub fn split_body(body: &[u8]) -> AppResult<(Vec<String>, Zeroizing<String>)> {
    if body.len() > MAX_BODY {
        return Err(AppError::refused("that isn't a recovery kit page"));
    }
    let nul = body
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| AppError::refused("that isn't a recovery kit page"))?;
    let notes: Vec<String> = serde_json::from_slice(&body[..nul])
        .map_err(|_| AppError::refused("that isn't a recovery kit page"))?;
    let page = std::str::from_utf8(&body[nul + 1..])
        .map_err(|_| AppError::refused("that isn't a recovery kit page"))?;
    Ok((notes, Zeroizing::new(page.to_string())))
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::{App, AppError, AppResult, kit_pdf};
    use tauri::ipc::{InvokeBody, Request};
    use tauri::{AppHandle, State, WebviewWindow};
    use tauri_plugin_dialog::DialogExt;
    use zeroize::Zeroizing;

    /// Opens the webview's own print panel for the page as drawn now. It
    /// returns once the panel is asked for, not when printing ends.
    #[tauri::command]
    pub fn print_page(window: WebviewWindow) -> AppResult<()> {
        window
            .print()
            .map_err(|e| AppError::refused(format!("the print panel didn't open: {e}")))
    }

    /// Saves the kit page the webview is showing as a PDF, where the
    /// person picks. `false` when they cancelled the dialog.
    #[tauri::command]
    pub async fn kit_save_pdf(
        app: State<'_, App>,
        handle: AppHandle,
        request: Request<'_>,
    ) -> AppResult<bool> {
        let InvokeBody::Raw(body) = request.body() else {
            return Err(AppError::refused("the kit page has to arrive as bytes"));
        };
        let body = Zeroizing::new(body.clone());
        let (notes, page) = super::split_body(&body)?;
        drop(body);
        // A page that isn't a kit page is refused before any dialog opens.
        kit_pdf::check(&page).map_err(AppError::refused)?;
        let path = tauri::async_runtime::spawn_blocking(move || {
            handle
                .dialog()
                .file()
                .set_title("Save the recovery kit")
                .set_file_name(kit_pdf::FILE_NAME)
                .add_filter("PDF", &["pdf"])
                .blocking_save_file()
        })
        .await
        .map_err(|e| AppError::refused(format!("the save dialog didn't open: {e}")))?;
        let Some(path) = path else {
            return Ok(false);
        };
        let path = path.into_path().map_err(|_| {
            AppError::refused(
                "that place can't be written from here; pick a folder on this computer",
            )
        })?;
        let core = app.inner().clone();
        tauri::async_runtime::spawn_blocking(move || core.kit_pdf_save(&notes, &page, &path))
            .await
            .map_err(|e| AppError::refused(format!("the call didn't finish: {e}")))??;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_the_notes_then_the_page() {
        let mut body = br#"["made here"]"#.to_vec();
        body.push(0);
        body.extend_from_slice(b"LOCKDOWN RECOVERY KIT\n");
        let (notes, page) = split_body(&body).unwrap();
        assert_eq!(notes, ["made here"]);
        assert_eq!(page.as_str(), "LOCKDOWN RECOVERY KIT\n");
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(split_body(b"no separator").is_err());
        assert!(split_body(b"not json\0page").is_err());
        assert!(split_body(b"[]\0\xff\xfe").is_err());
        let mut big = b"[]\0".to_vec();
        big.resize(MAX_BODY + 1, b'x');
        assert!(split_body(&big).is_err());
    }
}
