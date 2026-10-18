//! The sharing endpoints, in process (`docs/plan/18-sharing.md` §4):
//! shared-vault access by right, the membership chain and its key
//! generations, items shared on their own, share links, request links,
//! emergency access and the pages behind the links.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lockdown_crypto::SigningKey;
use lockdown_crypto::auth::{LoginClient, RegistrationClient, device_pop_message};
use lockdown_crypto::identity::kem::{DEFAULT_KEM, KemPublicKey, KemSecretKey, kem};
use lockdown_crypto::identity::{AppDataKey, SymmetricKey};
use lockdown_server::{AppState, Config, app};
use lockdown_sync::kt::AccountEntries;
use lockdown_sync::sharing::{
    EmergencyCreate, EmergencyList, EmergencyRelease, EmergencyState, EmergencyStepPush,
    EmergencyWire, EnvelopeWire, ItemShareCreate, MembershipAccepted, MembershipPush,
    ReceivedItems, RequestAnswer, RequestCreate, RequestKey, RequestList, SendCreate, SendMeta,
    SendOpen, SendOpened, SharedVaults,
};
use lockdown_sync::wire::{
    self, CONTENT_TYPE, DeviceProofWire, ErrorBody, ErrorCode, HeadAccepted, HeadRefused,
    HeadsSince, ItemPush, LoginFinish, LoginFinishReply, LoginStart, LoginStartReply, PushReply,
    RegisterFinish, RegisterStart, RegisterStartReply,
};
use lockdown_vault::keys::VaultKey;
use lockdown_vault::send::{SendPayload, seal_send, send_access};
use lockdown_vault::sharing::{
    Change, EmergencyAction, EmergencyGrant, EmergencyStep, MembershipEvent, Party, Right, Roster,
    ShareEnvelope, SharePlace, SharePurpose, VaultKind,
};
use lockdown_vault::test_support::{sample_context, sample_login_fields};
use lockdown_vault::{Id, Item, ItemClass, ItemKind, Vault};
use serde_bytes::ByteBuf;
use tower::ServiceExt;

struct Harness {
    router: Router,
    clock: Arc<AtomicI64>,
    state: Arc<AppState>,
}

fn harness() -> Harness {
    let clock = Arc::new(AtomicI64::new(1_700_000_000));
    // Sessions outlive the emergency test's waiting periods.
    let config = Config {
        session_ttl: std::time::Duration::from_secs(60 * 86_400),
        ..Config::default()
    };
    let mut state = AppState::in_memory(config).unwrap();
    let c = clock.clone();
    state.clock = Box::new(move || c.load(Ordering::SeqCst));
    let state = Arc::new(state);
    Harness {
        router: app(state.clone()),
        clock,
        state,
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
    ) -> Result<R, (StatusCode, ErrorCode)> {
        let (status, bytes) = self
            .call(method, path, token, wire::encode(body).unwrap())
            .await;
        if status.is_success() {
            Ok(wire::decode(&bytes).unwrap_or_else(|e| panic!("decode {path}: {e}")))
        } else {
            let body: ErrorBody = wire::decode(&bytes).expect("an error body");
            Err((status, body.code))
        }
    }

    async fn status<T: serde::Serialize>(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> StatusCode {
        self.call(method, path, token, wire::encode(body).unwrap())
            .await
            .0
    }

    async fn get<R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
    ) -> Result<R, (StatusCode, ErrorCode)> {
        let (status, bytes) = self.call("GET", path, token, Vec::new()).await;
        if status.is_success() {
            Ok(wire::decode(&bytes).unwrap())
        } else {
            let body: ErrorBody = wire::decode(&bytes).expect("an error body");
            Err((status, body.code))
        }
    }

    /// Registers `login`, logs in with a fresh device and returns the person.
    async fn person(&self, login: &str) -> Person {
        let password = "correct horse battery staple";
        let profile = Id::new();
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
        let (upload, _) = client.finish(password, &reply.response).unwrap();
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
        assert_eq!(status, StatusCode::CREATED);
        let key = SigningKey::generate().unwrap();
        let device = Id::new();
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
            .unwrap();
        let finished = client.finish(password, &started.response).unwrap();
        let pop = key.sign(&device_pop_message(login, &started.login_id));
        let reply: LoginFinishReply = self
            .cbor(
                "POST",
                "/v1/account/login/finish",
                None,
                &LoginFinish {
                    login_id: started.login_id,
                    finalization: finished.finalization,
                    device: DeviceProofWire {
                        device_id: device,
                        device_pub: key.verifying_key().to_bytes(),
                        pop: pop.to_vec(),
                    },
                },
            )
            .await
            .unwrap();
        let entries: AccountEntries = self
            .get("/v1/kt/entries?from=0", Some(&reply.token))
            .await
            .unwrap();
        let (kem_sk, kem_pk) = kem(DEFAULT_KEM)
            .keypair(lockdown_crypto::rand::random_32().unwrap())
            .unwrap();
        Person {
            token: reply.token,
            party: Party {
                account_id: entries.account_id,
                profile_id: profile,
                persona_id: Id::new(),
            },
            device,
            key,
            kem_sk,
            kem_pk,
        }
    }
}

struct Person {
    token: String,
    party: Party,
    device: Id,
    key: SigningKey,
    kem_sk: KemSecretKey,
    kem_pk: KemPublicKey,
}

impl Person {
    fn t(&self) -> Option<&str> {
        Some(&self.token)
    }

    fn signer(&self) -> Box<SigningKey> {
        Box::new(SigningKey::from_bytes(self.key.inner().to_bytes()))
    }
}

/// The owner's vault: made, recorded and first head pushed.
async fn owner_vault(h: &Harness, owner: &mut Person) -> (Vault, Party) {
    let ctx = sample_context();
    owner.party.profile_id = ctx.profile_id;
    owner.party.persona_id = ctx.persona_id;
    let mut vault = Vault::create(ctx, owner.device, owner.signer(), None).unwrap();
    let adk = AppDataKey::generate().unwrap();
    let record = vault.record(&adk).unwrap().to_bytes().unwrap();
    let (status, _) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}", ctx.vault_id),
            owner.t(),
            record,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = vault
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let item = vault.item(id).unwrap().clone();
    let rev = push(h, owner, &item, 0).await.unwrap();
    vault.set_item_rev(id, rev).unwrap();
    vault.truncate_heads_after(0);
    vault.commit().unwrap();
    put_head(h, owner, &vault).await.unwrap();
    let party = owner.party;
    (vault, party)
}

async fn push(h: &Harness, who: &Person, item: &Item, if_match: u64) -> Result<u64, ErrorCode> {
    let (status, bytes) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/items/{}", item.vault_id, item.item_id),
            who.t(),
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
        Err(wire::decode::<ErrorBody>(&bytes).unwrap().code)
    }
}

async fn put_head(h: &Harness, who: &Person, vault: &Vault) -> Result<HeadAccepted, ErrorCode> {
    let head = vault.head().unwrap();
    let (status, bytes) = h
        .call(
            "PUT",
            &format!("/v1/vaults/{}/heads", head.vault_id),
            who.t(),
            head.to_bytes().unwrap(),
        )
        .await;
    if status == StatusCode::OK {
        Ok(wire::decode(&bytes).unwrap())
    } else {
        match wire::decode::<HeadRefused>(&bytes) {
            Ok(r) => Err(r.code),
            Err(_) => Err(wire::decode::<ErrorBody>(&bytes).unwrap().code),
        }
    }
}

fn event(
    roster: Option<&Roster>,
    vault: &Vault,
    change: Change,
    key_ref: Option<Id>,
    by: &Person,
) -> MembershipEvent {
    let mut e = match roster {
        None => {
            let Change::Create { owner, kind } = change else {
                panic!("the first event creates");
            };
            Roster::genesis(
                vault.context().vault_id,
                owner,
                kind,
                vault.vault_key_id(),
                by.device,
            )
        }
        Some(r) => r.propose(change, key_ref, by.party.account_id, by.device),
    };
    e.sign(&by.key).unwrap();
    e
}

async fn post_event(
    h: &Harness,
    by: &Person,
    e: &MembershipEvent,
    envelopes: Vec<ShareEnvelope>,
) -> Result<MembershipAccepted, (StatusCode, ErrorCode)> {
    h.cbor(
        "POST",
        &format!("/v1/vaults/{}/members", e.vault_id),
        by.t(),
        &MembershipPush {
            event: ByteBuf::from(e.to_bytes().unwrap()),
            envelopes: envelopes
                .iter()
                .map(|e| ByteBuf::from(e.to_bytes().unwrap()))
                .collect(),
        },
    )
    .await
}

/// A member's own copy of a shared vault: the owner's context, the Vault
/// Key from the member's envelope.
async fn member_vault(h: &Harness, member: &Person, owner: &Vault, from: Id) -> Vault {
    let env: EnvelopeWire = h
        .get(
            &format!("/v1/vaults/{}/key", owner.context().vault_id),
            member.t(),
        )
        .await
        .unwrap();
    let env = ShareEnvelope::from_bytes(&env.envelope).unwrap();
    let place = env.place;
    assert_eq!(place.from_account, from);
    let key: VaultKey = env
        .open(&member.kem_sk, &place, &owner.verifying_key())
        .unwrap();
    let mut v = Vault::create(*owner.context(), member.device, member.signer(), None).unwrap();
    v.adopt_vault_key(place.key_id, key);
    v
}

#[tokio::test]
async fn a_shared_vault_is_read_and_written_by_its_people_by_right() {
    let h = harness();
    let mut alice = h.person("alice@example.com").await;
    let bob = h.person("bob@example.com").await;
    let carol = h.person("carol@example.com").await;
    let dave = h.person("dave@example.com").await;
    let (mut vault, owner) = owner_vault(&h, &mut alice).await;
    let vid = vault.context().vault_id;

    // Before sharing, nobody else reads it.
    assert_eq!(
        h.get::<HeadsSince>(&format!("/v1/vaults/{vid}/heads"), bob.t())
            .await
            .unwrap_err()
            .1,
        ErrorCode::Forbidden
    );
    // Shared by its owner as the persona that holds it.
    let genesis = event(
        None,
        &vault,
        Change::Create {
            owner,
            kind: VaultKind::Shared,
        },
        None,
        &alice,
    );
    post_event(&h, &alice, &genesis, Vec::new()).await.unwrap();
    let mut roster = Roster::replay_trusted(std::slice::from_ref(&genesis)).unwrap();
    // Bob may view: his envelope comes with the change, or it is refused.
    let add_bob = event(
        Some(&roster),
        &vault,
        Change::Add {
            member: bob.party,
            right: Right::View,
        },
        None,
        &alice,
    );
    assert_eq!(
        post_event(&h, &alice, &add_bob, Vec::new())
            .await
            .unwrap_err()
            .0,
        StatusCode::BAD_REQUEST
    );
    let bob_env = vault
        .seal_vault_key(
            bob.party,
            &bob.kem_pk,
            owner.account_id,
            SharePurpose::Vault,
            None,
        )
        .unwrap();
    post_event(&h, &alice, &add_bob, vec![bob_env])
        .await
        .unwrap();
    roster.apply_trusted(&add_bob).unwrap();
    let add_carol = event(
        Some(&roster),
        &vault,
        Change::Add {
            member: carol.party,
            right: Right::Edit,
        },
        None,
        &alice,
    );
    let carol_env = vault
        .seal_vault_key(
            carol.party,
            &carol.kem_pk,
            owner.account_id,
            SharePurpose::Vault,
            None,
        )
        .unwrap();
    post_event(&h, &alice, &add_carol, vec![carol_env])
        .await
        .unwrap();
    roster.apply_trusted(&add_carol).unwrap();

    // Bob reads and lists it as shared with him, with his right.
    let heads: HeadsSince = h
        .get(&format!("/v1/vaults/{vid}/heads"), bob.t())
        .await
        .unwrap();
    assert_eq!(heads.current_seq, 1);
    let shared: SharedVaults = h.get("/v1/shared", bob.t()).await.unwrap();
    assert_eq!(shared.vaults.len(), 1);
    assert_eq!(shared.vaults[0].right, Right::View);
    assert_eq!(shared.vaults[0].owner, owner);
    assert_eq!(shared.vaults[0].owner_login, "alice@example.com");
    let alice_view: SharedVaults = h.get("/v1/shared", alice.t()).await.unwrap();
    assert_eq!(alice_view.sharing.len(), 1);
    // Dave still can't.
    assert_eq!(
        h.get::<HeadsSince>(&format!("/v1/vaults/{vid}/heads"), dave.t())
            .await
            .unwrap_err()
            .1,
        ErrorCode::Forbidden
    );

    // Bob opens his key but can't write; Carol can.
    let mut bob_copy = member_vault(&h, &bob, &vault, owner.account_id).await;
    let bob_item = bob_copy
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let bob_item = bob_copy.item(bob_item).unwrap().clone();
    assert_eq!(
        push(&h, &bob, &bob_item, 0).await.unwrap_err(),
        ErrorCode::Forbidden
    );
    let mut carol_copy = member_vault(&h, &carol, &vault, owner.account_id).await;
    let carol_item = carol_copy
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let carol_item = carol_copy.item(carol_item).unwrap().clone();
    let rev = push(&h, &carol, &carol_item, 0).await.unwrap();
    assert!(rev > 1);
    // Bob can't manage the membership either.
    let by_bob = event(
        Some(&roster),
        &vault,
        Change::Add {
            member: dave.party,
            right: Right::View,
        },
        None,
        &bob,
    );
    assert_eq!(
        post_event(&h, &bob, &by_bob, Vec::new())
            .await
            .unwrap_err()
            .1,
        ErrorCode::Fork
    );

    // Carol leaves: the change names a new generation, and Bob's envelope at it.
    let new_ref = vault.rotate_vault_key().unwrap();
    let remove = event(
        Some(&roster),
        &vault,
        Change::Remove {
            account_id: carol.party.account_id,
        },
        Some(new_ref),
        &alice,
    );
    let bob_env2 = vault
        .seal_vault_key(
            bob.party,
            &bob.kem_pk,
            owner.account_id,
            SharePurpose::Vault,
            None,
        )
        .unwrap();
    post_event(&h, &alice, &remove, vec![bob_env2])
        .await
        .unwrap();
    assert_eq!(
        h.get::<HeadsSince>(&format!("/v1/vaults/{vid}/heads"), carol.t())
            .await
            .unwrap_err()
            .1,
        ErrorCode::Forbidden
    );
    // Carol, removed, can't write any more either.
    let mut carol_copy = carol_copy;
    let late = carol_copy
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let late = carol_copy.item(late).unwrap().clone();
    assert_eq!(
        push(&h, &carol, &late, 0).await.unwrap_err(),
        ErrorCode::Forbidden
    );
    // Alice's own rotated items go through; a head under the new key too.
    let first = vault.item_ids()[0];
    let item = vault.item(first).unwrap().clone();
    let rev = push(&h, &alice, &item, item.rev).await.unwrap();
    vault.set_item_rev(first, rev).unwrap();
    vault.truncate_heads_after(1);
    vault.commit().unwrap();
    assert_eq!(vault.head().unwrap().vault_key_ref, new_ref);
    put_head(&h, &alice, &vault).await.unwrap();
    // Bob's envelope is at the new generation.
    drop(bob_copy);
    let env: EnvelopeWire = h
        .get(&format!("/v1/vaults/{vid}/key"), bob.t())
        .await
        .unwrap();
    assert_eq!(
        ShareEnvelope::from_bytes(&env.envelope)
            .unwrap()
            .place
            .key_id,
        new_ref
    );
}

#[tokio::test]
async fn a_head_or_item_under_an_old_generation_is_refused_as_key_rotated() {
    let h = harness();
    let mut alice = h.person("alice@example.com").await;
    let bob = h.person("bob@example.com").await;
    let (mut vault, owner) = owner_vault(&h, &mut alice).await;
    let genesis = event(
        None,
        &vault,
        Change::Create {
            owner,
            kind: VaultKind::Family,
        },
        None,
        &alice,
    );
    post_event(&h, &alice, &genesis, Vec::new()).await.unwrap();
    let mut roster = Roster::replay_trusted(&[genesis]).unwrap();
    let add = event(
        Some(&roster),
        &vault,
        Change::Add {
            member: bob.party,
            right: Right::Edit,
        },
        None,
        &alice,
    );
    let env = vault
        .seal_vault_key(
            bob.party,
            &bob.kem_pk,
            owner.account_id,
            SharePurpose::Vault,
            None,
        )
        .unwrap();
    post_event(&h, &alice, &add, vec![env]).await.unwrap();
    roster.apply_trusted(&add).unwrap();
    let mut bob_copy = member_vault(&h, &bob, &vault, owner.account_id).await;

    // Bob edits under the old generation; then Alice changes his right
    // without rotating, which keeps it valid; then removes him.
    let id = bob_copy
        .create_item(ItemKind::Login, ItemClass::Standard, sample_login_fields())
        .unwrap();
    let item = bob_copy.item(id).unwrap().clone();
    push(&h, &bob, &item, 0).await.unwrap();
    let demote = event(
        Some(&roster),
        &vault,
        Change::SetRight {
            account_id: bob.party.account_id,
            right: Right::View,
        },
        None,
        &alice,
    );
    post_event(&h, &alice, &demote, Vec::new()).await.unwrap();
    roster.apply_trusted(&demote).unwrap();
    assert_eq!(
        push(&h, &bob, &item, 2).await.unwrap_err(),
        ErrorCode::Forbidden
    );

    // Alice's own item under the old generation after a rotation she
    // hasn't finished: key_rotated.
    let stale_item = vault.item(vault.item_ids()[0]).unwrap().clone();
    let new_ref = Id::new();
    let remove = event(
        Some(&roster),
        &vault,
        Change::Remove {
            account_id: bob.party.account_id,
        },
        Some(new_ref),
        &alice,
    );
    post_event(&h, &alice, &remove, Vec::new()).await.unwrap();
    assert_eq!(
        push(&h, &alice, &stale_item, stale_item.rev)
            .await
            .unwrap_err(),
        ErrorCode::KeyRotated
    );
    vault.truncate_heads_after(1);
    vault.commit().unwrap();
    assert_eq!(
        put_head(&h, &alice, &vault).await.unwrap_err(),
        ErrorCode::KeyRotated
    );
}

#[tokio::test]
async fn items_shared_on_their_own_reach_only_their_recipient() {
    let h = harness();
    let mut alice = h.person("alice@example.com").await;
    let bob = h.person("bob@example.com").await;
    let carol = h.person("carol@example.com").await;
    let (vault, owner) = owner_vault(&h, &mut alice).await;
    let item_id = vault.item_ids()[0];
    let share_id = Id::new();
    let envelope = vault
        .seal_item_key(item_id, bob.party, &bob.kem_pk, owner.account_id, share_id)
        .unwrap();
    let create = ItemShareCreate {
        share_id,
        vault_id: vault.context().vault_id,
        item_id,
        recipient: bob.party,
        right: Right::FillOnly,
        envelope: ByteBuf::from(envelope.to_bytes().unwrap()),
    };
    // Bob can't share Alice's item; an edit right isn't offered.
    assert_eq!(
        h.status("POST", "/v1/shares/items", bob.t(), &create).await,
        StatusCode::FORBIDDEN
    );
    let mut edit = create.clone();
    edit.right = Right::Edit;
    assert_eq!(
        h.status("POST", "/v1/shares/items", alice.t(), &edit).await,
        StatusCode::BAD_REQUEST
    );
    // An envelope for someone else is refused.
    let mut wrong = create.clone();
    wrong.recipient = carol.party;
    assert_eq!(
        h.status("POST", "/v1/shares/items", alice.t(), &wrong)
            .await,
        StatusCode::BAD_REQUEST
    );
    let mut heard = h.state.live.subscribe();
    assert_eq!(
        h.status("POST", "/v1/shares/items", alice.t(), &create)
            .await,
        StatusCode::NO_CONTENT
    );
    // Bob hears of it on his live channel at once (self-host-push).
    let pinged = std::iter::from_fn(|| heard.try_recv().ok())
        .any(|c| c.account_id == bob.party.account_id && c.kind == wire::LiveKind::Sync);
    assert!(pinged, "the recipient's devices are told to sync");
    // And of every change Alice makes to the item.
    let current = vault.item(item_id).unwrap().clone();
    push(&h, &alice, &current, current.rev).await.unwrap();
    let pinged = std::iter::from_fn(|| heard.try_recv().ok())
        .any(|c| c.account_id == bob.party.account_id && c.kind == wire::LiveKind::Sync);
    assert!(pinged, "an edit of a shared item reaches its recipient");
    assert!(
        !std::iter::from_fn(|| heard.try_recv().ok())
            .any(|c| c.account_id == carol.party.account_id),
        "nobody else hears"
    );

    let got: ReceivedItems = h.get("/v1/shares/items", bob.t()).await.unwrap();
    assert_eq!(got.items.len(), 1);
    assert_eq!(got.items[0].right, Right::FillOnly);
    let env = ShareEnvelope::from_bytes(&got.items[0].envelope).unwrap();
    let key: lockdown_vault::keys::ItemKey = env
        .open(&bob.kem_sk, &env.place, &vault.verifying_key())
        .unwrap();
    let item = Item::from_bytes(&got.items[0].item.item).unwrap();
    let mut trusted = std::collections::BTreeMap::new();
    trusted.insert(vault.device_id(), vault.verifying_key());
    let held = Vault::received(
        *vault.context(),
        bob.device,
        bob.signer(),
        &trusted,
        vec![(item, key)],
    )
    .unwrap();
    assert!(held.open_item(item_id).is_ok());
    let none: ReceivedItems = h.get("/v1/shares/items", carol.t()).await.unwrap();
    assert!(none.items.is_empty());
    // A share of one item opens nothing else of the vault.
    assert_eq!(
        h.get::<HeadsSince>(
            &format!("/v1/vaults/{}/heads", vault.context().vault_id),
            bob.t()
        )
        .await
        .unwrap_err()
        .1,
        ErrorCode::Forbidden
    );
    // Withdrawn by Alice: gone for Bob.
    let (status, _) = h
        .call(
            "DELETE",
            &format!("/v1/shares/items/{share_id}"),
            carol.t(),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = h
        .call(
            "DELETE",
            &format!("/v1/shares/items/{share_id}"),
            alice.t(),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        std::iter::from_fn(|| heard.try_recv().ok()).any(|c| c.account_id == bob.party.account_id),
        "a withdrawn share reaches its recipient too"
    );
    let got: ReceivedItems = h.get("/v1/shares/items", bob.t()).await.unwrap();
    assert!(got.items.is_empty());
}

#[tokio::test]
async fn share_links_expire_count_their_views_and_check_their_password() {
    let h = harness();
    let alice = h.person("alice@example.com").await;
    let now = h.clock.load(Ordering::SeqCst);
    let payload = SendPayload::text("Door", "4711");
    let (secret, sealed) = seal_send(&payload, Some(b"blue")).unwrap();
    let create = SendCreate {
        send_id: sealed.send_id,
        ciphertext: ByteBuf::from(sealed.ciphertext.clone()),
        expires_at: now + 3_600,
        max_views: Some(2),
        password: sealed.password.clone(),
    };
    let mut too_long = create.clone();
    too_long.expires_at = now + 31 * 86_400;
    assert_eq!(
        h.status("POST", "/v1/sends", alice.t(), &too_long).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.status("POST", "/v1/sends", None, &create).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.status("POST", "/v1/sends", alice.t(), &create).await,
        StatusCode::CREATED
    );

    let id = sealed.send_id;
    let meta: SendMeta = h.get(&format!("/v1/sends/{id}/meta"), None).await.unwrap();
    assert_eq!(meta.views_left, Some(2));
    let pw = meta.password.unwrap();
    assert_eq!(pw.salt, sealed.password.as_ref().unwrap().salt);
    // No proof, or a wrong one: refused, and no view is used.
    let refused = h
        .cbor::<_, SendOpened>(
            "POST",
            &format!("/v1/sends/{id}/open"),
            None,
            &SendOpen::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.0, StatusCode::FORBIDDEN);
    let (wrong, _) = send_access(id, b"red", sealed.password.as_ref().unwrap()).unwrap();
    let refused = h
        .cbor::<_, SendOpened>(
            "POST",
            &format!("/v1/sends/{id}/open"),
            None,
            &SendOpen {
                access: Some(ByteBuf::from(wrong.to_vec())),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(refused.0, StatusCode::FORBIDDEN);
    let (proof, pk) = send_access(id, b"blue", sealed.password.as_ref().unwrap()).unwrap();
    let open = SendOpen {
        access: Some(ByteBuf::from(proof.to_vec())),
    };
    for _ in 0..2 {
        let opened: SendOpened = h
            .cbor("POST", &format!("/v1/sends/{id}/open"), None, &open)
            .await
            .unwrap();
        let back =
            lockdown_vault::send::open_send(id, &secret, Some(&pk), &opened.ciphertext).unwrap();
        assert_eq!(back, payload);
    }
    // Used up.
    assert_eq!(
        h.cbor::<_, SendOpened>("POST", &format!("/v1/sends/{id}/open"), None, &open)
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );

    // Another link expires by the clock, and its owner can delete it.
    let (_, sealed) = seal_send(&payload, None).unwrap();
    let mut create = create.clone();
    create.send_id = sealed.send_id;
    create.password = None;
    create.max_views = None;
    create.ciphertext = ByteBuf::from(sealed.ciphertext);
    assert_eq!(
        h.status("POST", "/v1/sends", alice.t(), &create).await,
        StatusCode::CREATED
    );
    h.clock.store(now + 3_601, Ordering::SeqCst);
    assert_eq!(
        h.get::<SendMeta>(&format!("/v1/sends/{}/meta", create.send_id), None)
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_request_link_takes_one_answer_for_its_asker() {
    let h = harness();
    let alice = h.person("alice@example.com").await;
    let bob = h.person("bob@example.com").await;
    let now = h.clock.load(Ordering::SeqCst);
    let request_id = Id::new();
    let create = RequestCreate {
        request_id,
        kem_pub: alice.kem_pk.clone(),
        profile_id: alice.party.profile_id,
        persona_id: alice.party.persona_id,
        expires_at: now + 86_400,
    };
    assert_eq!(
        h.status("POST", "/v1/requests", alice.t(), &create).await,
        StatusCode::CREATED
    );
    let key: RequestKey = h
        .get(&format!("/v1/requests/{request_id}/key"), None)
        .await
        .unwrap();
    assert!(key.open);
    assert_eq!(key.kem_pub, alice.kem_pk);
    let link = lockdown_vault::send::parse_request_link(
        &lockdown_vault::send::request_link("http://x", request_id, &alice.kem_pk, "wifi").unwrap(),
    )
    .unwrap();
    let sealed = lockdown_vault::send::seal_response(
        &link,
        &key.kem_pub,
        key.profile_id,
        key.persona_id,
        &SendPayload::text("Wi-Fi", "hunter2"),
    )
    .unwrap();
    let answer = RequestAnswer {
        answer: ByteBuf::from(sealed),
    };
    assert_eq!(
        h.status(
            "POST",
            &format!("/v1/requests/{request_id}/answer"),
            None,
            &answer
        )
        .await,
        StatusCode::NO_CONTENT
    );
    // Only once.
    assert_eq!(
        h.status(
            "POST",
            &format!("/v1/requests/{request_id}/answer"),
            None,
            &answer
        )
        .await,
        StatusCode::NOT_FOUND
    );
    let key: RequestKey = h
        .get(&format!("/v1/requests/{request_id}/key"), None)
        .await
        .unwrap();
    assert!(!key.open);
    let list: RequestList = h.get("/v1/requests", alice.t()).await.unwrap();
    let got = list.requests[0].answer.as_ref().unwrap();
    let opened = lockdown_vault::send::open_response(
        got,
        &alice.kem_sk,
        request_id,
        alice.party.profile_id,
        alice.party.persona_id,
    )
    .unwrap();
    assert_eq!(opened.text.unwrap().as_str(), "hunter2");
    // Bob sees none of Alice's requests and can't delete them.
    let theirs: RequestList = h.get("/v1/requests", bob.t()).await.unwrap();
    assert!(theirs.requests.is_empty());
    let (status, _) = h
        .call(
            "DELETE",
            &format!("/v1/requests/{request_id}"),
            bob.t(),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = h
        .call(
            "DELETE",
            &format!("/v1/requests/{request_id}"),
            alice.t(),
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

async fn step(
    h: &Harness,
    who: &Person,
    grant: Id,
    action: EmergencyAction,
) -> Result<EmergencyWire, (StatusCode, ErrorCode)> {
    let mut s = EmergencyStep::new(grant, action, who.party.account_id, who.device);
    s.sign(&who.key).unwrap();
    h.cbor(
        "POST",
        &format!("/v1/emergency/{grant}/step"),
        who.t(),
        &EmergencyStepPush {
            step: ByteBuf::from(s.to_bytes().unwrap()),
        },
    )
    .await
}

#[tokio::test]
async fn emergency_access_waits_can_be_stopped_and_releases_once() {
    let h = harness();
    let mut alice = h.person("alice@example.com").await;
    let bob = h.person("bob@example.com").await;
    let (vault, owner) = owner_vault(&h, &mut alice).await;
    let vid = vault.context().vault_id;
    let mut grant = EmergencyGrant::new(owner, bob.party, 7, vec![vid], alice.device).unwrap();
    grant.sign(&alice.key).unwrap();
    let env = vault
        .seal_vault_key(
            bob.party,
            &bob.kem_pk,
            owner.account_id,
            SharePurpose::Emergency,
            Some(grant.grant_id),
        )
        .unwrap();
    let create = EmergencyCreate {
        grant: ByteBuf::from(grant.to_bytes().unwrap()),
        envelopes: vec![ByteBuf::from(env.to_bytes().unwrap())],
    };
    // Without the envelope, refused; with it, recorded.
    let bare = EmergencyCreate {
        grant: create.grant.clone(),
        envelopes: Vec::new(),
    };
    assert_eq!(
        h.status("POST", "/v1/emergency", alice.t(), &bare).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.status("POST", "/v1/emergency", alice.t(), &create).await,
        StatusCode::CREATED
    );
    let g = grant.grant_id;

    // Bob can't ask before he accepts, and Alice can't take his steps.
    assert_eq!(
        step(&h, &bob, g, EmergencyAction::Request)
            .await
            .unwrap_err()
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        step(&h, &alice, g, EmergencyAction::Accept)
            .await
            .unwrap_err()
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        step(&h, &bob, g, EmergencyAction::Accept)
            .await
            .unwrap()
            .state,
        EmergencyState::Accepted
    );
    let w = step(&h, &bob, g, EmergencyAction::Request).await.unwrap();
    assert_eq!(w.state, EmergencyState::Requested);
    let start = h.clock.load(Ordering::SeqCst);
    assert_eq!(w.release_at, Some(start + 7 * 86_400));
    // Too early: nothing released, and the vault stays closed.
    assert_eq!(
        h.get::<EmergencyRelease>(&format!("/v1/emergency/{g}/release"), bob.t())
            .await
            .unwrap_err()
            .0,
        StatusCode::FORBIDDEN
    );
    // Alice stops it; the clock running out then releases nothing.
    assert_eq!(
        step(&h, &alice, g, EmergencyAction::Stop)
            .await
            .unwrap()
            .state,
        EmergencyState::Accepted
    );
    h.clock.store(start + 8 * 86_400, Ordering::SeqCst);
    assert!(
        h.get::<EmergencyRelease>(&format!("/v1/emergency/{g}/release"), bob.t())
            .await
            .is_err()
    );
    // Asked again, and this time not stopped.
    step(&h, &bob, g, EmergencyAction::Request).await.unwrap();
    h.clock.store(start + 16 * 86_400, Ordering::SeqCst);
    let list: EmergencyList = h.get("/v1/emergency", alice.t()).await.unwrap();
    assert_eq!(list.as_owner[0].state, EmergencyState::Ready);
    let released: EmergencyRelease = h
        .get(&format!("/v1/emergency/{g}/release"), bob.t())
        .await
        .unwrap();
    assert_eq!(released.vaults.len(), 1);
    let env = ShareEnvelope::from_bytes(&released.vaults[0].envelope).unwrap();
    let place = SharePlace {
        purpose: SharePurpose::Emergency,
        vault_id: vid,
        item_id: None,
        key_id: vault.vault_key_id(),
        to: bob.party,
        from_account: owner.account_id,
        grant_id: Some(g),
    };
    let _: VaultKey = env
        .open(&bob.kem_sk, &place, &vault.verifying_key())
        .unwrap();
    // Released: Bob reads the vault, but writes nothing.
    let heads: HeadsSince = h
        .get(&format!("/v1/vaults/{vid}/heads"), bob.t())
        .await
        .unwrap();
    assert_eq!(heads.current_seq, 1);
    let item = vault.item(vault.item_ids()[0]).unwrap().clone();
    assert_eq!(
        push(&h, &bob, &item, item.rev).await.unwrap_err(),
        ErrorCode::Forbidden
    );
    // Every step is on the record, with the server's time.
    let list: EmergencyList = h.get("/v1/emergency", bob.t()).await.unwrap();
    let kinds: Vec<&str> = list.as_contact[0]
        .events
        .iter()
        .map(|e| e.kind.as_str())
        .collect();
    assert_eq!(
        kinds,
        [
            "created", "accept", "request", "stop", "request", "released"
        ]
    );
    assert_eq!(list.as_contact[0].state, EmergencyState::Released);
    // Revoked: the vault closes again.
    step(&h, &alice, g, EmergencyAction::Revoke).await.unwrap();
    assert_eq!(
        h.get::<HeadsSince>(&format!("/v1/vaults/{vid}/heads"), bob.t())
            .await
            .unwrap_err()
            .1,
        ErrorCode::Forbidden
    );
    assert!(step(&h, &bob, g, EmergencyAction::Request).await.is_err());
}

#[tokio::test]
async fn the_pages_behind_the_links_load_nothing_and_say_where_to_open_them() {
    let h = harness();
    for path in [format!("/s/{}", Id::new()), format!("/r/{}", Id::new())] {
        let req = Request::builder().uri(&path).body(Body::empty()).unwrap();
        let res = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("default-src 'none'"), "{csp}");
        assert_eq!(res.headers()[header::REFERRER_POLICY], "no-referrer");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("Lockdown"));
        assert!(!text.contains("<script"));
    }
}
