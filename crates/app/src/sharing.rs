//! Sharing, family and emergency access as a shell sees them
//! (`docs/plan/18-sharing.md`): the views every shell draws, and the
//! device's own record of who it pinned and what is shared with it.
//!
//! The record is one file per persona, `sharing-<persona>.cbor` beside
//! `remembered.cbor`, sealed under the persona's App Data Key with
//! XChaCha20-Poly1305: the people this device pinned (their signing and
//! receiving keys, their devices, whether a fingerprint was compared), the
//! signed membership chain of every shared vault the persona is in, and
//! the items others shared on their own with the Item Keys their
//! envelopes carried. Unlike the recency file, a file that doesn't open is
//! an error: it holds the pins that stop a key swap, so silently starting
//! over would let a server swap a key unnoticed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ecosystem_identity::keys::AppDataKey;
use lockdown_crypto::aead::{Sealed, open, seal};
use lockdown_vault::contact::Contact;
use lockdown_vault::sharing::{Party, Right};
use lockdown_vault::{Id, VaultContext};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use zeroize::Zeroize;

const AAD_PREFIX: &[u8] = b"lockdown/sharing/v1";
const SHARING_V: u16 = 1;

/// A vault shared with this persona or by it: its signed membership chain.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultShare {
    /// Every membership event, oldest first, as signed.
    pub events: Vec<ByteBuf>,
    /// Login names by account, as the server named them (display only).
    #[serde(default)]
    pub logins: BTreeMap<Id, String>,
    /// Whether this persona owns it.
    pub owned: bool,
}

/// An item someone shared with this persona on its own.
#[derive(Clone, Serialize, Deserialize)]
pub struct ReceivedItem {
    /// Who shared it.
    pub owner: Party,
    /// Their login name.
    pub owner_login: String,
    /// What this persona may do with it.
    pub right: Right,
    /// The owner's vault context, which every field is bound to.
    pub ctx: VaultContext,
    /// The item envelope as last fetched (revision included).
    pub item: ByteBuf,
    /// The Item Key the share envelope carried.
    pub item_key: ByteBuf,
    /// The newest version number seen; an older one is refused.
    pub highest_n: u64,
}

impl Drop for ReceivedItem {
    fn drop(&mut self) {
        self.item_key.as_mut_slice().zeroize();
    }
}

impl std::fmt::Debug for ReceivedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceivedItem")
            .field("owner_login", &self.owner_login)
            .field("right", &self.right)
            .finish_non_exhaustive()
    }
}

/// Vaults an emergency contact may read after a release.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Released {
    /// The owner.
    pub owner: Party,
    /// Their login name.
    pub owner_login: String,
    /// The vaults opened here.
    pub vaults: Vec<Id>,
}

#[derive(Default, Serialize, Deserialize)]
struct Body {
    v: u16,
    /// This persona's account on the server, once learned.
    account_id: Option<Id>,
    /// People pinned, by account.
    contacts: BTreeMap<Id, Contact>,
    /// Shared vaults, by vault.
    vaults: BTreeMap<Id, VaultShare>,
    /// Items shared on their own, by share.
    received: BTreeMap<Id, ReceivedItem>,
    /// Emergency releases, by grant.
    #[serde(default)]
    released: BTreeMap<Id, Released>,
}

/// The device's sharing record for one persona.
pub struct SharingState {
    path: PathBuf,
    persona: Id,
    body: Body,
}

impl std::fmt::Debug for SharingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharingState")
            .field("contacts", &self.body.contacts.len())
            .field("vaults", &self.body.vaults.len())
            .finish_non_exhaustive()
    }
}

fn aad(persona: Id) -> Vec<u8> {
    let mut out = AAD_PREFIX.to_vec();
    out.extend_from_slice(persona.as_bytes());
    out
}

impl SharingState {
    /// Where the record for `persona` lives in `data`.
    pub fn path_in(data: &Path, persona: Id) -> PathBuf {
        data.join(format!("sharing-{persona}.cbor"))
    }

    /// Reads the record, or starts an empty one when there is none. A
    /// record that doesn't open is refused, never replaced.
    pub fn open(path: PathBuf, persona: Id, key: &AppDataKey) -> Result<Self, String> {
        let body = match std::fs::read(&path) {
            Ok(bytes) => {
                let sealed: Sealed = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| format!("{} didn't decode", path.display()))?;
                let plain = open(key, &aad(persona), &sealed).map_err(|_| {
                    format!(
                        "{} didn't open with this persona's key; it was left as it is",
                        path.display()
                    )
                })?;
                let body: Body = lockdown_crypto::cbor::from_slice(&plain)
                    .map_err(|_| format!("{} didn't decode", path.display()))?;
                if body.v > SHARING_V {
                    return Err(format!(
                        "{} was written by a newer Lockdown; update the app",
                        path.display()
                    ));
                }
                body
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Body {
                v: SHARING_V,
                ..Body::default()
            },
            Err(e) => return Err(format!("couldn't read {}: {e}", path.display())),
        };
        Ok(Self {
            path,
            persona,
            body,
        })
    }

    /// Writes the record.
    pub fn save(&self, key: &AppDataKey) -> Result<(), String> {
        let plain = zeroize::Zeroizing::new(
            lockdown_crypto::cbor::to_canonical(&self.body).map_err(|e| e.to_string())?,
        );
        let sealed = seal(key, &aad(self.persona), &plain).map_err(|e| e.to_string())?;
        let bytes = lockdown_crypto::cbor::to_canonical(&sealed).map_err(|e| e.to_string())?;
        lockdown_store::write_private(&self.path, &bytes)
            .map_err(|e| format!("couldn't write {}: {e}", self.path.display()))
    }

    /// This persona's account, once learned.
    pub fn account_id(&self) -> Option<Id> {
        self.body.account_id
    }

    /// Remembers this persona's account.
    pub fn set_account_id(&mut self, id: Id) {
        self.body.account_id = Some(id);
    }

    /// The people pinned, by account.
    pub fn contacts(&self) -> &BTreeMap<Id, Contact> {
        &self.body.contacts
    }

    /// One person pinned.
    pub fn contact(&self, account: Id) -> Option<&Contact> {
        self.body.contacts.get(&account)
    }

    /// One person pinned, by login name.
    pub fn contact_by_login(&self, login: &str) -> Option<&Contact> {
        let login = login.trim();
        self.body.contacts.values().find(|c| c.login == login)
    }

    /// Pins or moves on a contact (the caller checked it continues).
    pub fn put_contact(&mut self, contact: Contact) {
        self.body.contacts.insert(contact.account_id, contact);
    }

    /// Forgets a contact.
    pub fn remove_contact(&mut self, account: Id) -> bool {
        self.body.contacts.remove(&account).is_some()
    }

    /// The shared vaults.
    pub fn vaults(&self) -> &BTreeMap<Id, VaultShare> {
        &self.body.vaults
    }

    /// One shared vault.
    pub fn vault(&self, vault_id: Id) -> Option<&VaultShare> {
        self.body.vaults.get(&vault_id)
    }

    /// One shared vault, to change.
    pub fn vault_mut(&mut self, vault_id: Id) -> &mut VaultShare {
        self.body.vaults.entry(vault_id).or_default()
    }

    /// Forgets a shared vault.
    pub fn remove_vault(&mut self, vault_id: Id) -> Option<VaultShare> {
        self.body.vaults.remove(&vault_id)
    }

    /// Items shared with this persona, by share.
    pub fn received(&self) -> &BTreeMap<Id, ReceivedItem> {
        &self.body.received
    }

    /// Replaces the items shared with this persona.
    pub fn set_received(&mut self, received: BTreeMap<Id, ReceivedItem>) {
        self.body.received = received;
    }

    /// Emergency releases opened here.
    pub fn released(&self) -> &BTreeMap<Id, Released> {
        &self.body.released
    }

    /// Records an emergency release.
    pub fn put_released(&mut self, grant: Id, released: Released) {
        self.body.released.insert(grant, released);
    }

    /// Forgets an emergency release.
    pub fn remove_released(&mut self, grant: Id) -> Option<Released> {
        self.body.released.remove(&grant)
    }
}

// --- what a shell draws ------------------------------------------------------------

/// Another person, as pinned on this device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContactView {
    /// Their account on the server.
    pub account_id: String,
    /// Their login name.
    pub login: String,
    /// The persona shares go to.
    pub persona: String,
    /// Six groups of five digits to compare with them.
    pub fingerprint: String,
    /// The QR payload of the same fingerprint (`lockdown:contact:v1:…`).
    pub qr: String,
    /// Whether the person confirmed the fingerprint matched.
    pub verified: bool,
    /// How many of their devices may sign.
    pub devices: usize,
    /// The key-transparency log entry their keys were checked against.
    pub log_index: u64,
    /// Whether their identity signs with a post-quantum key beside
    /// Ed25519, pinned here (D248; additive).
    pub post_quantum: bool,
}

impl ContactView {
    /// The view of a pinned contact.
    pub fn of(c: &Contact) -> Self {
        let fp = c.fingerprint().ok();
        Self {
            account_id: c.account_id.to_string(),
            login: c.login.clone(),
            persona: c.persona_label.clone(),
            fingerprint: fp.as_ref().map(|f| f.digits.clone()).unwrap_or_default(),
            qr: fp.map(|f| f.qr).unwrap_or_default(),
            verified: c.verified_at.is_some(),
            devices: c.devices.values().filter(|d| !d.revoked).count(),
            log_index: c.log_index,
            post_quantum: c.pq_isk.is_some(),
        }
    }
}

/// One person in a shared vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MemberView {
    /// Their account.
    pub account_id: String,
    /// Their login name.
    pub login: String,
    /// `fill-only`, `view`, `edit` or `manage`.
    pub right: String,
    /// What that right allows, in words.
    pub right_words: String,
    /// Whether they own the vault (organize the household).
    pub owner: bool,
    /// Whether this is the person using this device.
    pub you: bool,
    /// Whether their fingerprint was compared here.
    pub verified: bool,
}

/// A vault's sharing, for its panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VaultSharingView {
    /// The vault.
    pub vault_id: String,
    /// Whether it is shared at all.
    pub shared: bool,
    /// `shared` or `family`.
    pub kind: String,
    /// What this person may do.
    pub right: String,
    /// Whether this person may add, remove and change members.
    pub can_manage: bool,
    /// Everyone in it, owner first.
    pub members: Vec<MemberView>,
    /// Every membership change, oldest first, in words, with the time.
    pub history: Vec<HistoryLine>,
    /// Why it can't be shared yet, if it can't (a crown jewel inside).
    pub blocked: Option<String>,
}

/// One line of a record: when and what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryLine {
    /// Seconds since the epoch.
    pub at: i64,
    /// What happened, in words.
    pub words: String,
}

/// One item this persona shared on its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SentShareView {
    /// The share.
    pub share_id: String,
    /// The item.
    pub item_id: String,
    /// Who.
    pub login: String,
    /// `view` or `fill-only`.
    pub right: String,
    /// When.
    pub created_at: i64,
}

/// One share link this persona made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SendView {
    /// The link's id (the secret is not kept anywhere).
    pub send_id: String,
    /// When it was made.
    pub created_at: i64,
    /// When it stops opening.
    pub expires_at: i64,
    /// The view limit.
    pub max_views: Option<u32>,
    /// Opens so far.
    pub views: u32,
    /// Whether it needs a password.
    pub password: bool,
}

/// What a person asks for when making a share link.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SendOptions {
    /// Days until it stops opening (1–30).
    pub days: u32,
    /// How many times it may open; `None` for no limit before expiry.
    pub max_views: Option<u32>,
}

/// What an opened share link or answer holds, without its values: those
/// are fetched one at a time as bytes ([`crate::App::opened_value`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OpenedView {
    /// `text`, `file` or `item`.
    pub kind: String,
    /// Its title.
    pub title: String,
    /// The labelled values, by index; the text, when there is one, is index 0.
    pub values: Vec<OpenedValue>,
    /// The file, when there is one.
    pub file: Option<OpenedFile>,
}

/// One value of an opened link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OpenedValue {
    /// What it is.
    pub label: String,
    /// Whether to mask it until asked.
    pub secret: bool,
}

/// The file of an opened link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OpenedFile {
    /// Its name.
    pub name: String,
    /// Its media type.
    pub mime: String,
    /// Its size in bytes.
    pub size: u64,
}

/// One request link this persona made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RequestView {
    /// The request.
    pub request_id: String,
    /// When it was made.
    pub created_at: i64,
    /// When it stops taking an answer.
    pub expires_at: i64,
    /// When it was answered.
    pub answered_at: Option<i64>,
}

/// One emergency grant, as either side sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmergencyView {
    /// The grant.
    pub grant_id: String,
    /// Whether this person made it (else they are the contact).
    pub mine: bool,
    /// The other person's login.
    pub other: String,
    /// `invited`, `accepted`, `requested`, `ready`, `released`, `declined`, `revoked`.
    pub state: String,
    /// The waiting period in days.
    pub wait_days: u32,
    /// The vaults it covers, by name where this device knows it.
    pub vaults: Vec<String>,
    /// When the keys may be released, while a request waits.
    pub release_at: Option<i64>,
    /// Every step, in words, with the server's time.
    pub history: Vec<HistoryLine>,
}

/// A household (02-model §1 "Family"): its vault and its people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HouseholdView {
    /// The household vault.
    pub vault_id: String,
    /// Its name.
    pub name: String,
    /// The organizer's login.
    pub organizer: String,
    /// Whether this person organizes it.
    pub organizing: bool,
    /// Everyone in it.
    pub members: Vec<MemberView>,
}

/// Everything on the Sharing screen at once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SharingOverview {
    /// While the server's public log disagrees with a mirror: what is
    /// paused and why, in words (D328 as changed by D375). Added under D62.
    pub paused: Option<String>,
    /// Whether an account (and so a server) is here; sharing needs one.
    pub online: bool,
    /// This person's own fingerprint, for someone comparing it with theirs.
    pub fingerprint: String,
    /// The same as a QR payload.
    pub fingerprint_qr: String,
    /// The people pinned here.
    pub contacts: Vec<ContactView>,
    /// Every shared vault this persona is in.
    pub vaults: Vec<VaultSharingView>,
    /// Households.
    pub households: Vec<HouseholdView>,
    /// Items this persona shared on their own.
    pub sent: Vec<SentShareView>,
    /// Items shared with this persona on their own: id, title, from whom, right.
    pub received: Vec<ReceivedView>,
    /// Emergency grants either way.
    pub emergency: Vec<EmergencyView>,
    /// Emergency requests waiting on this person to stop them, if they want.
    pub waiting_on_you: usize,
}

/// An item shared with this persona on its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReceivedView {
    /// The share.
    pub share_id: String,
    /// The item.
    pub item_id: String,
    /// Who shared it.
    pub from: String,
    /// `view` or `fill-only`.
    pub right: String,
}

// --- the command surface (docs/plan/18-sharing.md §8) -------------------------

use crate::session::sharing::{SendWhat, answer_request, request_label};
use crate::session::split_pause::Paused;
use crate::{App, AppError, AppResult, ApprovalKind, Core, GateInput, Revealed};
use lockdown_vault::sharing::VaultKind;

/// A paused step is refused before its gate asks for anything (the session
/// refuses it again on its own; split_pause).
fn paused(core: &mut Core, step: Paused) -> AppResult<()> {
    core.session()?
        .refuse_while_split(step)
        .map_err(AppError::refused)
}

impl App {
    /// Everything the Sharing screen draws.
    pub fn sharing_overview(&self) -> AppResult<SharingOverview> {
        self.with_session(|s| s.sharing_overview())
    }

    /// Looks someone up by login name, checks their keys against the
    /// server's key-transparency log, and pins them.
    pub fn lookup_contact(&self, login: &str) -> AppResult<ContactView> {
        let login = login.to_string();
        self.with_session(move |s| s.lookup_contact(&login))
    }

    /// Records that the person compared fingerprints and they matched.
    pub fn verify_contact(&self, account: &str, fingerprint: &str) -> AppResult<ContactView> {
        let (account, fingerprint) = (account.to_string(), fingerprint.to_string());
        self.with_session(move |s| s.verify_contact(&account, &fingerprint))
    }

    /// Forgets someone pinned here.
    pub fn forget_contact(&self, account: &str) -> AppResult<()> {
        let account = account.to_string();
        self.with_session(move |s| s.forget_contact(&account))
    }

    /// One vault's sharing.
    pub fn vault_sharing(&self, vault_id: &str) -> AppResult<VaultSharingView> {
        let vault_id = vault_id.to_string();
        self.with_session(move |s| s.vault_sharing(&vault_id))
    }

    /// Shares a vault with someone pinned here, at a right. Handing the
    /// Vault Key to another person goes through the `Approve` gate.
    pub fn share_vault(
        &self,
        vault_id: &str,
        login: &str,
        right: &str,
        gate: GateInput,
    ) -> AppResult<VaultSharingView> {
        let (vault_id, login, right) = (vault_id.to_string(), login.to_string(), right.to_string());
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            paused(core, Paused::ShareVault)?;
            core.approve(ApprovalKind::Approve, "share this vault", &gate)?;
            core.session()?
                .share_vault(&vault_id, &login, &right, VaultKind::Shared)
                .map_err(AppError::refused)
        })
    }

    /// Changes a member's right.
    pub fn set_member_right(
        &self,
        vault_id: &str,
        account: &str,
        right: &str,
    ) -> AppResult<VaultSharingView> {
        let (vault_id, account, right) =
            (vault_id.to_string(), account.to_string(), right.to_string());
        self.with_session(move |s| s.set_member_right(&vault_id, &account, &right))
    }

    /// Removes a member; the vault moves to new keys at once.
    pub fn remove_member(&self, vault_id: &str, account: &str) -> AppResult<VaultSharingView> {
        let (vault_id, account) = (vault_id.to_string(), account.to_string());
        self.with_session(move |s| s.remove_member(&vault_id, &account))
    }

    /// Makes a household vault with this person as its organizer.
    pub fn create_household(&self, name: &str) -> AppResult<String> {
        let name = name.to_string();
        self.with_session(move |s| s.create_household(&name))
    }

    /// Adds someone pinned here to a household (the household's vault).
    pub fn add_to_household(
        &self,
        vault_id: &str,
        login: &str,
        right: &str,
        gate: GateInput,
    ) -> AppResult<VaultSharingView> {
        let (vault_id, login, right) = (vault_id.to_string(), login.to_string(), right.to_string());
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            paused(core, Paused::ShareVault)?;
            core.approve(
                ApprovalKind::Approve,
                "add someone to your household",
                &gate,
            )?;
            core.session()?
                .share_vault(&vault_id, &login, &right, VaultKind::Family)
                .map_err(AppError::refused)
        })
    }

    /// Shares one item on its own, to view or fill only, through the
    /// `Approve` gate.
    pub fn share_item(
        &self,
        id: &str,
        login: &str,
        right: &str,
        gate: GateInput,
    ) -> AppResult<SentShareView> {
        let (id, login, right) = (id.to_string(), login.to_string(), right.to_string());
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            if core
                .session()?
                .is_crown_jewel(&id)
                .map_err(AppError::refused)?
            {
                return Err(AppError::refused("a crown jewel is never shared"));
            }
            paused(core, Paused::ShareItem)?;
            core.approve(ApprovalKind::Approve, "share this item", &gate)?;
            core.session()?
                .share_item(&id, &login, &right)
                .map_err(AppError::refused)
        })
    }

    /// Stops sharing an item on its own (or lets go of one shared with you).
    pub fn unshare_item(&self, share_id: &str) -> AppResult<()> {
        let share_id = share_id.to_string();
        self.with_session(move |s| s.unshare_item(&share_id))
    }

    /// A share link for an item's fields as they are now, through the
    /// `Approve` gate (it hands the item's secrets to whoever holds the
    /// link). Returns the link, once.
    pub fn send_item(
        &self,
        id: &str,
        password: Option<Vec<u8>>,
        options: SendOptions,
        gate: GateInput,
    ) -> AppResult<Revealed> {
        let id = id.to_string();
        let password = password.map(zeroize::Zeroizing::new);
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            core.session()?
                .refuse_fill_only(&id)
                .map_err(AppError::refused)?;
            if core
                .session()?
                .is_crown_jewel(&id)
                .map_err(AppError::refused)?
            {
                return Err(AppError::refused(
                    "a crown jewel never goes into a share link",
                ));
            }
            paused(core, Paused::ShareLink)?;
            core.approve(
                ApprovalKind::Approve,
                "put this item in a share link",
                &gate,
            )?;
            // The session notes the use (D151): the item's secrets left the vault.
            core.session()?
                .create_send(
                    SendWhat::Item(id),
                    password.as_deref().map(Vec::as_slice),
                    &options,
                )
                .map_err(AppError::refused)
        })
    }

    /// A share link for some text. Returns the link, once.
    pub fn send_text(
        &self,
        title: &str,
        text: Vec<u8>,
        password: Option<Vec<u8>>,
        options: SendOptions,
    ) -> AppResult<Revealed> {
        let title = title.to_string();
        let password = password.map(zeroize::Zeroizing::new);
        self.with_session(move |s| {
            s.create_send(
                SendWhat::Text { title, text },
                password.as_deref().map(Vec::as_slice),
                &options,
            )
        })
    }

    /// A share link for a file. Returns the link, once.
    pub fn send_file(
        &self,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
        password: Option<Vec<u8>>,
        options: SendOptions,
    ) -> AppResult<Revealed> {
        let (name, mime) = (name.to_string(), mime.to_string());
        let password = password.map(zeroize::Zeroizing::new);
        self.with_session(move |s| {
            s.create_send(
                SendWhat::File { name, mime, bytes },
                password.as_deref().map(Vec::as_slice),
                &options,
            )
        })
    }

    /// The share links this person made.
    pub fn sends(&self) -> AppResult<Vec<SendView>> {
        self.with_session(|s| s.sends())
    }

    /// Deletes a share link.
    pub fn delete_send(&self, send_id: &str) -> AppResult<()> {
        let send_id = send_id.to_string();
        self.with_session(move |s| s.delete_send(&send_id))
    }

    /// Opens a share link; its values stay in the core until fetched.
    pub fn open_send(&self, link: &[u8], password: Option<Vec<u8>>) -> AppResult<OpenedView> {
        let link =
            String::from_utf8(link.to_vec()).map_err(|_| AppError::refused("that isn't a link"))?;
        let password = password.map(zeroize::Zeroizing::new);
        self.with_session(move |s| s.open_send_link(&link, password.as_deref().map(Vec::as_slice)))
    }

    /// One value of what was opened, as bytes.
    pub fn opened_value(&self, index: usize) -> AppResult<Revealed> {
        self.with_session(move |s| s.opened_value(index))
    }

    /// The file of what was opened, as bytes.
    pub fn opened_file(&self) -> AppResult<Revealed> {
        self.with_session(|s| s.opened_file())
    }

    /// Keeps what was opened as a note in the first vault.
    pub fn save_opened(&self) -> AppResult<String> {
        self.with_session(|s| s.save_opened())
    }

    /// A request link: someone answers it with a secret only you can open.
    pub fn create_request(&self, label: &str, days: u32) -> AppResult<Revealed> {
        let label = label.to_string();
        self.with_session(move |s| s.create_request(&label, days))
    }

    /// The request links this person made.
    pub fn requests(&self) -> AppResult<Vec<RequestView>> {
        self.with_session(|s| s.requests())
    }

    /// Opens the answer to a request.
    pub fn open_answer(&self, request_id: &str) -> AppResult<OpenedView> {
        let request_id = request_id.to_string();
        self.with_session(move |s| s.open_answer(&request_id))
    }

    /// Deletes a request.
    pub fn delete_request(&self, request_id: &str) -> AppResult<()> {
        let request_id = request_id.to_string();
        self.with_session(move |s| s.delete_request(&request_id))
    }

    /// What someone's request link asks for.
    pub fn request_label(&self, link: &[u8]) -> AppResult<String> {
        let link =
            String::from_utf8(link.to_vec()).map_err(|_| AppError::refused("that isn't a link"))?;
        request_label(&link).map_err(AppError::refused)
    }

    /// Answers someone's request link with a secret, sealed here to the key
    /// the link pins. Needs no vault.
    pub fn answer_request(&self, link: &[u8], title: &str, secret: Vec<u8>) -> AppResult<()> {
        let link =
            String::from_utf8(link.to_vec()).map_err(|_| AppError::refused("that isn't a link"))?;
        let secret = zeroize::Zeroizing::new(secret);
        answer_request(&link, title, &secret).map_err(AppError::refused)
    }

    /// Names an emergency contact (their fingerprint compared first), for
    /// some vaults, after a wait of `days`; through the `Approve` gate.
    pub fn add_emergency_contact(
        &self,
        login: &str,
        days: u32,
        vaults: Vec<String>,
        gate: GateInput,
    ) -> AppResult<Vec<EmergencyView>> {
        let login = login.to_string();
        self.call(move |core| {
            core.clock.touch(std::time::Instant::now());
            paused(core, Paused::EmergencyContact)?;
            core.approve(ApprovalKind::Approve, "name an emergency contact", &gate)?;
            core.session()?
                .add_emergency_contact(&login, days, &vaults)
                .map_err(AppError::refused)
        })
    }

    /// Every emergency grant either way.
    pub fn emergency(&self) -> AppResult<Vec<EmergencyView>> {
        self.with_session(|s| s.emergency())
    }

    /// One emergency step: accept, decline, request, stop or revoke.
    pub fn emergency_step(&self, grant: &str, action: &str) -> AppResult<Vec<EmergencyView>> {
        let (grant, action) = (grant.to_string(), action.to_string());
        self.with_session(move |s| s.emergency_step(&grant, &action))
    }

    /// As the contact, after the wait: opens the released vaults to read.
    pub fn emergency_open(&self, grant: &str) -> AppResult<Vec<EmergencyView>> {
        let grant = grant.to_string();
        self.with_session(move |s| s.emergency_open(&grant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::SymmetricKey;

    #[test]
    fn the_record_is_sealed_and_a_wrong_key_is_an_error_not_a_reset() {
        let dir = tempfile::tempdir().unwrap();
        let key = AppDataKey::from_bytes([5; 32]);
        let persona = Id::new();
        let path = SharingState::path_in(dir.path(), persona);
        let mut s = SharingState::open(path.clone(), persona, &key).unwrap();
        let account = Id::new();
        s.set_account_id(account);
        s.vault_mut(Id::new()).owned = true;
        s.save(&key).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes.windows(16).any(|w| w == account.as_bytes()),
            "sealed"
        );
        let back = SharingState::open(path.clone(), persona, &key).unwrap();
        assert_eq!(back.account_id(), Some(account));
        assert_eq!(back.vaults().len(), 1);
        assert!(
            SharingState::open(path.clone(), persona, &AppDataKey::from_bytes([6; 32])).is_err()
        );
        assert!(SharingState::open(path, Id::new(), &key).is_err());
    }
}
