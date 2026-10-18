//! Log mirrors, the server's side (`key-transparency`;
//! docs/plan/27-web-server-finish.md §4; the shapes and the client's
//! comparison are `lockdown_sync::mirror`).
//!
//! **Mirror mode** (`--mirror-of URL`). This server copies another
//! server's signed tree heads: every `--mirror-every-secs` (300) it fetches
//! the upstream head, checks the signature under the log key it pinned on
//! first sight, and, with a consistency proof from the upstream, that the
//! head continues the last one it copied. It keeps the newest heads it
//! verified in `kt-mirror.cbor` beside its data, serves the newest at
//! `GET /v1/kt/mirror` (CBOR, open to any origin, since a mirror is public),
//! and with `--mirror-out DIR` writes the same bytes to `DIR/v1/kt/mirror`
//! for a static file host to publish. It never holds a leaf, so it learns
//! nothing but sizes and roots. When the upstream stops continuing what it
//! copied (a shorter log, a second root at one size, a failed proof), the
//! mirror records the problem in words and keeps serving its last good
//! head, so every client holding a newer head from the upstream sees the
//! split too. The problem stays until the operator starts the mirror over
//! (deletes the file).
//!
//! **Naming mirrors** (`--kt-mirror URL`, repeatable). An account server
//! lists the mirrors its clients should compare against at
//! `GET /v1/kt/mirrors`, and adds their origins to the web vault's
//! `connect-src` so the web vault can fetch them too. Clients remember a
//! mirror once named, so a server that later drops one from the list
//! doesn't stop the check.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use lockdown_crypto::VerifyingKey;
use lockdown_sync::mirror::{KtMirrors, MIRROR_PATH, MIRROR_V, MirrorHead, continues};
use lockdown_sync::wire;
use lockdown_vault::transparency::SignedTreeHead;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::api::Cbor;

/// The state file's name, beside the server's data.
pub const STATE_FILE: &str = "kt-mirror.cbor";
/// How many verified heads the file keeps (the newest).
pub const KEEP_HEADS: usize = 256;
/// The most mirrors an account server names.
pub const MAX_NAMED: usize = 8;

/// What a mirror keeps between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    v: u16,
    upstream: String,
    #[serde(with = "serde_bytes")]
    log_key: [u8; 32],
    /// The post-quantum log key, pinned when first offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pq_log_key: Option<serde_bytes::ByteBuf>,
    /// The newest verified heads, oldest first.
    heads: Vec<SignedTreeHead>,
    copied: u64,
    checked_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

/// A mirror of one upstream server.
pub struct Mirror {
    upstream: String,
    file: PathBuf,
    out: Option<PathBuf>,
    every: Duration,
    saved: Mutex<Option<Saved>>,
}

impl std::fmt::Debug for Mirror {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mirror")
            .field("upstream", &self.upstream)
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes `bytes` to `path` through a temporary file beside it.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no folder", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

impl Mirror {
    /// A mirror of `upstream` keeping its state in `dir`; with `out`, the
    /// head is also written there for a static host. Reads what an earlier
    /// run kept; a file for another upstream is refused.
    pub fn open(
        upstream: &str,
        dir: &Path,
        out: Option<&Path>,
        every: Duration,
    ) -> Result<Self, String> {
        let upstream = lockdown_sync::mirror::normalise_mirror(upstream).ok_or_else(|| {
            format!("{upstream} isn't an https:// address (or http:// to this machine)")
        })?;
        let file = dir.join(STATE_FILE);
        let saved = match std::fs::read(&file) {
            Ok(bytes) => {
                let s: Saved = wire::decode(&bytes)
                    .map_err(|e| format!("{} is unreadable: {e}", file.display()))?;
                if s.upstream != upstream {
                    return Err(format!(
                        "{} copies {}, not {upstream}; move it away to mirror another server",
                        file.display(),
                        s.upstream
                    ));
                }
                Some(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("couldn't read {}: {e}", file.display())),
        };
        Ok(Self {
            upstream,
            file,
            out: out.map(Path::to_path_buf),
            every: every.max(Duration::from_secs(30)),
            saved: Mutex::new(saved),
        })
    }

    /// The upstream it copies.
    pub fn upstream(&self) -> &str {
        &self.upstream
    }

    /// The newest head, as served; `None` before the first copy.
    pub fn head(&self) -> Option<MirrorHead> {
        lock(&self.saved).as_ref().and_then(|s| {
            Some(MirrorHead {
                v: MIRROR_V,
                upstream: s.upstream.clone(),
                log_key: s.log_key,
                sth: s.heads.last()?.clone(),
                pq_log_key: s.pq_log_key.clone(),
                checked_at: s.checked_at,
                copied: s.copied,
                problem: s.problem.clone(),
            })
        })
    }

    /// Fetches the upstream head once and copies it when it continues the
    /// last one. `Ok` says what happened; `Err` is a fetch that failed (the
    /// upstream unreachable), which changes nothing kept.
    pub fn poll_once(&self, now: i64) -> Result<String, String> {
        let upstream =
            lockdown_sync::HttpTransport::new(&self.upstream).map_err(|e| e.to_string())?;
        use lockdown_sync::Transport as _;
        let wire = upstream.kt_sth().map_err(|e| e.to_string())?;
        // One poller per mirror, so the copy taken here is the latest; the
        // lock isn't held across the network.
        let mut saved = lock(&self.saved).clone().unwrap_or(Saved {
            v: 1,
            upstream: self.upstream.clone(),
            log_key: wire.log_key,
            pq_log_key: None,
            heads: Vec::new(),
            copied: 0,
            checked_at: now,
            problem: None,
        });
        saved.checked_at = now;
        let said = if saved.problem.is_some() {
            "a problem was recorded earlier; the mirror keeps its last good head".to_string()
        } else if wire.log_key != saved.log_key {
            saved.problem = Some(format!(
                "{} changed its log key since the mirror pinned it",
                self.upstream
            ));
            "the upstream's log key changed".into()
        } else if saved.pq_log_key.is_some() && wire.pq_log_key.is_none() {
            saved.problem = Some(format!(
                "{} dropped the post-quantum log key the mirror pinned",
                self.upstream
            ));
            "the upstream dropped its post-quantum log key".into()
        } else if saved.pq_log_key.is_some() && wire.pq_log_key != saved.pq_log_key {
            saved.problem = Some(format!(
                "{} changed its post-quantum log key since the mirror pinned it",
                self.upstream
            ));
            "the upstream's post-quantum log key changed".into()
        } else if !signed_by(&wire) {
            // Not signed by the key: not the server's word at all, so it
            // proves nothing either way. Nothing is copied.
            "the upstream's head isn't signed by its log key; nothing copied".into()
        } else {
            // Pinned the first time the upstream offers one.
            if saved.pq_log_key.is_none() {
                saved.pq_log_key = wire.pq_log_key.clone();
            }
            match saved.heads.last() {
                None => {
                    saved.heads.push(wire.sth.clone());
                    saved.copied += 1;
                    format!("first head copied: {} entries", wire.sth.size)
                }
                Some(last) => {
                    let proof = if wire.sth.size > last.size {
                        Some(
                            upstream
                                .kt_consistency(last.size, wire.sth.size)
                                .map_err(|e| e.to_string())?,
                        )
                    } else {
                        None
                    };
                    match continues(last, &wire.sth, proof.as_ref()) {
                        Ok(()) if wire.sth.size == last.size => {
                            format!("no change: {} entries", wire.sth.size)
                        }
                        Ok(()) => {
                            saved.heads.push(wire.sth.clone());
                            saved.copied += 1;
                            if saved.heads.len() > KEEP_HEADS {
                                let extra = saved.heads.len() - KEEP_HEADS;
                                saved.heads.drain(..extra);
                            }
                            format!("copied: {} entries", wire.sth.size)
                        }
                        Err(why) => {
                            saved.problem = Some(format!("{}: {why}", self.upstream));
                            format!("PROBLEM: {why}")
                        }
                    }
                }
            }
        };
        let bytes = wire::encode(&saved).map_err(|e| e.to_string())?;
        write_atomic(&self.file, &bytes)?;
        *lock(&self.saved) = Some(saved);
        if let (Some(out), Some(head)) = (&self.out, self.head()) {
            let path = out.join(MIRROR_PATH.trim_start_matches('/'));
            write_atomic(&path, &wire::encode(&head).map_err(|e| e.to_string())?)?;
        }
        Ok(said)
    }

    /// Copies on its own thread for as long as the process lives.
    pub fn spawn(self: Arc<Self>, clock: Arc<AppState>) -> std::io::Result<()> {
        std::thread::Builder::new()
            .name("lockdown-mirror".into())
            .spawn(move || {
                loop {
                    match self.poll_once(clock.now()) {
                        Ok(said) if said.starts_with("PROBLEM") => {
                            eprintln!("lockdown-server: mirror of {}: {said}", self.upstream)
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!(
                            "lockdown-server: mirror of {}: couldn't fetch its log head: {e}",
                            self.upstream
                        ),
                    }
                    std::thread::sleep(self.every);
                }
            })
            .map(|_| ())
    }
}

/// Whether the upstream's head is signed by the keys it names (both, when
/// it names a post-quantum one).
fn signed_by(wire: &lockdown_sync::kt::SthWire) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(&wire.log_key) else {
        return false;
    };
    let pq = match wire.pq_log_key.as_ref() {
        None => None,
        Some(b) => match lockdown_crypto::pq_sign::PqVerifyingKey::from_bytes(b) {
            Ok(k) => Some(k),
            Err(_) => return false,
        },
    };
    wire.sth.verify_hybrid(&key, pq.as_ref()).is_ok()
}

/// The mirror parts of the server's state.
#[derive(Debug, Default)]
pub struct MirrorState {
    mirror: OnceLock<Arc<Mirror>>,
    named: RwLock<Vec<String>>,
}

impl MirrorState {
    /// Makes this server a mirror (once).
    pub fn set_mirror(&self, mirror: Arc<Mirror>) -> bool {
        self.mirror.set(mirror).is_ok()
    }

    /// The mirror this server runs, if it does.
    pub fn mirror(&self) -> Option<&Arc<Mirror>> {
        self.mirror.get()
    }

    /// Names the mirrors this server's clients compare against; each must
    /// be an `https://` address (or `http://` to this machine).
    pub fn set_named(&self, urls: &[String]) -> Result<(), String> {
        let mut out = Vec::new();
        for u in urls {
            let n = lockdown_sync::mirror::normalise_mirror(u)
                .ok_or_else(|| format!("{u} isn't an https:// address"))?;
            if !out.contains(&n) {
                out.push(n);
            }
        }
        if out.len() > MAX_NAMED {
            return Err(format!("at most {MAX_NAMED} mirrors may be named"));
        }
        *self
            .named
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = out;
        Ok(())
    }

    /// The mirrors named, in the order given.
    pub fn named(&self) -> Vec<String> {
        self.named
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// `GET /v1/kt/mirror`: the newest head this mirror copied.
pub async fn head(State(state): State<Arc<AppState>>) -> Response {
    let Some(head) = state.mirrors.mirror().and_then(|m| m.head()) else {
        return (
            StatusCode::NOT_FOUND,
            "this server isn't a mirror, or hasn't copied a head yet",
        )
            .into_response();
    };
    let mut res = Cbor(head).into_response();
    let h = res.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=30"),
    );
    res
}

/// `GET /v1/kt/mirrors`: the mirrors this server names.
pub async fn named(State(state): State<Arc<AppState>>) -> Cbor<KtMirrors> {
    Cbor(KtMirrors {
        mirrors: state.mirrors.named(),
    })
}

/// The web vault's policy with the named mirrors' origins added to
/// `connect-src`, so the worker may fetch their heads (and nothing else
/// there: the worker's own check allows only the mirror path).
pub fn widen_csp(state: &AppState, headers: &mut HeaderMap) {
    let origins: Vec<String> = state
        .mirrors
        .named()
        .iter()
        .filter_map(|u| url::Url::parse(u).ok())
        .map(|u| u.origin().ascii_serialization())
        .filter(|o| o != "null")
        .collect();
    if origins.is_empty() {
        return;
    }
    let csp = crate::web_vault::CONTENT_SECURITY_POLICY.replacen(
        "connect-src 'self';",
        &format!("connect-src 'self' {};", origins.join(" ")),
        1,
    );
    if let Ok(v) = HeaderValue::from_str(&csp) {
        headers.insert(header::CONTENT_SECURITY_POLICY, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_mirrors_are_checked_and_deduplicated() {
        let s = MirrorState::default();
        s.set_named(&[
            "https://m1.example/".into(),
            "https://m1.example".into(),
            "http://127.0.0.1:9".into(),
        ])
        .unwrap();
        assert_eq!(s.named(), vec!["https://m1.example", "http://127.0.0.1:9"]);
        assert!(s.set_named(&["http://m.example".into()]).is_err());
        let many: Vec<String> = (0..9).map(|i| format!("https://m{i}.example")).collect();
        assert!(s.set_named(&many).is_err());
    }

    #[test]
    fn a_mirror_file_for_another_upstream_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let m = Mirror::open(
            "https://a.example",
            dir.path(),
            None,
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(m.head().is_none());
        let saved = Saved {
            v: 1,
            upstream: "https://b.example".into(),
            log_key: [0; 32],
            pq_log_key: None,
            heads: Vec::new(),
            copied: 0,
            checked_at: 0,
            problem: None,
        };
        std::fs::write(dir.path().join(STATE_FILE), wire::encode(&saved).unwrap()).unwrap();
        let e = Mirror::open(
            "https://a.example",
            dir.path(),
            None,
            Duration::from_secs(60),
        )
        .unwrap_err();
        assert!(e.contains("copies https://b.example"), "{e}");
    }
}
