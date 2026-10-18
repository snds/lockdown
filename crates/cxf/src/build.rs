//! The one place an importer turns a source's values into Lockdown fields.
//!
//! Every importer reads its own format and calls a [`Builder`]: a value
//! for a template field lands in that field with the template's kind and
//! label; a value for a field the template lacks (or a second value for
//! one it has) lands as a field of the person's own (`custom.<n>`) with the
//! source's label, so nothing is dropped; a value that can't be carried
//! (a code set-up that doesn't parse) is reported in words.

use lockdown_vault::template::{Control, template_for};
use lockdown_vault::{FieldKind, FieldValue, ItemKind, PlainField};
use zeroize::Zeroizing;

use crate::convert::{ImportedAttachment, ImportedItem, mime_for};

/// Builds one [`ImportedItem`].
#[derive(Debug)]
pub struct Builder {
    item: ImportedItem,
    notes: Vec<String>,
}

fn trimmed(value: &str) -> Option<&str> {
    let t = value.trim();
    (!t.is_empty()).then_some(t)
}

/// `YYYY-MM-DD` from seconds since the epoch (UTC), for sources that keep
/// dates as timestamps.
pub fn date_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM` from a month and a year in any of the shapes managers write
/// (`3`, `03`, `March`; `27`, `2027`), or `None`.
pub fn month_of(month: &str, year: &str) -> Option<String> {
    const NAMES: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let m = month.trim().to_ascii_lowercase();
    let m: u32 = match m.parse::<u32>() {
        Ok(n) => n,
        Err(_) => NAMES.iter().position(|n| m.starts_with(n))? as u32 + 1,
    };
    if !(1..=12).contains(&m) {
        return None;
    }
    let y = year.trim();
    let y: u32 = y.parse().ok()?;
    let y = if y < 100 { 2000 + y } else { y };
    (1000..=9999).contains(&y).then(|| format!("{y:04}-{m:02}"))
}

/// A month from one string: `MM/YYYY`, `MM/YY`, `YYYY-MM`, `MMYYYY`,
/// `March,2027`.
pub fn month_from(text: &str) -> Option<String> {
    let t = text.trim();
    if let Some((a, b)) = t.split_once(['/', ',', ' ']) {
        return month_of(a, b);
    }
    if let Some((y, m)) = t.split_once('-')
        && y.len() == 4
    {
        return month_of(m.get(..2).unwrap_or(m), y);
    }
    if t.len() == 6 && t.bytes().all(|b| b.is_ascii_digit()) {
        return month_of(&t[..2], &t[2..]);
    }
    if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
        return month_of(&t[..2], &t[2..]);
    }
    None
}

/// A date as `YYYY-MM-DD` from the shapes managers write, or `None`.
pub fn date_from(text: &str) -> Option<String> {
    let t = text.trim();
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = t.split(['-', '/', '.']).collect();
    let (y, m, d) = match parts.as_slice() {
        [y, m, d] if digits(y, 4) => (*y, *m, d.get(..2).unwrap_or(d)),
        [m, d, y] if digits(y, 4) => (*y, *m, *d),
        _ => {
            if digits(t, 10) || digits(t, 9) {
                return t.parse::<i64>().ok().map(date_from_unix);
            }
            return None;
        }
    };
    let (m, d): (u32, u32) = (m.parse().ok()?, d.trim().parse().ok()?);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| format!("{y}-{m:02}-{d:02}"))
}

impl Builder {
    /// A new item; an empty title becomes "Imported <kind>".
    pub fn new(source_id: impl Into<String>, kind: ItemKind, title: &str) -> Self {
        let title = match trimmed(title) {
            Some(t) => t.replace(['\n', '\r'], " "),
            None => format!("Imported {}", template_for(kind).name.to_lowercase()),
        };
        Self {
            item: ImportedItem::new(source_id, kind, title),
            notes: Vec::new(),
        }
    }

    /// The kind so far.
    pub fn kind(&self) -> ItemKind {
        self.item.kind
    }

    /// Whether the item has a field with this id.
    pub fn has(&self, field_id: &str) -> bool {
        self.item.fields.iter().any(|f| f.field_id == field_id)
    }

    fn next(&self, prefix: &str) -> usize {
        self.item
            .fields
            .iter()
            .filter_map(|f| f.field_id.strip_prefix(prefix)?.parse::<usize>().ok())
            .max()
            .map_or(0, |n| n + 1)
    }

    fn push(&mut self, field: PlainField) {
        self.item.fields.push(field);
    }

    /// A value for the kind's template field `field_id`. Empty values are
    /// skipped. A value the field can't hold as written (a date that isn't
    /// one, a choice that isn't on the list), a second value, or an id the
    /// template lacks lands as a field of the person's own under `label`
    /// (or the template's label).
    pub fn field(&mut self, field_id: &str, value: &str) -> &mut Self {
        self.field_labelled(field_id, value, None)
    }

    /// As [`Self::field`], with the source's label for the fallback.
    pub fn field_labelled(
        &mut self,
        field_id: &str,
        value: &str,
        label: Option<&str>,
    ) -> &mut Self {
        let Some(v) = trimmed(value) else {
            return self;
        };
        let template = template_for(self.item.kind);
        let Some(t) = template.field(field_id) else {
            let kind = if field_id.contains("pin") || field_id.contains("password") {
                FieldKind::Hidden
            } else {
                FieldKind::Text
            };
            return self.custom(label.unwrap_or(field_id), kind, value);
        };
        let fallback_label = label.unwrap_or(t.label);
        if self.has(field_id) {
            let kind = if t.control.is_secret() {
                FieldKind::Hidden
            } else {
                FieldKind::Text
            };
            return self.custom(fallback_label, kind, value);
        }
        let stored = match t.control {
            Control::Text | Control::Number | Control::Email | Control::Phone => {
                Some(FieldValue::Text(v.replace(['\n', '\r'], " ")))
            }
            Control::Multiline => Some(FieldValue::Text(value.trim_end().to_string())),
            Control::Secret => Some(FieldValue::Text(v.to_string())),
            Control::SecretMultiline => Some(FieldValue::Text(value.trim().to_string())),
            Control::Url => Some(FieldValue::Url {
                value: if v.contains("://") {
                    v.to_string()
                } else {
                    format!("https://{v}")
                },
                match_rule: "exact_host".into(),
            }),
            Control::Date => date_from(v).map(FieldValue::Text),
            Control::Month => month_from(v).map(FieldValue::Text),
            Control::Select => {
                let up = v.to_ascii_uppercase();
                let mut options: Vec<&&str> = t.options.iter().collect();
                options.sort_by_key(|o| std::cmp::Reverse(o.len()));
                options
                    .into_iter()
                    .find(|o| up == o.to_ascii_uppercase() || up.contains(&o.to_ascii_uppercase()))
                    .map(|o| FieldValue::Text((*o).to_string()))
            }
            Control::Boolean => match v.to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Some(FieldValue::Boolean(true)),
                "false" | "no" | "0" | "off" => Some(FieldValue::Boolean(false)),
                _ => None,
            },
            Control::Totp => {
                self.totp(v);
                return self;
            }
            // An import never names another item's field; kept as text.
            Control::Passkey | Control::Linked => None,
        };
        match stored {
            Some(value) => self.push(PlainField::new(
                t.field_id,
                t.control.storage(),
                t.label,
                value,
            )),
            None => {
                let kind = if t.control.is_secret() {
                    FieldKind::Hidden
                } else {
                    FieldKind::Text
                };
                self.custom(fallback_label, kind, value);
            }
        }
        self
    }

    /// A web address, as `url.<n>`.
    pub fn url(&mut self, value: &str) -> &mut Self {
        let Some(v) = trimmed(value) else {
            return self;
        };
        if v.chars().any(char::is_whitespace) {
            return self.custom("Website", FieldKind::Text, v);
        }
        let exists = self.item.fields.iter().any(|f| {
            matches!(&f.value, FieldValue::Url { value, .. } if value == v || value == &format!("https://{v}"))
        });
        if exists {
            return self;
        }
        let n = self.next("url.");
        let value = if v.contains("://") {
            v.to_string()
        } else {
            format!("https://{v}")
        };
        let mut f = PlainField::url(&format!("url.{n}"), &value, "exact_host");
        f.label = "Website".into();
        self.push(f);
        self
    }

    /// A web address with a match rule from the source.
    pub fn url_with_rule(&mut self, value: &str, rule: &str) -> &mut Self {
        let before = self.item.fields.len();
        self.url(value);
        if self.item.fields.len() > before
            && let Some(FieldValue::Url { match_rule, .. }) =
                self.item.fields.last_mut().map(|f| &mut f.value)
        {
            *match_rule = rule.to_string();
        }
        self
    }

    /// A one-time code set-up: an `otpauth://` or `steam://` link, or a
    /// bare base32 secret (made into a link titled after the item). One
    /// that doesn't parse is reported, not kept half-read.
    pub fn totp(&mut self, value: &str) -> &mut Self {
        let Some(v) = trimmed(value) else {
            return self;
        };
        let uri = if v.starts_with("otpauth://") || v.starts_with("steam://") {
            v.to_string()
        } else {
            let label: String = self
                .item
                .title
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
                .collect();
            let secret: String = v.chars().filter(|c| !c.is_whitespace()).collect();
            format!(
                "otpauth://totp/{}?secret={secret}",
                label.trim().replace(' ', "%20")
            )
        };
        match lockdown_codes::parse_uri(&uri) {
            Ok(_) => {
                let n = self.next("code.");
                let label = if self.item.kind == ItemKind::Code {
                    "Set-up key"
                } else {
                    "One-time code"
                };
                self.push(PlainField::new(
                    format!("code.{n}"),
                    FieldKind::Totp,
                    label,
                    FieldValue::Text(uri),
                ));
            }
            Err(e) => {
                self.report(format!(
                    "a one-time code didn't read ({e}) and was left out"
                ));
            }
        }
        self
    }

    /// A field of the person's own, `custom.<n>`, of one of the kinds a
    /// person may add by hand (a code goes to [`Self::totp`]). Empty values
    /// are skipped.
    pub fn custom(&mut self, label: &str, kind: FieldKind, value: &str) -> &mut Self {
        let Some(v) = trimmed(value) else {
            return self;
        };
        let label = trimmed(label).unwrap_or("Field").replace(['\n', '\r'], " ");
        let value = match kind {
            FieldKind::Totp => return self.totp(v),
            FieldKind::Url => FieldValue::Url {
                value: v.to_string(),
                match_rule: "exact_host".into(),
            },
            FieldKind::Boolean => match v.to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => FieldValue::Boolean(true),
                "false" | "no" | "0" | "off" => FieldValue::Boolean(false),
                _ => FieldValue::Text(v.to_string()),
            },
            _ => FieldValue::Text(value.trim_end().to_string()),
        };
        let kind = match (&value, kind) {
            (FieldValue::Text(_), FieldKind::Boolean) => FieldKind::Text,
            (FieldValue::Text(t), FieldKind::Date) if date_from(t).is_none() => FieldKind::Text,
            (_, FieldKind::Hidden) => FieldKind::Hidden,
            (_, FieldKind::Url | FieldKind::Email | FieldKind::Phone | FieldKind::Date) => kind,
            (_, FieldKind::Boolean) => FieldKind::Boolean,
            _ => FieldKind::Text,
        };
        let value = match (kind, &value) {
            (FieldKind::Date, FieldValue::Text(t)) => {
                FieldValue::Text(date_from(t).unwrap_or_else(|| t.clone()))
            }
            _ => value,
        };
        let n = self.next("custom.");
        self.push(PlainField::new(format!("custom.{n}"), kind, label, value));
        self
    }

    /// A passkey: the same field ids CXF import writes.
    #[allow(clippy::too_many_arguments)]
    pub fn passkey(
        &mut self,
        rp_id: &str,
        user_name: &str,
        display_name: &str,
        credential_id: Vec<u8>,
        user_handle: Vec<u8>,
        key: Vec<u8>,
    ) -> &mut Self {
        let n = self.next_passkey();
        let p = |part: &str| format!("passkey.{n}.{part}");
        self.push(PlainField::text(&p("rp_id"), "relying party", rp_id));
        self.push(PlainField::text(&p("user_name"), "user name", user_name));
        self.push(PlainField::text(
            &p("user_display_name"),
            "display name",
            display_name,
        ));
        for (part, label, bytes) in [
            ("credential_id", "credential id", credential_id),
            ("user_handle", "user handle", user_handle),
            ("key", "private key", key),
        ] {
            self.push(PlainField::new(
                p(part),
                FieldKind::Passkey,
                label,
                FieldValue::Bytes(bytes),
            ));
        }
        self
    }

    fn next_passkey(&self) -> usize {
        self.item
            .fields
            .iter()
            .filter_map(|f| {
                f.field_id
                    .strip_prefix("passkey.")?
                    .split('.')
                    .next()?
                    .parse::<usize>()
                    .ok()
            })
            .max()
            .map_or(0, |n| n + 1)
    }

    /// Text for the item's notes (a Note's body), joined with blank lines.
    pub fn note(&mut self, text: &str) -> &mut Self {
        if let Some(t) = trimmed(text) {
            self.notes.push(t.to_string());
        }
        self
    }

    /// A file.
    pub fn attach(
        &mut self,
        name: &str,
        mime: Option<&str>,
        bytes: Zeroizing<Vec<u8>>,
    ) -> &mut Self {
        let name = trimmed(name).unwrap_or("attachment").to_string();
        let mime = mime
            .and_then(trimmed)
            .map(str::to_string)
            .unwrap_or_else(|| mime_for(&name).to_string());
        self.item
            .attachments
            .push(ImportedAttachment { name, mime, bytes });
        self
    }

    /// The source's folder path.
    pub fn folder(&mut self, path: &str) -> &mut Self {
        if let Some(p) = trimmed(path) {
            self.item.folder = Some(p.to_string());
        }
        self
    }

    /// Tags.
    pub fn tags<'a>(&mut self, tags: impl IntoIterator<Item = &'a str>) -> &mut Self {
        for t in tags.into_iter().filter_map(trimmed) {
            if !self.item.tags.iter().any(|x| x == t) {
                self.item.tags.push(t.to_string());
            }
        }
        self
    }

    /// Favourite in the source.
    pub fn favourite(&mut self, on: bool) -> &mut Self {
        self.item.favourite = on;
        self
    }

    /// Archived in the source.
    pub fn archived(&mut self, on: bool) -> &mut Self {
        self.item.archived = on;
        self
    }

    /// Created and modified, seconds since the epoch.
    pub fn times(&mut self, created: Option<u64>, modified: Option<u64>) -> &mut Self {
        self.item.created_at = created;
        self.item.modified_at = modified;
        self
    }

    /// An earlier version's fields, oldest first.
    pub fn history(&mut self, fields: Vec<PlainField>) -> &mut Self {
        self.item.history.push(fields);
        self
    }

    /// Something that couldn't be carried, in words.
    pub fn report(&mut self, why: impl Into<String>) -> &mut Self {
        self.item.report.push(why.into());
        self
    }

    /// The finished item: notes land in `notes` (a Note's `body`).
    pub fn finish(mut self) -> ImportedItem {
        if !self.notes.is_empty() {
            let id = if self.item.kind == ItemKind::Note {
                "body"
            } else {
                "notes"
            };
            let text = self.notes.join("\n\n");
            match self.item.fields.iter_mut().find(|f| f.field_id == id) {
                Some(f) => {
                    if let FieldValue::Text(t) = &mut f.value {
                        t.push_str("\n\n");
                        t.push_str(&text);
                    }
                }
                None => {
                    let label = template_for(self.item.kind)
                        .field(id)
                        .map_or("Notes", |t| t.label);
                    self.item.fields.push(PlainField::text(id, label, &text));
                }
            }
        }
        self.item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(item: &ImportedItem, id: &str) -> Option<FieldValue> {
        item.fields
            .iter()
            .find(|f| f.field_id == id)
            .map(|f| f.value.clone())
    }

    #[test]
    fn values_land_in_template_fields_or_your_own() {
        let mut b = Builder::new("x", ItemKind::Card, "  Visa ");
        b.field("card.number", "4111 1111 1111 1111")
            .field("card.expiry", "03/27")
            .field("card.holder", "")
            .field("card.number", "5500 0000 0000 0004")
            .field("card.issuer", "Example Bank")
            .note("first")
            .note("second");
        let item = b.finish();
        assert_eq!(item.title, "Visa");
        assert_eq!(
            value(&item, "card.expiry"),
            Some(FieldValue::Text("2027-03".into()))
        );
        assert!(value(&item, "card.holder").is_none(), "empty is skipped");
        let second = item
            .fields
            .iter()
            .find(|f| f.field_id == "custom.0")
            .unwrap();
        assert_eq!(
            (second.kind, second.label.as_str()),
            (FieldKind::Hidden, "Card number")
        );
        let issuer = item
            .fields
            .iter()
            .find(|f| f.field_id == "custom.1")
            .unwrap();
        assert_eq!(issuer.label, "card.issuer");
        assert_eq!(
            value(&item, "notes"),
            Some(FieldValue::Text("first\n\nsecond".into()))
        );
    }

    #[test]
    fn codes_choices_dates_and_urls_are_checked() {
        let mut b = Builder::new("x", ItemKind::WiFi, "");
        b.field("wifi.ssid", "home")
            .field("wifi.security", "WPA2 Personal")
            .field("wifi.hidden", "yes")
            .totp("JBSWY3DPEHPK3PXP")
            .totp("otpauth://totp/x?secret=!!!")
            .url("example.com")
            .url("https://example.com")
            .custom("Bought", FieldKind::Date, "12/31/2024");
        let item = b.finish();
        assert_eq!(item.title, "Imported wi-fi network");
        assert_eq!(
            value(&item, "wifi.security"),
            Some(FieldValue::Text("WPA2".into()))
        );
        assert_eq!(value(&item, "wifi.hidden"), Some(FieldValue::Boolean(true)));
        assert!(
            matches!(&value(&item, "code.0"), Some(FieldValue::Text(t)) if t.starts_with("otpauth://totp/"))
        );
        assert!(value(&item, "code.1").is_none());
        assert_eq!(item.report.len(), 1, "{:?}", item.report);
        assert!(value(&item, "url.1").is_none(), "the same site once");
        assert_eq!(
            value(&item, "custom.0"),
            Some(FieldValue::Text("2024-12-31".into()))
        );
    }

    #[test]
    fn date_shapes() {
        assert_eq!(date_from_unix(0), "1970-01-01");
        assert_eq!(date_from_unix(1_709_251_200), "2024-03-01");
        assert_eq!(month_from("2030-01"), Some("2030-01".into()));
        assert_eq!(month_from("012030"), Some("2030-01".into()));
        assert_eq!(month_from("January,2025"), Some("2025-01".into()));
        assert_eq!(month_from("13/2025"), None);
        assert_eq!(date_from("2024-02-29"), Some("2024-02-29".into()));
        assert_eq!(date_from("nonsense"), None);
    }
}
