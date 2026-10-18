//! The server's endpoints, driven in process through `tower::ServiceExt::oneshot`:
//! the happy path, every distinct head refusal, a stale push and its merge
//! instruction, calls without a session, token expiry, and the identity
//! chain: pages accepted in order, the pinned signing key, and a revoked
//! device refused on heads, items and manifests.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::auth::{LoginClient, RegistrationClient, device_pop_message};
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_crypto::identity::{FileKeystore, Identity, Record};
use lockdown_crypto::{Key32, SigningKey};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::wire::{
    self, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, HeadAccepted, HeadRefused,
    HeadsSince, IdentityPush, IdentitySince, ItemPush, ItemsSince, LoginFinish, LoginFinishReply,
    LoginStart, LoginStartReply, PushReply, RecordWire, RegisterFinish, RegisterStart,
    RegisterStartReply, VaultList,
};
use lockdown_vault::identity::identity_signing_key;
use lockdown_vault::identity_set::{RecordSet, Revocation, VaultBoundary};
use lockdown_vault::manifest::MANIFEST_V;
use lockdown_vault::test_support::{sample_context, sample_login_fields};
use lockdown_vault::{
    Hash32, Id, Item, ItemClass, ItemKind, ManifestEntry, ManifestHead, Timestamp, Vault,
};
use tower::ServiceExt;

struct Harness {
    router: Router,
    clock: Arc<AtomicI64>,
}

fn harness(config: Config) -> Harness {
    let clock = Arc::new(AtomicI64::new(1_700_000_000));
    let mut state = AppState::in_memory(config).unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    Harness {
        router: app(Arc::new(state)),
        clock,
    }
}

impl Harness {
    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, Bytes) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, CONTENT_TYPE);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, bytes)
    }

    async fn cbor<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> Result<R, (StatusCode, ErrorBody)> {
        let (status, bytes) = self
            .call(method, path, token, wire::encode(body).unwrap())
            .await;
        if status.is_success() {
            Ok(wire::decode(&bytes).unwrap_or_else(|e| panic!("decode {path}: {e}")))
        } else {
            Err((status, wire::decode(&bytes).expect("an error body")))
        }
    }

    async fn register(&self, login: &str, password: &str, profile: Id) -> StatusCode {
        let (client, request) = RegistrationClient::start(password).unwrap();
        let reply: RegisterStartReply = self
            .cbor(
                "POST",
                "/v1/account/register/start",
                None,
                &RegisterStart {
                    login: login.into(),
                    request,
                },
            )
            .await
            .unwrap();
        let (upload, _export) = client.finish(password, &reply.response).unwrap();
        let (status, _) = self
            .call(
                "POST",
                "/v1/account/register/finish",
                None,
                wire::encode(&RegisterFinish {
                    login: login.into(),
                    profile_id: profile,
                    upload,
                })
                .unwrap(),
            )
            .await;
        status
    }

    async fn login(
        &self,
        login: &str,
        password: &str,
        device: &Device,
    ) -> Result<LoginFinishReply, (StatusCode, ErrorCode)> {
        let (client, request) = LoginClient::start(password).unwrap();
        let started: LoginStartReply = self
            .cbor(
                "POST",
                "/v1/account/login/start",
                None,
                &LoginStart {
                    login: login.into(),
                    request,
                },
            )
            .await
            .map_err(|(s, e)| (s, e.code))?;
        let finished = client
            .finish(password, &started.response)
            .map_err(|_| (StatusCode::UNAUTHORIZED, ErrorCode::LoginFailed))?;
        let pop = device
            .key()
            .sign(&device_pop_message(login, &started.login_id));
        self.cbor(
            "POST",
            "/v1/account/login/finish",
            None,
            &LoginFinish {
                login_id: started.login_id,
                finalization: finished.finalization,
                device: DeviceProofWire {
                    device_id: device.id,
                    device_pub: device.key().verifying_key().to_bytes(),
                    pop: pop.to_vec(),
                },
            },
        )
        .await
        .map_err(|(s, e)| (s, e.code))
    }
}

/// A device: an id and a signing key seed.
struct Device {
    id: Id,
    seed: [u8; 32],
}

impl Device {
    fn new() -> Self {
        Self {
            id: Id::new(),
            seed: SigningKey::generate().unwrap().inner().to_bytes(),
        }
    }

    fn key(&self) -> SigningKey {
        SigningKey::from_bytes(self.seed)
    }
}

/// A vault on a device, with a way to make items and heads the server
/// will accept.
struct LocalVault {
    vault: Vault,
    device: Device,
    adk: AppDataKey,
}

impl LocalVault {
    fn new(device: Device) -> Self {
        let vault =
            Vault::create(sample_context(), device.id, Box::new(device.key()), None).unwrap();
        Self {
            vault,
            device,
            adk: AppDataKey::generate().unwrap(),
        }
    }

    fn id(&self) -> Id {
        self.vault.context().vault_id
    }

    fn record(&self) -> Vec<u8> {
        self.vault.record(&self.adk).unwrap().to_bytes().unwrap()
    }

    fn new_item(&mut self) -> Item {
        let id = self
            .vault
            .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
            .unwrap();
        self.vault.item(id).unwrap().clone()
    }

    /// A head over `items` (each carrying its server revision).
    fn head(&self, seq: u64, prev_hash: Hash32, items: &[&Item]) -> ManifestHead {
        let mut entries: Vec<ManifestEntry> = items
            .iter()
            .map(|i| ManifestEntry {
                item_id: i.item_id,
                rev: i.rev,
                version_hash: i.latest_hash().unwrap(),
                state: i.state,
                shredded: i.is_shredded(),
            })
            .collect();
        entries.sort_by_key(|e| e.item_id);
        let mut head = ManifestHead {
            v: MANIFEST_V,
            vault_id: self.id(),
            seq,
            prev_hash,
            vault_key_ref: Id::new(),
            members: lockdown_crypto::seal(&Key32::generate().unwrap(), b"members", b"[]").unwrap(),
            entries,
            signer: self.device.id,
            signed_at: Timestamp::now(),
            sth: None,
            sig: Vec::new(),
            checkpoint: None,
        };
        head.sign(&self.device.key()).unwrap();
        head
    }
}

async fn push(
    h: &Harness,
    token: &str,
    vault: Id,
    item: &Item,
    if_match: u64,
) -> Result<u64, (StatusCode, Bytes)> {
    let (status, bytes) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{vault}/items/{}", item.item_id),
            Some(token),
            wire::encode(&ItemPush {
                if_match,
                item: item.to_bytes().unwrap(),
            })
            .unwrap(),
        )
        .await;
    if status == StatusCode::OK {
        match wire::decode::<PushReply>(&bytes).unwrap() {
            PushReply::Accepted { rev } => Ok(rev),
            other => panic!("unexpected {other:?}"),
        }
    } else {
        Err((status, bytes))
    }
}

async fn put_head(
    h: &Harness,
    token: &str,
    head: &ManifestHead,
) -> Result<HeadAccepted, HeadRefused> {
    let (status, bytes) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/heads", head.vault_id),
            Some(token),
            head.to_bytes().unwrap(),
        )
        .await;
    if status == StatusCode::OK {
        Ok(wire::decode(&bytes).unwrap())
    } else {
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        Err(wire::decode(&bytes).unwrap())
    }
}

#[tokio::test]
async fn discovery_now_offers_opaque() {
    let h = harness(Config::default());
    let (status, body) = h
        .call("GET", "/.well-known/lockdown", None, Vec::new())
        .await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["features"]["opaque"], true);
    assert_eq!(json["api"], serde_json::json!([1]));
}

#[tokio::test]
async fn register_login_push_head_pull() {
    let h = harness(Config::default());
    let profile = Id::new();
    assert_eq!(
        h.register("me@example.com", "correct horse battery staple", profile)
            .await,
        StatusCode::CREATED
    );
    // The same login twice is refused up front.
    let (client, request) = RegistrationClient::start("another long password").unwrap();
    drop(client);
    let err = h
        .cbor::<_, RegisterStartReply>(
            "POST",
            "/v1/account/register/start",
            None,
            &RegisterStart {
                login: "me@example.com".into(),
                request,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::CONFLICT);
    assert_eq!(err.1.code, ErrorCode::LoginTaken);

    let mut local = LocalVault::new(Device::new());
    let session = h
        .login(
            "me@example.com",
            "correct horse battery staple",
            &local.device,
        )
        .await
        .unwrap();
    assert_eq!(session.profile_id, profile);
    assert!(session.token.starts_with("lk1."));
    let token = session.token.as_str();

    // The vault must be claimed before anything goes under it.
    let item = local.new_item();
    let (status, body) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/items/{}", local.id(), item.item_id),
            Some(token),
            wire::encode(&ItemPush {
                if_match: 0,
                item: item.to_bytes().unwrap(),
            })
            .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let (status, _) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}", local.id()),
            Some(token),
            local.record(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    // Again: fine, and not a second vault.
    let (status, _) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}", local.id()),
            Some(token),
            local.record(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // A head before its items: refused, naming the item.
    let mut item = item;
    item.rev = 1;
    let early = local.head(1, Hash32::ZERO, &[&item]);
    let refused = put_head(&h, token, &early).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::UnknownItem);
    assert!(refused.message.contains(&item.item_id.to_string()));
    assert_eq!(refused.current_seq, 0);

    // Push, then the head.
    item.rev = 0;
    let rev = push(&h, token, local.id(), &item, 0).await.unwrap();
    assert_eq!(rev, 1);
    item.rev = rev;
    let head1 = local.head(1, Hash32::ZERO, &[&item]);
    let accepted = put_head(&h, token, &head1).await.unwrap();
    assert_eq!(accepted.seq, 1);
    assert_eq!(accepted.hash, head1.hash().unwrap());

    // Pull: heads since 0, items since 0, the vault list.
    let heads: HeadsSince = h
        .cbor(
            "GET",
            &format!("/v1/vaults/{}/heads?since=0", local.id()),
            Some(token),
            &(),
        )
        .await
        .unwrap();
    assert_eq!(heads.current_seq, 1);
    assert_eq!(heads.current_hash, Some(head1.hash().unwrap()));
    assert_eq!(heads.heads.len(), 1);
    assert_eq!(ManifestHead::from_bytes(&heads.heads[0]).unwrap(), head1);
    let heads: HeadsSince = h
        .cbor(
            "GET",
            &format!("/v1/vaults/{}/heads?since=1", local.id()),
            Some(token),
            &(),
        )
        .await
        .unwrap();
    assert!(heads.heads.is_empty());
    let items: ItemsSince = h
        .cbor(
            "GET",
            &format!("/v1/vaults/{}/items?since=0", local.id()),
            Some(token),
            &(),
        )
        .await
        .unwrap();
    assert_eq!(items.current_rev, 1);
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].rev, 1);
    // The body is what the device signed and pushed (its own `rev` field
    // is whatever the pusher had); the revision that counts rides beside it.
    let mut pulled = Item::from_bytes(&items.items[0].item).unwrap();
    assert_eq!(pulled.rev, 0);
    pulled.rev = items.items[0].rev;
    assert_eq!(pulled, item);
    let vaults: VaultList = h.cbor("GET", "/v1/vaults", Some(token), &()).await.unwrap();
    assert_eq!(vaults.vaults.len(), 1);
    assert_eq!(vaults.vaults[0].vault_id, local.id());
    assert_eq!(vaults.vaults[0].head_seq, 1);
    assert_eq!(vaults.vaults[0].item_rev, 1);

    // No identity manifest yet: an empty page, seq 0.
    let since: IdentitySince = h
        .cbor("GET", "/v1/identity?since=0", Some(token), &())
        .await
        .unwrap();
    assert_eq!(since, IdentitySince::default());

    // Another account sees none of it.
    let other = Device::new();
    h.register("you@example.com", "another long password", Id::new())
        .await;
    let theirs = h
        .login("you@example.com", "another long password", &other)
        .await
        .unwrap();
    let (status, body) = h
        .call(
            "GET",
            &format!("/v1/vaults/{}/heads", local.id()),
            Some(&theirs.token),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::Forbidden
    );
    let (status, _) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}", local.id()),
            Some(&theirs.token),
            local.record(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let vaults: VaultList = h
        .cbor("GET", "/v1/vaults", Some(&theirs.token), &())
        .await
        .unwrap();
    assert!(vaults.vaults.is_empty());
}

#[tokio::test]
async fn gap_fork_older_seq_and_unknown_device_are_distinct_refusals() {
    let h = harness(Config::default());
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let mut local = LocalVault::new(Device::new());
    let token = h
        .login(
            "me@example.com",
            "correct horse battery staple",
            &local.device,
        )
        .await
        .unwrap()
        .token;
    h.call(
        "PUT",
        &format!("/v1/vaults/{}", local.id()),
        Some(&token),
        local.record(),
    )
    .await;
    let mut item = local.new_item();
    item.rev = push(&h, &token, local.id(), &item, 0).await.unwrap();

    // First head must be seq 1 with no predecessor.
    let refused = put_head(&h, &token, &local.head(2, Hash32::ZERO, &[&item]))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Gap);
    let refused = put_head(&h, &token, &local.head(1, Hash32([7; 32]), &[&item]))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Fork);
    let head1 = local.head(1, Hash32::ZERO, &[&item]);
    put_head(&h, &token, &head1).await.unwrap();
    let h1 = head1.hash().unwrap();

    // Gap: seq 3 when the server is at 1.
    let refused = put_head(&h, &token, &local.head(3, h1, &[&item]))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Gap);
    assert_eq!(refused.current_seq, 1);
    assert_eq!(refused.current_hash, Some(h1));
    // Fork: seq 2 naming a predecessor that isn't the server's head.
    let refused = put_head(&h, &token, &local.head(2, Hash32([9; 32]), &[&item]))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Fork);
    // Older: seq 1 again, even if it is the very same head.
    let refused = put_head(&h, &token, &head1).await.unwrap_err();
    assert_eq!(refused.code, ErrorCode::OlderSeq);
    // Unknown device: a head signed by a key the account never logged in with.
    let stranger = LocalVault::new(Device::new());
    let mut forged = local.head(2, h1, &[&item]);
    forged.signer = stranger.device.id;
    forged.sign(&stranger.device.key()).unwrap();
    let (status, body) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/heads", local.id()),
            Some(&token),
            forged.to_bytes().unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::UnknownDevice
    );
    // Bad signature: the right device, a changed head.
    let mut tampered = local.head(2, h1, &[&item]);
    tampered.signed_at = Timestamp(tampered.signed_at.0 + 1);
    let (status, body) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/heads", local.id()),
            Some(&token),
            tampered.to_bytes().unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::BadSignature
    );
    // Wrong vault in the path.
    let (status, body) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/heads", Id::new()),
            Some(&token),
            local.head(2, h1, &[&item]).to_bytes().unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::WrongVault
    );
    // And the good seq 2 still goes through after all that.
    let accepted = put_head(&h, &token, &local.head(2, h1, &[&item]))
        .await
        .unwrap();
    assert_eq!(accepted.seq, 2);
}

#[tokio::test]
async fn a_stale_push_returns_the_merge_instruction() {
    let h = harness(Config::default());
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let mut local = LocalVault::new(Device::new());
    let token = h
        .login(
            "me@example.com",
            "correct horse battery staple",
            &local.device,
        )
        .await
        .unwrap()
        .token;
    h.call(
        "PUT",
        &format!("/v1/vaults/{}", local.id()),
        Some(&token),
        local.record(),
    )
    .await;
    let item = local.new_item();
    assert_eq!(push(&h, &token, local.id(), &item, 0).await.unwrap(), 1);
    // Another device's copy, still at "new": stale, and the server's copy
    // comes back so the client can merge.
    let (status, body) = push(&h, &token, local.id(), &item, 0).await.unwrap_err();
    assert_eq!(status, StatusCode::CONFLICT);
    match wire::decode::<PushReply>(&body).unwrap() {
        PushReply::Stale { current } => {
            assert_eq!(current.rev, 1);
            assert_eq!(
                Item::from_bytes(&current.item).unwrap().item_id,
                item.item_id
            );
        }
        other => panic!("expected stale, got {other:?}"),
    }
    // A stale push is a `PushReply` under status 409, tagged `outcome`.
    let as_value: ciborium::Value = ciborium::from_reader(&body[..]).expect("cbor");
    let map = as_value.as_map().expect("a map");
    assert!(
        map.iter()
            .any(|(k, v)| k.as_text() == Some("outcome") && v.as_text() == Some("stale"))
    );
    // With the right revision it goes through.
    let mut newer = item.clone();
    newer.rev = 1;
    assert_eq!(push(&h, &token, local.id(), &newer, 1).await.unwrap(), 2);
    // if_match for an item the server never saw is a client bug, not stale.
    let mut other = local.new_item();
    other.rev = 5;
    let (status, body) = push(&h, &token, local.id(), &other, 5).await.unwrap_err();
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "{}",
        String::from_utf8_lossy(&body)
    );
}

#[tokio::test]
async fn no_session_wrong_token_and_expiry_are_refused() {
    let h = harness(Config {
        session_ttl: Duration::from_secs(60),
        ..Config::default()
    });
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    for path in ["/v1/vaults", "/v1/identity"] {
        let (status, body) = h.call("GET", path, None, Vec::new()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            wire::decode::<ErrorBody>(&body).unwrap().code,
            ErrorCode::Unauthenticated
        );
    }
    let (status, body) = h
        .call("GET", "/v1/vaults", Some("lk1.not.real"), Vec::new())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::Unauthenticated
    );

    let device = Device::new();
    let session = h
        .login("me@example.com", "correct horse battery staple", &device)
        .await
        .unwrap();
    assert_eq!(session.expires_at, 1_700_000_000 + 60);
    let (status, _) = h
        .call("GET", "/v1/vaults", Some(&session.token), Vec::new())
        .await;
    assert_eq!(status, StatusCode::OK);
    h.clock.store(1_700_000_000 + 61, Ordering::SeqCst);
    let (status, body) = h
        .call("GET", "/v1/vaults", Some(&session.token), Vec::new())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::TokenExpired
    );
    // A fresh login works again.
    let again = h
        .login("me@example.com", "correct horse battery staple", &device)
        .await
        .unwrap();
    assert_ne!(again.token, session.token);
    let (status, _) = h
        .call("GET", "/v1/vaults", Some(&again.token), Vec::new())
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn wrong_password_unknown_login_and_a_strangers_key_fail_alike() {
    let h = harness(Config::default());
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let device = Device::new();
    let wrong = h
        .login("me@example.com", "wrong horse", &device)
        .await
        .unwrap_err();
    let unknown = h
        .login(
            "nobody@example.com",
            "correct horse battery staple",
            &device,
        )
        .await
        .unwrap_err();
    assert_eq!(wrong, (StatusCode::UNAUTHORIZED, ErrorCode::LoginFailed));
    assert_eq!(unknown, wrong);

    // A second device that re-uses the first device's id with another key
    // is refused; the same id with the same key is fine.
    let first = h
        .login("me@example.com", "correct horse battery staple", &device)
        .await
        .unwrap();
    let impostor = Device {
        id: device.id,
        seed: Device::new().seed,
    };
    let refused = h
        .login("me@example.com", "correct horse battery staple", &impostor)
        .await
        .unwrap_err();
    assert_eq!(refused, (StatusCode::FORBIDDEN, ErrorCode::Forbidden));
    let again = h
        .login("me@example.com", "correct horse battery staple", &device)
        .await
        .unwrap();
    assert_eq!(again.profile_id, first.profile_id);
    // A finish for a login id the server never issued is just "failed".
    let (status, body) = h
        .call(
            "POST",
            "/v1/account/login/finish",
            None,
            wire::encode(&LoginFinish {
                login_id: Id::new(),
                finalization: vec![0; 64],
                device: DeviceProofWire {
                    device_id: device.id,
                    device_pub: device.key().verifying_key().to_bytes(),
                    pop: vec![0; 64],
                },
            })
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wire::decode::<ErrorBody>(&body).unwrap().code,
        ErrorCode::LoginFailed
    );
}

#[tokio::test]
async fn the_brake_stops_a_guessing_run() {
    let h = harness(Config {
        per_login: (5, 0.0001),
        ..Config::default()
    });
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let device = Device::new();
    let mut seen_limit = false;
    for _ in 0..8 {
        match h.login("me@example.com", "wrong horse", &device).await {
            Err((StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited)) => {
                seen_limit = true;
                break;
            }
            Err((StatusCode::UNAUTHORIZED, ErrorCode::LoginFailed)) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(seen_limit, "the per-login bucket never emptied");
    // The header says when to come back.
    let (client, request) = LoginClient::start("wrong horse").unwrap();
    drop(client);
    let res = h
        .router
        .clone()
        .oneshot(
            Request::post("/v1/account/login/start")
                .header(header::CONTENT_TYPE, CONTENT_TYPE)
                .body(Body::from(
                    wire::encode(&LoginStart {
                        login: "me@example.com".into(),
                        request,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(res.headers().contains_key(header::RETRY_AFTER));
    // Another login name is unaffected.
    h.register("you@example.com", "another long password", Id::new())
        .await;
    assert!(
        h.login("you@example.com", "another long password", &device)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn closed_signups_refuse_registration_but_not_login() {
    let h = harness(Config {
        signups_open: false,
        ..Config::default()
    });
    let (client, request) = RegistrationClient::start("a long password").unwrap();
    drop(client);
    let err = h
        .cbor::<_, RegisterStartReply>(
            "POST",
            "/v1/account/register/start",
            None,
            &RegisterStart {
                login: "me@example.com".into(),
                request,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::FORBIDDEN);
    assert_eq!(err.1.code, ErrorCode::SignupsClosed);
    let (_, body) = h
        .call("GET", "/.well-known/lockdown", None, Vec::new())
        .await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["features"]["signups"], false);
}

/// `PUT /v1/identity` with the manifest and `records`.
async fn put_identity(
    h: &Harness,
    token: &str,
    set: &RecordSet,
    records: &[&str],
) -> Result<HeadAccepted, (StatusCode, ErrorCode, String)> {
    let push = IdentityPush {
        manifest: set.manifest.to_bytes().unwrap(),
        records: records
            .iter()
            .map(|p| RecordWire {
                path: (*p).to_string(),
                body: set.records[*p].clone(),
            })
            .collect(),
    };
    let (status, bytes) = h
        .call(
            "PUT",
            "/v1/identity",
            Some(token),
            wire::encode(&push).unwrap(),
        )
        .await;
    if status == StatusCode::OK {
        Ok(wire::decode(&bytes).unwrap())
    } else {
        let body: ErrorBody = wire::decode(&bytes).unwrap();
        Err((status, body.code, body.message))
    }
}

/// A `Device` record for a test device, signed by the identity's key, so
/// the identity manifest can list it the way a kit enrolment would.
fn device_record(
    identity: &Identity,
    device: &Device,
    name: &str,
    isk: &lockdown_crypto::SigningKey,
) -> lockdown_crypto::identity::Device {
    let mut record = identity.devices().next().unwrap().clone();
    record.device_id = device.id;
    record.device_pub = device.key().verifying_key().to_bytes();
    record.name = name.into();
    record.revoked_at = None;
    record.sign(isk.inner()).unwrap();
    record
}

fn set_with(
    identity: &Identity,
    devices: &[lockdown_crypto::identity::Device],
    prev: Option<&lockdown_vault::identity_set::IdentityManifest>,
    revocations: Vec<Revocation>,
    isk: &lockdown_crypto::SigningKey,
) -> RecordSet {
    let mut set = RecordSet::from_records(
        identity.profile(),
        identity.personas().cloned(),
        identity.devices().cloned().chain(devices.iter().cloned()),
        identity.envelopes().cloned(),
        identity.consents().cloned(),
        prev,
        revocations,
        identity.device_id(),
    )
    .unwrap();
    set.sign(isk).unwrap();
    set
}

#[tokio::test]
async fn the_identity_chain_is_ordered_pinned_and_revokes_devices() {
    let dir = tempfile::tempdir().unwrap();
    let keystore = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
    let identity =
        Identity::create(&dir.path().join("root"), "lockdown", "laptop", &keystore).unwrap();
    let isk = identity_signing_key(&identity, &keystore).unwrap();
    let h = harness(Config::default());
    h.register(
        "me@example.com",
        "correct horse battery staple",
        identity.profile().profile_id,
    )
    .await;
    let (a, b) = (Device::new(), Device::new());
    let token_a = h
        .login("me@example.com", "correct horse battery staple", &a)
        .await
        .unwrap()
        .token;
    let token_b = h
        .login("me@example.com", "correct horse battery staple", &b)
        .await
        .unwrap()
        .token;
    let record_a = device_record(&identity, &a, "laptop", &isk);
    let record_b = device_record(&identity, &b, "phone", &isk);

    // Page 1: the identity's own records plus A. Every record travels.
    let first = set_with(
        &identity,
        std::slice::from_ref(&record_a),
        None,
        Vec::new(),
        &isk,
    );
    let paths: Vec<&str> = first.records.keys().map(String::as_str).collect();
    // A manifest listing a record the server lacks is refused by name.
    let err = put_identity(&h, &token_a, &first, &paths[1..])
        .await
        .unwrap_err();
    assert_eq!(err.1, ErrorCode::UnknownRecord, "{}", err.2);
    let accepted = put_identity(&h, &token_a, &first, &paths).await.unwrap();
    assert_eq!(accepted.seq, 1);
    // The same page again: older_seq. Not the first page: gap.
    let err = put_identity(&h, &token_a, &first, &[]).await.unwrap_err();
    assert_eq!(err.1, ErrorCode::OlderSeq);
    let mut skipped = set_with(
        &identity,
        std::slice::from_ref(&record_a),
        Some(&first.manifest),
        Vec::new(),
        &isk,
    );
    skipped.manifest.seq = 5;
    skipped.sign(&isk).unwrap();
    let err = put_identity(&h, &token_a, &skipped, &[]).await.unwrap_err();
    assert_eq!(err.1, ErrorCode::Gap);
    // Another identity's key: refused, the chain is pinned to the first.
    let other_ks = FileKeystore::open_or_create(&dir.path().join("k2.cbor")).unwrap();
    let other = Identity::create(&dir.path().join("root2"), "lockdown", "x", &other_ks).unwrap();
    let other_isk = identity_signing_key(&other, &other_ks).unwrap();
    let mut forged = set_with(
        &identity,
        std::slice::from_ref(&record_a),
        Some(&first.manifest),
        Vec::new(),
        &isk,
    );
    forged.sign(&other_isk).unwrap();
    let err = put_identity(&h, &token_a, &forged, &[]).await.unwrap_err();
    assert_eq!(err.1, ErrorCode::BadSignature, "{}", err.2);

    // Page 2 adds B; only B's record needs to travel. A fork is refused.
    let second = set_with(
        &identity,
        &[record_a.clone(), record_b.clone()],
        Some(&first.manifest),
        Vec::new(),
        &isk,
    );
    let b_path = format!("devices/{}.cbor", b.id);
    let accepted = put_identity(&h, &token_a, &second, &[&b_path])
        .await
        .unwrap();
    assert_eq!(accepted.seq, 2);
    let mut fork = set_with(
        &identity,
        &[record_a.clone(), record_b.clone()],
        Some(&first.manifest),
        Vec::new(),
        &isk,
    );
    fork.manifest.seq = 3;
    fork.sign(&isk).unwrap();
    let err = put_identity(&h, &token_a, &fork, &[]).await.unwrap_err();
    assert_eq!(err.1, ErrorCode::Fork);
    // Pulling from 1 gives page 2 and the records it lists.
    let since: IdentitySince = h
        .cbor("GET", "/v1/identity?since=1", Some(&token_b), &())
        .await
        .unwrap();
    assert_eq!(since.manifests.len(), 1);
    assert_eq!(since.current_seq, 2);
    assert_eq!(since.records.len(), second.records.len());
    let pulled = RecordSet {
        manifest: lockdown_vault::identity_set::IdentityManifest::from_bytes(&since.manifests[0])
            .unwrap(),
        records: since
            .records
            .into_iter()
            .map(|r| (r.path, r.body))
            .collect(),
    };
    let verified = pulled.verify(Some(&identity.profile().isk_pub)).unwrap();
    assert_eq!(verified.devices.len(), 3);

    // B makes the vault and writes head 1. Then A revokes B.
    let mut local = LocalVault::new(b);
    let vault_id = local.id();
    let (status, _) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{vault_id}"),
            Some(&token_b),
            local.record(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let mut item = local.new_item();
    item.rev = push(&h, &token_b, vault_id, &item, 0).await.unwrap();
    let head1 = local.head(1, Hash32::ZERO, &[&item]);
    let accepted = put_head(&h, &token_b, &head1).await.unwrap();
    let b_id = local.device.id;

    let mut revoked_b = record_b.clone();
    revoked_b.revoked_at = Some(Timestamp::now());
    revoked_b.sign(isk.inner()).unwrap();
    let revoke = |boundary: u64, prev: &lockdown_vault::identity_set::IdentityManifest| {
        set_with(
            &identity,
            &[record_a.clone(), revoked_b.clone()],
            Some(prev),
            vec![Revocation {
                device_id: b_id,
                vaults: vec![VaultBoundary {
                    vault_id,
                    seq: boundary,
                }],
            }],
            &isk,
        )
    };
    // A boundary that isn't the server's head: refused, with the head.
    let stale = revoke(0, &second.manifest);
    let err = put_identity(&h, &token_a, &stale, &[&b_path])
        .await
        .unwrap_err();
    assert_eq!(err.1, ErrorCode::BoundaryMoved, "{}", err.2);
    // B can't write the manifest that revokes it.
    let mut by_b = revoke(1, &second.manifest);
    by_b.manifest.written_by = b_id;
    by_b.sign(&isk).unwrap();
    let err = put_identity(&h, &token_a, &by_b, &[&b_path])
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST, "{}", err.2);
    let third = revoke(1, &second.manifest);
    let accepted3 = put_identity(&h, &token_a, &third, &[&b_path])
        .await
        .unwrap();
    assert_eq!(accepted3.seq, 3);

    // B's next head, item push and identity manifest are refused as a
    // revoked device; its head 1 is still served.
    let head2 = local.head(2, accepted.hash, &[&item]);
    let (status, bytes) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{vault_id}/heads"),
            Some(&token_b),
            head2.to_bytes().unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        wire::decode::<ErrorBody>(&bytes).unwrap().code,
        ErrorCode::RevokedDevice
    );
    let (status, bytes) = push(&h, &token_b, vault_id, &item, item.rev)
        .await
        .unwrap_err();
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        wire::decode::<ErrorBody>(&bytes).unwrap().code,
        ErrorCode::RevokedDevice
    );
    let mut undo = set_with(
        &identity,
        &[record_a.clone(), record_b.clone()],
        Some(&third.manifest),
        Vec::new(),
        &isk,
    );
    undo.manifest.written_by = b_id;
    undo.sign(&isk).unwrap();
    let err = put_identity(&h, &token_b, &undo, &[&b_path])
        .await
        .unwrap_err();
    assert_eq!(err.1, ErrorCode::RevokedDevice, "{}", err.2);
    // Nor may anyone else undo it.
    let err = put_identity(
        &h,
        &token_a,
        &set_with(
            &identity,
            &[record_a.clone(), record_b.clone()],
            Some(&third.manifest),
            Vec::new(),
            &isk,
        ),
        &[&b_path],
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST, "{}", err.2);
    assert!(err.2.contains("can't be undone"), "{}", err.2);
    let heads: HeadsSince = h
        .cbor(
            "GET",
            &format!("/v1/vaults/{vault_id}/heads?since=0"),
            Some(&token_a),
            &(),
        )
        .await
        .unwrap();
    assert_eq!(heads.heads.len(), 1, "B's history before revocation stands");

    // A still writes: head 2 over the same item, signed by A.
    let mut by_a = head2.clone();
    by_a.signer = a.id;
    by_a.sign(&a.key()).unwrap();
    let accepted = put_head(&h, &token_a, &by_a).await.unwrap();
    assert_eq!(accepted.seq, 2);
}

#[tokio::test]
async fn attachments_count_against_the_vaults_owner_up_to_the_quota() {
    let h = harness(Config {
        attachment_quota: Some(10_000),
        ..Config::default()
    });
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let local = LocalVault::new(Device::new());
    let token = h
        .login(
            "me@example.com",
            "correct horse battery staple",
            &local.device,
        )
        .await
        .unwrap()
        .token;
    h.call(
        "PUT",
        &format!("/v1/vaults/{}", local.id()),
        Some(&token),
        local.record(),
    )
    .await;
    let storage = || async {
        let (status, body) = h
            .call("GET", "/v1/account/storage", Some(&token), Vec::new())
            .await;
        assert_eq!(status, StatusCode::OK);
        wire::decode::<wire::StorageUsage>(&body).unwrap()
    };
    let start = storage().await;
    assert_eq!((start.v, start.used, start.quota), (1, 0, Some(10_000)));
    let put = |bytes: Vec<u8>| async {
        let name = lockdown_crypto::hash::hash(&bytes).to_hex();
        h.call(
            "PUT",
            &format!("/v1/vaults/{}/blobs/{name}", local.id()),
            Some(&token),
            bytes,
        )
        .await
    };
    let six = vec![6u8; 6_000];
    assert_eq!(put(six.clone()).await.0, StatusCode::CREATED);
    assert_eq!(storage().await.used, 6_000);
    // The same blob again costs nothing and is still accepted.
    assert_eq!(put(six).await.0, StatusCode::OK);
    // Over the room left: refused in words, nothing stored.
    let (status, body) = put(vec![5u8; 5_000]).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let err = wire::decode::<ErrorBody>(&body).unwrap();
    assert_eq!(err.code, ErrorCode::QuotaExceeded);
    assert!(
        err.message.contains("your account has used 6 kB of 10 kB"),
        "{}",
        err.message
    );
    assert_eq!(storage().await.used, 6_000);
    // What fits still goes in.
    assert_eq!(put(vec![4u8; 4_000]).await.0, StatusCode::CREATED);
    assert_eq!(storage().await.used, 10_000);
    // No session, no answer.
    let (status, _) = h.call("GET", "/v1/account/storage", None, Vec::new()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_server_without_a_quota_says_so() {
    let h = harness(Config {
        attachment_quota: None,
        ..Config::default()
    });
    h.register("me@example.com", "correct horse battery staple", Id::new())
        .await;
    let device = Device::new();
    let token = h
        .login("me@example.com", "correct horse battery staple", &device)
        .await
        .unwrap()
        .token;
    let (_, body) = h
        .call("GET", "/v1/account/storage", Some(&token), Vec::new())
        .await;
    let usage = wire::decode::<wire::StorageUsage>(&body).unwrap();
    assert_eq!((usage.used, usage.quota), (0, None));
    assert_eq!(
        Config::default().attachment_quota,
        Some(lockdown_server::DEFAULT_ATTACHMENT_QUOTA)
    );
    assert_eq!(lockdown_server::DEFAULT_ATTACHMENT_QUOTA, 5_000_000_000);
}
