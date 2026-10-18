//! The commands docs/plan/22-security-depth.md adds to the web vault's
//! table. The shared ones carry the desktop's names and arguments (the
//! same views call them over either client); the `web_*` ones are this
//! tab's sign-in and join; the `page_*` ones serve the pages the desktop
//! opens in the system browser, which hold no account. Every gate is the
//! app's, as on the desktop.

use serde_json::Value;

use super::{Reply, done, gate, json, opt, req, secret_text};
use crate::host::WebVault;
use crate::security::{
    CreateAnswer, GetAnswer, page_key_finish, page_key_request, page_sign_in_answer,
    page_sign_in_request,
};
use lockdown_app::{AppError, AppResult};

fn origin(web: &WebVault) -> AppResult<String> {
    web.serving_origin()
        .map(str::to_string)
        .ok_or_else(|| AppError::refused("the web vault doesn't know which server it came from"))
}

pub(super) fn dispatch(web: &mut WebVault, cmd: &str, args: &Value) -> AppResult<Reply> {
    match cmd {
        // --- per-item exposure (§1) ------------------------------------------------
        "exposure" => json(web.app().exposure(&req::<String>(args, "id")?)?),
        "exposures" => json(web.app().exposures()?),
        "set_exposure" => {
            let id: String = req(args, "id")?;
            let tier: Option<String> = opt(args, "tier")?;
            json(web.app().set_exposure(&id, tier.as_deref(), gate(args)?)?)
        }
        // --- crown jewels and emergency access (§8) --------------------------------
        "legacy_opt_in" => json(web.app().legacy_opt_in(&req::<String>(args, "id")?)?),
        "set_legacy_opt_in" => {
            let id: String = req(args, "id")?;
            json(
                web.app()
                    .set_legacy_opt_in(&id, req(args, "on")?, gate(args)?)?,
            )
        }
        // --- reminders and the drill (§5–§7) ---------------------------------------
        "reminders" => json(web.app().reminders()?),
        "password_check" => json(web.app().password_check(&secret_text(args, "password")?)?),
        "password_snooze" => json(web.app().password_snooze()?),
        "set_password_reminder_days" => {
            json(web.app().set_password_reminder_days(req(args, "days")?)?)
        }
        "drill_snooze" => json(web.app().drill_snooze()?),
        "kit_test_skip" => json(web.app().kit_test_skip(req(args, "understood")?)?),
        "drill_checks" => json(web.app().drill_checks()?),
        // --- checking a contact's keys (§9) ----------------------------------------
        "phrase_invite" => json(web.app().phrase_invite(&req::<String>(args, "login")?)?),
        "phrase_incoming" => json(web.app().phrase_incoming()?),
        "phrase_answer" => {
            let id: String = req(args, "verifyId")?;
            json(
                web.app()
                    .phrase_answer(&id, &secret_text(args, "phrase")?)?,
            )
        }
        "phrase_poll" => json(web.app().phrase_poll(&req::<String>(args, "verifyId")?)?),
        "phrase_checks" => json(web.app().phrase_checks()?),
        "phrase_cancel" => {
            web.app().phrase_cancel(&req::<String>(args, "verifyId")?)?;
            done()
        }
        // --- the account's security keys (§4) --------------------------------------
        "security_keys" => json(web.app().security_keys()?),
        "security_key_add_page" => json(web.app().security_key_add_page()?),
        "security_key_remove" => {
            let id: String = req(args, "id")?;
            json(web.app().security_key_remove(&id, gate(args)?)?)
        }
        // --- approving another device from here (§2) -------------------------------
        "link_offer" => json(web.app().link_offer()?),
        "link_poll" => json(web.app().link_poll()?),
        "link_approve" => json(web.app().link_approve(gate(args)?)?),
        "link_decline" => {
            web.app().link_decline()?;
            done()
        }
        "secret_key_status" => json(web.app().secret_key_status()?),
        // --- this tab's sign-in and join --------------------------------------------
        "web_sign_in_waiting" => json(web.sign_in_waiting()?),
        "web_sign_in_answer" => {
            let answer: GetAnswer = req(args, "answer")?;
            json(web.sign_in_answer(answer)?)
        }
        "web_sign_in_continue" => json(web.sign_in_continue()?),
        "web_join_start" => json(web.join_start(&secret_text(args, "invite")?)?),
        "web_join_poll" => json(web.join_poll()?),
        "web_join_finish" => {
            let login: String = req(args, "login")?;
            json(web.join_finish(&login, &secret_text(args, "password")?)?)
        }
        "web_join_cancel" => {
            web.app().join_cancel()?;
            done()
        }
        // --- the pages the desktop opens in the system browser ---------------------
        "page_sign_in_request" => json(page_sign_in_request(
            &origin(web)?,
            &req::<String>(args, "pendingId")?,
        )?),
        "page_sign_in_answer" => {
            let answer: GetAnswer = req(args, "answer")?;
            page_sign_in_answer(&origin(web)?, &req::<String>(args, "pendingId")?, answer)?;
            done()
        }
        "page_key_request" => json(page_key_request(
            &origin(web)?,
            &req::<String>(args, "pendingId")?,
        )?),
        "page_key_finish" => {
            let answer: CreateAnswer = req(args, "answer")?;
            json(page_key_finish(
                &origin(web)?,
                &req::<String>(args, "pendingId")?,
                answer,
            )?)
        }
        other => super::finish::dispatch(web, other, args),
    }
}
