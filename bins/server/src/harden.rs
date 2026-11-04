//! What the server does to its own process at start (docs/plan/53-harden-rest.md).
//!
//! The server holds its token key, its log keys and the OPAQUE setup for
//! its whole life. Their pages are locked out of swap
//! (`lockdown_crypto::locked`), and core dumps are turned off before any
//! key is read, so a crash never writes them to disk:
//!
//! - **Linux and macOS:** `RLIMIT_CORE` 0, soft and hard, through
//!   `lockdown_crypto::locked::harden_process`, the call the apps make
//!   (D314). Never `PR_SET_DUMPABLE`, the same as the apps.
//! - **Windows:** a process writes no core file. A crash goes to Windows
//!   Error Reporting, which writes a dump only where an administrator set
//!   up `LocalDumps`, so nothing is called.
//!
//! A refused call is logged once and the server starts anyway. An operator
//! who needs a crash dump to debug starts the server with
//! `--core-dumps on`, which leaves the limit as the server found it and
//! says so once (docs/self-host.md).

use lockdown_crypto::locked::MemoryReport;

/// What happened to core dumps at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreDumps {
    /// Turned off: the core limit is 0.
    Off,
    /// The system refused to lower the limit.
    Refused,
    /// This system writes no core file (Windows).
    NoCoreFiles,
    /// Left as they were, because the operator asked (`--core-dumps on`).
    LeftOn,
}

/// Turns core dumps off unless `keep` (the operator's `--core-dumps on`).
/// Call it once, first thing, before any key is read.
pub fn core_dumps(keep: bool) -> CoreDumps {
    if keep {
        CoreDumps::LeftOn
    } else if !cfg!(unix) {
        CoreDumps::NoCoreFiles
    } else if lockdown_crypto::locked::harden_process() {
        CoreDumps::Off
    } else {
        CoreDumps::Refused
    }
}

impl CoreDumps {
    /// The one line to log, when there is something to say.
    pub fn line(self) -> Option<&'static str> {
        match self {
            CoreDumps::Off | CoreDumps::NoCoreFiles => None,
            CoreDumps::Refused => Some(
                "couldn't turn core dumps off; a crash file could hold the server's keys (starting anyway)",
            ),
            CoreDumps::LeftOn => Some(
                "core dumps left on (--core-dumps on); a crash file can hold the server's keys",
            ),
        }
    }
}

/// The one line to log once the keys are open, when the system refused
/// to keep some of them out of swap (a low `RLIMIT_MEMLOCK`). The keys
/// work either way.
pub fn swap_line(report: &MemoryReport) -> Option<String> {
    (report.refused > 0).then(|| {
        format!(
            "this computer refused to keep the server's keys out of swap ({} locks refused; raise the locked-memory limit, `ulimit -l`); they work, and may reach swap",
            report.refused
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_outcome_logs_at_most_one_line_and_only_when_it_matters() {
        assert_eq!(CoreDumps::Off.line(), None);
        assert_eq!(CoreDumps::NoCoreFiles.line(), None);
        assert!(
            CoreDumps::Refused
                .line()
                .unwrap()
                .contains("starting anyway")
        );
        assert!(
            CoreDumps::LeftOn
                .line()
                .unwrap()
                .contains("--core-dumps on")
        );
        assert_eq!(core_dumps(true), CoreDumps::LeftOn);

        let mut report = lockdown_crypto::locked::report();
        report.refused = 0;
        assert_eq!(swap_line(&report), None);
        report.refused = 3;
        let line = swap_line(&report).unwrap();
        assert!(line.contains("(3 locks refused;"), "{line}");
    }

    /// The core limit as the kernel reports it for this process.
    #[cfg(target_os = "linux")]
    fn core_limit(limits: &str) -> Option<(String, String)> {
        let line = limits
            .lines()
            .find(|l| l.starts_with("Max core file size"))?;
        let mut words = line["Max core file size".len()..].split_whitespace();
        Some((words.next()?.to_string(), words.next()?.to_string()))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn turning_core_dumps_off_sets_the_core_limit_to_zero() {
        assert_eq!(core_dumps(false), CoreDumps::Off);
        let limits = std::fs::read_to_string("/proc/self/limits").unwrap();
        assert_eq!(
            core_limit(&limits),
            Some(("0".to_string(), "0".to_string())),
            "{limits}"
        );
        assert!(lockdown_crypto::locked::report().core_dumps_off);
        // A second call is harmless.
        assert_eq!(core_dumps(false), CoreDumps::Off);
    }
}
