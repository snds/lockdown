//! The commands docs/plan/38-mac-fixes.md adds to the web vault's table:
//! the recovery kit's page as a PDF (§4), the same bytes the desktop
//! writes, handed to the page for a download.
//!
//! The page sends back the kit page it was shown (its text and the notes
//! above it); the core lays it out and refuses anything that isn't a kit
//! page. Only while the vault is open, as the desktop. The text arrives as
//! a call argument, which `call` scrubs afterwards; the copy taken here and
//! the PDF are wiped when they drop.

use serde_json::Value;
use zeroize::Zeroizing;

use super::{Reply, opt, req, unknown};
use crate::host::WebVault;
use lockdown_app::AppResult;

pub(super) fn dispatch(web: &mut WebVault, cmd: &str, args: &Value) -> AppResult<Reply> {
    match cmd {
        "kit_pdf" => {
            let notes: Vec<String> = opt(args, "lines")?.unwrap_or_default();
            let page: Zeroizing<String> = Zeroizing::new(req(args, "page")?);
            Ok(Reply::Bytes(web.app().kit_pdf(&notes, &page)?))
        }
        other => unknown(other),
    }
}
