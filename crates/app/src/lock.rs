//! The lock timer (03-architecture §8, "lock on sleep and idle"): the
//! policy and the clock that decide when the open vault's keys are dropped.
//! Shells report activity and visibility; they never decide. The worker
//! thread in `lib.rs` runs [`LockClock::due`] on every call and once a
//! second between calls.

use std::time::{Duration, Instant, SystemTime};

/// When to lock. `None` on a timer means "never for that reason".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockPolicy {
    /// No call from a shell for this long.
    pub idle: Option<Duration>,
    /// The window has been hidden (minimised, another space, screen off)
    /// for this long.
    pub hidden: Option<Duration>,
    /// The wall clock jumped further than this between two ticks: the
    /// machine slept. Locks at once when it wakes.
    pub sleep_gap: Option<Duration>,
}

impl Default for LockPolicy {
    /// Proposed defaults (decision row in docs/plan/10-desktop-shell.md;
    /// 11-desktop-design-brief §4): ten minutes idle, five minutes with
    /// the window out of view, a thirty-second sleep gap.
    fn default() -> Self {
        Self {
            idle: Some(Duration::from_secs(10 * 60)),
            hidden: Some(Duration::from_secs(5 * 60)),
            sleep_gap: Some(Duration::from_secs(30)),
        }
    }
}

/// Why the vault locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LockReason {
    /// A shell asked.
    Requested,
    /// Nothing happened for the idle timeout.
    Idle,
    /// The window stayed hidden past its timeout.
    Hidden,
    /// The machine slept.
    Sleep,
}

/// What the timer has seen.
#[derive(Clone, Debug)]
pub struct LockClock {
    last_activity: Instant,
    hidden_since: Option<Instant>,
    last_tick: SystemTime,
}

impl LockClock {
    /// A clock that has just seen activity.
    pub fn new(now: Instant, wall: SystemTime) -> Self {
        Self {
            last_activity: now,
            hidden_since: None,
            last_tick: wall,
        }
    }

    /// A shell call, a keystroke, a click.
    pub fn touch(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// The window went out of view (`true`) or came back.
    pub fn set_hidden(&mut self, hidden: bool, now: Instant) {
        self.hidden_since = if hidden {
            Some(self.hidden_since.unwrap_or(now))
        } else {
            None
        };
        if !hidden {
            self.last_activity = now;
        }
    }

    /// How long since the last activity.
    pub fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_activity)
    }

    /// Checks the timers and advances the sleep detector. Returns the
    /// reason to lock, if any.
    pub fn due(
        &mut self,
        policy: &LockPolicy,
        now: Instant,
        wall: SystemTime,
    ) -> Option<LockReason> {
        let jumped = wall
            .duration_since(self.last_tick)
            .unwrap_or(Duration::ZERO);
        let since_tick = now.saturating_duration_since(self.last_activity.min(now));
        self.last_tick = wall;
        if let Some(gap) = policy.sleep_gap
            && jumped > gap
            && jumped > since_tick + gap
        {
            return Some(LockReason::Sleep);
        }
        if let Some(limit) = policy.idle
            && self.idle_for(now) >= limit
        {
            return Some(LockReason::Idle);
        }
        if let (Some(limit), Some(since)) = (policy.hidden, self.hidden_since)
            && now.saturating_duration_since(since) >= limit
        {
            return Some(LockReason::Hidden);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn idle_locks_at_the_timeout_and_touch_resets_it() {
        let policy = LockPolicy {
            idle: Some(secs(300)),
            hidden: None,
            sleep_gap: None,
        };
        let t0 = Instant::now();
        let w0 = SystemTime::now();
        let mut clock = LockClock::new(t0, w0);
        assert_eq!(clock.due(&policy, t0 + secs(299), w0 + secs(299)), None);
        clock.touch(t0 + secs(299));
        assert_eq!(clock.due(&policy, t0 + secs(598), w0 + secs(598)), None);
        assert_eq!(
            clock.due(&policy, t0 + secs(599), w0 + secs(599)),
            Some(LockReason::Idle)
        );
    }

    #[test]
    fn hidden_locks_after_its_own_timeout_and_showing_again_clears_it() {
        let policy = LockPolicy {
            idle: None,
            hidden: Some(secs(60)),
            sleep_gap: None,
        };
        let t0 = Instant::now();
        let w0 = SystemTime::now();
        let mut clock = LockClock::new(t0, w0);
        clock.set_hidden(true, t0 + secs(10));
        assert_eq!(clock.due(&policy, t0 + secs(69), w0 + secs(69)), None);
        clock.set_hidden(false, t0 + secs(69));
        assert_eq!(clock.due(&policy, t0 + secs(200), w0 + secs(200)), None);
        clock.set_hidden(true, t0 + secs(200));
        assert_eq!(
            clock.due(&policy, t0 + secs(260), w0 + secs(260)),
            Some(LockReason::Hidden)
        );
    }

    #[test]
    fn a_wall_clock_jump_means_the_machine_slept() {
        let policy = LockPolicy {
            idle: Some(secs(3600)),
            hidden: None,
            sleep_gap: Some(secs(30)),
        };
        let t0 = Instant::now();
        let w0 = SystemTime::now();
        let mut clock = LockClock::new(t0, w0);
        // Ticks a second apart: no jump.
        assert_eq!(clock.due(&policy, t0 + secs(1), w0 + secs(1)), None);
        // The monotonic clock saw two seconds; the wall clock saw an hour.
        assert_eq!(
            clock.due(&policy, t0 + secs(3), w0 + secs(3601)),
            Some(LockReason::Sleep)
        );
    }

    #[test]
    fn never_means_never() {
        let policy = LockPolicy {
            idle: None,
            hidden: None,
            sleep_gap: None,
        };
        let t0 = Instant::now();
        let w0 = SystemTime::now();
        let mut clock = LockClock::new(t0, w0);
        clock.set_hidden(true, t0);
        assert_eq!(
            clock.due(&policy, t0 + secs(86_400), w0 + secs(86_400 * 7)),
            None
        );
    }
}
