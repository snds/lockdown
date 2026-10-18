//! Clock-skew correction for time-based codes (02-model §4 "Code",
//! 03-architecture §2 `codes` row).
//!
//! A TOTP code is only as right as the clock it is worked out from. A
//! laptop that drifted forty seconds shows codes the site already
//! refuses. Lockdown measures the device's clock against its own server's
//! `Date` header and, within bounds, works codes out from the corrected
//! time.
//!
//! The server is not trusted with this (04-security §1: it may be
//! hostile), so a sample is **checked** and the correction is
//! **bounded**:
//!
//! - A sample whose round trip took longer than [`MAX_ROUND_TRIP`] says
//!   too little about when the server wrote its header; it is refused.
//! - The header must be an HTTP date (RFC 9110 §5.6.7, IMF-fixdate or one
//!   of the two obsolete forms) between 2020 and 2100.
//! - An offset under [`IGNORE_BELOW`] is noise (the header has one-second
//!   resolution) and changes nothing.
//! - An offset over [`MAX_CORRECTION`] is not applied: a clock that far
//!   off is the person's to fix, and a server must not be able to move
//!   codes by minutes. The shell says so and offers the clock settings.
//!
//! Nothing here reads the system clock; callers pass the times in, so the
//! rules are tested without waiting.

use serde::Serialize;

/// The longest round trip a sample may take, in milliseconds.
pub const MAX_ROUND_TRIP_MS: u64 = 4_000;
/// [`MAX_ROUND_TRIP_MS`] as seconds, for words.
pub const MAX_ROUND_TRIP: u64 = MAX_ROUND_TRIP_MS / 1000;
/// Offsets smaller than this (in seconds) are left alone.
pub const IGNORE_BELOW: i64 = 3;
/// Offsets larger than this (in seconds) are reported, never applied.
pub const MAX_CORRECTION: i64 = 300;

/// The earliest date a server may claim (2020-01-01).
const EARLIEST: i64 = 1_577_836_800;
/// The latest (2100-01-01).
const LATEST: i64 = 4_102_444_800;

/// Why a sample was not used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SkewError {
    /// The answer had no `Date` header.
    #[error("the server didn't say what time it is")]
    NoDate,
    /// The header didn't read as an HTTP date.
    #[error("the server's time didn't read as a date")]
    BadDate,
    /// The date is outside any plausible range.
    #[error("the server's time is implausible, so it was ignored")]
    Implausible,
    /// The round trip was too slow to say when the header was written.
    #[error(
        "the server took over {MAX_ROUND_TRIP} seconds to answer, so its time can't be trusted"
    )]
    TooSlow,
    /// The local clock went backwards during the sample.
    #[error("this computer's clock changed during the check")]
    ClockMoved,
}

/// What a sample says about this device's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkewVerdict {
    /// Within [`IGNORE_BELOW`]: nothing to do.
    InStep,
    /// Corrected: codes are worked out from the server's time.
    Corrected,
    /// Too far off to correct: the person should fix the clock.
    TooFar,
}

/// One checked sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Skew {
    /// Seconds to add to this device's clock to get the server's
    /// (positive: this clock is behind).
    pub offset_secs: i64,
    /// The round trip the sample took, in milliseconds.
    pub round_trip_ms: u64,
    /// What the offset means.
    pub verdict: SkewVerdict,
}

impl Skew {
    /// The seconds to add to the local clock when working out a code:
    /// the offset when it was corrected, else nothing.
    pub fn correction(&self) -> i64 {
        match self.verdict {
            SkewVerdict::Corrected => self.offset_secs,
            SkewVerdict::InStep | SkewVerdict::TooFar => 0,
        }
    }

    /// `now` (seconds since the epoch) with the correction applied,
    /// never below zero.
    pub fn apply(&self, now: u64) -> u64 {
        let corrected = now as i64 + self.correction();
        corrected.max(0) as u64
    }

    /// The offset in plain words, for a shell to show as is.
    pub fn words(&self) -> String {
        let n = self.offset_secs.unsigned_abs();
        let unit = |n: u64| {
            if n >= 120 {
                format!("{} minutes", n / 60)
            } else if n == 1 {
                "1 second".to_string()
            } else {
                format!("{n} seconds")
            }
        };
        let way = if self.offset_secs > 0 {
            "behind"
        } else {
            "ahead of"
        };
        match self.verdict {
            SkewVerdict::InStep => "This computer's clock matches your server's.".into(),
            SkewVerdict::Corrected => format!(
                "This computer's clock is {} {way} your server's, so codes are worked out from the server's time.",
                unit(n)
            ),
            SkewVerdict::TooFar => format!(
                "This computer's clock is {} {way} your server's. That's too far to correct safely; set the clock right and codes will work again.",
                unit(n)
            ),
        }
    }
}

/// Checks one sample: the local time in milliseconds when the request
/// left (`sent_ms`) and when the answer arrived (`received_ms`), and the
/// answer's `Date` header.
///
/// The server wrote its header somewhere between the two; its one-second
/// resolution means the moment is taken as half a second after the
/// stamped second, compared with the middle of the round trip.
pub fn measure(
    date_header: Option<&str>,
    sent_ms: u64,
    received_ms: u64,
) -> Result<Skew, SkewError> {
    let header = date_header.ok_or(SkewError::NoDate)?;
    if received_ms < sent_ms {
        return Err(SkewError::ClockMoved);
    }
    let round_trip_ms = received_ms - sent_ms;
    if round_trip_ms > MAX_ROUND_TRIP_MS {
        return Err(SkewError::TooSlow);
    }
    let server = parse_http_date(header).ok_or(SkewError::BadDate)?;
    if !(EARLIEST..LATEST).contains(&server) {
        return Err(SkewError::Implausible);
    }
    let server_ms = server * 1000 + 500;
    let local_ms = (sent_ms + round_trip_ms / 2) as i64;
    let diff_ms = server_ms - local_ms;
    // Round to the nearest second, away from zero at the half.
    let offset_secs = (diff_ms + diff_ms.signum() * 500) / 1000;
    let verdict = if offset_secs.abs() < IGNORE_BELOW {
        SkewVerdict::InStep
    } else if offset_secs.abs() <= MAX_CORRECTION {
        SkewVerdict::Corrected
    } else {
        SkewVerdict::TooFar
    };
    Ok(Skew {
        offset_secs,
        round_trip_ms,
        verdict,
    })
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|m| *m == lower)
        .map(|i| i as u32 + 1)
}

/// Days from 1970-01-01 to the given civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

fn hms(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.split(':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let s: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    Some((h, m, s.min(59)))
}

fn to_unix(y: i64, mo: u32, d: u32, (h, mi, s): (u32, u32, u32)) -> Option<i64> {
    if !(1..=12).contains(&mo) || d == 0 || d > days_in_month(y, mo) {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + (h * 3600 + mi * 60 + s) as i64)
}

/// Seconds since the epoch for an HTTP date: IMF-fixdate
/// (`Sun, 06 Nov 1994 08:49:37 GMT`), the obsolete RFC 850 form
/// (`Sunday, 06-Nov-94 08:49:37 GMT`) or asctime
/// (`Sun Nov  6 08:49:37 1994`). Anything else is `None`.
pub fn parse_http_date(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.len() > 64 || !text.is_ascii() {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.as_slice() {
        // IMF-fixdate: day-name "," SP day SP month SP year SP time SP "GMT"
        [day_name, day, mon, year, time, "GMT"] if day_name.ends_with(',') => {
            let d: u32 = day.parse().ok()?;
            let y: i64 = year.parse().ok()?;
            if year.len() != 4 {
                return None;
            }
            to_unix(y, month(mon)?, d, hms(time)?)
        }
        // RFC 850: day-name "," SP dd-mon-yy SP time SP "GMT"
        [day_name, date, time, "GMT"] if day_name.ends_with(',') => {
            let mut p = date.split('-');
            let d: u32 = p.next()?.parse().ok()?;
            let mo = month(p.next()?)?;
            let yy = p.next()?;
            if p.next().is_some() || yy.len() != 2 {
                return None;
            }
            let yy: i64 = yy.parse().ok()?;
            // RFC 9110: a two-digit year more than 50 years ahead is in the past.
            let y = if yy < 70 { 2000 + yy } else { 1900 + yy };
            to_unix(y, mo, d, hms(time)?)
        }
        // asctime: day-name SP month SP day SP time SP year
        [_, mon, day, time, year] => {
            let d: u32 = day.parse().ok()?;
            let y: i64 = year.parse().ok()?;
            to_unix(y, month(mon)?, d, hms(time)?)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: i64 = 1_760_786_400; // 2025-10-18 11:20:00 GMT

    #[test]
    fn http_dates_read_in_all_three_forms_and_nothing_else() {
        assert_eq!(
            parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(
            parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(
            parse_http_date("Sun Nov  6 08:49:37 1994"),
            Some(784_111_777)
        );
        assert_eq!(parse_http_date("Sat, 18 Oct 2025 11:20:00 GMT"), Some(AT));
        assert_eq!(
            parse_http_date("Thu, 29 Feb 2024 00:00:00 GMT"),
            Some(1_709_164_800)
        );
        for bad in [
            "",
            "yesterday",
            "Sat, 18 Oct 2025 11:20:00 PST",
            "Sat, 31 Feb 2025 11:20:00 GMT",
            "Fri, 29 Feb 2025 00:00:00 GMT",
            "Sat, 18 Foo 2025 11:20:00 GMT",
            "Sat, 18 Oct 25 11:20:00 GMT",
            "Sat, 18 Oct 2025 25:20:00 GMT",
            "Sat, 18 Oct 2025 11:20 GMT",
            "Sat, 18 Oct 2025 11:20:00:00 GMT",
            "Sat, 18 Oct 2025 11:20:00 GMT and then some text that makes it far too long",
        ] {
            assert_eq!(parse_http_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_sample_is_checked_and_its_correction_bounded() {
        let date = "Sat, 18 Oct 2025 11:20:00 GMT";
        let server_ms = AT as u64 * 1000;
        // In step: the local clock reads the server's second.
        let s = measure(Some(date), server_ms + 200, server_ms + 600).unwrap();
        assert_eq!(s.verdict, SkewVerdict::InStep);
        assert_eq!(s.correction(), 0);
        assert_eq!(s.round_trip_ms, 400);
        // Forty seconds behind: corrected forward.
        let s = measure(Some(date), server_ms - 40_000, server_ms - 39_000).unwrap();
        assert_eq!(s.offset_secs, 40);
        assert_eq!(s.verdict, SkewVerdict::Corrected);
        assert_eq!(s.apply(1_000), 1_040);
        assert!(s.words().contains("40 seconds behind"), "{}", s.words());
        // Ninety seconds ahead: corrected back.
        let s = measure(Some(date), server_ms + 90_000, server_ms + 91_000).unwrap();
        assert_eq!(s.offset_secs, -90);
        assert_eq!(s.apply(1_000), 910);
        assert!(s.words().contains("ahead of"));
        // An hour off: reported, never applied.
        let s = measure(Some(date), server_ms - 3_600_000, server_ms - 3_599_500).unwrap();
        assert_eq!(s.verdict, SkewVerdict::TooFar);
        assert_eq!(s.correction(), 0);
        assert_eq!(s.apply(1_000), 1_000);
        assert!(s.words().contains("60 minutes behind"), "{}", s.words());
        assert!(s.words().contains("set the clock right"));
        // The edges of the bound.
        let s = measure(Some(date), server_ms - 300_000, server_ms - 299_000).unwrap();
        assert_eq!((s.offset_secs, s.verdict), (300, SkewVerdict::Corrected));
        let s = measure(Some(date), server_ms - 302_000, server_ms - 301_000).unwrap();
        assert_eq!(s.verdict, SkewVerdict::TooFar);
    }

    #[test]
    fn a_sample_that_says_too_little_is_refused() {
        let date = "Sat, 18 Oct 2025 11:20:00 GMT";
        let t = AT as u64 * 1000;
        assert_eq!(measure(None, t, t + 10), Err(SkewError::NoDate));
        assert_eq!(measure(Some("soon"), t, t + 10), Err(SkewError::BadDate));
        assert_eq!(measure(Some(date), t, t + 4_001), Err(SkewError::TooSlow));
        assert_eq!(measure(Some(date), t + 10, t), Err(SkewError::ClockMoved));
        assert_eq!(
            measure(Some("Mon, 01 Jan 1990 00:00:00 GMT"), t, t + 10),
            Err(SkewError::Implausible)
        );
        assert_eq!(
            measure(Some("Fri, 01 Jan 2200 00:00:00 GMT"), t, t + 10),
            Err(SkewError::Implausible)
        );
    }

    #[test]
    fn a_corrected_clock_gives_the_servers_code() {
        let code = crate::parse_uri(
            "otpauth://totp/Example:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example",
        )
        .unwrap();
        let date = "Sat, 18 Oct 2025 11:20:00 GMT";
        let server_ms = AT as u64 * 1000;
        let local_now = (AT - 45) as u64;
        let s = measure(Some(date), server_ms - 45_000, server_ms - 44_000).unwrap();
        assert_eq!(
            code.generate_at(s.apply(local_now)).unwrap(),
            code.generate_at(AT as u64).unwrap()
        );
        assert_ne!(
            code.generate_at(local_now).unwrap(),
            code.generate_at(AT as u64).unwrap()
        );
    }
}
