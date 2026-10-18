//! `lockdown-security`: the parts of the security center that need no
//! vault (docs/plan/13-security-center.md). The report over a person's
//! items is built in `lockdown-app`, where the vault is open; everything
//! here is either pure data or a network call that carries no secret.
//!
//! - [`breach`] — the breached-password check by k-anonymity: a password's
//!   SHA-1 is worked out on the device and only its first five hex
//!   characters leave it; the range that comes back (padded, so its size
//!   says nothing) is compared locally. The source is injectable: the
//!   public Pwned Passwords range API by default, any compatible endpoint,
//!   or a directory of range files for a computer that is never online.
//! - [`twofa`] — a bundled, versioned snapshot of 2fa.directory (MIT), so
//!   the missing-2FA report never asks anyone which sites you use.
//! - [`change_password`] — a site's `/.well-known/change-password` page,
//!   checked before it is offered, with the site's front page as the
//!   fallback.
//! - [`playbook`] — short, plain steps for each kind of finding.
//! - [`http`] — the one HTTP client both network calls share, behind the
//!   [`http::Fetch`] trait so tests (and a shell with its own stack) swap it.
//!
//! Nothing here logs, and no error message names a password or a hash.

#![forbid(unsafe_code)]

pub mod breach;
pub mod change_password;
pub mod http;
pub mod playbook;
pub mod twofa;

pub use breach::{BreachError, Candidate, DEFAULT_RANGE_ENDPOINT, RangeSource};
pub use change_password::{ChangeLink, LinkKind};
pub use http::{Fetch, FetchError, Fetched, HttpFetch};
pub use playbook::{PLAYBOOKS, Playbook, playbook};
pub use twofa::{TwoFaDirectory, TwoFaEntry};
