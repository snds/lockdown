//! `lockdown-server`: the server (03-architecture §5; 05-self-host.md; D7).
//!
//! **Status: skeleton.** One axum binary, the same code for cloud and
//! self-host. Today it answers `/healthz` and `/.well-known/lockdown`
//! (the discovery document a client reads to learn which features this
//! install offers). Items, manifests, OPAQUE, the key-transparency log,
//! checkpoints, blobs and push arrive in order. The server never holds a
//! vault key, an item key, a password, a plain title or URL, or a device
//! signing key; it orders writes and keeps receipts.

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

/// What `/.well-known/lockdown` says about this install.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// The discovery document version.
    pub v: u16,
    /// The API versions this server speaks.
    pub api: Vec<u16>,
    /// Features this install offers. Differ by configuration, never by fork (D7).
    pub features: Features,
}

/// Feature flags a client reads before relying on a server.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Features {
    /// OPAQUE account login (04-security §3).
    pub opaque: bool,
    /// Key-transparency log with published checkpoints (04-security §6).
    pub key_transparency: bool,
    /// Manifest-head anti-rollback checkpoints (02-model §8).
    pub checkpoints: bool,
    /// Attachment blobs.
    pub blobs: bool,
    /// Content-free push pings.
    pub push: bool,
}

/// The discovery document this build serves.
pub fn discovery() -> Discovery {
    Discovery {
        v: 1,
        api: vec![],
        features: Features {
            opaque: false,
            key_transparency: false,
            checkpoints: false,
            blobs: false,
            push: false,
        },
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn well_known() -> Json<Discovery> {
    Json(discovery())
}

/// The router, so tests can drive it without a socket.
pub fn app() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/lockdown", get(well_known))
}

#[tokio::main]
async fn main() {
    let addr = std::env::var("LOCKDOWN_LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("lockdown-server: couldn't listen on {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("lockdown-server listening on {addr}");
    if let Err(e) = axum::serve(listener, app()).await {
        eprintln!("lockdown-server: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_and_discovery_answer() {
        let res = app()
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app()
            .oneshot(
                Request::get("/.well-known/lockdown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["features"]["opaque"], false);
    }
}
