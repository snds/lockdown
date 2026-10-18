//! Clipboard and screen hygiene (docs/plan/26-desktop-finish.md §6–§7).
//!
//! **Copies carry the marks that keep them out of history.** A secret is
//! written to the clipboard here, with `arboard` (the crate the clipboard
//! plugin wraps), so the write can say what the operating system and the
//! clipboard managers should do with it:
//!
//! | Where | Marks | Effect |
//! |---|---|---|
//! | Windows | `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` = 0, `CanUploadToCloudClipboard` = 0 | not in Win+V history, not synced to other PCs, skipped by monitors that honour it |
//! | macOS | `org.nspasteboard.ConcealedType`, `org.nspasteboard.TransientType` | clipboard managers that follow nspasteboard.org neither keep nor show it |
//! | Linux | `x-kde-passwordManagerHint` = `secret` | Klipper and the managers that follow it keep no history of it |
//!
//! The clear timer and its rule (D85, D119) are unchanged; the marks only
//! decide who else keeps a copy meanwhile. Reading back for the clear still
//! goes through the plugin.
//!
//! **Screen capture is blocked while something is revealed.** Each reveal
//! the webview draws (a revealed field, large type, the Wi-Fi card, a
//! scanned code) holds a [`CaptureCount`]; while it is above zero the main
//! window is content-protected (`set_content_protected`: Windows'
//! `WDA_EXCLUDEFROMCAPTURE`, macOS's `NSWindowSharingNone`), so screenshots,
//! screen sharing and recordings show it blank. Linux has no such call:
//! there it does nothing, and the doc says so. The quick-access window is
//! protected for its whole life, since it shows codes.

use std::sync::Mutex;

/// The marks a copy carries on this platform, in the order written.
pub fn clipboard_marks() -> &'static [&'static str] {
    if cfg!(windows) {
        &[
            "ExcludeClipboardContentFromMonitorProcessing",
            "CanIncludeInClipboardHistory",
            "CanUploadToCloudClipboard",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "org.nspasteboard.ConcealedType",
            "org.nspasteboard.TransientType",
        ]
    } else {
        &["x-kde-passwordManagerHint"]
    }
}

/// Whether this platform can keep a window out of screen captures.
pub fn capture_protection_available() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

/// Whether the webview in window `label` may use the camera: the main
/// window only, for scanning a TV's code or a contact's QR
/// (docs/plan/26-desktop-finish.md §3). Every other device permission a
/// page asks for is refused by the shell.
pub fn allows_camera(label: &str) -> bool {
    label == "main"
}

/// How many reveals are drawn now; protection is on while any is.
#[derive(Default)]
pub struct CaptureCount(Mutex<u32>);

impl CaptureCount {
    /// One reveal more (`on`) or fewer; answers whether the window should
    /// be protected now. A release with nothing held stays at zero.
    pub fn step(&self, on: bool) -> bool {
        let Ok(mut n) = self.0.lock() else {
            return true;
        };
        if on {
            *n = n.saturating_add(1);
        } else {
            *n = n.saturating_sub(1);
        }
        *n > 0
    }

    /// Every reveal gone (the vault locked).
    pub fn reset(&self) {
        if let Ok(mut n) = self.0.lock() {
            *n = 0;
        }
    }
}

/// Writes `text` to the clipboard with this platform's marks.
#[cfg(feature = "shell")]
pub fn write_secret(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("the clipboard isn't available: {e}"))?;
    let set = clipboard.set();
    #[cfg(windows)]
    let set = {
        use arboard::SetExtWindows;
        set.exclude_from_monitoring()
            .exclude_from_history()
            .exclude_from_cloud()
    };
    #[cfg(target_os = "macos")]
    let set = {
        use arboard::SetExtApple;
        set.exclude_from_history()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let set = {
        use arboard::SetExtLinux;
        set.exclude_from_history()
    };
    set.text(text).map_err(|e| format!("couldn't copy: {e}"))?;
    #[cfg(target_os = "macos")]
    mark_transient();
    Ok(())
}

/// Adds `org.nspasteboard.TransientType` to what is on the pasteboard now
/// (arboard writes `ConcealedType`, not this one): "don't keep this".
#[cfg(all(feature = "shell", target_os = "macos"))]
fn mark_transient() {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString};
    let kind = NSString::from_str("org.nspasteboard.TransientType");
    let empty = NSString::from_str("");
    // SAFETY: the general pasteboard is the process-wide AppKit object the
    // write above just used; adding a type and an empty string for it are
    // plain Objective-C calls with retained arguments.
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.addTypes_owner(&NSArray::from_retained_slice(&[kind.clone()]), None);
        pasteboard.setString_forType(&empty, &kind);
    }
}

#[cfg(feature = "shell")]
pub mod commands {
    use lockdown_app::{App, AppResult};
    use tauri::{AppHandle, Manager, State};

    use super::CaptureCount;

    /// A reveal began (`on`) or ended in the webview: the main window is
    /// kept out of screen captures while any reveal is drawn.
    #[tauri::command]
    pub async fn capture_protect(
        handle: AppHandle,
        count: State<'_, CaptureCount>,
        on: bool,
    ) -> AppResult<bool> {
        let protect = count.step(on);
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.set_content_protected(protect);
        }
        Ok(protect && super::capture_protection_available())
    }

    /// What the core kept out of swap, for Settings (§7).
    #[tauri::command]
    pub async fn memory_report(app: State<'_, App>) -> AppResult<lockdown_app::MemoryReport> {
        Ok(app.memory_report())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_platform_names_its_marks() {
        let marks = clipboard_marks();
        assert!(!marks.is_empty());
        if cfg!(target_os = "linux") {
            assert_eq!(marks, ["x-kde-passwordManagerHint"]);
            assert!(!capture_protection_available());
        }
    }

    #[test]
    fn only_the_main_window_may_use_the_camera() {
        assert!(allows_camera("main"));
        assert!(!allows_camera("quick"));
        assert!(!allows_camera(""));
    }

    #[test]
    fn protection_holds_while_any_reveal_is_drawn() {
        let c = CaptureCount::default();
        assert!(c.step(true));
        assert!(c.step(true));
        assert!(c.step(false), "one reveal still drawn");
        assert!(!c.step(false));
        assert!(!c.step(false), "an extra release stays at zero");
        assert!(c.step(true));
        c.reset();
        assert!(!c.step(false));
    }
}
