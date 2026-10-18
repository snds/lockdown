//! The web vault, served at `/vault/` from a directory of built files
//! (`apps/web/dist`, `--web-vault DIR`), with headers that leave the page
//! nothing to load but its own files (docs/plan/17-web-vault.md §6).
//!
//! The server only hands out the files. It cannot read a vault through
//! them any more than through the API, but it could hand out different
//! files: whoever serves the web vault's code can change it. That trust
//! problem, and what Lockdown does about it now and later, is written up
//! in the plan doc; what this module does is the part a server can do:
//!
//! - a Content Security Policy with no inline script, no `eval` (only the
//!   WebAssembly compile the core needs), no origin but this one, no
//!   frames, no forms, no `<base>`;
//! - `Cross-Origin-Opener-Policy`, `-Embedder-Policy` and
//!   `-Resource-Policy` set to this origin, so no other site's window can
//!   hold a handle on the vault's and nothing is embedded from elsewhere;
//! - `frame-ancestors 'none'` and `X-Frame-Options: DENY` against framing;
//! - `nosniff`, `no-referrer`, and a permissions policy that turns off
//!   every device API the vault does not use;
//! - only known file types, only regular files under the directory, no
//!   dot-files, no `..`, no symlinks out; `index.html` never cached, the
//!   hashed assets cached for good (their names change when they do).
//!
//! The asset tags in `index.html` carry Subresource Integrity hashes from
//! the build, and the worker checks the WebAssembly module's hash before
//! it compiles it; the server takes no part in either.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// The policy every `/vault/` answer carries.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; \
script-src 'self' 'wasm-unsafe-eval'; \
worker-src 'self'; \
connect-src 'self'; \
img-src 'self' data:; \
style-src 'self'; \
font-src 'self'; \
manifest-src 'self'; \
base-uri 'none'; \
form-action 'none'; \
frame-ancestors 'none'";

/// Device APIs the vault never uses, all off.
const PERMISSIONS_POLICY: &str = "accelerometer=(), camera=(), display-capture=(), \
geolocation=(), gyroscope=(), hid=(), magnetometer=(), microphone=(), midi=(), \
payment=(), publickey-credentials-get=(), serial=(), usb=(), xr-spatial-tracking=()";

/// The largest file served (the WebAssembly module is about 10 MB).
const MAX_FILE: u64 = 64 << 20;

/// The type of a file the vault is built from, by its extension; `None`
/// for anything else, which is not served.
fn content_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

/// The headers on every `/vault/` answer, found or not.
pub fn security_headers(headers: &mut HeaderMap) {
    let set = |headers: &mut HeaderMap, name: &'static str, value: &'static str| {
        headers.insert(name, HeaderValue::from_static(value));
    };
    set(headers, "content-security-policy", CONTENT_SECURITY_POLICY);
    set(headers, "cross-origin-opener-policy", "same-origin");
    set(headers, "cross-origin-embedder-policy", "require-corp");
    set(headers, "cross-origin-resource-policy", "same-origin");
    set(headers, "x-frame-options", "DENY");
    set(headers, "x-content-type-options", "nosniff");
    set(headers, "referrer-policy", "no-referrer");
    set(headers, "permissions-policy", PERMISSIONS_POLICY);
}

fn plain(status: StatusCode, text: &'static str) -> Response {
    let mut res = (status, text).into_response();
    security_headers(res.headers_mut());
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// The file `rel` under `root`, when it is one this module serves: plain
/// segments only, none hidden, a known type, a regular file that is still
/// under `root` once links are followed.
fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\\') || rel.contains('\0') || rel.len() > 512 {
        return None;
    }
    let rel_path = Path::new(rel);
    for c in rel_path.components() {
        match c {
            Component::Normal(s) if !s.to_string_lossy().starts_with('.') => {}
            _ => return None,
        }
    }
    content_type(rel_path)?;
    let root = root.canonicalize().ok()?;
    let full = root.join(rel_path).canonicalize().ok()?;
    if !full.starts_with(&root) || !full.is_file() {
        return None;
    }
    Some(full)
}

async fn serve(state: &AppState, rel: &str) -> Response {
    let Some(root) = state.config.web_vault.as_deref() else {
        return plain(
            StatusCode::NOT_FOUND,
            "this server doesn't serve the web vault",
        );
    };
    let Some(path) = resolve(root, rel) else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let too_big = std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_FILE);
    if too_big {
        return plain(StatusCode::NOT_FOUND, "not found");
    }
    let read = path.clone();
    let Ok(Ok(bytes)) = tokio::task::spawn_blocking(move || std::fs::read(read)).await else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let ty = content_type(&path).unwrap_or("application/octet-stream");
    let cache = if rel.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-store"
    };
    let mut res = Response::new(Body::from(bytes));
    security_headers(res.headers_mut());
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ty));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res
}

/// `GET /vault`: to `/vault/`, so the page's relative asset paths resolve.
pub async fn redirect() -> Response {
    let mut res = (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, "/vault/")],
    )
        .into_response();
    security_headers(res.headers_mut());
    res
}

/// `GET /vault/`: the page.
pub async fn index(State(state): State<Arc<AppState>>) -> Response {
    serve(&state, "index.html").await
}

/// `GET /vault/{*path}`: one of the page's files, or the page itself for
/// a link it opens (`s/{id}`, `r/{id}`).
pub async fn file(State(state): State<Arc<AppState>>, UrlPath(path): UrlPath<String>) -> Response {
    if link_path(&path) {
        return serve(&state, "index.html").await;
    }
    serve(&state, &path).await
}

/// Whether `path` (under `/vault/`) is a link the page opens: `s/` or
/// `r/` and one id-shaped segment.
fn link_path(path: &str) -> bool {
    let Some((kind, id)) = path.split_once('/') else {
        return false;
    };
    matches!(kind, "s" | "r") && id_shaped(id)
}

/// A link id as the server makes them: a UUID's characters only.
fn id_shaped(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// A share link (`s`) or request link (`r`) opened in a browser, sent on
/// to the web vault's page for it when this server serves one. The
/// redirect names no fragment, so the browser keeps the link's own (the
/// key) and never sends it here (RFC 9110 §10.2.2). `None` when there is
/// no web vault, or the id isn't one.
pub fn link_redirect(state: &AppState, kind: &str, id: &str) -> Option<Response> {
    state.config.web_vault.as_ref()?;
    if !id_shaped(id) || !matches!(kind, "s" | "r") {
        return None;
    }
    let location = HeaderValue::from_str(&format!("/vault/{kind}/{id}")).ok()?;
    let mut res = (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response();
    security_headers(res.headers_mut());
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_known_files_under_the_root_resolve() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(dir.path().join("assets/app-1.js"), "x").unwrap();
        std::fs::write(dir.path().join("assets/notes.md"), "x").unwrap();
        std::fs::write(dir.path().join(".env"), "x").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.js"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            outside.path().join("secret.js"),
            dir.path().join("assets/link.js"),
        )
        .unwrap();

        assert!(resolve(dir.path(), "index.html").is_some());
        assert!(resolve(dir.path(), "assets/app-1.js").is_some());
        for bad in [
            "",
            "../secret.js",
            "assets/../../secret.js",
            "/etc/passwd",
            ".env",
            "assets/notes.md",
            "assets\\app-1.js",
            "assets/missing.js",
            "assets",
        ] {
            assert!(resolve(dir.path(), bad).is_none(), "{bad}");
        }
        #[cfg(unix)]
        assert!(resolve(dir.path(), "assets/link.js").is_none());
    }

    #[test]
    fn only_a_link_and_an_id_open_the_page() {
        assert!(link_path("s/0f6c3a52-7d2e-4f0e-9c1b-2d7f5e8a9b10"));
        assert!(link_path("r/0f6c3a52-7d2e-4f0e-9c1b-2d7f5e8a9b10"));
        for bad in ["s/", "s", "x/abc", "s/../index.html", "s/abc/def", "r/<script>", "assets/s/abc"] {
            assert!(!link_path(bad), "{bad}");
        }
    }

    #[test]
    fn the_policy_allows_no_inline_script_and_no_other_origin() {
        let csp = CONTENT_SECURITY_POLICY;
        assert!(!csp.contains("unsafe-inline") && !csp.contains("'unsafe-eval'"));
        assert!(!csp.contains("http") && !csp.contains('*'));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("default-src 'none'"));
    }
}
