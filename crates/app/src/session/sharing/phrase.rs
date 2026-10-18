//! Checking a contact's keys with a phrase said aloud, over the server's
//! check relay (`key-transparency`; `lockdown_vault::phrase_check`;
//! docs/plan/22-security-depth.md §9).
//!
//! The inviter picks someone, gets a nine-digit phrase to tell them, and
//! waits; the other person sees the check among their incoming ones, types
//! the phrase, and both apps run the exchange. A match marks the contact
//! verified on both sides, exactly as comparing the thirty digits does. The
//! phrase is shown to the inviter only while the check is open; it opens
//! nothing after.

use lockdown_vault::contact::fingerprint_bytes;
use lockdown_vault::phrase_check::{Inviter, Joiner, Outcome, Phrase, Views};
use serde::Serialize;

use super::*;

/// A check under way, kept in memory until the lock.
pub(crate) struct Pending {
    side: Side,
    account: Id,
    login: String,
    views: Views,
    after: u32,
    state: &'static str,
    phrase_shown: Option<String>,
    expires_at: i64,
}

enum Side {
    Inviter(Box<Inviter>),
    Joiner(Box<Joiner>),
}

/// A check as a shell draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PhraseCheckView {
    /// The check's id.
    pub verify_id: String,
    /// `inviter` (this side said the phrase) or `joiner` (typed it).
    pub role: String,
    /// The other person.
    pub login: String,
    /// Their account.
    pub account_id: String,
    /// The phrase to tell them, while the check is open (inviter only).
    pub phrase: Option<String>,
    /// `waiting`, `matched`, `wrong-phrase`, `keys-differ`, `failed` or
    /// `expired`.
    pub state: String,
    /// In plain words.
    pub words: String,
    /// When it closes on its own.
    pub expires_at: i64,
}

/// Someone asked to check keys with this person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PhraseIncoming {
    /// The check's id.
    pub verify_id: String,
    /// Who asked.
    pub login: String,
    /// Their account.
    pub account_id: String,
    /// When it closes.
    pub expires_at: i64,
}

fn words(state: &str, login: &str) -> String {
    match state {
        "waiting" => format!("waiting for {login} to type the phrase"),
        "matched" => format!("{login}'s keys are verified: the phrase and both fingerprints matched"),
        "wrong-phrase" => "the phrase didn't match; start a new check and read it out again".into(),
        "keys-differ" => format!(
            "the phrase matched but the keys didn't: the keys you were given for {login} aren't theirs. Don't share with them, and compare the thirty digits in person"
        ),
        "expired" => "the check ran out; start a new one".into(),
        _ => "the check broke off; start a new one".into(),
    }
    .to_string()
}

impl Pending {
    fn view(&self, verify_id: Id) -> PhraseCheckView {
        PhraseCheckView {
            verify_id: verify_id.to_string(),
            role: match self.side {
                Side::Inviter(_) => "inviter",
                Side::Joiner(_) => "joiner",
            }
            .into(),
            login: self.login.clone(),
            account_id: self.account.to_string(),
            phrase: (self.state == "waiting")
                .then(|| self.phrase_shown.clone())
                .flatten(),
            state: self.state.into(),
            words: words(self.state, &self.login),
            expires_at: self.expires_at,
        }
    }
}

impl Session {
    fn own_fingerprint_bytes(&self) -> Result<[u8; 32], String> {
        fingerprint_bytes(
            self.identity.profile().profile_id,
            &self.identity.profile().isk_pub,
            self.persona(),
            &self.kem_public()?,
        )
        .map_err(|e| e.to_string())
    }

    fn contact_fingerprint_bytes(&self, account: Id) -> Result<[u8; 32], String> {
        let c = self
            .sharing
            .contact(account)
            .ok_or("that person isn't pinned here")?;
        fingerprint_bytes(c.profile_id, &c.isk_pub, c.persona_id, &c.kem_pub)
            .map_err(|e| e.to_string())
    }

    /// Opens a check with `login`: pins them from the log and returns the
    /// phrase to tell them.
    pub fn phrase_invite(&mut self, login: &str) -> Result<PhraseCheckView, String> {
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.learn_account(&transport, &token, &sth)?;
        let login = login.trim();
        if Some(login) == self.sync.as_ref().map(|c| c.login.as_str()) {
            return Err("that's you".into());
        }
        let contact = self.pin(&transport, &token, &sth, Err(login))?;
        let opened = transport
            .verify_open(&token, contact.account_id)
            .map_err(|e| e.to_string())?;
        let views = Views {
            verify_id: opened.verify_id,
            inviter: me,
            inviter_fp: self.own_fingerprint_bytes()?,
            joiner: contact.account_id,
            joiner_fp: self.contact_fingerprint_bytes(contact.account_id)?,
        };
        let phrase = Phrase::generate().map_err(|e| e.to_string())?;
        let inviter = Inviter::new(&phrase, views.clone()).map_err(|e| e.to_string())?;
        let pending = Pending {
            side: Side::Inviter(Box::new(inviter)),
            account: contact.account_id,
            login: contact.login.clone(),
            views,
            after: 0,
            state: "waiting",
            phrase_shown: Some(phrase.shown()),
            expires_at: opened.expires_at,
        };
        let view = pending.view(opened.verify_id);
        self.phrase_checks.insert(opened.verify_id, pending);
        Ok(view)
    }

    /// The checks others opened with this person, their logins checked
    /// against the log.
    pub fn phrase_incoming(&mut self) -> Result<Vec<PhraseIncoming>, String> {
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.learn_account(&transport, &token, &sth)?;
        let list = transport.verify_list(&token).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for e in list.open {
            if e.to_account != me || self.phrase_checks.contains_key(&e.verify_id) {
                continue;
            }
            let contact = self.pin(&transport, &token, &sth, Ok(e.from_account))?;
            out.push(PhraseIncoming {
                verify_id: e.verify_id.to_string(),
                login: contact.login,
                account_id: e.from_account.to_string(),
                expires_at: e.expires_at,
            });
        }
        Ok(out)
    }

    /// Answers someone's check with the phrase they said.
    pub fn phrase_answer(
        &mut self,
        verify_id: &str,
        phrase: &str,
    ) -> Result<PhraseCheckView, String> {
        let verify_id = parse_id(verify_id)?;
        let phrase = Phrase::parse(phrase).map_err(|e| e.to_string())?;
        if self.phrase_checks.contains_key(&verify_id) {
            return Err("that check already has an answer from here".into());
        }
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.learn_account(&transport, &token, &sth)?;
        let entry = transport
            .verify_list(&token)
            .map_err(|e| e.to_string())?
            .open
            .into_iter()
            .find(|e| e.verify_id == verify_id && e.to_account == me)
            .ok_or("that check is closed or wasn't for you")?;
        let contact = self.pin(&transport, &token, &sth, Ok(entry.from_account))?;
        let views = Views {
            verify_id,
            inviter: entry.from_account,
            inviter_fp: self.contact_fingerprint_bytes(entry.from_account)?,
            joiner: me,
            joiner_fp: self.own_fingerprint_bytes()?,
        };
        let (joiner, first) = Joiner::start(phrase, views.clone()).map_err(|e| e.to_string())?;
        transport
            .verify_push(&token, verify_id, &first)
            .map_err(|e| e.to_string())?;
        self.phrase_checks.insert(
            verify_id,
            Pending {
                side: Side::Joiner(Box::new(joiner)),
                account: entry.from_account,
                login: contact.login,
                views,
                after: 0,
                state: "waiting",
                phrase_shown: None,
                expires_at: entry.expires_at,
            },
        );
        self.phrase_poll(&verify_id.to_string())
    }

    /// Moves a check on with whatever the other side sent.
    pub fn phrase_poll(&mut self, verify_id: &str) -> Result<PhraseCheckView, String> {
        let verify_id = parse_id(verify_id)?;
        let (transport, token, _) = self.online()?;
        let mut pending = self
            .phrase_checks
            .remove(&verify_id)
            .ok_or("no such check here; it ends with a lock")?;
        let result = self.advance(&transport, &token, verify_id, &mut pending);
        let view = pending.view(verify_id);
        self.phrase_checks.insert(verify_id, pending);
        result.map(|()| view)
    }

    fn advance(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        verify_id: Id,
        p: &mut Pending,
    ) -> Result<(), String> {
        if p.state != "waiting" {
            return Ok(());
        }
        let frames = match transport.verify_poll(token, verify_id, p.after) {
            Ok(f) => f,
            Err(lockdown_sync::transport::TransportError::Refused { status: 404, .. }) => {
                p.state = "expired";
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        };
        for frame in frames.frames {
            p.after += 1;
            let step = match &mut p.side {
                Side::Inviter(i) => i.on_frame(&frame),
                Side::Joiner(j) => j.on_frame(&frame),
            };
            let step = match step {
                Ok(s) => s,
                Err(_) => {
                    p.state = "failed";
                    return Ok(());
                }
            };
            if let Some(send) = step.send {
                transport
                    .verify_push(token, verify_id, &send)
                    .map_err(|e| e.to_string())?;
            }
            if let Some(outcome) = step.outcome {
                p.state = match outcome {
                    Outcome::Matched => self.mark_verified(p)?,
                    Outcome::WrongPhrase => "wrong-phrase",
                    Outcome::KeysDiffer => "keys-differ",
                };
                return Ok(());
            }
        }
        if frames.closed || lockdown_vault::Timestamp::now().0 >= p.expires_at {
            p.state = "expired";
        }
        Ok(())
    }

    /// Marks the contact verified, if the pin is still the one the check
    /// proved (a pin that moved in between proves nothing).
    fn mark_verified(&mut self, p: &Pending) -> Result<&'static str, String> {
        let theirs = if p.views.inviter == p.account {
            p.views.inviter_fp
        } else {
            p.views.joiner_fp
        };
        if self.contact_fingerprint_bytes(p.account)? != theirs {
            return Ok("failed");
        }
        let mut contact = self
            .sharing
            .contact(p.account)
            .cloned()
            .ok_or("that person isn't pinned here")?;
        contact.verified_at = Some(lockdown_vault::Timestamp::now());
        self.sharing.put_contact(contact);
        self.save_sharing()?;
        Ok("matched")
    }

    /// Drops a check from here (the relay closes it on its own).
    pub fn phrase_cancel(&mut self, verify_id: &str) -> Result<(), String> {
        let verify_id = parse_id(verify_id)?;
        self.phrase_checks.remove(&verify_id);
        Ok(())
    }

    /// Every check under way here.
    pub fn phrase_checks(&self) -> Vec<PhraseCheckView> {
        self.phrase_checks
            .iter()
            .map(|(id, p)| p.view(*id))
            .collect()
    }
}
