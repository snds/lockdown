//! Code clock-skew correction (docs/plan/23-bridges.md §8): this device's
//! clock against the account's own server, checked and bounded by
//! `lockdown_codes::skew`, and applied to every code the core works out
//! while the setting is on (on by default).
//!
//! The sample is one `GET /healthz` timed on this device, with the
//! answer's `Date` header. It runs off the worker, like every network
//! call, and only the checked verdict comes back to it. No account, no
//! server to ask: codes use the device's clock and the view says so.

use lockdown_codes::skew::{self, Skew, SkewVerdict};
use serde::Serialize;

use crate::{App, AppError, AppResult, session};

/// What a shell shows about the clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClockView {
    /// When it was checked (seconds since the epoch).
    pub checked_at: i64,
    /// Seconds to add to this device's clock to reach the server's.
    pub offset_secs: i64,
    /// `in_step`, `corrected` or `too_far`.
    pub verdict: SkewVerdict,
    /// Whether codes are being corrected now (the verdict and the setting).
    pub correcting: bool,
    /// What it means, in words.
    pub words: String,
}

fn view(at: i64, s: &Skew, enabled: bool) -> ClockView {
    ClockView {
        checked_at: at,
        offset_secs: s.offset_secs,
        verdict: s.verdict,
        correcting: enabled && s.verdict == SkewVerdict::Corrected,
        words: s.describe(enabled),
    }
}

impl App {
    /// The last clock check, when there was one this run.
    pub fn clock_status(&self) -> AppResult<Option<ClockView>> {
        self.call(|core| {
            let enabled = core.settings.code_clock_correction;
            Ok(core
                .bridges
                .clock
                .as_ref()
                .map(|(at, s)| view(*at, s, enabled)))
        })
    }

    /// Compares this device's clock with the account's server now.
    pub fn clock_check(&self) -> AppResult<ClockView> {
        let server = self.call(|core| {
            core.session()?
                .account_session()
                .map(|r| r.map(|(server, _)| server))
                .transpose()
                .map_err(AppError::refused)
        })?;
        let server = server.ok_or_else(|| {
            AppError::refused(
                "there's no account on this device, so there's no server to compare the clock with",
            )
        })?;
        let sample = check_against(&server).map_err(AppError::refused)?;
        let now = session::now_secs();
        self.call(move |core| {
            core.bridges.clock = Some((now, sample));
            Ok(view(now, &sample, core.settings.code_clock_correction))
        })
    }
}

/// One checked sample from `server`.
#[cfg(not(target_os = "wasi"))]
pub(crate) fn check_against(server: &str) -> Result<Skew, String> {
    let transport = lockdown_sync::HttpTransport::new(server).map_err(|e| e.to_string())?;
    let s = transport.clock_sample().map_err(|e| e.to_string())?;
    skew::measure(s.date.as_deref(), s.sent_ms, s.received_ms).map_err(|e| e.to_string())
}

/// In the web vault the browser's own clock is the only one the page may
/// read (the worker sees no response headers).
#[cfg(target_os = "wasi")]
pub(crate) fn check_against(_server: &str) -> Result<Skew, String> {
    let _ = skew::MAX_CORRECTION;
    Err("the web vault uses this browser's clock for codes".into())
}

#[cfg(all(test, not(target_os = "wasi")))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    /// A server whose `Date` header is `ahead` seconds off this clock.
    fn skewed(ahead: i64) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(1) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut l = String::new();
                    if reader.read_line(&mut l).is_err() || l.trim().is_empty() {
                        break;
                    }
                }
                let t = session::now_secs() + ahead;
                let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
                // Civil date from days since 1970 (the inverse of the parser's).
                let z = days + 719_468;
                let era = z.div_euclid(146_097);
                let doe = z - era * 146_097;
                let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let d = doy - (153 * mp + 2) / 5 + 1;
                let m = if mp < 10 { mp + 3 } else { mp - 9 };
                let y = yoe + era * 400 + i64::from(m <= 2);
                const MONTHS: [&str; 12] = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec",
                ];
                let date = format!(
                    "Mon, {d:02} {} {y} {:02}:{:02}:{:02} GMT",
                    MONTHS[(m - 1) as usize],
                    secs / 3600,
                    secs / 60 % 60,
                    secs % 60
                );
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nDate: {date}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                );
            }
        });
        url
    }

    #[test]
    fn a_server_a_minute_ahead_corrects_codes_and_an_hour_off_does_not() {
        let s = check_against(&skewed(60)).unwrap();
        assert_eq!(s.verdict, SkewVerdict::Corrected);
        assert!((58..=62).contains(&s.offset_secs), "{s:?}");
        let mut state = crate::bridges::BridgesState::default();
        let now = session::now_secs();
        state.clock = Some((now, s));
        assert_eq!(state.code_time(now, true), (now + s.offset_secs) as u64);
        assert_eq!(
            state.code_time(now, false),
            now as u64,
            "the setting off, no correction"
        );
        assert!(!view(now, &s, false).correcting);
        let far = check_against(&skewed(3600)).unwrap();
        assert_eq!(far.verdict, SkewVerdict::TooFar);
        state.clock = Some((now, far));
        assert_eq!(state.code_time(now, true), now as u64);
    }
}
