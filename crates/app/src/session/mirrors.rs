//! Log mirrors on the device (`key-transparency`;
//! docs/plan/27-web-server-finish.md §4; the comparison itself is
//! `lockdown_sync::mirror::compare`).
//!
//! After every sync's last key-transparency check the device fetches the
//! head each mirror it knows copied and holds it against the head it just
//! verified. It knows a mirror from the person's settings, or because the
//! account server named it at `/v1/kt/mirrors`; a server's mirrors are
//! **remembered once seen** (`kt-mirrors.cbor` beside the other memory
//! files), so a server that later stops naming one doesn't stop the check.
//! The last result per mirror is kept there too, so Status and Settings
//! say it after a restart, and a split view isn't forgotten by the next
//! quiet sync. Mirrors are public: no token goes to them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lockdown_sync::mirror::{MAX_MIRRORS, MirrorVerdict, compare, normalise_mirror};
use lockdown_sync::{HttpTransport, KtSession};
use serde::{Deserialize, Serialize};

pub use crate::types::MirrorCheck;

/// The file, beside `transparency.cbor`.
pub(crate) const FILE: &str = "kt-mirrors.cbor";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Memory {
    v: u16,
    /// Per account server: the mirrors it has named, in the order first seen.
    known: BTreeMap<String, Vec<String>>,
    /// Per account server: the last comparisons.
    last: BTreeMap<String, Vec<MirrorCheck>>,
}

fn path(data: &Path) -> PathBuf {
    data.join(FILE)
}

fn load(data: &Path) -> Memory {
    std::fs::read(path(data))
        .ok()
        .and_then(|b| lockdown_crypto::cbor::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(data: &Path, m: &Memory) -> Result<(), String> {
    let bytes = lockdown_crypto::cbor::to_canonical(m).map_err(|e| e.to_string())?;
    lockdown_store::write_private(&path(data), &bytes)
        .map_err(|e| format!("couldn't write {}: {e}", path(data).display()))
}

/// The last comparisons for `server`, oldest mirror first.
pub(crate) fn last(data: &Path, server: &str) -> Vec<MirrorCheck> {
    load(data).last.get(server).cloned().unwrap_or_default()
}

/// Compares every mirror known for `server` with the head `kt` verified;
/// `configured` are the person's own. Remembers what the server named and
/// what each mirror said.
pub(crate) fn check(
    data: &Path,
    server: &str,
    kt: &KtSession<'_>,
    transport: &HttpTransport,
    configured: &[String],
) -> Vec<MirrorCheck> {
    let mut memory = load(data);
    memory.v = 1;
    let known = memory.known.entry(server.to_string()).or_default();
    if let Ok(named) = transport.kt_mirrors() {
        for url in named.mirrors.iter().filter_map(|u| normalise_mirror(u)) {
            if !known.contains(&url) && known.len() < MAX_MIRRORS {
                known.push(url);
            }
        }
    }
    let mut list: Vec<(String, &'static str)> = Vec::new();
    for url in configured.iter().filter_map(|u| normalise_mirror(u)) {
        if !list.iter().any(|(u, _)| *u == url) {
            list.push((url, "settings"));
        }
    }
    for url in known.iter() {
        if !list.iter().any(|(u, _)| u == url) {
            list.push((url.clone(), "server"));
        }
    }
    list.truncate(MAX_MIRRORS);
    let now = lockdown_vault::Timestamp::now().0;
    let checks: Vec<MirrorCheck> = list
        .into_iter()
        .map(|(url, named_by)| {
            let verdict = match HttpTransport::new(&url).and_then(|t| t.kt_mirror_head()) {
                Ok(head) => compare(kt, server, &head, now),
                Err(e) => MirrorVerdict::Unreachable {
                    message: format!("couldn't read its copy: {e}"),
                },
            };
            MirrorCheck {
                url,
                named_by: named_by.to_string(),
                verdict,
                at: now,
            }
        })
        .collect();
    memory.last.insert(server.to_string(), checks.clone());
    let _ = save(data, &memory);
    checks
}

/// The sync's lines for `checks`: a split loudly, the rest briefly.
pub(crate) fn lines(checks: &[MirrorCheck]) -> Vec<String> {
    checks
        .iter()
        .map(|c| match &c.verdict {
            MirrorVerdict::Agrees { size, .. } => {
                format!("mirror {}: agrees (its copy has {size} entries)", c.url)
            }
            MirrorVerdict::Split { message } => format!("mirror {}: {message}", c.url),
            MirrorVerdict::Unusable { message } => {
                format!("mirror {}: can't vouch for this log: {message}", c.url)
            }
            MirrorVerdict::Unreachable { message } => format!("mirror {}: {message}", c.url),
        })
        .collect()
}
