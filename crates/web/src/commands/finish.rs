//! The commands docs/plan/26-desktop-finish.md adds to the web vault's
//! table, under the desktop's names and argument shapes: quick access over
//! the core's one search index, the scan flow (a camera frame from the
//! tab's own `getUserMedia`, decoded here, never in the page), the contacts
//! screen's lookups and checks, and what the core keeps out of swap. Every
//! gate is the app's, as on the desktop. What the web vault can't do (fill
//! in another tab, keep a window out of screen captures) is refused or
//! answered in words.

use serde_json::Value;

use super::{Reply, bytes, done, gate, json, opt, req};
use crate::host::WebVault;
use lockdown_app::scan::ScanSource;
use lockdown_app::{AppError, AppResult};
use zeroize::Zeroizing;

pub(super) fn dispatch(web: &mut WebVault, cmd: &str, args: &Value) -> AppResult<Reply> {
    match cmd {
        // --- quick access (§1) ------------------------------------------------------
        "quick_search" => {
            let query: String = opt(args, "query")?.unwrap_or_default();
            let limit: usize = opt(args, "limit")?.unwrap_or(50);
            json(web.app().search(&query, limit)?)
        }
        "quick_code" => {
            let id: String = req(args, "itemId")?;
            let field: String = req(args, "fieldId")?;
            let code = web.app().quick_code(&id, &field, gate(args)?)?;
            Ok(Reply::Bytes(Zeroizing::new(code.expose().to_vec())))
        }
        "quick_fill" => Err(AppError::refused(
            "the web vault can't fill another tab; use the browser extension with the desktop app",
        )),
        // --- device codes and the proximity gate (§3–§4) ----------------------------
        "scan_frame" => {
            let width: u32 = req(args, "width")?;
            let height: u32 = req(args, "height")?;
            let source = match req::<String>(args, "source")?.as_str() {
                "camera" => ScanSource::Camera,
                "image" => ScanSource::Image,
                _ => {
                    return Err(AppError::refused(
                        "a frame comes from the camera or an image file",
                    ));
                }
            };
            let luma = bytes(args, "luma")?;
            json(web.app().scan_image(width, height, luma.to_vec(), source)?)
        }
        "scan_text" => json(
            web.app()
                .scan_text(&req::<String>(args, "text")?, ScanSource::Pasted)?,
        ),
        "activation_approve" => {
            let id: String = req(args, "activationId")?;
            let item: Option<String> = opt(args, "itemId")?;
            json(
                web.app()
                    .activation_approve(&id, item.as_deref(), gate(args)?)?,
            )
        }
        "activation_cancel" => {
            web.app()
                .activation_cancel(&req::<String>(args, "activationId")?)?;
            done()
        }
        "qr_contact_frame" => {
            let width: u32 = req(args, "width")?;
            let height: u32 = req(args, "height")?;
            let luma = bytes(args, "luma")?;
            json(lockdown_app::scan::contact_qr(width, height, &luma))
        }
        // --- contacts (key transparency, §8) ----------------------------------------
        "sharing_overview" => json(web.app().sharing_overview()?),
        "contact_lookup" => json(web.app().lookup_contact(&req::<String>(args, "login")?)?),
        "contact_verify" => {
            let account: String = req(args, "account")?;
            let fingerprint: String = req(args, "fingerprint")?;
            json(web.app().verify_contact(&account, &fingerprint)?)
        }
        "contact_forget" => {
            web.app().forget_contact(&req::<String>(args, "account")?)?;
            done()
        }
        // --- hygiene (§6–§7) --------------------------------------------------------
        "memory_report" => json(web.app().memory_report()),
        "capture_protect" => json(false),
        other => super::server::dispatch(web, other, args),
    }
}
