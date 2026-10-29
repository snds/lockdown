//! A confined home: the app moved, without a restart, to another data
//! directory and identity root, where nothing reaches the network or
//! leaves the device. The developer demo runs here
//! (docs/plan/40-devtools.md §3); nothing else uses it today.
//!
//! - [`App::rehome`] locks the vault open now (its keys are dropped as on
//!   any lock), then starts the session over at the new paths, locked, as
//!   at launch. The shell's platform check (Touch ID, Windows Hello) moves
//!   with it. It returns the paths it left, so the caller can go back.
//! - While confined, every request through `lockdown_sync` and
//!   `lockdown_security` is held for the whole process (sync, sign-in,
//!   sharing, breach ranges, change-password pages, alias services, the
//!   live channel), and exports and backups are refused here, before any
//!   gate is asked.
//! - Going back with `confined: None` releases the network.

use crate::session::Paths;
use crate::{App, AppError, AppResult, Core, LockReason};

impl App {
    /// Moves the app to `paths`, locked. With `confined`, the new home
    /// never reaches the network and refuses exports and backups, each
    /// with that sentence; with `None`, everything is as at launch.
    /// Returns the paths the app left.
    pub fn rehome(&self, paths: Paths, confined: Option<String>) -> AppResult<Paths> {
        #[cfg(not(target_os = "wasi"))]
        let mirror = self.confined.clone();
        self.call(move |core| {
            core.lock(LockReason::Requested);
            let mut next = Core::new(paths, core.persona.clone(), core.policy);
            next.approver = core.approver.take();
            next.confined = confined.clone();
            let left = std::mem::replace(core, next);
            #[cfg(not(target_os = "wasi"))]
            match mirror.write() {
                Ok(mut m) => *m = confined.clone(),
                Err(poisoned) => *poisoned.into_inner() = confined.clone(),
            }
            lockdown_sync::hold_network(confined.clone());
            lockdown_security::http::hold_network(confined);
            Ok(left.paths)
        })
    }

    /// The sentence a confined home refuses with, while the app is in one.
    /// Answered without waiting for the worker (a long call there never
    /// holds it up).
    pub fn confinement(&self) -> AppResult<Option<String>> {
        #[cfg(not(target_os = "wasi"))]
        {
            Ok(match self.confined.read() {
                Ok(c) => c.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            })
        }
        #[cfg(target_os = "wasi")]
        self.call(|core| Ok(core.confined.clone()))
    }
}

impl Core {
    /// Refuses in a confined home, in its own words.
    pub(crate) fn refuse_confined(&self) -> AppResult<()> {
        match &self.confined {
            Some(reason) => Err(AppError::refused(sentence(reason))),
            None => Ok(()),
        }
    }
}

/// `reason` as a sentence on its own: a capital first letter and a stop.
fn sentence(reason: &str) -> String {
    let mut chars = reason.trim().chars();
    let mut out: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => return String::new(),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn the_reason_reads_as_a_sentence() {
        assert_eq!(
            sentence("the demo never goes online"),
            "The demo never goes online."
        );
        assert_eq!(sentence("Done."), "Done.");
        assert_eq!(sentence("  "), "");
    }
}
