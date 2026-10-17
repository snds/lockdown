//! One-time codes (D102): the Codes view reads every code item and every
//! code inside a Login through [`Session::codes`], which hands a shell the
//! current and next value and the seconds left, never the set-up. The
//! maths is `lockdown-codes`; nothing is reimplemented here.

use lockdown_codes::{Code, Variant, parse_uri};
use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};

use super::{Session, parse_id, text_of};
use crate::Revealed;
use crate::types::{CodeAdvanced, CodeView};

/// The warning when a code's issuer is the account's own login (02-model
/// §4): a code that protects the vault's own account must not live in it.
pub(super) const ISSUER_WARNING: &str = "issuer-equals-login";

/// Whether `code`'s issuer names the sync login.
pub(super) fn issuer_equals_login(code: &Code, login: Option<&str>) -> bool {
    matches!(
        (code.issuer.as_deref(), login),
        (Some(issuer), Some(login)) if issuer.trim().eq_ignore_ascii_case(login.trim())
    )
}

fn variant_name(v: Variant) -> &'static str {
    match v {
        Variant::Totp => "totp",
        Variant::Hotp => "hotp",
        Variant::Steam => "steam",
    }
}

/// The view of one code at `now`. Only derived values leave: the two
/// codes and the timing. `Code` itself (the seed) stays here.
fn code_view(
    item_id: Id,
    field_id: &str,
    title: &str,
    code: &Code,
    now: u64,
    login: Option<&str>,
) -> Result<CodeView, String> {
    let current = code.generate_at(now).map_err(|e| e.to_string())?;
    let next = match code.variant {
        Variant::Hotp => None,
        Variant::Totp | Variant::Steam => Some(
            code.code_for_counter(code.step_at(now) + 1)
                .map_err(|e| e.to_string())?,
        ),
    };
    let mut warnings = Vec::new();
    if issuer_equals_login(code, login) {
        warnings.push(ISSUER_WARNING.to_string());
    }
    Ok(CodeView {
        item_id: item_id.to_string(),
        field_id: field_id.to_string(),
        title: title.to_string(),
        issuer: code.issuer.clone(),
        account: code.account.clone(),
        variant: variant_name(code.variant).to_string(),
        digits: code.digits,
        period: code.period,
        current,
        next,
        seconds_left: code.next_code_in(now).map(|d| d.as_secs()),
        counter: (code.variant == Variant::Hotp).then_some(code.counter),
        warnings,
    })
}

impl Session {
    /// Every code in the vault at `now` (seconds since the epoch): each
    /// Code item and each code field inside any other live item, by title.
    /// A set-up that doesn't parse is skipped, never fails the view.
    pub fn codes(&self, now: u64) -> Result<Vec<CodeView>, String> {
        let vault = self.vault();
        let login = self.sync.as_ref().map(|c| c.login.as_str());
        let mut out = Vec::new();
        for item in vault.items() {
            if item.kind.hidden() || item.state != ItemState::Live {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let title = text_of(&fields, "title").unwrap_or_default();
            for f in &fields {
                if f.kind != FieldKind::Totp {
                    continue;
                }
                let FieldValue::Text(uri) = &f.value else {
                    continue;
                };
                let Ok(code) = parse_uri(uri) else {
                    continue;
                };
                out.push(code_view(
                    item.item_id,
                    &f.field_id,
                    &title,
                    &code,
                    now,
                    login,
                )?);
            }
        }
        out.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then(a.field_id.cmp(&b.field_id))
        });
        Ok(out)
    }

    /// The code valid at `now` for one code field (or, with `next`, the one
    /// after it, which the display offers in a code's last seconds), as
    /// bytes for the clipboard path: the value is short-lived and derived,
    /// the set-up stays here.
    pub fn code_now(
        &self,
        id: &str,
        field_id: &str,
        now: u64,
        next: bool,
    ) -> Result<Revealed, String> {
        let item_id = parse_id(id)?;
        let fields = self.vault().open_item(item_id).map_err(|e| e.to_string())?;
        let field = fields
            .iter()
            .find(|f| f.field_id == field_id && f.kind == FieldKind::Totp)
            .ok_or_else(|| format!("no code '{field_id}' on this item"))?;
        let FieldValue::Text(uri) = &field.value else {
            return Err("this code's set-up isn't readable".into());
        };
        let code = parse_uri(uri).map_err(|e| e.to_string())?;
        let value = if next {
            if code.variant == Variant::Hotp {
                return Err("a counter-based code has no next value until it is asked for".into());
            }
            code.code_for_counter(code.step_at(now) + 1)
                .map_err(|e| e.to_string())?
        } else {
            code.generate_at(now).map_err(|e| e.to_string())?
        };
        Ok(Revealed::from_text(&value))
    }

    /// Moves a counter-based (HOTP) code on by one: the counter in its
    /// set-up goes up, the item gets a new version (so the step syncs to
    /// every device and the history keeps the old counter), and the code
    /// for the new counter comes back with the counter. A time-based code
    /// (TOTP, Steam) has nothing to advance and is refused, as is a trashed
    /// item: nothing is written then.
    pub fn advance_code(&mut self, id: &str, field_id: &str) -> Result<CodeAdvanced, String> {
        let item_id = parse_id(id)?;
        let item = self.vault().item(item_id).map_err(|e| e.to_string())?;
        if item.state != ItemState::Live {
            return Err("that item isn't live; restore it first".into());
        }
        let mut fields = self.vault().open_item(item_id).map_err(|e| e.to_string())?;
        let field = fields
            .iter_mut()
            .find(|f| f.field_id == field_id && f.kind == FieldKind::Totp)
            .ok_or_else(|| format!("no code '{field_id}' on this item"))?;
        let FieldValue::Text(uri) = &field.value else {
            return Err("this code's set-up isn't readable".into());
        };
        let mut code = parse_uri(uri).map_err(|e| e.to_string())?;
        if code.variant != Variant::Hotp {
            return Err(
                "only a counter-based code moves on when asked; this one follows the clock".into(),
            );
        }
        code.counter = code
            .counter
            .checked_add(1)
            .ok_or("this code's counter can't go any higher")?;
        let value = code
            .code_for_counter(code.counter)
            .map_err(|e| e.to_string())?;
        field.value = FieldValue::Text(code.to_otpauth());
        let version = self
            .opened
            .vault
            .update_item(item_id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(CodeAdvanced {
            code: value,
            counter: code.counter,
            version,
        })
    }

    /// Adds a code from its `otpauth://` or `steam://` URI: inside the item
    /// `into` (a Login keeps its codes, brief §1), or as a Code item of its
    /// own. The URI is checked to be one this device can generate from
    /// before anything is written.
    pub fn add_code(
        &mut self,
        uri: &str,
        title: Option<&str>,
        into: Option<&str>,
    ) -> Result<Id, String> {
        let uri = uri.trim();
        let code = parse_uri(uri).map_err(|e| e.to_string())?;
        code.validate().map_err(|e| e.to_string())?;
        let id = match into {
            Some(into) => {
                let item_id = parse_id(into)?;
                let item = self.vault().item(item_id).map_err(|e| e.to_string())?;
                if item.state != ItemState::Live {
                    return Err("that item isn't live; restore it first".into());
                }
                let mut fields = self.vault().open_item(item_id).map_err(|e| e.to_string())?;
                let n = fields.iter().filter(|f| f.kind == FieldKind::Totp).count();
                fields.push(PlainField::new(
                    format!("code.{n}"),
                    FieldKind::Totp,
                    "one-time code",
                    FieldValue::Text(uri.to_string()),
                ));
                self.opened
                    .vault
                    .update_item(item_id, fields)
                    .map_err(|e| e.to_string())?;
                item_id
            }
            None => {
                let title = title
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| match (&code.issuer, &code.account) {
                        (Some(i), Some(a)) => format!("{i} ({a})"),
                        (Some(i), None) => i.clone(),
                        (None, Some(a)) => a.clone(),
                        (None, None) => "One-time code".into(),
                    });
                let fields = vec![
                    PlainField::text("title", "title", &title),
                    PlainField::new(
                        "code.0",
                        FieldKind::Totp,
                        "one-time code",
                        FieldValue::Text(uri.to_string()),
                    ),
                ];
                self.opened
                    .vault
                    .create_item(ItemKind::Code, ItemClass::Standard, fields)
                    .map_err(|e| e.to_string())?
            }
        };
        self.save()?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Mode, Paths};

    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn session(dir: &tempfile::TempDir, mode: Mode) -> Session {
        let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
        Session::open(paths, None, mode).unwrap()
    }

    #[test]
    fn an_hotp_advance_is_a_new_version_and_outlives_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(&dir, Mode::Init);
        // RFC 4226 test key ("12345678901234567890"): counter 3 is 969429, 4 is 338314.
        let uri = format!("otpauth://hotp/Example:alice?secret={SEED}&counter=3");
        let id = s.add_code(&uri, None, None).unwrap();
        let before = s.vault().item(id).unwrap().versions.len();
        let view = s.codes(0).unwrap();
        assert_eq!(
            (view[0].current.as_str(), view[0].counter),
            ("969429", Some(3))
        );
        assert_eq!(view[0].next, None);

        let step = s.advance_code(&id.to_string(), "code.0").unwrap();
        assert_eq!((step.code.as_str(), step.counter), ("338314", 4));
        assert_eq!(s.vault().item(id).unwrap().versions.len(), before + 1);
        assert_eq!(step.version as usize, before + 1);
        // What `code_now` copies is the code the display now shows.
        let copied = s.code_now(&id.to_string(), "code.0", 0, false).unwrap();
        assert_eq!(copied.expose(), b"338314");
        drop(s);

        // Locked (the session dropped) and opened again: the counter held.
        let mut s = session(&dir, Mode::Open);
        let view = s.codes(0).unwrap();
        assert_eq!(
            (view[0].current.as_str(), view[0].counter),
            ("338314", Some(4))
        );
        let step = s.advance_code(&id.to_string(), "code.0").unwrap();
        assert_eq!((step.code.as_str(), step.counter), ("254676", 5));
        // The set-up's other parts came through the rewrite unchanged.
        let fields = s.vault().open_item(id).unwrap();
        let FieldValue::Text(uri) = &fields
            .iter()
            .find(|f| f.field_id == "code.0")
            .unwrap()
            .value
        else {
            panic!("a code's set-up is text");
        };
        let code = parse_uri(uri).unwrap();
        assert_eq!(code.issuer.as_deref(), Some("Example"));
        assert_eq!(code.account.as_deref(), Some("alice"));
        assert_eq!((code.digits, code.counter), (6, 5));
    }

    #[test]
    fn a_time_based_code_has_nothing_to_advance() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(&dir, Mode::Init);
        let totp = s
            .add_code(&format!("otpauth://totp/Ex:me?secret={SEED}"), None, None)
            .unwrap();
        let steam = s
            .add_code(
                &format!("otpauth://totp/Steam:me?secret={SEED}&issuer=Steam"),
                None,
                None,
            )
            .unwrap();
        for id in [totp, steam] {
            let versions = s.vault().item(id).unwrap().versions.len();
            let err = s.advance_code(&id.to_string(), "code.0").unwrap_err();
            assert!(err.contains("follows the clock"), "{err}");
            assert_eq!(
                s.vault().item(id).unwrap().versions.len(),
                versions,
                "nothing written"
            );
        }
        assert_eq!(
            s.codes(0)
                .unwrap()
                .iter()
                .map(|c| c.counter)
                .collect::<Vec<_>>(),
            [None, None]
        );
        assert!(s.advance_code(&totp.to_string(), "title").is_err());
        s.trash(&totp.to_string()).unwrap();
        assert!(
            s.advance_code(&totp.to_string(), "code.0")
                .unwrap_err()
                .contains("isn't live")
        );
    }
}
