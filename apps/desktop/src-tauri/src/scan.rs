//! The webview's commands for approving a TV or console sign-in
//! (docs/plan/26-desktop-finish.md §3; the core's half is
//! `lockdown_app::scan`). A camera frame or an image file crosses as raw
//! grey pixels with its size in the request's headers, never as an image
//! the webview decoded; the core finds and reads the QR code, runs the
//! proximity gate and the signed allow-list, and holds the activation
//! until the person's yes. The yes opens the real page in the browser.

/// The headers a frame carries.
pub const WIDTH_HEADER: &str = "x-lockdown-width";
pub const HEIGHT_HEADER: &str = "x-lockdown-height";
pub const SOURCE_HEADER: &str = "x-lockdown-source";

/// A frame's size and source from its headers, refusing anything else.
pub fn frame_meta(
    width: Option<&str>,
    height: Option<&str>,
    source: Option<&str>,
    len: usize,
) -> Result<(u32, u32, lockdown_app::scan::ScanSource), String> {
    let side = |v: Option<&str>| {
        v.and_then(|s| s.parse::<u32>().ok())
            .filter(|n| (1..=lockdown_app::scan::MAX_SIDE).contains(n))
    };
    let (Some(w), Some(h)) = (side(width), side(height)) else {
        return Err("a frame needs its width and height (1 to 4096)".into());
    };
    if len != (w as usize) * (h as usize) {
        return Err("the frame's size doesn't match its pixels".into());
    }
    let source = match source {
        Some("camera") => lockdown_app::scan::ScanSource::Camera,
        Some("image") => lockdown_app::scan::ScanSource::Image,
        _ => return Err("a frame comes from the camera or an image file".into()),
    };
    Ok((w, h, source))
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::scan::{ActivationStarted, ScanOutcome, ScanSource};
    use lockdown_app::{App, AppError, AppResult, GateInput};
    use tauri::ipc::{InvokeBody, Request};
    use tauri::{AppHandle, State};
    use tauri_plugin_opener::OpenerExt;

    use super::{HEIGHT_HEADER, SOURCE_HEADER, WIDTH_HEADER, frame_meta};

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

    /// One grey frame from the camera or an image file.
    #[tauri::command]
    pub async fn scan_frame(app: State<'_, App>, request: Request<'_>) -> AppResult<ScanOutcome> {
        let InvokeBody::Raw(luma) = request.body() else {
            return Err(AppError::refused("a frame is sent as raw bytes"));
        };
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let (w, h, source) = frame_meta(
            header(WIDTH_HEADER).as_deref(),
            header(HEIGHT_HEADER).as_deref(),
            header(SOURCE_HEADER).as_deref(),
            luma.len(),
        )
        .map_err(AppError::refused)?;
        let luma = luma.clone();
        blocking(&app, move |a| a.scan_image(w, h, luma, source)).await
    }

    /// A contact's QR code from a frame, for the contacts screen's scan:
    /// its text, or `null` when the frame holds none (a fingerprint, not a
    /// secret; `contact_verify` checks it against the pinned keys).
    #[tauri::command]
    pub async fn qr_contact_frame(request: Request<'_>) -> AppResult<Option<String>> {
        let InvokeBody::Raw(luma) = request.body() else {
            return Err(AppError::refused("a frame is sent as raw bytes"));
        };
        let header = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let (w, h, _) = frame_meta(
            header(WIDTH_HEADER).as_deref(),
            header(HEIGHT_HEADER).as_deref(),
            header(SOURCE_HEADER).as_deref(),
            luma.len(),
        )
        .map_err(AppError::refused)?;
        Ok(lockdown_app::scan::contact_qr(w, h, luma))
    }

    /// Text pasted or typed: always refused by the proximity gate when it
    /// is a sign-in code, and said so.
    #[tauri::command]
    pub async fn scan_text(app: State<'_, App>, text: String) -> AppResult<ScanOutcome> {
        blocking(&app, move |a| a.scan_text(&text, ScanSource::Pasted)).await
    }

    /// The person's yes: the `Approve` gate in the core, then the real page
    /// opened in the browser, where the extension offers the login first.
    #[tauri::command]
    pub async fn activation_approve(
        app: State<'_, App>,
        handle: AppHandle,
        activation_id: String,
        item_id: Option<String>,
        gate: Option<GateInput>,
    ) -> AppResult<ActivationStarted> {
        let gate = gate.unwrap_or_default();
        let started = blocking(&app, move |a| {
            a.activation_approve(&activation_id, item_id.as_deref(), gate)
        })
        .await?;
        if !started.url.starts_with("https://") {
            return Err(AppError::refused(
                "an activation page opens over https only",
            ));
        }
        handle
            .opener()
            .open_url(started.url.clone(), None::<&str>)
            .map_err(|e| AppError::refused(format!("couldn't open the browser: {e}")))?;
        Ok(started)
    }

    /// The person said no, or closed the sheet.
    #[tauri::command]
    pub async fn activation_cancel(app: State<'_, App>, activation_id: String) -> AppResult<()> {
        blocking(&app, move |a| a.activation_cancel(&activation_id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_app::scan::ScanSource;

    #[test]
    fn a_frame_names_its_size_and_source_or_is_refused() {
        assert_eq!(
            frame_meta(Some("4"), Some("2"), Some("camera"), 8),
            Ok((4, 2, ScanSource::Camera))
        );
        assert_eq!(
            frame_meta(Some("4"), Some("2"), Some("image"), 8),
            Ok((4, 2, ScanSource::Image))
        );
        // A paste never arrives as a frame; a frame never claims to be one.
        assert!(frame_meta(Some("4"), Some("2"), Some("pasted"), 8).is_err());
        assert!(frame_meta(Some("4"), Some("2"), None, 8).is_err());
        assert!(frame_meta(Some("4"), Some("2"), Some("camera"), 7).is_err());
        assert!(frame_meta(Some("0"), Some("2"), Some("camera"), 0).is_err());
        assert!(frame_meta(Some("5000"), Some("1"), Some("camera"), 5000).is_err());
        assert!(frame_meta(Some("x"), Some("2"), Some("camera"), 8).is_err());
    }
}
