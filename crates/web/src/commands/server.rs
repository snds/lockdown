//! The commands docs/plan/27-web-server-finish.md adds to the web vault's
//! table: the live channel the worker holds itself (§2), the account's
//! session list (the desktop's names and shapes), the server's restore
//! drill (§3) and the key-transparency mirrors (§4).
//!
//! **Worker-only calls.** Commands named `worker_…` are the worker's own:
//! `worker.ts` refuses them from the page, so the session token
//! `worker_live_target` hands out stays in the worker, beside the HTTP hook
//! that already carries it on every request.

use serde_json::{Value, json as j};

use super::{Reply, done, json, req, unknown};
use crate::host::WebVault;
use lockdown_app::{AppError, AppResult};

pub(super) fn dispatch(web: &mut WebVault, cmd: &str, args: &Value) -> AppResult<Reply> {
    match cmd {
        // --- the live channel, held by the worker (§2) ------------------------------
        "worker_live_target" => Ok(Reply::Json(match web.app().live_account() {
            Some((server, token)) => j!({
                "url": format!("{}/v1/events", server.trim_end_matches('/')),
                "token": token,
            }),
            None => Value::Null,
        })),
        "worker_live_sync" => json(web.app().live_sync()?),
        "worker_live_ended" => {
            web.app()
                .live_session_ended(&req::<String>(args, "token")?)?;
            done()
        }
        // --- where the account is signed in -----------------------------------------
        "session_list" => json(web.app().sessions()?),
        "session_end" => {
            let id: String = req(args, "id")?;
            if !(id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())) {
                return Err(AppError::refused("that isn't a session"));
            }
            web.app().end_session(&id)?;
            done()
        }
        "session_end_others" => json(web.app().end_other_sessions()?),
        // --- the server's restore drill and the log mirrors (§3–§4) -----------------
        "server_restore_drill" => json(web.app().restore_drill()?),
        "kt_mirrors" => json(web.app().kt_mirrors()?),
        "kt_mirrors_set" => json(web.app().set_kt_mirrors(req(args, "urls")?)?),
        other => unknown(other),
    }
}
