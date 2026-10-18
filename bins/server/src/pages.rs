//! The server's own HTML pages (the admin page, the email approval page):
//! written on the server, no script at all, one inline style sheet allowed
//! by its hash, forms that post back to the same server only, never framed,
//! never cached, never sent a referrer. Every value that came from a
//! person, a device or the network is escaped with [`esc`].

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

/// The pages' one style sheet: plain, readable, light and dark.
pub const STYLE: &str = "\
:root{color-scheme:light dark;--fg:#1d1d1f;--muted:#5b5b62;--bg:#fbfbfc;--card:#fff;--line:#d9d9de;--accent:#0b57d0;--bad:#b3261e;--ok:#146c2e}\
@media (prefers-color-scheme:dark){:root{--fg:#ececf0;--muted:#a3a3ad;--bg:#151518;--card:#1e1e22;--line:#3a3a41;--accent:#8ab4f8;--bad:#f2b8b5;--ok:#7dd99a}}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif}\
main{max-width:64rem;margin:0 auto;padding:1.5rem 1rem 3rem}h1{font-size:1.4rem;margin:.2rem 0 1rem}h2{font-size:1.1rem;margin:1.6rem 0 .6rem}\
nav{display:flex;flex-wrap:wrap;gap:.25rem 1rem;align-items:center;border-bottom:1px solid var(--line);padding-bottom:.6rem;margin-bottom:1rem}\
nav a{color:var(--accent);text-decoration:none}nav a[aria-current]{font-weight:600;color:var(--fg)}nav form{margin-left:auto}\
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:1rem;margin:.8rem 0}\
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(12rem,1fr));gap:.8rem}\
.stat b{display:block;font-size:1.3rem}.muted{color:var(--muted)}.bad{color:var(--bad)}.ok{color:var(--ok)}\
.banner{border:2px solid var(--bad);border-radius:10px;padding:.8rem 1rem;color:var(--bad);font-weight:600}\
table{width:100%;border-collapse:collapse;font-size:.92rem}th,td{text-align:left;padding:.45rem .5rem;border-bottom:1px solid var(--line);vertical-align:top}\
.scroll{overflow-x:auto}code,.mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.88rem;overflow-wrap:anywhere}\
label{display:block;font-weight:600;margin:.6rem 0 .2rem}input[type=text],input[type=password],input[type=email],input[type=number]{width:100%;max-width:28rem;padding:.5rem .6rem;border:1px solid var(--line);border-radius:8px;background:var(--bg);color:var(--fg);font:inherit}\
button{font:inherit;padding:.45rem .9rem;border-radius:8px;border:1px solid var(--accent);background:var(--accent);color:var(--card);cursor:pointer}\
button.quiet{background:transparent;color:var(--accent)}button.danger{border-color:var(--bad);background:transparent;color:var(--bad)}\
form.inline{display:inline}.row{display:flex;flex-wrap:wrap;gap:.6rem;align-items:center}";

/// Escapes text for HTML text and attribute values.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// The Content-Security-Policy of these pages: nothing but the one style
/// sheet, forms to this origin only.
pub fn csp() -> String {
    let hash = data_encoding::BASE64.encode(&Sha256::digest(STYLE.as_bytes()));
    format!(
        "default-src 'none'; style-src 'sha256-{hash}'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    )
}

/// A whole page.
pub fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><title>{}</title><style>{STYLE}</style></head><body><main>{body}</main></body></html>",
        esc(title)
    );
    let mut response = (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )],
        html,
    )
        .into_response();
    secure(&mut response);
    response
}

/// The headers every one of these pages and their redirects carry.
pub fn secure(response: &mut Response) {
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&csp()) {
        headers.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

/// A `303 See Other` to `to` (a path on this server).
pub fn see_other(to: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(v) = HeaderValue::from_str(to) {
        response.headers_mut().insert(header::LOCATION, v);
    }
    secure(&mut response);
    response
}

/// Reads an `application/x-www-form-urlencoded` body into its fields
/// (each name once; the last one wins), capped at 64 fields.
pub fn form_fields(body: &[u8]) -> std::collections::BTreeMap<String, String> {
    url::form_urlencoded::parse(body)
        .take(64)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_and_the_policy_names_the_sheet() {
        assert_eq!(
            esc("<a href=\"x\">'&'</a>\u{1b}"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
        assert!(csp().starts_with("default-src 'none'; style-src 'sha256-"));
        let r = page(StatusCode::OK, "<t>", "<p>x</p>");
        assert_eq!(r.headers()[header::X_FRAME_OPTIONS], "DENY");
        let f = form_fields(b"a=1&b=two+words&a=3");
        assert_eq!(f["a"], "3");
        assert_eq!(f["b"], "two words");
    }
}
