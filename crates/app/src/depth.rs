//! Vault depth on the app's command surface (docs/plan/21-vault-depth.md):
//! the person's own templates, link targets, the generator's options and a
//! site's rules, the Wi-Fi guest card, sign-ins logged elsewhere, the
//! account's attachment room and files attached from disk.
//!
//! Every call that hands out something secret goes through the same gates
//! as a reveal: the Wi-Fi card's code is the network's password, so it
//! takes the `Reveal` gate on a crown jewel, is refused for a fill-only
//! item and counts as a use (D151).

use lockdown_crypto::generator::{PasswordRule, Recipe, SiteRule};
use lockdown_matcher::{MatchDecision, Origin};
use lockdown_vault::{ItemKind, ItemState};
use zeroize::Zeroizing;

use crate::browser::{best_decision, matcher_data, saved_urls, text};
use crate::session::Session;
use crate::session::depth::{RULE_FIELD, site_rule_of};
use crate::types::{
    AwayReminder, GeneratorPreview, LinkTargetView, SiteRuleView, StorageView, UserTemplateInput,
    UserTemplateView,
};
use crate::{App, AppError, AppResult, ApprovalKind, GateInput, Revealed};

/// zxcvbn's word for a password of `bits` of entropy, as the editor's
/// meter says it (D116): under 50 bits weak, under 80 fair, else strong.
pub fn strength_word(bits: f64) -> &'static str {
    if bits < 50.0 {
        "weak"
    } else if bits < 80.0 {
        "fair"
    } else {
        "strong"
    }
}

/// The rules saved on the logins exactly for `page`, when they agree on
/// one (several logins with different rules leave the site's own).
fn saved_rule_for(session: &Session, page: &str) -> AppResult<Option<String>> {
    let current = Origin::parse(page).map_err(|e| AppError::refused(e.to_string()))?;
    let data = matcher_data()?;
    let mut rules = std::collections::BTreeSet::new();
    for vault in session.vaults() {
        for item in vault.items() {
            if item.kind != ItemKind::Login || item.state != ItemState::Live {
                continue;
            }
            let fields = vault
                .open_item(item.item_id)
                .map_err(|e| AppError::refused(e.to_string()))?;
            let Some(rule) = text(&fields, RULE_FIELD) else {
                continue;
            };
            if best_decision(&saved_urls(&fields), &current, &data).0 == MatchDecision::Exact {
                rules.insert(rule);
            }
        }
    }
    Ok(if rules.len() == 1 {
        rules.pop_first()
    } else {
        None
    })
}

impl App {
    // --- a person's own templates (D232) ----------------------------------

    /// Every template the person made, by name.
    pub fn user_templates(&self) -> AppResult<Vec<UserTemplateView>> {
        self.with_session(|s| s.user_templates())
    }

    /// Saves a template, new (in `vault_id`, the first vault when `None`)
    /// or as a new version of `id`. Resolves to its id.
    pub fn save_user_template(
        &self,
        vault_id: Option<&str>,
        id: Option<&str>,
        input: UserTemplateInput,
    ) -> AppResult<String> {
        let (vault_id, id) = (vault_id.map(str::to_string), id.map(str::to_string));
        self.with_session(move |s| s.save_user_template(vault_id.as_deref(), id.as_deref(), &input))
    }

    /// Moves a template to the trash; what was made from it stays.
    pub fn delete_user_template(&self, id: &str) -> AppResult<()> {
        let id = id.to_string();
        self.with_session(move |s| s.delete_user_template(&id))
    }

    // --- links (D235) -------------------------------------------------------

    /// What a link in `vault_id` may point at, by label only.
    pub fn link_targets(
        &self,
        vault_id: &str,
        except: Option<&str>,
    ) -> AppResult<Vec<LinkTargetView>> {
        let (vault_id, except) = (vault_id.to_string(), except.map(str::to_string));
        self.with_session(move |s| s.link_targets(&vault_id, except.as_deref()))
    }

    // --- the generator (D230, D236) -------------------------------------------

    /// What `recipe` would make, without making it: its words, its bits
    /// and, for a login whose site publishes rules (or that keeps its
    /// own, `rule`), the rules that will apply instead. Static: reads
    /// nothing from the vault.
    pub fn generator_preview(
        &self,
        recipe: &Recipe,
        url: Option<&str>,
        rule: Option<&str>,
    ) -> AppResult<GeneratorPreview> {
        recipe
            .check()
            .map_err(|e| AppError::refused(e.to_string()))?;
        let site = site_rule_of(rule, url);
        let bits = match &site {
            Some((r, _)) => recipe
                .site_rule(r)
                .map_err(|e| AppError::refused(e.to_string()))?
                .entropy_bits(),
            None => recipe
                .entropy_bits()
                .map_err(|e| AppError::refused(e.to_string()))?,
        };
        Ok(GeneratorPreview {
            words: match &site {
                Some((r, _)) => format!("the site's rules: {}", r.describe()),
                None => recipe.describe(),
            },
            bits: (bits * 10.0).round() / 10.0,
            strength: strength_word(bits).into(),
            site: site.map(|(_, v)| v),
        })
    }

    /// The rules a site publishes for new passwords, if it does.
    pub fn site_rule(&self, url: &str) -> Option<SiteRuleView> {
        site_rule_of(None, Some(url)).map(|(_, v)| v)
    }

    /// What a strong password suggested in the browser for `page` follows
    /// (D236): the rules saved on the one login that is exactly for this
    /// page, else the rules the site publishes, else Lockdown's own strong
    /// recipe. Names the rules only; nothing is made.
    pub fn password_suggestion(&self, page: &str) -> AppResult<GeneratorPreview> {
        let page = page.to_string();
        let recipe = Recipe::from(PasswordRule::STRONG);
        let rule =
            self.call(move |core| saved_rule_for(core.session()?, &page).map(|r| (r, page)))?;
        self.generator_preview(&recipe, Some(&rule.1), rule.0.as_deref())
    }

    /// A strong password for a new-password field on `page`, under the
    /// rules [`App::password_suggestion`] names. It is new, not read from
    /// the vault, so no gate applies; it reaches the vault only when the
    /// person saves the sign-up (`save_login`).
    pub fn suggest_password(&self, page: &str) -> AppResult<(Zeroizing<String>, GeneratorPreview)> {
        let preview = self.password_suggestion(page)?;
        let recipe = Recipe::from(PasswordRule::STRONG);
        let value = match preview
            .site
            .as_ref()
            .and_then(|s| SiteRule::parse(&s.rule).ok())
        {
            Some(rule) => recipe.generate_for(&rule),
            None => recipe.generate(),
        }
        .map_err(|e| AppError::refused(e.to_string()))?;
        Ok((value, preview))
    }

    // --- the Wi-Fi guest card (D239) -------------------------------------------

    /// The network's joining code as a QR grid ([`crate::session::depth`]
    /// `qr_bytes`): the password is in it, so it goes through the `Reveal`
    /// gate on a crown jewel, never to a fill-only item, and is a use.
    pub fn wifi_qr(&self, id: &str, gate: GateInput) -> AppResult<Revealed> {
        let id = id.to_string();
        self.call(move |core| {
            core.session()?
                .refuse_fill_only(&id)
                .map_err(AppError::refused)?;
            core.gate_if_crown(&id, ApprovalKind::Reveal, "show this network's code", &gate)?;
            let grid = core.session()?.wifi_qr(&id).map_err(AppError::refused)?;
            core.note_use(&id);
            Ok(grid)
        })
    }

    // --- sign-ins elsewhere (D237) -----------------------------------------------

    /// Logs a sign-in on someone else's screen, to be reminded in
    /// `remind_in_secs` to sign out there. A crown jewel changes only
    /// through the `Use` gate, as any edit of one does.
    pub fn log_away(
        &self,
        id: &str,
        place: &str,
        remind_in_secs: i64,
        gate: GateInput,
    ) -> AppResult<u64> {
        let (id, place) = (id.to_string(), place.to_string());
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            let now = crate::session::now_secs();
            core.session()?
                .log_away(&id, &place, now, now.saturating_add(remind_in_secs.max(0)))
                .map_err(AppError::refused)
        })
    }

    /// Marks a logged sign-in signed out.
    pub fn clear_away(&self, id: &str, at: i64, gate: GateInput) -> AppResult<u64> {
        let id = id.to_string();
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            core.session()?
                .clear_away(&id, at)
                .map_err(AppError::refused)
        })
    }

    /// Every logged sign-in elsewhere, the soonest reminder first.
    pub fn away_reminders(&self) -> AppResult<Vec<AwayReminder>> {
        self.with_session(|s| s.away_reminders(crate::session::now_secs()))
    }

    // --- the account's attachment room (D234) ------------------------------------

    /// How much of the account's attachment room on the server its vaults
    /// use; `None` without an account. Asks the server.
    pub fn storage_usage(&self) -> AppResult<Option<StorageView>> {
        let target = self.call(|core| {
            core.clock.touch(std::time::Instant::now());
            Ok(core.session()?.account_session())
        })?;
        let (server, token) = match target {
            None => return Ok(None),
            Some(Ok(pair)) => pair,
            Some(Err(e)) => return Err(AppError::refused(e)),
        };
        let usage = lockdown_sync::HttpTransport::new(&server)
            .map_err(|e| AppError::refused(e.to_string()))?
            .storage(&token)
            .map_err(|e| AppError::refused(e.to_string()))?;
        Ok(Some(StorageView {
            used: usage.used,
            quota: usage.quota,
            used_words: lockdown_sync::bytes_words(usage.used),
            quota_words: usage
                .quota
                .map(lockdown_sync::bytes_words)
                .unwrap_or_default(),
        }))
    }

    // --- files from disk, a chunk at a time (D233) ---------------------------------

    /// Attaches the file at `path` (the shell's own file dialog chose it;
    /// the webview never holds the bytes), sealed from the disk a chunk at
    /// a time. To a crown jewel, through the `Use` gate.
    #[cfg(not(target_os = "wasi"))]
    pub fn attach_file(
        &self,
        id: &str,
        path: std::path::PathBuf,
        mime: Option<&str>,
        gate: GateInput,
    ) -> AppResult<String> {
        let (id, mime) = (id.to_string(), mime.map(str::to_string));
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Use, "change this protected item", &gate)?;
            core.session()?
                .attach_file(&id, &path, mime.as_deref())
                .map(|a| a.to_string())
                .map_err(AppError::refused)
        })
    }

    /// Writes an attached file to `path` a chunk at a time. A crown jewel's
    /// goes through the `Reveal` gate; a save is a use.
    #[cfg(not(target_os = "wasi"))]
    pub fn save_attachment(
        &self,
        id: &str,
        att_id: &str,
        path: std::path::PathBuf,
        gate: GateInput,
    ) -> AppResult<()> {
        let (id, att_id) = (id.to_string(), att_id.to_string());
        self.call(move |core| {
            core.gate_if_crown(&id, ApprovalKind::Reveal, "open this protected file", &gate)?;
            core.session()?
                .save_attachment_to(&id, &att_id, &path)
                .map_err(AppError::refused)?;
            core.note_use(&id);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldChange, LockPolicy, Paths};
    use lockdown_vault::{Change, FieldInput, FieldKind};

    fn confirmed_gate() -> GateInput {
        let mut g = GateInput::default();
        g.confirmed = true;
        g
    }

    fn fresh() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        let app = App::start(paths, None, LockPolicy::default());
        app.open_or_create(true).unwrap();
        (dir, app)
    }

    #[test]
    fn variants_and_their_markers_cross_as_template_ids() {
        let (_dir, app) = fresh();
        let views = app.templates();
        let ids: Vec<&str> = views.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"identity/passport") && ids.contains(&"wi-fi/router"));
        assert_eq!(views.iter().filter(|t| t.creatable).count(), 30);
        let contact = views.iter().find(|t| t.id == "identity/contact").unwrap();
        assert!(!contact.fills);
        let passport = app
            .add_item(
                "identity/passport",
                Some("Passport"),
                false,
                vec![
                    FieldChange::secret("passport.number", b"X1234567"),
                    FieldChange::text("expires_at", "2034-01-31"),
                ],
            )
            .unwrap();
        let row = app
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == passport)
            .unwrap();
        assert_eq!(
            (row.kind.as_str(), row.template.as_str()),
            ("identity", "identity/passport")
        );
        let detail = app.show(&passport, false).unwrap();
        assert_eq!(detail.template, "identity/passport");
        // The marker is Lockdown's own: never shown, kept by an edit.
        assert!(
            detail
                .fields
                .iter()
                .all(|f| f.field_id != "lockdown.template")
        );
        let changes: Vec<FieldChange> = detail
            .fields
            .iter()
            .filter(|f| f.field_id != "title")
            .map(|f| FieldChange::keep(&f.field_id))
            .collect();
        app.edit_item(
            &passport,
            Some("My passport"),
            changes,
            GateInput::default(),
        )
        .unwrap();
        assert_eq!(
            app.show(&passport, false).unwrap().template,
            "identity/passport"
        );
        assert!(
            app.add_item("identity/starship", None, false, vec![])
                .is_err()
        );
        // An email account is named after its address.
        let mail = app
            .add_item(
                "server/email-account",
                None,
                false,
                vec![FieldChange::text("email.address", "me@example.com")],
            )
            .unwrap();
        let row = app
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == mail)
            .unwrap();
        assert_eq!(row.title, "me@example.com");
        // A contact is in Contacts and never offered to a form.
        let c = app
            .add_item(
                "identity/contact",
                Some("Gran"),
                false,
                vec![FieldChange::text("address.city", "Leeds")],
            )
            .unwrap();
        let row = app.list().unwrap().into_iter().find(|r| r.id == c).unwrap();
        assert!(row.views.contains(&"contacts".to_string()));
        assert!(app.wallet().unwrap().iter().all(|w| w.item_id != c));
    }

    #[test]
    fn a_persons_template_makes_items_and_keeps_a_history() {
        let (_dir, app) = fresh();
        let input = UserTemplateInput {
            name: "Gym locker".into(),
            summary: "Where my things are".into(),
            icon: "fitness_center".into(),
            fields: vec![
                crate::types::UserFieldView {
                    key: "locker".into(),
                    field_id: String::new(),
                    label: "Locker".into(),
                    kind: "text".into(),
                    required: true,
                    hint: String::new(),
                    section: String::new(),
                    secret: false,
                },
                crate::types::UserFieldView {
                    key: "combo".into(),
                    field_id: String::new(),
                    label: "Combination".into(),
                    kind: "hidden".into(),
                    required: false,
                    hint: String::new(),
                    section: String::new(),
                    secret: false,
                },
            ],
        };
        let tid = app.save_user_template(None, None, input.clone()).unwrap();
        let listed = app.user_templates().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].fields[1].field_id, "tpl.combo");
        assert!(listed[0].fields[1].secret);
        let item = app
            .add_item(
                &format!("user:{tid}"),
                None,
                false,
                vec![
                    FieldChange::text("tpl.locker", "41"),
                    FieldChange::secret("tpl.combo", b"12-34-56"),
                ],
            )
            .unwrap();
        let row = app
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == item)
            .unwrap();
        assert_eq!(
            (row.title.as_str(), row.kind.as_str()),
            ("Gym locker", "custom")
        );
        assert_eq!(row.template, format!("user:{tid}"));
        assert_eq!(
            app.reveal(&item, "tpl.combo", GateInput::default())
                .unwrap()
                .expose(),
            b"12-34-56"
        );
        // A required field left out is refused; a secret as text is refused.
        assert!(
            app.add_item(&format!("user:{tid}"), None, false, vec![])
                .is_err()
        );
        assert!(
            app.add_item(
                &format!("user:{tid}"),
                None,
                false,
                vec![
                    FieldChange::text("tpl.locker", "1"),
                    FieldChange::text("tpl.combo", "plain")
                ]
            )
            .is_err()
        );
        // A new version, then the trash: what was made stays.
        let mut v2 = input;
        v2.name = "Locker".into();
        app.save_user_template(None, Some(&tid), v2).unwrap();
        assert_eq!(app.user_templates().unwrap()[0].versions, 2);
        app.delete_user_template(&tid).unwrap();
        assert!(app.user_templates().unwrap().is_empty());
        assert!(app.show(&item, false).is_ok());
        assert!(app.delete_user_template(&item).is_err(), "not a template");
        // Templates are hidden items: never in the list.
        assert!(app.list().unwrap().iter().all(|r| r.id != tid));
    }

    #[test]
    fn a_linked_field_reads_through_its_targets_gates() {
        let (_dir, app) = fresh();
        let wifi = app
            .add_item(
                "wi-fi",
                None,
                false,
                vec![
                    FieldChange::text("wifi.ssid", "home"),
                    FieldChange::secret("wifi.passphrase", b"k7Hq3mWx9rTa4nPe6uYc"),
                ],
            )
            .unwrap();
        let wallet = app
            .add_item(
                "crypto-wallet",
                Some("Cold"),
                false,
                vec![FieldChange::secret(
                    "seed",
                    b"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                )],
            )
            .unwrap();
        let link = |target: &str, field: &str| {
            FieldChange::custom(
                "Points at",
                FieldKind::Linked,
                FieldInput::Text(format!("{target}#{field}")),
            )
        };
        let note = app
            .add_item(
                "custom",
                Some("Guest info"),
                false,
                vec![link(&wifi, "wifi.passphrase")],
            )
            .unwrap();
        let shown = app.show(&note, false).unwrap();
        let f = shown
            .fields
            .iter()
            .find(|f| f.field_id == "custom.0")
            .unwrap();
        assert_eq!(f.kind, "linked");
        let l = f.link.as_ref().unwrap();
        assert_eq!(
            (l.title.as_str(), l.label.as_str(), l.secret, l.missing),
            ("home", "Password", true, false)
        );
        // Its value is the network's, read through the network.
        assert_eq!(
            app.reveal(&note, "custom.0", GateInput::default())
                .unwrap()
                .expose(),
            b"k7Hq3mWx9rTa4nPe6uYc"
        );
        // A link to a crown jewel asks as the crown jewel does.
        let guarded = app
            .add_item(
                "custom",
                Some("Wallet pointer"),
                false,
                vec![link(&wallet, "seed")],
            )
            .unwrap();
        let err = app
            .reveal(&guarded, "custom.0", GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::ApprovalNeeded, "{err:?}");
        // Bad targets are refused: another link, itself, nothing there.
        assert!(
            app.add_item("custom", Some("x"), false, vec![link(&note, "custom.0")])
                .is_err()
        );
        assert!(
            app.add_item(
                "custom",
                Some("x"),
                false,
                vec![link(&wifi, "wifi.nothing")]
            )
            .is_err()
        );
        assert!(
            app.add_item(
                "custom",
                Some("x"),
                false,
                vec![link(&lockdown_vault::Id::new().to_string(), "pw")]
            )
            .is_err()
        );
        // The pickers list fields by label, never a value; a crown jewel by its template's.
        let vault = app.vaults().unwrap()[0].id.clone();
        let targets = app.link_targets(&vault, Some(&note)).unwrap();
        assert!(targets.iter().all(|t| t.item_id != note));
        let w = targets.iter().find(|t| t.item_id == wallet).unwrap();
        assert!(w.crown_jewel && w.fields.iter().any(|f| f.label == "Recovery phrase"));
        // When the target goes to the trash, the link says so and reads nothing.
        app.trash(&wifi).unwrap();
        let shown = app.show(&note, false).unwrap();
        assert!(
            shown
                .fields
                .iter()
                .find(|f| f.field_id == "custom.0")
                .unwrap()
                .link
                .as_ref()
                .unwrap()
                .missing
        );
        assert!(app.reveal(&note, "custom.0", GateInput::default()).is_err());
    }

    #[test]
    fn a_code_of_your_own_goes_on_any_item() {
        let (_dir, app) = fresh();
        let id = app
            .add_item(
                "membership",
                Some("Gym"),
                false,
                vec![FieldChange::custom(
                    "Door code",
                    FieldKind::Totp,
                    FieldInput::Secret(zeroize::Zeroizing::new(
                        b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_vec(),
                    )),
                )],
            )
            .unwrap();
        let codes = app.codes().unwrap();
        let c = codes.iter().find(|c| c.item_id == id).unwrap();
        assert_eq!((c.field_id.as_str(), c.title.as_str()), ("custom.0", "Gym"));
        assert_eq!(
            app.code_now(&id, "custom.0", false, GateInput::default())
                .unwrap()
                .expose()
                .len(),
            6
        );
    }

    #[test]
    fn the_card_asks_before_its_security_code() {
        let (_dir, app) = fresh();
        let card = app
            .add_item(
                "card",
                Some("Everyday"),
                false,
                vec![
                    FieldChange::secret("card.number", b"4242424242424242"),
                    FieldChange::secret("card.cvc", b"123"),
                    FieldChange::text("card.reprompt", "true"),
                ],
            )
            .unwrap();
        assert!(app.show(&card, false).unwrap().reprompt);
        // The number goes as before; the code asks.
        assert!(
            app.reveal(&card, "card.number", GateInput::default())
                .is_ok()
        );
        let err = app
            .reveal_to_use(&card, "card.cvc", GateInput::default())
            .unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::ApprovalNeeded);
        let confirmed = confirmed_gate();
        assert_eq!(
            app.reveal_to_use(&card, "card.cvc", confirmed)
                .unwrap()
                .expose(),
            b"123"
        );
        // Turned off, it doesn't ask.
        let detail = app.show(&card, false).unwrap();
        let mut changes: Vec<FieldChange> = detail
            .fields
            .iter()
            .filter(|f| f.field_id != "title" && f.field_id != "card.reprompt")
            .map(|f| FieldChange::keep(&f.field_id))
            .collect();
        changes.push(FieldChange::text("card.reprompt", "false"));
        app.edit_item(&card, None, changes, GateInput::default())
            .unwrap();
        assert!(app.reveal(&card, "card.cvc", GateInput::default()).is_ok());
    }

    #[test]
    fn the_wifi_card_is_gated_and_counted_as_a_use() {
        let (_dir, app) = fresh();
        let id = app
            .add_item(
                "wi-fi",
                None,
                true,
                vec![
                    FieldChange::text("wifi.ssid", "attic"),
                    FieldChange::secret("wifi.passphrase", b"k7Hq3mWx9rTa4nPe6uYc"),
                ],
            )
            .unwrap();
        // A crown jewel (opted in): the code waits for the gate.
        let err = app.wifi_qr(&id, GateInput::default()).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::ApprovalNeeded);
        let grid = app.wifi_qr(&id, confirmed_gate()).unwrap();
        let w = usize::from(grid.expose()[0]);
        assert_eq!(grid.expose().len(), 1 + w * w);
        assert_eq!(
            app.list()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .uses,
            1
        );
        let login = app
            .add_login("example.com", "me", b"pw".to_vec(), None)
            .unwrap();
        assert!(app.wifi_qr(&login, GateInput::default()).is_err());
    }

    #[test]
    fn a_sign_in_elsewhere_is_logged_reminded_and_cleared() {
        let (_dir, app) = fresh();
        let id = app
            .add_login("netflix.com", "me", b"pw".to_vec(), None)
            .unwrap();
        app.log_away(&id, "Hotel room TV", 0, GateInput::default())
            .unwrap();
        app.log_away(&id, "Rental flat", 86_400, GateInput::default())
            .unwrap();
        let due = app.away_reminders().unwrap();
        assert_eq!(due.len(), 2);
        assert!(due[0].due && !due[1].due);
        assert_eq!(due[0].place, "Hotel room TV");
        let row = app
            .list()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap();
        assert!(row.views.contains(&"attention".to_string()));
        let detail = app.show(&id, false).unwrap();
        assert_eq!(detail.away.len(), 2);
        assert!(detail.fields.iter().all(|f| f.field_id != "lockdown.away"));
        app.clear_away(&id, due[0].at, GateInput::default())
            .unwrap();
        assert_eq!(app.away_reminders().unwrap().len(), 1);
        assert!(
            app.clear_away(&id, due[0].at, GateInput::default())
                .is_err()
        );
        assert!(app.log_away(&id, "  ", 10, GateInput::default()).is_err());
        assert!(
            app.log_away(&id, "two\nlines", 10, GateInput::default())
                .is_err()
        );
    }

    #[test]
    fn the_generator_previews_options_and_a_sites_rules() {
        let (_dir, app) = fresh();
        let words = app
            .generator_preview(&Recipe::words(6), None, None)
            .unwrap();
        assert_eq!(words.words, "6 words joined by \"-\"");
        assert!((words.bits - 77.5).abs() < 0.1);
        assert_eq!(words.strength, "fair");
        let site = app
            .generator_preview(&Recipe::words(6), Some("https://www.1800flowers.com"), None)
            .unwrap();
        assert!(site.site.is_some() && site.words.starts_with("the site's rules"));
        let saved = app
            .generator_preview(
                &lockdown_crypto::generator::PasswordRule::STRONG.into(),
                Some("https://example.com"),
                Some("minlength: 6; maxlength: 6; required: digit;"),
            )
            .unwrap();
        assert_eq!(saved.site.unwrap().source, "saved");
        assert_eq!(saved.strength, "weak");
        assert!(
            app.generator_preview(&Recipe::words(2), None, None)
                .is_err()
        );
        // A Login generated under its rule meets it.
        let id = app
            .add_item(
                "login",
                Some("PIN site"),
                false,
                vec![
                    FieldChange::text("url.0", "https://pin.example"),
                    FieldChange::text("pw.rule", "minlength: 6; maxlength: 6; required: digit;"),
                    FieldChange::new("pw", Change::Generate),
                ],
            )
            .unwrap();
        let pw = app.reveal(&id, "pw", GateInput::default()).unwrap();
        assert_eq!(pw.expose().len(), 6);
        assert!(pw.expose().iter().all(u8::is_ascii_digit));
        assert!(app.site_rule("https://unlisted.example").is_none());
        assert_eq!(strength_word(125.0), "strong");
    }

    #[test]
    fn a_suggested_password_follows_the_saved_rules_then_the_sites_own() {
        let (_dir, app) = fresh();
        let plain = app
            .suggest_password("https://unlisted.example/join")
            .unwrap();
        assert!(plain.1.site.is_none() && plain.1.strength == "strong");
        assert_eq!(plain.0.chars().count(), 20);
        let (flowers, view) = app
            .suggest_password("https://www.1800flowers.com/signup")
            .unwrap();
        assert_eq!(view.site.as_ref().unwrap().source, "published");
        assert!(flowers.chars().any(|c| c.is_ascii_digit()));
        assert_eq!(
            app.password_suggestion("https://www.1800flowers.com/signup")
                .unwrap(),
            view,
            "the menu names what a pick then makes"
        );
        // A login exactly for the page that keeps its own rules wins.
        app.add_item(
            "login",
            Some("PIN site"),
            false,
            vec![
                FieldChange::text("url.0", "https://pin.example"),
                FieldChange::text("pw.rule", "minlength: 6; maxlength: 6; required: digit;"),
            ],
        )
        .unwrap();
        let (pin, view) = app.suggest_password("https://pin.example/change").unwrap();
        assert_eq!(view.site.unwrap().source, "saved");
        assert!(pin.len() == 6 && pin.bytes().all(|b| b.is_ascii_digit()));
        // Two logins with different rules leave the choice to the site.
        app.add_item(
            "login",
            Some("PIN site, again"),
            false,
            vec![
                FieldChange::text("url.0", "https://pin.example"),
                FieldChange::text("pw.rule", "minlength: 8; maxlength: 8; required: digit;"),
            ],
        )
        .unwrap();
        assert!(
            app.password_suggestion("https://pin.example/")
                .unwrap()
                .site
                .is_none()
        );
        assert!(app.suggest_password("not a page").is_err());
        app.lock().unwrap();
        assert!(app.suggest_password("https://pin.example/").is_err());
    }

    #[test]
    fn a_file_from_disk_attaches_and_saves_in_chunks() {
        let (dir, app) = fresh();
        let id = app
            .add_item("document", Some("Lease"), false, vec![])
            .unwrap();
        let src = dir.path().join("lease.pdf");
        let data: Vec<u8> = (0..(3 << 20) + 5).map(|i| (i % 249) as u8).collect();
        std::fs::write(&src, &data).unwrap();
        let att = app
            .attach_file(&id, src.clone(), None, GateInput::default())
            .unwrap();
        let detail = app.show(&id, false).unwrap();
        let a = detail
            .place
            .attachments
            .iter()
            .find(|a| a.id == att)
            .unwrap();
        assert_eq!(
            (a.name.as_str(), a.mime.as_str(), a.size),
            ("lease.pdf", "application/pdf", data.len() as u64)
        );
        let out = dir.path().join("copy.pdf");
        app.save_attachment(&id, &att, out.clone(), GateInput::default())
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), data);
        assert!(!dir.path().join("copy.pdf.partial").exists());
        // The whole-file path still reads it (sync, the old desktop call).
        assert_eq!(
            app.attachment(&id, &att, GateInput::default())
                .unwrap()
                .expose(),
            data.as_slice()
        );
        assert!(
            app.attach_file(
                &id,
                dir.path().join("missing.bin"),
                None,
                GateInput::default()
            )
            .is_err()
        );
    }
}
