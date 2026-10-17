//! The smart views of the sidebar (brief §3), decided here from an item's
//! kind, fields and state so every shell shows the same list for the same
//! vault. Nothing hidden is read: the facts come from the masked summary.
//!
//! - **Passkeys**: a Passkey item, or a Login with a passkey in it.
//! - **Codes**: a Code item, or a Login with a code in it.
//! - **Wi-Fi**: Wi-Fi items.
//! - **Expiring**: an `expires_at` date within the next thirty days, or
//!   already past.
//! - **Needs attention**: conflict copies (D58), expiring items, and codes
//!   whose issuer is the sync login (02-model §4).
//! - **Deleted**: trashed items, in their grace period. A trashed item is
//!   in no other view.

use serde::Serialize;

use crate::types::ItemSummary;

/// Days before an expiry date that an item counts as expiring.
pub const EXPIRING_DAYS: i64 = 30;

/// One sidebar view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartView {
    Passkeys,
    Codes,
    WiFi,
    Expiring,
    Attention,
    Deleted,
}

impl SmartView {
    /// The name a shell keys on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passkeys => "passkeys",
            Self::Codes => "codes",
            Self::WiFi => "wi-fi",
            Self::Expiring => "expiring",
            Self::Attention => "attention",
            Self::Deleted => "deleted",
        }
    }
}

/// The marker a conflict copy's title carries (`lockdown_sync::engine`).
pub const CONFLICT_MARK: &str = "(conflict copy from ";

/// Every view the item belongs to, given `now` (seconds since the epoch).
pub fn classify(item: &ItemSummary, now: i64) -> Vec<SmartView> {
    if item.state == "trashed" {
        return vec![SmartView::Deleted];
    }
    let mut views = Vec::new();
    if item.kind == "passkey" || item.passkeys > 0 {
        views.push(SmartView::Passkeys);
    }
    if item.kind == "code" || item.codes > 0 {
        views.push(SmartView::Codes);
    }
    if item.kind == "wi-fi" {
        views.push(SmartView::WiFi);
    }
    let expiring = item
        .expires_at
        .as_deref()
        .and_then(parse_date)
        .is_some_and(|expiry| expiry - now <= EXPIRING_DAYS * 86_400);
    if expiring {
        views.push(SmartView::Expiring);
    }
    if expiring || item.conflict_copy || !item.warnings.is_empty() {
        views.push(SmartView::Attention);
    }
    views
}

/// Whether a title marks a conflict copy (D58).
pub fn is_conflict_copy(title: &str) -> bool {
    title.contains(CONFLICT_MARK)
}

/// `YYYY-MM-DD`, `YYYY-MM` (the month's last day) or `YYYY` (the year's
/// last day) as seconds since the epoch at the end of that day, UTC.
/// Anything else is not a date this view can read.
pub fn parse_date(text: &str) -> Option<i64> {
    let text = text.trim();
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    if !(1970..=9999).contains(&year) {
        return None;
    }
    let month: u32 = match parts.next() {
        Some(m) => m.parse().ok()?,
        None => 12,
    };
    if !(1..=12).contains(&month) {
        return None;
    }
    let day: u32 = match parts.next() {
        Some(d) => d.parse().ok()?,
        None => days_in_month(year, month),
    };
    if parts.next().is_some() || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + 86_399)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str) -> ItemSummary {
        ItemSummary {
            id: "x".into(),
            kind: kind.into(),
            state: "live".into(),
            title: "T".into(),
            site: String::new(),
            username: String::new(),
            edited_at: 0,
            crown_jewel: false,
            methods: Vec::new(),
            passkeys: 0,
            codes: 0,
            expires_at: None,
            conflict_copy: false,
            warnings: Vec::new(),
            views: Vec::new(),
            subtitle: String::new(),
            copy: None,
        }
    }

    /// 2026-10-17T00:00:00Z.
    const NOW: i64 = 1_792_195_200;

    #[test]
    fn dates_parse_to_the_end_of_the_day_month_or_year() {
        assert_eq!(parse_date("1970-01-01"), Some(86_399));
        assert_eq!(parse_date("2026-10-17"), Some(NOW + 86_399));
        assert_eq!(parse_date("2026-02"), parse_date("2026-02-28"));
        assert_eq!(parse_date("2028-02"), parse_date("2028-02-29"));
        assert_eq!(parse_date("2027"), parse_date("2027-12-31"));
        assert_eq!(parse_date("2026-13"), None);
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("soon"), None);
        assert_eq!(parse_date(""), None);
    }

    #[test]
    fn kinds_and_embedded_methods_pick_the_views() {
        assert_eq!(classify(&item("passkey"), NOW), [SmartView::Passkeys]);
        assert_eq!(classify(&item("code"), NOW), [SmartView::Codes]);
        assert_eq!(classify(&item("wi-fi"), NOW), [SmartView::WiFi]);
        assert!(classify(&item("note"), NOW).is_empty());
        let mut login = item("login");
        login.passkeys = 1;
        login.codes = 2;
        assert_eq!(
            classify(&login, NOW),
            [SmartView::Passkeys, SmartView::Codes],
            "a Login keeps its passkeys and codes inside it and shows in both views"
        );
    }

    #[test]
    fn expiring_and_attention() {
        let mut doc = item("document");
        doc.expires_at = Some("2026-11-10".into());
        assert_eq!(
            classify(&doc, NOW),
            [SmartView::Expiring, SmartView::Attention]
        );
        doc.expires_at = Some("2026-12-24".into());
        assert!(classify(&doc, NOW).is_empty(), "more than thirty days away");
        doc.expires_at = Some("2020-01-01".into());
        assert_eq!(
            classify(&doc, NOW),
            [SmartView::Expiring, SmartView::Attention],
            "already past still needs the person"
        );
        doc.expires_at = Some("whenever".into());
        assert!(classify(&doc, NOW).is_empty());

        let mut copy = item("login");
        copy.title = "Bank (conflict copy from laptop, 2026-10-17)".into();
        copy.conflict_copy = is_conflict_copy(&copy.title);
        assert!(copy.conflict_copy);
        assert_eq!(classify(&copy, NOW), [SmartView::Attention]);

        let mut code = item("code");
        code.warnings = vec!["issuer-equals-login".into()];
        assert_eq!(
            classify(&code, NOW),
            [SmartView::Codes, SmartView::Attention]
        );
    }

    #[test]
    fn a_trashed_item_is_only_in_deleted() {
        let mut gone = item("login");
        gone.state = "trashed".into();
        gone.codes = 3;
        gone.expires_at = Some("2020-01-01".into());
        assert_eq!(classify(&gone, NOW), [SmartView::Deleted]);
    }
}
