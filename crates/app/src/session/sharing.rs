//! Sharing in the session (`docs/plan/18-sharing.md` §2–§7): people
//! pinned through the key-transparency log, shared vaults and their
//! membership chain, households, items shared on their own, share links,
//! request links and emergency access.
//!
//! Every key handed to someone is wrapped to a receiving key this device
//! checked against the server's log (`verify_directory_entry`) and pinned;
//! every key taken from someone is opened only from an envelope signed by
//! a device of the person the signed membership chain says may send it.
//! Rights are decided here, not in a shell: a member without `edit` can't
//! change anything ([`Session::require_edit`], wired into every mutation
//! path), and a fill-only item never leaves the core except into a fill.

use std::collections::{BTreeMap, BTreeSet};

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::kem::{KemPublicKey, KemSecretKey};
use lockdown_crypto::VerifyingKey;
use lockdown_sync::sharing::{
    DirectoryEntry, EmergencyCreate, EmergencyEnvelopes, EmergencyState, EmergencyStepPush,
    EmergencyWire, ItemShareCreate, MembershipPush, RequestAnswer, RequestCreate, SendCreate,
    SendOpen, verify_directory_entry,
};
use lockdown_vault::contact::Contact;
use lockdown_vault::keys::{ItemKey, VaultKey};
use lockdown_vault::send::{
    SendField, SendKind, SendPayload, open_response, open_send, parse_request_link,
    parse_send_link, request_link, seal_response, seal_send, send_access, send_link,
};
use lockdown_vault::sharing::{
    Change, EmergencyAction, EmergencyGrant, EmergencyStep, MembershipEvent, Party, Right, Roster,
    ShareEnvelope, SharePlace, SharePurpose, VaultKind,
};
use lockdown_vault::transparency::SignedTreeHead;
use lockdown_vault::{Item, ItemState, Member, Unlock, VaultRecord, member_record};
use serde_bytes::ByteBuf;

use super::split_pause::Paused;
use super::*;
use crate::sharing::{
    ContactView, EmergencyView, HistoryLine, HouseholdView, MemberView, OpenedFile, OpenedValue,
    OpenedView, ReceivedItem, ReceivedView, Released, RequestView, SendOptions, SendView,
    SentShareView, SharingOverview, VaultSharingView,
};
use lockdown_vault::identity::keys_for;

pub(crate) mod follow;
pub(crate) mod key_change;
pub(crate) mod phrase;
pub(crate) mod pq;
pub(crate) mod rekey;

/// The most a shared item or link title is cut to in a message.
const NAME_CHARS: usize = 60;

/// What a share link carries.
pub enum SendWhat {
    /// One item's fields as they are now.
    Item(String),
    /// Some text, with a title.
    Text { title: String, text: Vec<u8> },
    /// A file.
    File {
        name: String,
        mime: String,
        bytes: Vec<u8>,
    },
}

fn parse_vault_id(id: &str) -> Result<Id, String> {
    Id::parse(id).map_err(|_| format!("'{id}' isn't a vault id"))
}

fn transparency_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn right_named(text: &str) -> Result<Right, String> {
    Right::parse(text).map_err(|e| e.to_string())
}

/// Removes everything a vault left in the store (its items and heads); the
/// record, a few hundred bytes of the Vault Key under this persona's key,
/// stays and is never opened again.
/// What a vault key envelope is expected to carry: its purpose, the grant
/// it belongs to, and whether its sender's post-quantum signature must be
/// on it whatever the membership chain says (an emergency key whose grant
/// carried one; docs/plan/49-shared-vault-quiet.md).
#[derive(Clone, Copy)]
struct KeyFor {
    purpose: SharePurpose,
    grant: Option<Id>,
    require_pq: bool,
}

impl KeyFor {
    /// A shared vault's key, handed over through the membership chain.
    const VAULT: KeyFor = KeyFor {
        purpose: SharePurpose::Vault,
        grant: None,
        require_pq: false,
    };
}

fn clear_vault(store: &mut DeviceStore, vault_id: Id) -> Result<(), String> {
    use lockdown_vault::Store as _;
    for item in store.list_items(vault_id).map_err(|e| e.to_string())? {
        store
            .remove_item(vault_id, item)
            .map_err(|e| e.to_string())?;
    }
    store
        .remove_heads_after(vault_id, 0)
        .map_err(|e| e.to_string())
}

impl Session {
    // --- this persona ------------------------------------------------------------

    fn persona(&self) -> Id {
        self.vault().context().persona_id
    }

    fn save_sharing(&self) -> Result<(), String> {
        self.sharing.save(&self.opened.keys.app_data_key)
    }

    fn kem_secret(&self) -> Result<KemSecretKey, String> {
        self.identity
            .persona_kem_secret(self.persona())
            .map_err(|e| e.to_string())
    }

    fn kem_public(&self) -> Result<KemPublicKey, String> {
        let persona = self.persona();
        self.identity
            .personas()
            .find(|p| p.persona_id == persona)
            .map(|p| p.kem_pub.clone())
            .ok_or_else(|| "this persona has no receiving key".to_string())
    }

    fn me(&self) -> Result<Party, String> {
        let account_id = self
            .sharing
            .account_id()
            .ok_or("this device hasn't learned its account on the server yet; sync once first")?;
        Ok(Party {
            account_id,
            profile_id: self.identity.profile().profile_id,
            persona_id: self.persona(),
        })
    }

    fn online(&mut self) -> Result<(HttpTransport, String, String), String> {
        let cfg = self
            .sync
            .as_ref()
            .ok_or("sharing needs an account on a server; register or log in first")?;
        let server = cfg.server.clone();
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        Ok((transport, self.token()?, server))
    }

    /// The server's tree head, verified as continuing the one this device
    /// remembers, and remembered.
    fn verified_sth(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        server: &str,
    ) -> Result<SignedTreeHead, String> {
        let remembered = self.transparency.view(server).cloned();
        let kt =
            KtSession::begin(transport, token, remembered.as_ref()).map_err(|e| e.to_string())?;
        let sth = kt.sth().clone();
        let view = kt.view.clone();
        drop(kt);
        self.transparency
            .remember(server, view)
            .map_err(|e| e.to_string())?;
        Ok(sth)
    }

    /// Learns this persona's account from the server's directory, checked
    /// like anyone's: the keys the log recorded must be this identity's own.
    fn learn_account(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
    ) -> Result<Id, String> {
        if let Some(id) = self.sharing.account_id() {
            return Ok(id);
        }
        let login = self
            .sync
            .as_ref()
            .map(|c| c.login.clone())
            .ok_or("no account on this device")?;
        let entry = transport
            .directory(token, &login)
            .map_err(|e| e.to_string())?;
        let me = verify_directory_entry(transport, sth, &entry, None, Some(self.persona()))
            .map_err(transparency_err)?;
        if me.isk_pub != self.identity.profile().isk_pub || me.kem_pub != self.kem_public()? {
            return Err(
                "the server's directory shows other keys for you than this identity holds; nothing was shared"
                    .into(),
            );
        }
        self.sharing.set_account_id(entry.account_id);
        self.save_sharing()?;
        Ok(entry.account_id)
    }

    /// Looks a person up, checks their keys against the log, and pins them
    /// (or checks they continue the pin, or moves it along a hand-over or
    /// an owner's vouch: `key_change`). `account` or `login` names them.
    /// Keys that changed with nothing to vouch for them are refused here,
    /// for the deliberate steps (looking someone up, sharing with them);
    /// a sync's lookups use [`Self::pin_quiet`].
    fn pin(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        who: Result<Id, &str>,
    ) -> Result<Contact, String> {
        let looked = self.look_up(transport, token, sth, who)?;
        if looked.changed {
            return Err(self.changed_words(&looked.contact));
        }
        Ok(looked.contact)
    }

    /// [`Self::pin`] for a sync: keys that changed with nothing to vouch
    /// for them leave the pin as it was and answer it, so the sync goes on
    /// and what they signed is held back with the quiet line (`quiet`).
    pub(super) fn pin_quiet(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        account: Id,
    ) -> Result<Contact, String> {
        Ok(self.look_up(transport, token, sth, Ok(account))?.contact)
    }

    /// Pins every account in `accounts` this device doesn't know yet.
    fn pin_all(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        accounts: impl IntoIterator<Item = Id>,
    ) -> Result<(), String> {
        let me = self.sharing.account_id();
        for account in accounts {
            if Some(account) == me || self.sharing.contact(account).is_some() {
                continue;
            }
            self.pin(transport, token, sth, Ok(account))?;
        }
        Ok(())
    }

    // --- whose devices sign what ----------------------------------------------

    /// This identity's own devices: its records and the synced set.
    pub(super) fn own_device_keys(&self) -> BTreeMap<Id, VerifyingKey> {
        crate::vault_open::own_device_keys(&self.identity, &self.synced)
    }

    /// Every device this device knows the owner of: its own identity's
    /// (under this persona's account) and every pinned contact's.
    pub(super) fn device_table(&self) -> BTreeMap<Id, (Id, VerifyingKey)> {
        crate::vault_open::device_table(&self.sharing, &self.own_device_keys())
    }

    fn device_keys_of(&self, account: Id) -> BTreeMap<Id, VerifyingKey> {
        crate::vault_open::device_keys_of(&self.sharing, &self.own_device_keys(), account)
    }

    fn replay(&self, vault_id: Id) -> Result<Option<Roster>, String> {
        let roster = crate::vault_open::roster(&self.sharing, &self.own_device_keys(), vault_id)?;
        if roster.is_some() {
            self.check_membership_pq(vault_id)?;
        }
        Ok(roster)
    }

    /// The keys of every device whose heads and versions a shared vault
    /// takes: the devices of every account that ever held `edit`.
    pub(super) fn writer_keys(&self, roster: &Roster) -> BTreeMap<Id, VerifyingKey> {
        crate::vault_open::writer_keys(&self.sharing, &self.own_device_keys(), roster)
    }

    fn members_list(roster: &Roster) -> Vec<Member> {
        roster
            .parties()
            .into_iter()
            .map(|(p, r)| Member {
                persona_id: p.persona_id,
                rights: vec![r.as_str().to_string()],
                key_env: roster.key_ref,
            })
            .collect()
    }

    /// Trusts a shared vault's writers' devices in the open copy and
    /// mirrors the roster into the members list its next head seals.
    pub(super) fn apply_roster(&mut self, vault_id: Id) -> Result<(), String> {
        let Some(roster) = self.rosters.get(&vault_id).cloned() else {
            return Ok(());
        };
        let keys = self.writer_keys(&roster);
        let vault = self.vault_slot(vault_id)?;
        for (id, key) in keys {
            vault.trust_device(id, key);
        }
        vault.set_members(Self::members_list(&roster));
        // The other writers' heads, under their own post-quantum rules
        // (docs/plan/47-shared-vault-pq.md).
        self.hold_members_to_pq(vault_id)
    }

    fn vault_slot(&mut self, vault_id: Id) -> Result<&mut Vault, String> {
        if self.opened.vault.context().vault_id == vault_id {
            return Ok(&mut self.opened.vault);
        }
        self.more
            .iter_mut()
            .map(|o| &mut o.vault)
            .find(|v| v.context().vault_id == vault_id)
            .ok_or_else(|| format!("vault {vault_id} isn't open here"))
    }

    // --- rights ------------------------------------------------------------------

    /// What this persona may do in a vault: `None` for its own (everything).
    pub(crate) fn vault_right(&self, vault_id: Id) -> Option<Right> {
        if let Some(share) = self.sharing.vault(vault_id)
            && !share.owned
        {
            let me = self.sharing.account_id()?;
            return Some(
                self.rosters
                    .get(&vault_id)
                    .and_then(|r| r.right_of(me))
                    .unwrap_or(Right::FillOnly),
            );
        }
        if self
            .sharing
            .released()
            .values()
            .any(|r| r.vaults.contains(&vault_id))
        {
            return Some(Right::View);
        }
        if self
            .received
            .iter()
            .any(|v| v.context().vault_id == vault_id)
        {
            return Some(Right::View);
        }
        None
    }

    /// What this persona may do with an item: `None` for its own. An item
    /// shared on its own follows its share while it is held as received;
    /// once its vault is open here for real, the vault's right answers.
    pub(crate) fn item_right(&self, item_id: Id) -> Option<Right> {
        let vault = self.holding(item_id).ok().map(|v| v.context().vault_id);
        if vault.is_none_or(|v| self.is_received_vault(v))
            && let Some(r) = self
                .sharing
                .received()
                .values()
                .find(|r| Item::from_bytes(&r.item).is_ok_and(|i| i.item_id == item_id))
        {
            return Some(r.right);
        }
        self.vault_right(vault?)
    }

    /// Refuses a change to a vault this persona can only view or fill.
    pub(crate) fn require_edit(&self, vault_id: Id) -> Result<(), String> {
        // Nothing is written while no vault of the persona could open yet.
        if self.stand_in && self.opened.vault.context().vault_id == vault_id {
            return Err(super::quiet::WAITING.into());
        }
        match self.vault_right(vault_id) {
            Some(r) if !r.can_edit() => Err(format!(
                "this is shared with you to {}; you can't change it",
                if r == Right::FillOnly {
                    "fill only"
                } else {
                    "view"
                }
            )),
            _ => Ok(()),
        }
    }

    /// Refuses showing or copying anything of a fill-only item: its values
    /// leave the core only into a fill the browser extension asked for.
    pub(crate) fn refuse_fill_only(&self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        if self.item_right(item_id) == Some(Right::FillOnly) {
            return Err(
                "this was shared with you to fill only: Lockdown fills it in the browser, and never shows or copies it"
                    .into(),
            );
        }
        Ok(())
    }

    /// Whether items may be shared from this vault (owned, no crown jewel).
    fn check_shareable(&self, vault_id: Id) -> Result<(), String> {
        if self.vault_right(vault_id).is_some_and(|r| !r.can_manage()) {
            return Err("only someone who manages a vault can share it".into());
        }
        if self.vault_by_id(vault_id)?.has_crown_jewels() {
            return Err(
                "this vault holds a crown jewel, which never leaves your own key; move it to another vault first"
                    .into(),
            );
        }
        Ok(())
    }

    /// Refuses a crown jewel in a shared vault.
    pub(crate) fn refuse_crown_in_shared(&self, vault_id: Id) -> Result<(), String> {
        if self.sharing.vault(vault_id).is_some() || self.vault_right(vault_id).is_some() {
            return Err(
                "a crown jewel can't be in a shared vault; it stays under your own key".into(),
            );
        }
        Ok(())
    }

    /// Whether a vault is someone else's items held only in memory.
    pub(crate) fn is_received_vault(&self, vault_id: Id) -> bool {
        self.received
            .iter()
            .any(|v| v.context().vault_id == vault_id)
    }

    /// Whether an export or backup takes this vault: not items held for
    /// someone else, not a vault this persona may only fill from.
    pub(crate) fn exportable(&self, vault_id: Id) -> bool {
        !(self.stand_in && self.opened.vault.context().vault_id == vault_id)
            && !self.is_received_vault(vault_id)
            && self.vault_right(vault_id) != Some(Right::FillOnly)
    }

    /// For every slot of `opened` and `more`, in order, whether this
    /// persona may change that vault (loops that write skip the rest).
    pub(crate) fn editable_slots(&self) -> Vec<bool> {
        std::iter::once(&self.opened)
            .chain(self.more.iter())
            .map(|o| self.require_edit(o.vault.context().vault_id).is_ok())
            .collect()
    }

    /// How a vault is shared, for the sidebar.
    pub(crate) fn share_badge(&self, vault_id: Id) -> Option<crate::types::VaultShareBadge> {
        if self.is_received_vault(vault_id) {
            let owner = self
                .sharing
                .received()
                .values()
                .find(|r| r.ctx.vault_id == vault_id)
                .map(|r| r.owner_login.clone());
            return Some(crate::types::VaultShareBadge {
                kind: "received".into(),
                right: "view".into(),
                owned: false,
                owner,
                people: 2,
            });
        }
        if let Some((_, r)) = self
            .sharing
            .released()
            .iter()
            .find(|(_, r)| r.vaults.contains(&vault_id))
        {
            return Some(crate::types::VaultShareBadge {
                kind: "emergency".into(),
                right: "view".into(),
                owned: false,
                owner: Some(r.owner_login.clone()),
                people: 2,
            });
        }
        let roster = self.rosters.get(&vault_id)?;
        let owned = Some(roster.owner.account_id) == self.sharing.account_id();
        Some(crate::types::VaultShareBadge {
            kind: roster.kind.as_str().into(),
            right: self
                .vault_right(vault_id)
                .unwrap_or(Right::Manage)
                .as_str()
                .into(),
            owned,
            owner: (!owned).then(|| self.name_of(roster.owner.account_id)),
            people: roster.members.len() + 1,
        })
    }

    // --- at unlock -----------------------------------------------------------------

    /// Opens the sharing record and every vault and item it names: shared
    /// vaults this persona is in, emergency releases, items shared with it.
    pub(crate) fn load_sharing(&mut self) -> Result<(), String> {
        let ids: Vec<Id> = self.sharing.vaults().keys().copied().collect();
        for vault_id in ids {
            match self.replay(vault_id) {
                Ok(Some(roster)) => {
                    self.rosters.insert(vault_id, roster);
                }
                Ok(None) => {}
                Err(_) => continue,
            }
            if !self.sharing.vault(vault_id).is_some_and(|s| s.owned) {
                let _ = self.open_shared_vault(vault_id);
            }
            // An open vault whose chain breaks a post-quantum rule under
            // its members' pinned keys stops the unlock, as one of the
            // identity's own would (doc 28, D338).
            if self.is_open(vault_id) {
                self.apply_roster(vault_id)?;
            }
        }
        let released: Vec<Id> = self
            .sharing
            .released()
            .values()
            .flat_map(|r| r.vaults.iter().copied())
            .collect();
        for vault_id in released {
            let _ = self.open_shared_vault(vault_id);
        }
        self.rebuild_received()
    }

    /// Opens a vault someone else owns from the store, under this
    /// persona's own keys, trusting the devices this device pinned.
    fn open_shared_vault(&mut self, vault_id: Id) -> Result<(), String> {
        if self.is_open(vault_id) {
            return Ok(());
        }
        let record =
            lockdown_vault::Store::get_vault(&self.store, vault_id).map_err(|e| e.to_string())?;
        let keys = keys_for(
            &self.identity,
            self.persona(),
            record.context.app_data_key_id,
            record.context.crown_jewel_key_id,
        )
        .map_err(|e| e.to_string())?;
        let mut known = self.own_device_keys();
        for (id, (_, key)) in self.device_table() {
            known.entry(id).or_insert(key);
        }
        let lookup = |id: Id| known.get(&id).cloned();
        let none = |_: Id| None;
        let vault = Vault::load(
            &self.store,
            vault_id,
            Unlock {
                app_data_key: &keys.app_data_key,
                crown_jewel_key: None,
                device_id: self.identity.device_id(),
                signer: Box::new(self.keystore.clone()),
                known_devices: &lookup,
                revoked_after: &none,
                last_accepted_seq: self.memory.last_accepted_seq(vault_id),
            },
        )
        .map_err(|e| e.to_string())?;
        // This identity's own heads in it follow doc 28's rules here too,
        // from where this device remembers the first one.
        let mut vault = vault;
        vault
            .set_post_quantum(self.own_pq()?)
            .map_err(|e| e.to_string())?;
        vault
            .require_post_quantum_from(self.memory.pq_since(vault_id))
            .map_err(|e| e.to_string())?;
        self.more.push(OpenVault {
            vault,
            keys,
            created: false,
        });
        self.drop_received(vault_id);
        Ok(())
    }

    /// A vault that just opened for real takes the place of the holder of
    /// items shared from it on their own: each is listed once, from the
    /// vault's own copy, under the vault's right. The shares themselves
    /// stay in the sharing record, so the items come back as received
    /// when the vault closes again ([`Self::rebuild_received`]).
    pub(super) fn drop_received(&mut self, vault_id: Id) {
        self.received.retain(|v| v.context().vault_id != vault_id);
    }

    /// Builds the in-memory vaults that hold items shared on their own,
    /// for every vault they came from that isn't open here for real.
    fn rebuild_received(&mut self) -> Result<(), String> {
        let mut by_vault: BTreeMap<Id, Vec<&ReceivedItem>> = BTreeMap::new();
        for r in self.sharing.received().values() {
            if self.is_open(r.ctx.vault_id) {
                continue;
            }
            by_vault.entry(r.ctx.vault_id).or_default().push(r);
        }
        let mut out = Vec::new();
        for (_, items) in by_vault {
            let ctx = items[0].ctx;
            let trusted = self.device_keys_of(items[0].owner.account_id);
            let mut held = Vec::new();
            for r in items {
                let Ok(mut item) = Item::from_bytes(&r.item) else {
                    continue;
                };
                item.item_key = None;
                let Ok(bytes): Result<[u8; 32], _> = r.item_key.as_slice().try_into() else {
                    continue;
                };
                held.push((item, ItemKey::from_bytes(bytes)));
            }
            if let Ok(v) = Vault::received(
                ctx,
                self.identity.device_id(),
                Box::new(self.keystore.clone()),
                &trusted,
                held,
            ) {
                out.push(v);
            }
        }
        self.received = out;
        Ok(())
    }

    // --- people ----------------------------------------------------------------------

    /// Looks someone up by login name, checks their keys against the
    /// server's log and pins them.
    pub fn lookup_contact(&mut self, login: &str) -> Result<ContactView, String> {
        self.refuse_while_split(Paused::Lookup)?;
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        self.learn_account(&transport, &token, &sth)?;
        let login = login.trim();
        if Some(login) == self.sync.as_ref().map(|c| c.login.as_str()) {
            return Err("that's you".into());
        }
        let contact = self.pin(&transport, &token, &sth, Err(login))?;
        Ok(ContactView::of(&contact))
    }

    /// This persona's account, learnt from the server's directory as
    /// [`Session::learn_account`] does.
    fn learn_own_account(&mut self) -> Result<Id, String> {
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        self.learn_account(&transport, &token, &sth)
    }

    /// This persona's own fingerprint: what someone who pinned it reads.
    pub fn own_fingerprint(&self) -> Result<(String, String), String> {
        let bytes = lockdown_vault::contact::fingerprint_bytes(
            self.identity.profile().profile_id,
            &self.identity.profile().isk_pub,
            self.persona(),
            &self.kem_public()?,
        )
        .map_err(|e| e.to_string())?;
        let qr = format!(
            "lockdown:contact:v1:{}:{}",
            self.sharing
                .account_id()
                .map(|a| a.to_string())
                .unwrap_or_default(),
            lockdown_vault::Hash32(bytes).to_hex()
        );
        Ok((lockdown_vault::contact::digits(&bytes), qr))
    }

    /// Everyone pinned here, by login.
    pub fn contacts(&self) -> Vec<ContactView> {
        let mut out: Vec<ContactView> = self
            .sharing
            .contacts()
            .values()
            .map(ContactView::of)
            .collect();
        out.sort_by(|a, b| a.login.cmp(&b.login));
        out
    }

    /// Records that the person compared fingerprints (read aloud or
    /// scanned) and they matched; `fingerprint` must be the one shown.
    pub fn verify_contact(
        &mut self,
        account: &str,
        fingerprint: &str,
    ) -> Result<ContactView, String> {
        let account = parse_id(account)?;
        let mut contact = self
            .sharing
            .contact(account)
            .cloned()
            .ok_or("that person isn't pinned here")?;
        let shown = contact.fingerprint().map_err(|e| e.to_string())?;
        let given: String = fingerprint.chars().filter(char::is_ascii_digit).collect();
        let want: String = shown.digits.chars().filter(char::is_ascii_digit).collect();
        if given != want && fingerprint.trim() != shown.qr {
            return Err("those numbers don't match; don't share with this key".into());
        }
        contact.verified_at = Some(lockdown_vault::Timestamp::now());
        self.sharing.put_contact(contact.clone());
        self.save_sharing()?;
        Ok(ContactView::of(&contact))
    }

    /// Forgets someone pinned here, unless a shared vault still names them.
    pub fn forget_contact(&mut self, account: &str) -> Result<(), String> {
        let account = parse_id(account)?;
        if self
            .rosters
            .values()
            .any(|r| r.owner.account_id == account || r.members.contains_key(&account))
        {
            return Err("they're still in a vault you share; remove them there first".into());
        }
        self.sharing.remove_contact(account);
        self.save_sharing()
    }

    fn contact_named(&self, login: &str) -> Result<Contact, String> {
        self.sharing
            .contact_by_login(login)
            .cloned()
            .ok_or_else(|| format!("look {login} up first so their keys are checked"))
    }

    fn name_of(&self, account: Id) -> String {
        if Some(account) == self.sharing.account_id() {
            return "you".into();
        }
        if let Some(c) = self.sharing.contact(account) {
            return c.login.clone();
        }
        for share in self.sharing.vaults().values() {
            if let Some(l) = share.logins.get(&account) {
                return l.clone();
            }
        }
        "someone".into()
    }

    // --- shared vaults -----------------------------------------------------------------

    /// One vault's sharing, for its panel.
    pub fn vault_sharing(&self, vault_id: &str) -> Result<VaultSharingView, String> {
        let vault_id = parse_vault_id(vault_id)?;
        let vault = self.vault_by_id(vault_id)?;
        let me = self.sharing.account_id();
        let right = self.vault_right(vault_id).unwrap_or(Right::Manage);
        let blocked = vault.has_crown_jewels().then(|| {
            "this vault holds a crown jewel, which never leaves your own key; move it to another vault to share this one".to_string()
        });
        let Some(roster) = self.rosters.get(&vault_id) else {
            return Ok(VaultSharingView {
                vault_id: vault_id.to_string(),
                shared: false,
                kind: "shared".into(),
                right: right.as_str().into(),
                can_manage: right.can_manage() && !self.is_received_vault(vault_id),
                members: Vec::new(),
                history: Vec::new(),
                blocked,
                held: self.held_words(vault_id),
            });
        };
        let members = roster
            .parties()
            .into_iter()
            .map(|(p, r)| MemberView {
                account_id: p.account_id.to_string(),
                login: self.name_of(p.account_id),
                right: r.as_str().into(),
                right_words: r.describe().into(),
                owner: p.account_id == roster.owner.account_id,
                you: Some(p.account_id) == me,
                verified: self
                    .sharing
                    .contact(p.account_id)
                    .is_some_and(|c| c.verified_at.is_some()),
            })
            .collect();
        let history = self
            .sharing
            .vault(vault_id)
            .map(|s| {
                s.events
                    .iter()
                    .filter_map(|b| MembershipEvent::from_bytes(b).ok())
                    .map(|e| HistoryLine {
                        at: e.at.0,
                        words: e.describe(&|a| self.name_of(a)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        // While something of the vault is held back, who is in it can't be
        // changed here, and the panel says so in the vault's line (`quiet`).
        let held_back = if right.can_manage() {
            self.unverified_read_only_line(vault_id)
        } else {
            self.unverified_line(vault_id)
        };
        Ok(VaultSharingView {
            vault_id: vault_id.to_string(),
            shared: true,
            kind: roster.kind.as_str().into(),
            right: right.as_str().into(),
            can_manage: right.can_manage() && held_back.is_none(),
            members,
            history,
            blocked,
            held: held_back.or_else(|| self.held_words(vault_id)),
        })
    }

    /// Posts one membership change and takes it into the local chain.
    fn post_change(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        vault_id: Id,
        event: MembershipEvent,
        envelopes: Vec<ShareEnvelope>,
    ) -> Result<(), String> {
        let push = MembershipPush {
            event: ByteBuf::from(event.to_bytes().map_err(|e| e.to_string())?),
            envelopes: envelopes
                .iter()
                .map(|e| e.to_bytes().map(ByteBuf::from))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?,
        };
        transport
            .push_membership(token, vault_id, &push)
            .map_err(|e| e.to_string())?;
        let owned = self.vault_right(vault_id).is_none();
        let share = self.sharing.vault_mut(vault_id);
        share.owned = owned;
        share.events.push(push.event);
        self.save_sharing()?;
        let roster = self
            .replay(vault_id)?
            .ok_or("the vault's membership didn't take")?;
        self.rosters.insert(vault_id, roster);
        self.apply_roster(vault_id)?;
        Ok(())
    }

    fn sign_event(&self, mut event: MembershipEvent) -> Result<MembershipEvent, String> {
        event.sign(&self.keystore).map_err(|e| e.to_string())?;
        if let Some(key) = self.sharing_pq_key(Some(event.vault_id))? {
            event.sign_post_quantum(&key).map_err(|e| e.to_string())?;
        }
        Ok(event)
    }

    /// Shares a vault with someone pinned here (it becomes shared, or a
    /// household with `kind: family`, on the first share), at `right`.
    pub fn share_vault(
        &mut self,
        vault_id: &str,
        login: &str,
        right: &str,
        kind: VaultKind,
    ) -> Result<VaultSharingView, String> {
        self.refuse_while_split(Paused::ShareVault)?;
        let vault_id = parse_vault_id(vault_id)?;
        let right = right_named(right)?;
        self.check_shareable(vault_id)?;
        let contact = self.contact_named(login)?;
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        self.learn_account(&transport, &token, &sth)?;
        // The pin must still hold: a key that changed since is refused.
        let contact = {
            let fresh = self.pin(&transport, &token, &sth, Ok(contact.account_id))?;
            if fresh.kem_pub != contact.kem_pub {
                return Err("their keys changed; compare fingerprints again".into());
            }
            fresh
        };
        // Everything the vault holds must be on the server first.
        self.sync()?;
        self.refuse_while_unverified(vault_id)?;
        let me = self.me()?;
        if !self.rosters.contains_key(&vault_id) {
            let key_ref = self.vault_by_id(vault_id)?.vault_key_id();
            let genesis = self.sign_event(Roster::genesis(
                vault_id,
                me,
                kind,
                key_ref,
                self.identity.device_id(),
            ))?;
            self.post_change(&transport, &token, vault_id, genesis, Vec::new())?;
        }
        let roster = self.rosters.get(&vault_id).cloned().ok_or("not shared")?;
        if roster.right_of(contact.account_id).is_some() {
            return Err(format!("{} is already in this vault", contact.login));
        }
        let mut envelope = self
            .vault_by_id(vault_id)?
            .seal_vault_key(
                contact.party(),
                &contact.kem_pub,
                me.account_id,
                SharePurpose::Vault,
                None,
            )
            .map_err(|e| e.to_string())?;
        self.pq_sign_envelope(&mut envelope, Some(vault_id))?;
        let event = self.sign_event(roster.propose(
            Change::Add {
                member: contact.party(),
                right,
                keys: None,
            },
            None,
            me.account_id,
            self.identity.device_id(),
        ))?;
        self.post_change(&transport, &token, vault_id, event, vec![envelope])?;
        self.save()?;
        self.vault_sharing(&vault_id.to_string())
    }

    /// Changes a member's right.
    pub fn set_member_right(
        &mut self,
        vault_id: &str,
        account: &str,
        right: &str,
    ) -> Result<VaultSharingView, String> {
        let vault_id = parse_vault_id(vault_id)?;
        let account = parse_id(account)?;
        let right = right_named(right)?;
        self.refuse_while_unverified(vault_id)?;
        let (transport, token, _) = self.online()?;
        let me = self.me()?;
        let roster = self
            .rosters
            .get(&vault_id)
            .cloned()
            .ok_or("that vault isn't shared")?;
        let event = self.sign_event(roster.propose(
            Change::SetRight {
                account_id: account,
                right,
            },
            None,
            me.account_id,
            self.identity.device_id(),
        ))?;
        self.post_change(&transport, &token, vault_id, event, Vec::new())?;
        self.vault_sharing(&vault_id.to_string())
    }

    /// Removes a member: the vault moves to a fresh Vault Key and every item
    /// to a fresh Item Key, the people who stay get the new key, and the
    /// vault syncs at once.
    pub fn remove_member(
        &mut self,
        vault_id: &str,
        account: &str,
    ) -> Result<VaultSharingView, String> {
        let vault_id = parse_vault_id(vault_id)?;
        let account = parse_id(account)?;
        let (transport, token, server) = self.online()?;
        // Pull everything first, so nothing lands under the old key after.
        self.sync()?;
        self.refuse_while_unverified(vault_id)?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.me()?;
        let roster = self
            .rosters
            .get(&vault_id)
            .cloned()
            .ok_or("that vault isn't shared")?;
        if !roster
            .right_of(me.account_id)
            .is_some_and(Right::can_manage)
        {
            return Err("only someone who manages the vault can remove people".into());
        }
        if !roster.members.contains_key(&account) {
            return Err("they aren't in this vault".into());
        }
        let staying: Vec<Party> = roster
            .parties()
            .into_iter()
            .map(|(p, _)| p)
            .filter(|p| p.account_id != account && p.account_id != me.account_id)
            .collect();
        self.pin_all(
            &transport,
            &token,
            &sth,
            staying.iter().map(|p| p.account_id),
        )?;
        let new_ref = self
            .vault_slot(vault_id)?
            .rotate_vault_key()
            .map_err(|e| e.to_string())?;
        // The items shared on their own from this vault, and the emergency
        // grants covering it, get the new keys once the re-keyed items are
        // on the server: at the end of the sync below, or a later one
        // (`rekey`; docs/plan/51-rekey-push.md §2).
        self.sharing.mark_reseal(vault_id, true, &[]);
        self.save_sharing()?;
        let mut envelopes = Vec::new();
        let vault = self.vault_by_id(vault_id)?;
        for p in &staying {
            let contact = self
                .sharing
                .contact(p.account_id)
                .ok_or("a member isn't pinned here")?;
            envelopes.push(
                vault
                    .seal_vault_key(
                        *p,
                        &contact.kem_pub,
                        me.account_id,
                        SharePurpose::Vault,
                        None,
                    )
                    .map_err(|e| e.to_string())?,
            );
        }
        // This persona's own other devices need the new key too.
        if roster.members.contains_key(&me.account_id) {
            envelopes.push(
                vault
                    .seal_vault_key(
                        me,
                        &self.kem_public()?,
                        me.account_id,
                        SharePurpose::Vault,
                        None,
                    )
                    .map_err(|e| e.to_string())?,
            );
        }
        for envelope in &mut envelopes {
            self.pq_sign_envelope(envelope, Some(vault_id))?;
        }
        let event = self.sign_event(roster.propose(
            Change::Remove {
                account_id: account,
            },
            Some(new_ref),
            me.account_id,
            self.identity.device_id(),
        ))?;
        self.post_change(&transport, &token, vault_id, event, envelopes)?;
        if roster.owner.account_id == me.account_id {
            // The owner's other devices find the new key in the vault's record.
            let record = {
                let open = self.open_by_id(vault_id)?;
                open.vault
                    .record(&open.keys.app_data_key)
                    .and_then(|r| r.to_bytes())
                    .map_err(|e| e.to_string())?
            };
            transport
                .put_vault(&token, vault_id, &record)
                .map_err(|e| e.to_string())?;
        }
        self.save()?;
        self.sync()?;
        self.vault_sharing(&vault_id.to_string())
    }

    fn open_by_id(&self, vault_id: Id) -> Result<&OpenVault, String> {
        std::iter::once(&self.opened)
            .chain(self.more.iter())
            .find(|o| o.vault.context().vault_id == vault_id)
            .ok_or_else(|| format!("vault {vault_id} isn't open here"))
    }

    /// Makes a household: a new vault, shared as a family with this person
    /// as its organizer. Members are added with [`Self::share_vault`].
    pub fn create_household(&mut self, name: &str) -> Result<String, String> {
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me_account = self.learn_account(&transport, &token, &sth)?;
        let vault_id = self.create_vault(name)?;
        self.sync()?;
        let key_ref = self.vault_by_id(vault_id)?.vault_key_id();
        let me = Party {
            account_id: me_account,
            profile_id: self.identity.profile().profile_id,
            persona_id: self.persona(),
        };
        let genesis = self.sign_event(Roster::genesis(
            vault_id,
            me,
            VaultKind::Family,
            key_ref,
            self.identity.device_id(),
        ))?;
        self.post_change(&transport, &token, vault_id, genesis, Vec::new())?;
        Ok(vault_id.to_string())
    }

    fn households(&self) -> Vec<HouseholdView> {
        let me = self.sharing.account_id();
        let mut out = Vec::new();
        for (vault_id, roster) in &self.rosters {
            if roster.kind != VaultKind::Family {
                continue;
            }
            let Ok(view) = self.vault_sharing(&vault_id.to_string()) else {
                continue;
            };
            let name = self
                .vault_by_id(*vault_id)
                .ok()
                .and_then(|v| v.info().ok())
                .and_then(|i| i.name)
                .unwrap_or_else(|| "Household".into());
            out.push(HouseholdView {
                vault_id: vault_id.to_string(),
                name,
                organizer: self.name_of(roster.owner.account_id),
                organizing: Some(roster.owner.account_id) == me,
                members: view.members,
            });
        }
        out
    }

    // --- sync ------------------------------------------------------------------------

    /// Brings sharing in step before the vaults sync: this persona's
    /// account, the vaults shared with it (and its own shared vaults'
    /// chains), new key generations, items shared on their own, and
    /// emergency releases. Called by [`Session::sync`] under its log check.
    pub(crate) fn sync_sharing(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
    ) -> Result<(), String> {
        let me = self.learn_account(transport, token, sth)?;
        let list = transport.shared_vaults(token).map_err(|e| e.to_string())?;
        let mut listed: BTreeSet<Id> = BTreeSet::new();
        // What waits for a split to clear is worked out again (`split_hold`).
        self.held_vaults.clear();
        // Own shared vaults: other managers' changes.
        for own in &list.sharing {
            listed.insert(own.vault_id);
            self.sharing.vault_mut(own.vault_id).owned = true;
            self.pull_membership(transport, token, sth, own.vault_id)?;
            if self.held_vaults.contains_key(&own.vault_id)
                || self.unverified.blocks_push(own.vault_id)
            {
                continue;
            }
            self.refresh_unready(transport, token, sth, own.vault_id)?;
            self.take_new_key(transport, token, own.vault_id, own.key_ref)?;
        }
        // Vaults shared with this persona.
        for shared in &list.vaults {
            let vault_id = shared.summary.vault_id;
            if shared.owner.account_id == me {
                continue;
            }
            listed.insert(vault_id);
            self.sharing.vault_mut(vault_id).owned = false;
            self.pull_membership(transport, token, sth, vault_id)?;
            // Changes from someone this device hasn't met wait during a
            // split: the vault isn't joined yet and its key doesn't move.
            if self.held_vaults.contains_key(&vault_id) || self.unverified.blocks_push(vault_id) {
                continue;
            }
            self.refresh_unready(transport, token, sth, vault_id)?;
            let open = self.is_open(vault_id);
            if !open {
                self.join_shared(transport, token, sth, shared)?;
            }
            self.take_new_key(transport, token, vault_id, shared.key_ref)?;
        }
        // A vault no longer listed was left or removed from: closed here.
        let gone: Vec<Id> = self
            .sharing
            .vaults()
            .iter()
            .filter(|(id, s)| !listed.contains(id) && !s.owned)
            .map(|(id, _)| *id)
            .collect();
        for vault_id in gone {
            self.close_vault(vault_id)?;
            self.sharing.remove_vault(vault_id);
            self.rosters.remove(&vault_id);
        }
        self.sync_received(transport, token, sth)?;
        self.sync_released(transport, token, sth)?;
        self.save_sharing()
    }

    fn pull_membership(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        vault_id: Id,
    ) -> Result<(), String> {
        let have = self
            .sharing
            .vault(vault_id)
            .map(|s| s.events.len())
            .unwrap_or(0) as u64;
        let since = transport
            .members_since(token, vault_id, have)
            .map_err(|e| e.to_string())?;
        let hints = since.identity_heads.clone();
        if since.events.is_empty() && self.rosters.contains_key(&vault_id) {
            return self.follow_keys(transport, token, sth, vault_id, &hints);
        }
        let events = since
            .events
            .iter()
            .map(|b| MembershipEvent::from_bytes(b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // During a split, the changes that would have this device meet
        // someone (a new member, a member's new device) wait with every
        // change after them (`split_hold`).
        let take = self.membership_take(&events);
        if take.count < events.len() {
            self.hold(vault_id, super::shared_sync::Hold::Members);
        }
        let mut accounts: BTreeSet<Id> = events[..take.count]
            .iter()
            .flat_map(super::split_hold::named)
            .collect();
        if let Some(r) = self.rosters.get(&vault_id) {
            accounts.insert(r.owner.account_id);
        }
        self.pin_all(transport, token, sth, accounts.iter().copied())?;
        // A pinned member who signed with a device this device doesn't know
        // is looked up again, and must continue the pin.
        for account in &take.refresh {
            self.pin_quiet(transport, token, sth, *account)?;
        }
        let lockdown_sync::sharing::MembershipSince {
            events: mut taken,
            logins,
            ..
        } = since;
        taken.truncate(take.count);
        let share = self.sharing.vault_mut(vault_id);
        let before = share.events.len();
        share.events.extend(taken);
        share
            .logins
            .extend(logins.into_iter().filter(|(a, _)| accounts.contains(a)));
        // A chain that doesn't check out isn't kept, even in memory: the
        // next pull fetches the same changes again and checks them afresh.
        // Another person's change that fails is held back with every change
        // after it, and the changes before it are taken (`quiet`); one of
        // this identity's own fails the sync as it always did.
        let replayed = match self.replay(vault_id) {
            Ok(r) => r,
            Err(e) => self.replay_held_back(vault_id, before, e)?,
        };
        let Some(roster) = replayed else {
            if self.held_vaults.contains_key(&vault_id) || self.unverified.vault_held(vault_id) {
                return Ok(());
            }
            return Err("the server listed a shared vault with no membership".into());
        };
        self.rosters.insert(vault_id, roster);
        if self.is_open(vault_id) {
            self.apply_roster(vault_id)?;
        }
        self.follow_keys(transport, token, sth, vault_id, &hints)
    }

    /// After the chain with the newly pulled changes (those after `before`)
    /// failed its check with `e`: keeps the longest run of them that checks
    /// out and holds the rest back, when the first that fails is another
    /// person's. Answers the roster of what was kept. One of this
    /// identity's own changes fails the sync with `e`, as it always did.
    fn replay_held_back(
        &mut self,
        vault_id: Id,
        before: usize,
        e: String,
    ) -> Result<Option<Roster>, String> {
        let pulled: Vec<ByteBuf> = self.sharing.vault_mut(vault_id).events.split_off(before);
        let mut keep = pulled.len();
        let mut why = e.clone();
        let replayed = loop {
            if keep == 0 {
                break self.replay(vault_id).ok().flatten();
            }
            keep -= 1;
            let share = self.sharing.vault_mut(vault_id);
            share.events.truncate(before);
            share.events.extend(pulled[..keep].iter().cloned());
            match self.replay(vault_id) {
                Ok(r) => break r,
                Err(next) => why = next,
            }
        };
        // `keep` changes check out; the one after them is the first that doesn't.
        let failed = MembershipEvent::from_bytes(&pulled[keep]).ok();
        let me = self.sharing.account_id();
        if failed.as_ref().is_none_or(|f| Some(f.by_account) == me) {
            self.sharing.vault_mut(vault_id).events.truncate(before);
            return Err(e);
        }
        let failed = failed.expect("checked above");
        // The first error names the change that broke the whole chain; when
        // a shorter run failed too, that one is what the change hit.
        let why = if keep + 1 == pulled.len() { e } else { why };
        self.hold_unverified(super::quiet::Held {
            kind: super::quiet::Kind::Membership,
            vault: Some(vault_id),
            who: self.known_other(failed.by_account),
            record: format!("membership change {}", failed.seq),
            why,
        });
        Ok(replayed)
    }

    /// Opens a Vault Key envelope addressed to this persona for `vault_id`
    /// at `key_ref`, sent by someone the chain says may manage it, for
    /// `what` it carries.
    fn open_vault_envelope(
        &self,
        bytes: &[u8],
        vault_id: Id,
        key_ref: Id,
        what: KeyFor,
        allowed_senders: &dyn Fn(Id) -> bool,
    ) -> Result<VaultKey, String> {
        let KeyFor {
            purpose,
            grant,
            require_pq,
        } = what;
        let env = ShareEnvelope::from_bytes(bytes).map_err(|e| e.to_string())?;
        let me = self.me()?;
        if !allowed_senders(env.place.from_account) {
            return Err("a vault key came from someone who can't hand it out; refused".into());
        }
        let expected = SharePlace {
            purpose,
            vault_id,
            item_id: None,
            key_id: key_ref,
            to: me,
            from_account: env.place.from_account,
            grant_id: grant,
        };
        let table = self.device_table();
        let (account, key) = table
            .get(&env.by_device)
            .cloned()
            .ok_or("a vault key came from a device this device doesn't know; refused")?;
        if account != env.place.from_account {
            return Err("a vault key was signed by a device of someone else; refused".into());
        }
        // Its sender's post-quantum signature: checked when this device
        // knows their key, required when the membership change that handed
        // it over carries one (docs/plan/47-shared-vault-pq.md).
        let required = require_pq
            || (purpose == SharePurpose::Vault
                && self.envelope_needs_pq(vault_id, key_ref, env.place.from_account));
        env.check_post_quantum(
            self.pq_key_by(env.place.from_account, env.by_device)
                .as_ref(),
            required,
        )
        .map_err(|e| e.to_string())?;
        env.open::<VaultKey>(&self.kem_secret()?, &expected, &key)
            .map_err(|e| e.to_string())
    }

    /// [`Self::open_vault_envelope`] for a shared vault's key: a grant
    /// another person sent that doesn't verify is held back (`quiet`) and
    /// answers `None`, so the vault stays where it was (not joined yet, or
    /// at the key it has). One from this identity's own account fails as
    /// it always did.
    fn vault_key_or_hold(
        &mut self,
        bytes: &[u8],
        vault_id: Id,
        key_ref: Id,
        allowed_senders: &dyn Fn(Id) -> bool,
    ) -> Result<Option<VaultKey>, String> {
        match self.open_vault_envelope(bytes, vault_id, key_ref, KeyFor::VAULT, allowed_senders) {
            Ok(key) => Ok(Some(key)),
            Err(why) => {
                let from = ShareEnvelope::from_bytes(bytes)
                    .ok()
                    .map(|e| e.place.from_account);
                if from.is_some() && from == self.sharing.account_id() {
                    return Err(why);
                }
                self.hold_unverified(super::quiet::Held {
                    kind: super::quiet::Kind::Grant,
                    vault: Some(vault_id),
                    who: from.and_then(|a| self.known_other(a)),
                    record: format!("key grant for generation {key_ref}"),
                    why,
                });
                Ok(None)
            }
        }
    }

    fn join_shared(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        shared: &lockdown_sync::sharing::SharedVaultWire,
    ) -> Result<(), String> {
        let vault_id = shared.summary.vault_id;
        let roster = self
            .rosters
            .get(&vault_id)
            .cloned()
            .ok_or("no membership")?;
        let env = transport
            .vault_envelope(token, vault_id)
            .map_err(|e| e.to_string())?;
        let managers = |a: Id| roster.right_of(a).is_some_and(Right::can_manage);
        let Some(key) =
            self.vault_key_or_hold(&env.envelope, vault_id, roster.key_ref, &managers)?
        else {
            return Ok(());
        };
        self.bootstrap_member_vault(transport, token, sth, &shared.summary, roster.key_ref, &key)?;
        self.apply_roster(vault_id)
    }

    /// Writes a vault someone else owns into the store under this persona's
    /// own key, pulls its chain and items (checked against this sync's
    /// tree head) and opens it.
    fn bootstrap_member_vault(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        _sth: &SignedTreeHead,
        summary: &lockdown_sync::wire::VaultSummary,
        key_ref: Id,
        key: &VaultKey,
    ) -> Result<(), String> {
        let owner_record = VaultRecord::from_bytes(&summary.record).map_err(|e| e.to_string())?;
        let mut ctx = owner_record.context;
        ctx.app_data_key_id = self.opened.keys.app_data_key_id;
        ctx.crown_jewel_key_id = self.opened.keys.crown_jewel_key_id;
        let record = member_record(ctx, key_ref, key, &self.opened.keys.app_data_key)
            .map_err(|e| e.to_string())?;
        let mine = lockdown_sync::wire::VaultSummary {
            record: record.to_bytes().map_err(|e| e.to_string())?,
            ..summary.clone()
        };
        let label = device_name();
        let syncer = Syncer {
            transport,
            token,
            device_label: &label,
        };
        let state = lockdown_sync::bootstrap_vault(&syncer, &mut self.store, &mine)
            .map_err(|e| e.to_string())?;
        let Some(state) = state else {
            return Ok(());
        };
        if let Some(cfg) = self.sync.as_mut() {
            cfg.vaults.insert(summary.vault_id, state);
            cfg.save(&self.paths.sync_file())?;
        }
        self.open_shared_vault(summary.vault_id)?;
        self.remember()
    }

    /// Takes a newer Vault Key generation of a shared vault when the chain
    /// moved to one, and moves this device's items under it (when this
    /// persona may change the vault).
    fn take_new_key(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        vault_id: Id,
        key_ref: Id,
    ) -> Result<(), String> {
        // Only a vault open here for real moves to a new key; a holder of
        // items shared from it on their own has none to move.
        let Ok(open) = self.open_by_id(vault_id) else {
            return Ok(());
        };
        if open.vault.vault_key_id() == key_ref {
            return Ok(());
        }
        let roster = self
            .rosters
            .get(&vault_id)
            .cloned()
            .ok_or("no membership")?;
        let me = self.me()?;
        let key = if roster.owner.account_id == me.account_id {
            // The owner's own record first (the owner rotated on another
            // device), else an envelope a manager left for the owner.
            let list = transport.list_vaults(token).map_err(|e| e.to_string())?;
            let from_record = list
                .vaults
                .iter()
                .find(|v| v.vault_id == vault_id)
                .and_then(|v| VaultRecord::from_bytes(&v.record).ok())
                .and_then(|r| {
                    let place = lockdown_vault::KeyPlace {
                        profile_id: r.context.profile_id,
                        persona_id: r.context.persona_id,
                        vault_id,
                        key_id: key_ref,
                        purpose: lockdown_vault::KeyPurpose::Vault,
                        wrapped_by: lockdown_vault::WrappedBy::new(
                            lockdown_vault::WrapperKind::AppData,
                            r.context.app_data_key_id,
                        ),
                        v: 1,
                    };
                    lockdown_vault::unwrap_key::<VaultKey, _>(
                        &r.vault_key,
                        &self.opened.keys.app_data_key,
                        &place,
                    )
                    .ok()
                });
            match from_record {
                Some(k) => k,
                None => {
                    let env = transport
                        .vault_envelope(token, vault_id)
                        .map_err(|e| e.to_string())?;
                    let managers = |a: Id| roster.right_of(a).is_some_and(Right::can_manage);
                    match self.vault_key_or_hold(&env.envelope, vault_id, key_ref, &managers)? {
                        Some(k) => k,
                        None => return Ok(()),
                    }
                }
            }
        } else {
            let env = transport
                .vault_envelope(token, vault_id)
                .map_err(|e| e.to_string())?;
            let managers = |a: Id| roster.right_of(a).is_some_and(Right::can_manage);
            match self.vault_key_or_hold(&env.envelope, vault_id, key_ref, &managers)? {
                Some(k) => k,
                None => return Ok(()),
            }
        };
        let can_edit = roster.right_of(me.account_id).is_some_and(Right::can_edit);
        let vault = self.vault_slot(vault_id)?;
        vault.adopt_vault_key(key_ref, key);
        if can_edit {
            for item in vault.items_under_old_keys() {
                vault.rekey_item(item).map_err(|e| e.to_string())?;
            }
        }
        // The owner's items shared on their own and emergency grants follow
        // the new key once the vault's items under it are on the server
        // (another device of the owner, or a manager, rotated it; `rekey`).
        if roster.owner.account_id == me.account_id {
            self.sharing.mark_reseal(vault_id, true, &[]);
            self.save_sharing()?;
        }
        self.save()
    }

    /// Looks every writer of a shared vault up again (each must continue its
    /// pin) and trusts their devices now: a member's new device, met once
    /// the split cleared (`split_hold`; docs/plan/35-revoke-split.md §2).
    pub(super) fn refresh_writers(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        vault_id: Id,
    ) -> Result<(), String> {
        let Some(roster) = self.rosters.get(&vault_id).cloned() else {
            return Ok(());
        };
        let me = self.sharing.account_id();
        for account in roster.writers.iter().copied().filter(|a| Some(*a) != me) {
            self.pin_quiet(transport, token, sth, account)?;
        }
        // A vault of this persona not open yet takes the roster as it opens.
        if self.is_open(vault_id) {
            self.apply_roster(vault_id)?;
        }
        Ok(())
    }

    /// Takes a vault out of this session and the store. Items still shared
    /// from it on their own come back as received.
    fn close_vault(&mut self, vault_id: Id) -> Result<(), String> {
        let before = self.more.len();
        self.more.retain(|o| o.vault.context().vault_id != vault_id);
        if self.more.len() != before {
            clear_vault(&mut self.store, vault_id)?;
            if let Some(cfg) = self.sync.as_mut() {
                cfg.vaults.remove(&vault_id);
                cfg.save(&self.paths.sync_file())?;
            }
            self.rebuild_received()?;
        }
        Ok(())
    }

    fn sync_received(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
    ) -> Result<(), String> {
        let got = transport.received_items(token).map_err(|e| e.to_string())?;
        // During a split, an item whose owner this device hasn't pinned, or
        // that a pinned owner's device this device doesn't know signed,
        // waits unapplied; outside one, owners are looked up, checked
        // against the log and pinned as ever, and a pinned owner with a new
        // device is looked up again and must continue the pin (`shared_sync`).
        // The devices are the envelope's and the newest version's, the only
        // ones checked here: an earlier version another member signed (one
        // the owner's device has since followed) names no device of the
        // owner's (docs/plan/54-share-follows-edits.md).
        let signers: Vec<(Id, Vec<Id>)> = got
            .items
            .iter()
            .map(|r| {
                let mut devices: Vec<Id> = ShareEnvelope::from_bytes(&r.envelope)
                    .map(|e| vec![e.by_device])
                    .unwrap_or_default();
                if let Some(latest) = Item::from_bytes(&r.item.item)
                    .ok()
                    .and_then(|i| i.latest().ok().map(|v| v.edited_by))
                {
                    devices.push(latest);
                }
                (r.owner.account_id, devices)
            })
            .collect();
        let me_account = self.sharing.account_id();
        let table = self.device_table();
        let take = super::shared_sync::received_take(
            self.split_view(),
            &signers,
            &|a| Some(a) == me_account || self.sharing.contact(a).is_some(),
            &|d| table.contains_key(&d),
        );
        self.held_items = take.take.iter().filter(|t| !**t).count();
        self.pin_all(
            transport,
            token,
            sth,
            signers
                .iter()
                .zip(&take.take)
                .filter(|(_, t)| **t)
                .map(|((o, _), _)| *o),
        )?;
        for account in &take.refresh {
            self.pin_quiet(transport, token, sth, *account)?;
        }
        let me = self.me()?;
        let kem = self.kem_secret()?;
        let table = self.device_table();
        let marked: BTreeSet<Id> = self.sharing.pq_senders().keys().copied().collect();
        let mut newly_marked: BTreeSet<Id> = BTreeSet::new();
        let mut out = BTreeMap::new();
        for (r, taken) in got.items.iter().zip(&take.take) {
            if !taken {
                // What this device already had of it stays as it was.
                if let Some(had) = self.sharing.received().get(&r.share_id) {
                    out.insert(r.share_id, had.clone());
                }
                continue;
            }
            // One that doesn't verify is held back, and what this device
            // already had of it stays as it was (`quiet`).
            match self.take_received(r, me, &kem, &table, &marked) {
                Ok((taken, signed)) => {
                    if signed {
                        newly_marked.insert(r.owner.account_id);
                    }
                    // A share whose key doesn't open the copy the server
                    // serves (the owner's re-key is on its way), or a copy
                    // none of the owner's devices signed, keeps what this
                    // device had of it while that opens. Nothing is held
                    // back over it; the next sync takes it again
                    // (`rekey`; docs/plan/51-rekey-push.md §2).
                    let had = self
                        .sharing
                        .received()
                        .get(&r.share_id)
                        .filter(|had| !self.received_opens(&taken) && self.received_opens(had));
                    match had {
                        Some(had) => out.insert(r.share_id, had.clone()),
                        None => out.insert(r.share_id, taken),
                    };
                }
                Err(why) => {
                    self.hold_unverified(super::quiet::Held {
                        kind: super::quiet::Kind::ItemShare,
                        vault: None,
                        who: self.known_other(r.owner.account_id),
                        record: format!("share {}", r.share_id),
                        why,
                    });
                    if let Some(had) = self.sharing.received().get(&r.share_id) {
                        out.insert(r.share_id, had.clone());
                    }
                }
            }
        }
        self.sharing.set_received(out);
        // Senders seen signing post-quantum are held to it from the next
        // sync on, so a share made before their update, seen in this same
        // sync, still arrives.
        let now = lockdown_vault::Timestamp::now().0;
        for account in newly_marked {
            self.sharing.mark_pq_sender(account, now);
        }
        self.rebuild_received()
    }

    /// Checks one item shared on its own and opens its key. Answers the
    /// item to keep and whether its envelope carried a post-quantum
    /// signature that verified under the owner's pinned key. Refused, with
    /// the check's words, when anything doesn't verify, when the server
    /// offers an older copy than this device took, or when its owner is in
    /// `marked` (their shares carried a verified post-quantum signature
    /// before) and this one carries none, unless it is the very envelope
    /// this device took before (D554 closed; docs/plan/49-shared-vault-quiet.md).
    fn take_received(
        &self,
        r: &lockdown_sync::sharing::ReceivedItemWire,
        me: Party,
        kem: &KemSecretKey,
        table: &BTreeMap<Id, (Id, VerifyingKey)>,
        marked: &BTreeSet<Id>,
    ) -> Result<(ReceivedItem, bool), String> {
        let env = ShareEnvelope::from_bytes(&r.envelope).map_err(|e| e.to_string())?;
        let mut item = Item::from_bytes(&r.item.item).map_err(|e| e.to_string())?;
        item.rev = r.item.rev;
        let record = VaultRecord::from_bytes(&r.record).map_err(|e| e.to_string())?;
        let expected = SharePlace {
            purpose: SharePurpose::Item,
            vault_id: record.context.vault_id,
            item_id: Some(item.item_id),
            key_id: item.item_id,
            to: me,
            from_account: r.owner.account_id,
            grant_id: Some(r.share_id),
        };
        let (account, sender) = table
            .get(&env.by_device)
            .cloned()
            .ok_or("the share was signed by a device this device doesn't know")?;
        if account != r.owner.account_id {
            return Err("the share was signed by a device of someone else".into());
        }
        if item.vault_id != record.context.vault_id {
            return Err("the shared item names another vault than its record".into());
        }
        // A post-quantum signature that is there must check out under the
        // owner's pinned key. One that isn't there is refused once the
        // owner's shares carried one, unless this device took this very
        // envelope before.
        let pq_key = self.pq_key_by(r.owner.account_id, env.by_device);
        env.check_post_quantum(pq_key.as_ref(), false)
            .map_err(|e| e.to_string())?;
        let signed = pq_key.is_some() && env.pq_sig.is_some();
        let hash = ByteBuf::from(lockdown_crypto::hash::hash(&r.envelope).as_bytes().to_vec());
        let had = self.sharing.received().get(&r.share_id);
        if !signed
            && marked.contains(&r.owner.account_id)
            && had.and_then(|p| p.envelope_hash.as_ref()) != Some(&hash)
        {
            return Err(
                "the share carries no post-quantum signature, though this person's shares before it did"
                    .into(),
            );
        }
        let key = env
            .open::<ItemKey>(kem, &expected, &sender)
            .map_err(|e| e.to_string())?;
        let n = item.latest().map(|v| v.n).unwrap_or(0);
        let highest = had.map(|p| p.highest_n).unwrap_or(0);
        if n < highest {
            return Err(format!(
                "the server offered version {n} of the item, older than version {highest} this device already took"
            ));
        }
        Ok((
            ReceivedItem {
                owner: r.owner,
                owner_login: r.owner_login.clone(),
                right: r.right,
                ctx: record.context,
                item: ByteBuf::from(item.to_bytes().map_err(|e| e.to_string())?),
                item_key: ByteBuf::from(key.expose().to_vec()),
                highest_n: n,
                envelope_hash: Some(hash),
            },
            signed,
        ))
    }

    fn sync_released(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
    ) -> Result<(), String> {
        // Every grant naming this persona is looked at each sync, so one made
        // before its owner signed post-quantum is known as such when it is
        // released later (`see_grants`). Without a release to take again, a
        // failed fetch only waits for the next sync.
        let list = match transport.emergency(token) {
            Ok(list) => list,
            Err(_) if self.sharing.released().is_empty() => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        self.see_grants(&list.as_contact);
        let grants: Vec<Id> = self.sharing.released().keys().copied().collect();
        for grant_id in grants {
            let still = list.as_contact.iter().any(|w| {
                w.state == EmergencyState::Released
                    && EmergencyGrant::from_bytes(&w.grant).is_ok_and(|g| g.grant_id == grant_id)
            });
            if !still {
                if let Some(r) = self.sharing.remove_released(grant_id) {
                    for v in r.vaults {
                        self.close_vault(v)?;
                    }
                }
                continue;
            }
            self.take_release(transport, token, sth, grant_id)?;
        }
        Ok(())
    }

    // --- items shared on their own -------------------------------------------------

    /// Shares one item with someone pinned here, to view or to fill only.
    pub fn share_item(
        &mut self,
        id: &str,
        login: &str,
        right: &str,
    ) -> Result<SentShareView, String> {
        self.refuse_while_split(Paused::ShareItem)?;
        let item_id = parse_id(id)?;
        let right = right_named(right)?;
        if !matches!(right, Right::View | Right::FillOnly) {
            return Err("an item shared on its own can be viewed or filled, not changed".into());
        }
        let vault_id = self.holding(item_id)?.context().vault_id;
        if self.vault_right(vault_id).is_some() {
            return Err("only items in your own vaults can be shared on their own".into());
        }
        let contact = self.contact_named(login)?;
        if self.item_anywhere(item_id).map_err(|e| e.to_string())?.rev == 0 {
            self.sync()?;
        }
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me_account = self.learn_account(&transport, &token, &sth)?;
        let contact = self.pin(&transport, &token, &sth, Ok(contact.account_id))?;
        let share_id = Id::new();
        let mut env = self
            .holding(item_id)?
            .seal_item_key(
                item_id,
                contact.party(),
                &contact.kem_pub,
                me_account,
                share_id,
            )
            .map_err(|e| e.to_string())?;
        self.pq_sign_envelope(&mut env, None)?;
        transport
            .share_item(
                &token,
                &ItemShareCreate {
                    share_id,
                    vault_id,
                    item_id,
                    recipient: contact.party(),
                    right,
                    envelope: ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?),
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(SentShareView {
            share_id: share_id.to_string(),
            item_id: item_id.to_string(),
            login: contact.login,
            right: right.as_str().into(),
            created_at: now_secs(),
        })
    }

    /// The items this persona shared on their own.
    pub fn sent_shares(&mut self) -> Result<Vec<SentShareView>, String> {
        let (transport, token, _) = self.online()?;
        let sent = transport.sent_items(&token).map_err(|e| e.to_string())?;
        Ok(sent
            .items
            .into_iter()
            .map(|s| SentShareView {
                share_id: s.share_id.to_string(),
                item_id: s.item_id.to_string(),
                login: s.recipient_login,
                right: s.right.as_str().into(),
                created_at: s.created_at,
            })
            .collect())
    }

    /// Stops sharing an item on its own: the share goes, the item moves to
    /// a fresh Item Key, and anyone it is still shared with gets the new one.
    pub fn unshare_item(&mut self, share_id: &str) -> Result<(), String> {
        let share_id = parse_id(share_id)?;
        let (transport, token, _) = self.online()?;
        let sent = transport.sent_items(&token).map_err(|e| e.to_string())?;
        if let Some(s) = sent.items.iter().find(|s| s.share_id == share_id) {
            transport
                .unshare_item(&token, share_id)
                .map_err(|e| e.to_string())?;
            let (vault_id, item_id) = (s.vault_id, s.item_id);
            self.holding_mut(item_id)?
                .rekey_item(item_id)
                .map_err(|e| e.to_string())?;
            // Anyone it is still shared with gets the new key once the
            // re-keyed item is on the server: at the end of this sync, or a
            // later one (`rekey`; docs/plan/51-rekey-push.md §2).
            self.sharing.mark_reseal(vault_id, false, &[item_id]);
            self.save_sharing()?;
            self.save()?;
            self.sync()?;
            return Ok(());
        }
        // Shared with this persona: it can let go of it too.
        transport
            .unshare_item(&token, share_id)
            .map_err(|e| e.to_string())?;
        let mut received = self.sharing.received().clone();
        received.remove(&share_id);
        self.sharing.set_received(received);
        self.save_sharing()?;
        self.rebuild_received()
    }

    fn received_views(&self) -> Vec<ReceivedView> {
        self.sharing
            .received()
            .iter()
            .filter_map(|(share, r)| {
                let item = Item::from_bytes(&r.item).ok()?;
                Some(ReceivedView {
                    share_id: share.to_string(),
                    item_id: item.item_id.to_string(),
                    from: r.owner_login.clone(),
                    right: r.right.as_str().into(),
                })
            })
            .collect()
    }

    // --- share links ---------------------------------------------------------------

    fn item_payload(&self, id: &str) -> Result<SendPayload, String> {
        let item_id = parse_id(id)?;
        self.refuse_fill_only(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if item.class == lockdown_vault::ItemClass::CrownJewel {
            return Err("a crown jewel never goes into a share link".into());
        }
        if !matches!(item.state, ItemState::Live | ItemState::Archived) {
            return Err("only a live item can be shared".into());
        }
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        let mut title = String::new();
        let mut out = Vec::new();
        for f in &fields {
            if lockdown_vault::is_org_field(&f.field_id)
                || lockdown_vault::is_reserved_field(&f.field_id)
                || f.kind == FieldKind::Passkey
            {
                continue;
            }
            let value = match &f.value {
                FieldValue::Text(t) => t.clone(),
                FieldValue::Url { value, .. } => value.clone(),
                FieldValue::Boolean(b) => if *b { "Yes" } else { "No" }.into(),
                _ => continue,
            };
            if f.field_id == "title" {
                title = value;
                continue;
            }
            if value.is_empty() {
                continue;
            }
            let label = lockdown_vault::template_for(item.kind)
                .field(&f.field_id)
                .map(|t| t.label.to_string())
                .unwrap_or_else(|| f.label.clone());
            out.push(SendField {
                label,
                value,
                secret: is_secret_kind(f.kind),
            });
        }
        Ok(SendPayload {
            v: lockdown_vault::send::SEND_V,
            kind: SendKind::Item,
            title,
            text: None,
            file: None,
            fields: out,
        })
    }

    /// Makes a share link and returns it (the only place the link secret
    /// ever exists outside the URL fragment the person shares).
    pub fn create_send(
        &mut self,
        what: SendWhat,
        password: Option<&[u8]>,
        options: &SendOptions,
    ) -> Result<Revealed, String> {
        self.refuse_while_split(Paused::ShareLink)?;
        if !(1..=30).contains(&options.days) {
            return Err("a share link lasts 1 to 30 days".into());
        }
        let payload = match what {
            SendWhat::Item(id) => {
                let p = self.item_payload(&id)?;
                if let Ok(item) = parse_id(&id) {
                    let _ = self.note_use(item);
                }
                p
            }
            SendWhat::Text { title, mut text } => {
                let t = String::from_utf8(std::mem::take(&mut text))
                    .map_err(|_| "the text isn't valid text".to_string())?;
                let p = SendPayload::text(&title, &t);
                drop(zeroize::Zeroizing::new(t));
                p
            }
            SendWhat::File { name, mime, bytes } => SendPayload {
                v: lockdown_vault::send::SEND_V,
                kind: SendKind::File,
                title: name.clone(),
                text: None,
                file: Some(lockdown_vault::send::SendFile { name, mime, bytes }),
                fields: Vec::new(),
            },
        };
        let (secret, sealed) = seal_send(&payload, password).map_err(|e| e.to_string())?;
        let (transport, token, server) = self.online()?;
        transport
            .create_send(
                &token,
                &SendCreate {
                    send_id: sealed.send_id,
                    ciphertext: ByteBuf::from(sealed.ciphertext),
                    expires_at: now_secs() + i64::from(options.days) * 86_400,
                    max_views: options.max_views,
                    password: sealed.password,
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(Revealed::from_text(&send_link(
            &server,
            sealed.send_id,
            &secret,
        )))
    }

    /// The links this persona made.
    pub fn sends(&mut self) -> Result<Vec<SendView>, String> {
        let (transport, token, _) = self.online()?;
        Ok(transport
            .sends(&token)
            .map_err(|e| e.to_string())?
            .sends
            .into_iter()
            .map(|s| SendView {
                send_id: s.send_id.to_string(),
                created_at: s.created_at,
                expires_at: s.expires_at,
                max_views: s.max_views,
                views: s.views,
                password: s.password,
            })
            .collect())
    }

    /// Deletes a link: it stops opening at once.
    pub fn delete_send(&mut self, send_id: &str) -> Result<(), String> {
        let (transport, token, _) = self.online()?;
        transport
            .delete_send(&token, parse_id(send_id)?)
            .map_err(|e| e.to_string())
    }

    fn hold_opened(&mut self, payload: SendPayload) -> OpenedView {
        let mut values = Vec::new();
        if payload.text.is_some() {
            values.push(OpenedValue {
                label: "Text".into(),
                secret: true,
            });
        }
        for f in &payload.fields {
            values.push(OpenedValue {
                label: f.label.clone(),
                secret: f.secret,
            });
        }
        let view = OpenedView {
            kind: match payload.kind {
                SendKind::Text => "text",
                SendKind::File => "file",
                SendKind::Item => "item",
            }
            .into(),
            title: payload.title.chars().take(NAME_CHARS * 2).collect(),
            values,
            file: payload.file.as_ref().map(|f| OpenedFile {
                name: f.name.clone(),
                mime: f.mime.clone(),
                size: f.bytes.len() as u64,
            }),
        };
        self.opened_send = Some(payload);
        view
    }

    /// Opens a share link (anyone's, on any server): its values are held in
    /// the core until the next one or the lock, fetched one at a time.
    pub fn open_send_link(
        &mut self,
        link: &str,
        password: Option<&[u8]>,
    ) -> Result<OpenedView, String> {
        let (server, send_id, secret) = parse_send_link(link).map_err(|e| e.to_string())?;
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let meta = transport.send_meta(send_id).map_err(|e| e.to_string())?;
        let (access, pk) = match (&meta.password, password.filter(|p| !p.is_empty())) {
            (Some(_), None) => {
                return Err("this link needs the password its sender gave you".into());
            }
            (Some(p), Some(pw)) => {
                let full = lockdown_vault::send::SendPassword {
                    salt: p.salt,
                    params: p.params,
                    verifier: lockdown_vault::Hash32::ZERO,
                };
                let (proof, pk) = send_access(send_id, pw, &full).map_err(|e| e.to_string())?;
                (Some(ByteBuf::from(proof.to_vec())), Some(pk))
            }
            (None, _) => (None, None),
        };
        let opened = transport
            .open_send(send_id, &SendOpen { access })
            .map_err(|e| e.to_string())?;
        let payload = open_send(send_id, &secret, pk.as_deref(), &opened.ciphertext)
            .map_err(|e| e.to_string())?;
        Ok(self.hold_opened(payload))
    }

    /// One value of the opened link or answer, by index.
    pub fn opened_value(&self, index: usize) -> Result<Revealed, String> {
        let p = self.opened_send.as_ref().ok_or("nothing is open")?;
        let mut all: Vec<&str> = Vec::new();
        if let Some(t) = &p.text {
            all.push(t.as_str());
        }
        all.extend(p.fields.iter().map(|f| f.value.as_str()));
        all.get(index)
            .map(|v| Revealed::from_text(v))
            .ok_or_else(|| "there's no such value".into())
    }

    /// The opened link's file.
    pub fn opened_file(&self) -> Result<Revealed, String> {
        let p = self.opened_send.as_ref().ok_or("nothing is open")?;
        p.file
            .as_ref()
            .map(|f| Revealed::new(f.bytes.clone()))
            .ok_or_else(|| "this link carries no file".into())
    }

    /// Keeps what was opened as a note in this persona's first vault (an
    /// item's fields become the note's lines; secrets stay in one hidden
    /// field each).
    pub fn save_opened(&mut self) -> Result<String, String> {
        let p = self.opened_send.as_ref().ok_or("nothing is open")?;
        if p.file.is_some() {
            return Err("save a file where you like instead".into());
        }
        let title = if p.title.trim().is_empty() {
            "Shared with me".to_string()
        } else {
            p.title.clone()
        };
        let mut changes = Vec::new();
        let mut notes = String::new();
        if let Some(t) = &p.text {
            notes.push_str(t.as_str());
        }
        let mut n = 0;
        for f in &p.fields {
            if f.secret {
                changes.push(FieldChange::custom(
                    &f.label,
                    FieldKind::Hidden,
                    lockdown_vault::FieldInput::Secret(zeroize::Zeroizing::new(
                        f.value.as_bytes().to_vec(),
                    )),
                ));
                n += 1;
            } else {
                if !notes.is_empty() {
                    notes.push('\n');
                }
                notes.push_str(&format!("{}: {}", f.label, f.value));
            }
        }
        let _ = n;
        changes.push(FieldChange::text("body", &notes));
        let vault_id = self.vault().context().vault_id;
        let id = self.add_item_in(vault_id, "note", Some(&title), false, changes)?;
        self.opened_send = None;
        Ok(id.to_string())
    }

    // --- request links -------------------------------------------------------------

    /// Makes a request link: someone else answers it with a secret sealed
    /// to this persona's receiving key.
    pub fn create_request(&mut self, label: &str, days: u32) -> Result<Revealed, String> {
        self.refuse_while_split(Paused::RequestLink)?;
        if !(1..=30).contains(&days) {
            return Err("a request stays open 1 to 30 days".into());
        }
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 120 {
            return Err("say in a few words what you're asking for".into());
        }
        let (transport, token, server) = self.online()?;
        let kem = self.kem_public()?;
        let request_id = Id::new();
        transport
            .create_request(
                &token,
                &RequestCreate {
                    request_id,
                    kem_pub: kem.clone(),
                    profile_id: self.identity.profile().profile_id,
                    persona_id: self.persona(),
                    expires_at: now_secs() + i64::from(days) * 86_400,
                },
            )
            .map_err(|e| e.to_string())?;
        let link = request_link(&server, request_id, &kem, label).map_err(|e| e.to_string())?;
        Ok(Revealed::from_text(&link))
    }

    /// The requests this persona made.
    pub fn requests(&mut self) -> Result<Vec<RequestView>, String> {
        let (transport, token, _) = self.online()?;
        Ok(transport
            .requests(&token)
            .map_err(|e| e.to_string())?
            .requests
            .into_iter()
            .map(|r| RequestView {
                request_id: r.request_id.to_string(),
                created_at: r.created_at,
                expires_at: r.expires_at,
                answered_at: r.answered_at,
            })
            .collect())
    }

    /// Opens the answer to one of this persona's requests.
    pub fn open_answer(&mut self, request_id: &str) -> Result<OpenedView, String> {
        let request_id = parse_id(request_id)?;
        let (transport, token, _) = self.online()?;
        let list = transport.requests(&token).map_err(|e| e.to_string())?;
        let r = list
            .requests
            .iter()
            .find(|r| r.request_id == request_id)
            .ok_or("that request isn't yours or has gone")?;
        let answer = r.answer.as_ref().ok_or("nobody has answered yet")?;
        let payload = open_response(
            answer,
            &self.kem_secret()?,
            request_id,
            self.identity.profile().profile_id,
            self.persona(),
        )
        .map_err(|e| e.to_string())?;
        Ok(self.hold_opened(payload))
    }

    /// Deletes a request (and its answer, on the server).
    pub fn delete_request(&mut self, request_id: &str) -> Result<(), String> {
        let (transport, token, _) = self.online()?;
        transport
            .delete_request(&token, parse_id(request_id)?)
            .map_err(|e| e.to_string())
    }

    // --- emergency access ---------------------------------------------------------

    fn emergency_view(&self, w: &EmergencyWire, mine: bool) -> Option<EmergencyView> {
        let grant = EmergencyGrant::from_bytes(&w.grant).ok()?;
        let names: Vec<String> = grant
            .vaults
            .iter()
            .map(|v| {
                self.vault_by_id(*v)
                    .ok()
                    .and_then(|vault| vault.info().ok())
                    .and_then(|i| i.name)
                    .unwrap_or_else(|| "a vault".into())
            })
            .collect();
        let other = if mine {
            &w.contact_login
        } else {
            &w.owner_login
        };
        let you = |a: Id| {
            if Some(a) == self.sharing.account_id() {
                "You".to_string()
            } else {
                other.clone()
            }
        };
        let history = w
            .events
            .iter()
            .map(|e| HistoryLine {
                at: e.at,
                words: match e.kind.as_str() {
                    "created" => format!(
                        "{} named {} an emergency contact",
                        you(e.by_account),
                        if mine { other.clone() } else { "you".into() }
                    ),
                    "accept" => format!("{} agreed to be the emergency contact", you(e.by_account)),
                    "decline" => format!("{} stepped down", you(e.by_account)),
                    "request" => format!(
                        "{} asked for emergency access; the wait started",
                        you(e.by_account)
                    ),
                    "stop" => format!("{} stopped the request", you(e.by_account)),
                    "revoke" => format!("{} ended emergency access", you(e.by_account)),
                    "released" => "The wait ran out and the keys were released".into(),
                    "envelopes" => "The keys were renewed after a vault moved to a new key".into(),
                    other => other.to_string(),
                },
            })
            .collect();
        Some(EmergencyView {
            grant_id: grant.grant_id.to_string(),
            mine,
            other: other.clone(),
            state: match w.state {
                EmergencyState::Invited => "invited",
                EmergencyState::Accepted => "accepted",
                EmergencyState::Requested => "requested",
                EmergencyState::Ready => "ready",
                EmergencyState::Released => "released",
                EmergencyState::Declined => "declined",
                EmergencyState::Revoked => "revoked",
            }
            .into(),
            wait_days: (grant.wait_secs / 86_400) as u32,
            vaults: names,
            release_at: w.release_at,
            history,
        })
    }

    /// Names a trusted contact for emergency access to some of this
    /// persona's vaults, after a wait of `days` the owner can stop. The
    /// contact's fingerprint must have been compared first (04-security §8).
    pub fn add_emergency_contact(
        &mut self,
        login: &str,
        days: u32,
        vaults: &[String],
    ) -> Result<Vec<EmergencyView>, String> {
        self.refuse_while_split(Paused::EmergencyContact)?;
        let contact = self.contact_named(login)?;
        if contact.verified_at.is_none() {
            return Err(format!(
                "compare fingerprints with {} first: an emergency contact's key must be checked in person",
                contact.login
            ));
        }
        let mut ids = Vec::new();
        for v in vaults {
            let id = parse_vault_id(v)?;
            if self.vault_right(id).is_some() {
                return Err("emergency access covers your own vaults only".into());
            }
            self.vault_by_id(id)?;
            ids.push(id);
        }
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me_account = self.learn_account(&transport, &token, &sth)?;
        let contact = self.pin(&transport, &token, &sth, Ok(contact.account_id))?;
        self.sync()?;
        let me = self.me()?;
        let mut grant =
            EmergencyGrant::new(me, contact.party(), days, ids, self.identity.device_id())
                .map_err(|e| e.to_string())?;
        grant.sign(&self.keystore).map_err(|e| e.to_string())?;
        if let Some(key) = self.sharing_pq_key(None)? {
            grant.sign_post_quantum(&key).map_err(|e| e.to_string())?;
        }
        let mut envelopes = Vec::new();
        for v in &grant.vaults {
            let mut env = self
                .vault_by_id(*v)?
                .seal_vault_key(
                    contact.party(),
                    &contact.kem_pub,
                    me_account,
                    SharePurpose::Emergency,
                    Some(grant.grant_id),
                )
                .map_err(|e| e.to_string())?;
            self.pq_sign_envelope(&mut env, None)?;
            envelopes.push(ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?));
        }
        transport
            .create_emergency(
                &token,
                &EmergencyCreate {
                    grant: ByteBuf::from(grant.to_bytes().map_err(|e| e.to_string())?),
                    envelopes,
                },
            )
            .map_err(|e| e.to_string())?;
        self.put_emergency_items(&transport, &token, &grant, &contact)?;
        self.emergency()
    }

    /// Every emergency grant this persona made or is named in.
    pub fn emergency(&mut self) -> Result<Vec<EmergencyView>, String> {
        let (transport, token, _) = self.online()?;
        let list = transport.emergency(&token).map_err(|e| e.to_string())?;
        let mut out: Vec<EmergencyView> = list
            .as_owner
            .iter()
            .filter_map(|w| self.emergency_view(w, true))
            .collect();
        out.extend(
            list.as_contact
                .iter()
                .filter_map(|w| self.emergency_view(w, false)),
        );
        Ok(out)
    }

    /// One step: `accept`, `decline` or `request` by the contact; `stop` or
    /// `revoke` by the owner. Each is signed by this device and recorded.
    pub fn emergency_step(
        &mut self,
        grant: &str,
        action: &str,
    ) -> Result<Vec<EmergencyView>, String> {
        let grant = parse_id(grant)?;
        let action = EmergencyAction::parse(action)
            .ok_or("that isn't a step: accept, decline, request, stop or revoke")?;
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        let me = self.learn_account(&transport, &token, &sth)?;
        let mut step = EmergencyStep::new(grant, action, me, self.identity.device_id());
        step.sign(&self.keystore).map_err(|e| e.to_string())?;
        transport
            .emergency_step(
                &token,
                grant,
                &EmergencyStepPush {
                    step: ByteBuf::from(step.to_bytes().map_err(|e| e.to_string())?),
                },
            )
            .map_err(|e| e.to_string())?;
        if action == EmergencyAction::Decline
            && let Some(r) = self.sharing.remove_released(grant)
        {
            for v in r.vaults {
                self.close_vault(v)?;
            }
            self.save_sharing()?;
        }
        self.emergency()
    }

    /// As the contact, once the wait ran out: takes the released keys and
    /// opens the vaults they cover, to read.
    pub fn emergency_open(&mut self, grant: &str) -> Result<Vec<EmergencyView>, String> {
        self.refuse_while_split(Paused::EmergencyRelease)?;
        let grant = parse_id(grant)?;
        let (transport, token, server) = self.online()?;
        let sth = self.verified_sth(&transport, &token, &server)?;
        self.learn_account(&transport, &token, &sth)?;
        self.take_release(&transport, &token, &sth, grant)?;
        self.save_sharing()?;
        if self
            .unverified
            .has(super::quiet::Kind::Emergency, &format!("grant {grant}"))
        {
            return Err(self.unverified_lines().pop().unwrap_or_default());
        }
        self.sync()?;
        self.emergency()
    }

    fn take_release(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        sth: &SignedTreeHead,
        grant_id: Id,
    ) -> Result<(), String> {
        let list = transport.emergency(token).map_err(|e| e.to_string())?;
        let w = list
            .as_contact
            .iter()
            .find(|w| EmergencyGrant::from_bytes(&w.grant).is_ok_and(|g| g.grant_id == grant_id))
            .ok_or("that grant doesn't name you")?;
        let grant = EmergencyGrant::from_bytes(&w.grant).map_err(|e| e.to_string())?;
        self.pin_all(transport, token, sth, [grant.owner.account_id])?;
        let owner_keys = self.device_keys_of(grant.owner.account_id);
        // A grant that doesn't verify is held back: nothing is released,
        // and what this device already opened from it stays (`quiet`).
        self.unverified
            .forget(super::quiet::Kind::Emergency, &format!("grant {grant_id}"));
        let hold = |s: &mut Self, why: String| {
            s.hold_unverified(super::quiet::Held {
                kind: super::quiet::Kind::Emergency,
                vault: None,
                who: s.known_other(grant.owner.account_id),
                record: format!("grant {grant_id}"),
                why,
            });
        };
        let signed = match self.check_grant(&w.grant, &grant, &owner_keys) {
            Ok(signed) => signed,
            Err(why) => {
                hold(self, why);
                return Ok(());
            }
        };
        let release = transport
            .emergency_release(token, grant_id)
            .map_err(|e| e.to_string())?;
        let mut opened = Vec::new();
        for v in &release.vaults {
            let vault_id = v.summary.vault_id;
            if !grant.vaults.contains(&vault_id) {
                continue;
            }
            let env = ShareEnvelope::from_bytes(&v.envelope).map_err(|e| e.to_string())?;
            let owner = grant.owner.account_id;
            // The keys a grant releases carry its owner's post-quantum
            // signature when the grant does.
            let key = match self.open_vault_envelope(
                &v.envelope,
                vault_id,
                env.place.key_id,
                KeyFor {
                    purpose: SharePurpose::Emergency,
                    grant: Some(grant_id),
                    require_pq: signed,
                },
                &|a| a == owner,
            ) {
                Ok(key) => key,
                Err(why) => {
                    hold(self, why);
                    return Ok(());
                }
            };
            if self.is_open(vault_id) {
                if self.open_by_id(vault_id)?.vault.vault_key_id() != env.place.key_id {
                    self.vault_slot(vault_id)?
                        .adopt_vault_key(env.place.key_id, key);
                }
            } else {
                self.bootstrap_member_vault(
                    transport,
                    token,
                    sth,
                    &v.summary,
                    env.place.key_id,
                    &key,
                )?;
                let keys = owner_keys.clone();
                let vault = self.vault_slot(vault_id)?;
                for (id, k) in keys {
                    vault.trust_device(id, k);
                }
            }
            opened.push(vault_id);
        }
        if signed {
            self.sharing
                .mark_pq_sender(grant.owner.account_id, lockdown_vault::Timestamp::now().0);
        }
        let owner_login = w.owner_login.clone();
        // The crown jewels the owner opted in come with the release (`legacy`).
        self.take_released_items(transport, token, &grant, &opened)?;
        self.sharing.put_released(
            grant_id,
            Released {
                owner: grant.owner,
                owner_login,
                vaults: opened,
            },
        );
        Ok(())
    }

    /// Checks an emergency grant (`bytes`, as the server holds it) under
    /// its owner's devices and post-quantum key. Answers whether it carried
    /// a post-quantum signature that verified. Refused when anything
    /// doesn't verify, or when its owner's shares or grants carried a
    /// verified post-quantum signature before and this one carries none,
    /// unless this device saw this very grant before that (D554 closed;
    /// docs/plan/49-shared-vault-quiet.md).
    fn check_grant(
        &self,
        bytes: &[u8],
        grant: &EmergencyGrant,
        owner_keys: &BTreeMap<Id, VerifyingKey>,
    ) -> Result<bool, String> {
        let signer = owner_keys
            .get(&grant.by_device)
            .ok_or("the grant was signed by a device its owner's identity doesn't list")?;
        grant.verify(signer).map_err(|e| e.to_string())?;
        let owner = grant.owner.account_id;
        let pq_key = self.pq_key_by(owner, grant.by_device);
        grant
            .check_post_quantum(pq_key.as_ref())
            .map_err(|e| e.to_string())?;
        let signed = pq_key.is_some() && grant.pq_sig.is_some();
        let hash = ByteBuf::from(lockdown_crypto::hash::hash(bytes).as_bytes().to_vec());
        if !signed
            && self.sharing.pq_senders().contains_key(&owner)
            && self.sharing.grants_seen().get(&grant.grant_id) != Some(&hash)
        {
            return Err(
                "the grant carries no post-quantum signature, though this person's shares or grants before it did"
                    .into(),
            );
        }
        Ok(signed)
    }

    /// Notes each emergency grant naming this persona the first time this
    /// device sees it: while its owner isn't known to sign post-quantum, as
    /// one that may be released without it later; with a post-quantum
    /// signature that verifies under the owner's pinned key, its owner is
    /// held to one from the next sync on. Grants no longer listed are
    /// forgotten.
    fn see_grants(&mut self, as_contact: &[EmergencyWire]) {
        let marked: BTreeSet<Id> = self.sharing.pq_senders().keys().copied().collect();
        let mut listed = BTreeSet::new();
        let mut signed = BTreeSet::new();
        for w in as_contact {
            let Ok(grant) = EmergencyGrant::from_bytes(&w.grant) else {
                continue;
            };
            listed.insert(grant.grant_id);
            let owner = grant.owner.account_id;
            if let Some(key) = self.pq_key_by(owner, grant.by_device)
                && grant.pq_sig.is_some()
                && grant.check_post_quantum(Some(&key)).is_ok()
            {
                signed.insert(owner);
            }
            if !marked.contains(&owner) {
                let hash = ByteBuf::from(lockdown_crypto::hash::hash(&w.grant).as_bytes().to_vec());
                self.sharing.see_grant(grant.grant_id, hash);
            }
        }
        self.sharing.keep_grants(&listed);
        let now = lockdown_vault::Timestamp::now().0;
        for owner in signed {
            self.sharing.mark_pq_sender(owner, now);
        }
    }

    // --- the Sharing screen ----------------------------------------------------------

    /// Everything the Sharing screen draws. What needs the server (links,
    /// requests, sent items, emergency) is left empty when it can't be reached.
    pub fn sharing_overview(&mut self) -> Result<SharingOverview, String> {
        let online = self.sync.is_some();
        let mut vaults = Vec::new();
        let ids: Vec<Id> = self.rosters.keys().copied().collect();
        for id in ids {
            if let Ok(v) = self.vault_sharing(&id.to_string()) {
                vaults.push(v);
            }
        }
        let (sent, emergency) = if online {
            (
                self.sent_shares().unwrap_or_default(),
                self.emergency().unwrap_or_default(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let waiting_on_you = emergency
            .iter()
            .filter(|e| e.mine && e.state == "requested")
            .count();
        // The code others paste or scan names this account: learn it from the
        // directory (checked against the log) the first time, so a device
        // that never shared yet still shows a whole code.
        if online && self.sharing.account_id().is_none() {
            let _ = self.learn_own_account();
        }
        let (fingerprint, fingerprint_qr) = self.own_fingerprint().unwrap_or_default();
        Ok(SharingOverview {
            paused: self.sharing_paused(),
            online,
            fingerprint,
            fingerprint_qr,
            contacts: self.contacts(),
            vaults,
            households: self.households(),
            sent,
            received: self.received_views(),
            emergency,
            waiting_on_you,
            resets: self.key_resets(),
        })
    }
}

/// Answers someone's request link: the secret is sealed on this device to
/// the key the link pins, and the server takes it once. Needs no vault.
pub fn answer_request(link: &str, title: &str, secret: &[u8]) -> Result<(), String> {
    let link = parse_request_link(link).map_err(|e| e.to_string())?;
    let transport = HttpTransport::new(&link.server).map_err(|e| e.to_string())?;
    let key = transport
        .request_key(link.request_id)
        .map_err(|e| e.to_string())?;
    if !key.open {
        return Err("this request was already answered or has expired".into());
    }
    let text =
        std::str::from_utf8(secret).map_err(|_| "the secret isn't valid text".to_string())?;
    let payload = SendPayload::text(title, text);
    let sealed = seal_response(
        &link,
        &key.kem_pub,
        key.profile_id,
        key.persona_id,
        &payload,
    )
    .map_err(|e| e.to_string())?;
    transport
        .answer_request(
            link.request_id,
            &RequestAnswer {
                answer: ByteBuf::from(sealed),
            },
        )
        .map_err(|e| e.to_string())
}

/// What a request link asks for, before answering it.
pub fn request_label(link: &str) -> Result<String, String> {
    parse_request_link(link)
        .map(|l| l.label)
        .map_err(|e| e.to_string())
}

// --- crown jewels in emergency access (`high-risk-class`) ------------------------------

impl Session {
    /// Opts a crown jewel in to (or out of) emergency access, as a new
    /// version, and re-seals the held-back set of every grant covering its
    /// vault (the gate in front of turning it on is the App's). Offline,
    /// the choice is kept and the grants catch up at the next change.
    pub fn set_legacy_opt_in(&mut self, id: &str, on: bool) -> Result<u64, String> {
        let item_id = parse_id(id)?;
        let vault_id = self.holding(item_id)?.context().vault_id;
        let n = self
            .holding_mut(item_id)?
            .set_legacy_opt_in(item_id, on)
            .map_err(|e| e.to_string())?;
        self.save()?;
        if self.sync.is_some() {
            self.refresh_emergency_items(Some(vault_id))?;
        }
        Ok(n)
    }

    /// Re-seals the opted-in crown jewels of every grant this persona made
    /// that is still alive (and covers `only`, when given).
    pub(crate) fn refresh_emergency_items(&mut self, only: Option<Id>) -> Result<(), String> {
        let (transport, token, _) = self.online()?;
        let list = transport.emergency(&token).map_err(|e| e.to_string())?;
        for w in &list.as_owner {
            let Ok(grant) = EmergencyGrant::from_bytes(&w.grant) else {
                continue;
            };
            if matches!(w.state, EmergencyState::Declined | EmergencyState::Revoked)
                || only.is_some_and(|v| !grant.vaults.contains(&v))
            {
                continue;
            }
            let Some(contact) = self.sharing.contact(grant.contact.account_id).cloned() else {
                continue;
            };
            self.put_emergency_items(&transport, &token, &grant, &contact)?;
        }
        Ok(())
    }

    fn put_emergency_items(
        &self,
        transport: &HttpTransport,
        token: &str,
        grant: &EmergencyGrant,
        contact: &Contact,
    ) -> Result<(), String> {
        let me = self.me()?;
        let mut envelopes = Vec::new();
        for vault_id in &grant.vaults {
            let vault = self.vault_by_id(*vault_id)?;
            for item_id in vault.legacy_items() {
                let env = vault
                    .seal_for_emergency(
                        item_id,
                        contact.party(),
                        &contact.kem_pub,
                        me.account_id,
                        grant.grant_id,
                    )
                    .map_err(|e| e.to_string())?;
                envelopes.push(ByteBuf::from(env.to_bytes().map_err(|e| e.to_string())?));
            }
        }
        transport
            .emergency_items_put(
                token,
                grant.grant_id,
                &lockdown_sync::account_security::EmergencyItems { envelopes },
            )
            .map_err(|e| e.to_string())
    }

    /// As the contact, after a release: the crown jewels the owner opted
    /// in, each checked (for this grant, this person, a vault it covers,
    /// signed by one of the owner's devices) before its key is taken.
    fn take_released_items(
        &mut self,
        transport: &HttpTransport,
        token: &str,
        grant: &EmergencyGrant,
        opened: &[Id],
    ) -> Result<(), String> {
        let items = transport
            .emergency_items_get(token, grant.grant_id)
            .map_err(|e| e.to_string())?;
        let me = self.me()?;
        let owner_keys = self.device_keys_of(grant.owner.account_id);
        for bytes in &items.envelopes {
            let env = ShareEnvelope::from_bytes(bytes).map_err(|e| e.to_string())?;
            let (Some(item_id), vault_id) = (env.place.item_id, env.place.vault_id) else {
                continue;
            };
            if !opened.contains(&vault_id) {
                continue;
            }
            let expected = SharePlace {
                purpose: SharePurpose::Emergency,
                vault_id,
                item_id: Some(item_id),
                key_id: item_id,
                to: me,
                from_account: grant.owner.account_id,
                grant_id: Some(grant.grant_id),
            };
            let signer = owner_keys.get(&env.by_device).ok_or(
                "a crown jewel's key came from a device its owner's identity doesn't list; refused",
            )?;
            let key: ItemKey = env
                .open(&self.kem_secret()?, &expected, signer)
                .map_err(|e| e.to_string())?;
            self.vault_slot(vault_id)?
                .adopt_released_item(item_id, key)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
