//! The web vault's files at `/vault/`, through the router: the page, its
//! assets and the module with their types, the headers on every answer
//! (found or not), and nothing outside the directory.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_server::{AppState, Config, app};
use tower::ServiceExt;

fn router(web_vault: Option<std::path::PathBuf>) -> axum::Router {
    let state = AppState::in_memory(Config {
        web_vault,
        ..Config::default()
    })
    .unwrap();
    app(Arc::new(state))
}

async fn get(router: &axum::Router, path: &str) -> axum::response::Response {
    router
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn header_of<'a>(res: &'a axum::response::Response, name: &str) -> &'a str {
    res.headers()
        .get(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .to_str()
        .unwrap()
}

fn assert_locked_down(res: &axum::response::Response) {
    let csp = header_of(res, "content-security-policy");
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval'"), "{csp}");
    assert!(csp.contains("connect-src 'self'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    assert!(!csp.contains("unsafe-inline"), "{csp}");
    assert_eq!(header_of(res, "cross-origin-opener-policy"), "same-origin");
    assert_eq!(header_of(res, "cross-origin-embedder-policy"), "require-corp");
    assert_eq!(header_of(res, "cross-origin-resource-policy"), "same-origin");
    assert_eq!(header_of(res, "x-frame-options"), "DENY");
    assert_eq!(header_of(res, "x-content-type-options"), "nosniff");
    assert_eq!(header_of(res, "referrer-policy"), "no-referrer");
}

#[tokio::test]
async fn the_page_its_assets_and_the_module_come_with_the_headers() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("index.html"), "<!doctype html><title>Lockdown</title>").unwrap();
    std::fs::write(dir.path().join("assets/index-abc.js"), "export {}").unwrap();
    std::fs::write(dir.path().join("assets/core-abc.wasm"), b"\0asm").unwrap();
    let router = router(Some(dir.path().to_path_buf()));

    let res = get(&router, "/vault").await;
    assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(header_of(&res, "location"), "/vault/");

    let res = get(&router, "/vault/").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_locked_down(&res);
    assert_eq!(header_of(&res, "content-type"), "text/html; charset=utf-8");
    assert_eq!(header_of(&res, "cache-control"), "no-store");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"<!doctype html>"));

    let res = get(&router, "/vault/assets/index-abc.js").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_locked_down(&res);
    assert_eq!(header_of(&res, "content-type"), "text/javascript; charset=utf-8");
    assert!(header_of(&res, "cache-control").contains("immutable"));

    let res = get(&router, "/vault/assets/core-abc.wasm").await;
    assert_eq!(header_of(&res, "content-type"), "application/wasm");

    for bad in [
        "/vault/../Cargo.toml",
        "/vault/assets/%2e%2e/%2e%2e/etc/passwd",
        "/vault/.env",
        "/vault/assets/missing.js",
    ] {
        let res = get(&router, bad).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{bad}");
        assert_locked_down(&res);
    }
    // The API is untouched by the web vault's routes.
    let res = get(&router, "/healthz").await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_server_without_a_web_vault_says_so() {
    let router = router(None);
    let res = get(&router, "/vault/").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_locked_down(&res);
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"this server doesn't serve the web vault");
}
