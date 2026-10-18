//! Sharing a vault with other people (02-model §1 "Shared" and "Family";
//! 03-architecture §6 "membership is server-ordered, never merged";
//! 04-security §6, §18; `docs/plan/18-sharing.md`).
//!
//! Three pieces, each signed by the device that made it so a hostile
//! server can store and order them but never forge one:
//!
//! - [`MembershipEvent`]: one change to who is in a shared vault and what
//!   each may do, chained by hash like a manifest head. [`Roster`] replays
//!   the chain and refuses any event an account without the `manage` right
//!   signed, any removal that does not move the vault to a fresh Vault Key
//!   generation, and any return to an earlier generation.
//! - [`ShareEnvelope`]: a 256-bit key (a Vault Key generation, an Item Key,
//!   or a Vault Key held back for emergency access) wrapped to another
//!   persona's receiving key with the identity crate's peer wrap
//!   (`x-wing-xchacha20poly1305`, purpose `share`), inside a Lockdown
//!   envelope that names exactly where the key belongs and is signed by the
//!   sending device. A public-key wrap alone is unauthenticated (anyone can
//!   encrypt to a public key, the server included); the signature is what
//!   makes a swapped key fail (04-security §18, the ETH finding on
//!   unauthenticated sharing keys).
//! - [`Right`]: what a member may do, decided here so every shell says and
//!   enforces the same thing.
//!
//! Fill-only is a promise honest clients keep, not cryptography: a member
//! holds the Vault Key and a modified client could read what it fills. The
//! core enforces it (no reveal, no copy, no export, no code view); a
//! person who must not learn a password should not be given the vault.

use std::collections::BTreeMap;

use ecosystem_identity::SymmetricKey;
use ecosystem_identity::kem::{KemPublicKey, KemSecretKey};
use ecosystem_identity::keys::{aad as identity_aad, unwrap_key_from_peer, wrap_to_peer};
use ecosystem_identity::records::{KeyEnvelope, KeyRef, KeyRefKind, Purpose, Record};
use lockdown_crypto::VerifyingKey;
use lockdown_crypto::cbor::to_canonical;
use lockdown_crypto::hash::hash;
use serde::{Deserialize, Serialize};

use crate::signer::Signer;
use crate::{Error, Hash32, Id, Result, Timestamp};

/// The format version of membership events and share envelopes.
pub const SHARING_V: u16 = 1;

/// Domain separation for the membership event signature.
const EVENT_SIG_PREFIX: &[u8] = b"lockdown-membership-v1\0";
/// Domain separation for the share envelope signature.
const SHARE_SIG_PREFIX: &[u8] = b"lockdown-share-v1\0";

/// The most members a shared vault holds on this build (owner included).
pub const MAX_MEMBERS: usize = 50;

/// What a member may do with a shared vault or item, weakest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Right {
    /// The browser extension may fill it; nothing is ever shown or copied.
    FillOnly,
    /// See, reveal, copy and fill; no changes.
    View,
    /// Everything `view` allows, and add, change and delete items.
    Edit,
    /// Everything `edit` allows, and add or remove members and change rights.
    Manage,
}

impl Right {
    /// Every right, weakest first.
    pub const ALL: [Right; 4] = [Self::FillOnly, Self::View, Self::Edit, Self::Manage];

    /// The word on the wire and in the UI's code.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FillOnly => "fill-only",
            Self::View => "view",
            Self::Edit => "edit",
            Self::Manage => "manage",
        }
    }

    /// The right a word names.
    pub fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == text.trim())
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "'{text}' isn't a right; use fill-only, view, edit or manage"
                ))
            })
    }

    /// What a person reads: one plain sentence per right.
    pub fn describe(self) -> &'static str {
        match self {
            Self::FillOnly => "Can fill sign-ins in the browser, never see or copy them",
            Self::View => "Can see, copy and fill",
            Self::Edit => "Can see, fill and change items",
            Self::Manage => "Can change items and who has access",
        }
    }

    /// Whether a secret may be shown or copied (anything but fill-only).
    pub fn can_reveal(self) -> bool {
        self != Self::FillOnly
    }

    /// Whether items may be added, changed or deleted.
    pub fn can_edit(self) -> bool {
        matches!(self, Self::Edit | Self::Manage)
    }

    /// Whether members may be added, removed or given other rights.
    pub fn can_manage(self) -> bool {
        self == Self::Manage
    }
}

/// One person on the other end of a share: the server's account, and the
/// profile and persona whose receiving key a share is wrapped to (D13: a
/// share to a work persona never opens in a personal one).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct Party {
    /// The account on the server.
    pub account_id: Id,
    /// The person's profile.
    pub profile_id: Id,
    /// The persona that receives.
    pub persona_id: Id,
}

/// What kind of group a shared vault is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VaultKind {
    /// An ad-hoc group.
    Shared,
    /// A household: the owner is its organizer (02-model §1 "Family").
    Family,
}

impl VaultKind {
    /// The word on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Family => "family",
        }
    }
}

/// One change to a shared vault's membership.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Change {
    /// The first event: the vault becomes shared, owned (organized) by `owner`.
    Create {
        /// Who owns the vault; always holds `manage`.
        owner: Party,
        /// A group or a household.
        kind: VaultKind,
    },
    /// A member joins with a right.
    Add {
        /// Who.
        member: Party,
        /// What they may do.
        right: Right,
    },
    /// A member's right changes.
    SetRight {
        /// Whose.
        account_id: Id,
        /// To what.
        right: Right,
    },
    /// A member leaves; the event names the Vault Key generation that
    /// replaces the one they held.
    Remove {
        /// Who.
        account_id: Id,
    },
}

/// One signed event in a shared vault's membership chain.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MembershipEvent {
    /// Format version.
    pub v: u16,
    /// The vault.
    pub vault_id: Id,
    /// Rises by one on every event; the first is 1.
    pub seq: u64,
    /// Hash of the event before; zero for the first.
    pub prev_hash: Hash32,
    /// What changed.
    pub change: Change,
    /// The Vault Key generation in force after this event: the same as
    /// before, except a removal, which must name a fresh one.
    pub key_ref: Id,
    /// The account that made the change.
    pub by_account: Id,
    /// The device that signed it, a device of `by_account`.
    pub by_device: Id,
    /// When.
    pub at: Timestamp,
    /// Ed25519 by `by_device` over the prefix and the event with `sig` empty.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

impl MembershipEvent {
    /// The bytes the signature covers.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig.clear();
        let mut out = EVENT_SIG_PREFIX.to_vec();
        out.extend(to_canonical(&copy)?);
        Ok(out)
    }

    /// Signs with this device.
    pub fn sign(&mut self, signer: &dyn Signer) -> Result<()> {
        self.sig = signer.sign(&self.signable()?)?.to_vec();
        Ok(())
    }

    /// Checks the device signature.
    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        key.verify(
            &self.signable()?,
            &self.sig,
            &format!("membership event {}", self.seq),
        )
        .map_err(|_| Error::ChainBroken {
            seq: self.seq,
            why: "a membership change's device signature didn't check out".into(),
        })
    }

    /// BLAKE3 of the signed event: what the next one names as `prev_hash`.
    pub fn hash(&self) -> Result<Hash32> {
        Ok(hash(&self.to_bytes()?))
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }

    /// One line for a person: what happened, in plain words.
    pub fn describe(&self, name: &dyn Fn(Id) -> String) -> String {
        match &self.change {
            Change::Create { kind, .. } => match kind {
                VaultKind::Shared => {
                    format!("{} started sharing this vault", name(self.by_account))
                }
                VaultKind::Family => {
                    format!("{} made this the household vault", name(self.by_account))
                }
            },
            Change::Add { member, right } => format!(
                "{} added {} ({})",
                name(self.by_account),
                name(member.account_id),
                right.as_str()
            ),
            Change::SetRight { account_id, right } => format!(
                "{} changed {} to {}",
                name(self.by_account),
                name(*account_id),
                right.as_str()
            ),
            Change::Remove { account_id } => format!(
                "{} removed {}; the vault moved to a new key",
                name(self.by_account),
                name(*account_id)
            ),
        }
    }
}

/// One member as the roster holds them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Membership {
    /// Who.
    pub party: Party,
    /// What they may do.
    pub right: Right,
    /// The event that added them.
    pub since: u64,
}

/// A shared vault's membership as its signed chain says it is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Roster {
    /// The vault.
    pub vault_id: Id,
    /// A group or a household.
    pub kind: VaultKind,
    /// The owner (the organizer of a household): always `manage`, never removed.
    pub owner: Party,
    /// Every current member but the owner, by account.
    pub members: BTreeMap<Id, Membership>,
    /// Accounts that were members, with the event that removed them. What
    /// they signed before then still verifies.
    pub former: BTreeMap<Id, (Party, u64)>,
    /// The Vault Key generation in force.
    pub key_ref: Id,
    /// Every generation the chain has named, oldest first.
    pub key_history: Vec<Id>,
    /// Every account that ever held `edit` or `manage` (the owner
    /// included): whose devices' heads and versions verify here.
    pub writers: std::collections::BTreeSet<Id>,
    /// The newest event's `seq`.
    pub seq: u64,
    /// The newest event's hash.
    pub head: Hash32,
}

/// Finds the account and public key of a device: the device lookup a
/// replay verifies signatures with. A client builds it from the signed
/// identity records of the people it pinned; the server from the devices
/// that logged in.
pub type DeviceLookup<'a> = &'a dyn Fn(Id) -> Option<(Id, VerifyingKey)>;

impl Roster {
    /// The first event of a chain: `owner` shares the vault, under the
    /// Vault Key generation `key_ref`. Unsigned.
    pub fn genesis(
        vault_id: Id,
        owner: Party,
        kind: VaultKind,
        key_ref: Id,
        by_device: Id,
    ) -> MembershipEvent {
        MembershipEvent {
            v: SHARING_V,
            vault_id,
            seq: 1,
            prev_hash: Hash32::ZERO,
            change: Change::Create { owner, kind },
            key_ref,
            by_account: owner.account_id,
            by_device,
            at: Timestamp::now(),
            sig: Vec::new(),
        }
    }

    /// Replays a whole chain, oldest first, verifying every event.
    pub fn replay(events: &[MembershipEvent], devices: DeviceLookup<'_>) -> Result<Self> {
        Self::replay_with(events, Some(devices))
    }

    /// Replays a chain whose signatures the caller already checked when it
    /// accepted each event (the server, against its own device table):
    /// every other rule still applies.
    pub fn replay_trusted(events: &[MembershipEvent]) -> Result<Self> {
        Self::replay_with(events, None)
    }

    fn replay_with(events: &[MembershipEvent], devices: Option<DeviceLookup<'_>>) -> Result<Self> {
        let first = events
            .first()
            .ok_or_else(|| Error::NotFound("the vault's membership chain".into()))?;
        let broken = |seq: u64, why: &str| Error::ChainBroken {
            seq,
            why: why.into(),
        };
        let Change::Create { owner, kind } = &first.change else {
            return Err(broken(
                1,
                "the membership chain doesn't start by sharing the vault",
            ));
        };
        if first.seq != 1 || first.prev_hash != Hash32::ZERO {
            return Err(broken(
                first.seq,
                "the first membership change names a predecessor",
            ));
        }
        if first.v > SHARING_V {
            return Err(broken(1, "a newer membership format than this app reads"));
        }
        if first.by_account != owner.account_id {
            return Err(broken(
                1,
                "the vault was shared by someone other than its owner",
            ));
        }
        if let Some(devices) = devices {
            Self::check_signer(first, devices)?;
        }
        let mut roster = Self {
            vault_id: first.vault_id,
            kind: *kind,
            owner: *owner,
            members: BTreeMap::new(),
            former: BTreeMap::new(),
            key_ref: first.key_ref,
            key_history: vec![first.key_ref],
            writers: std::collections::BTreeSet::from([owner.account_id]),
            seq: 1,
            head: first.hash()?,
        };
        for event in &events[1..] {
            match devices {
                Some(devices) => roster.apply(event, devices)?,
                None => roster.apply_trusted(event)?,
            }
        }
        Ok(roster)
    }

    /// [`Self::apply`] for an event whose signature the caller checked.
    pub fn apply_trusted(&mut self, event: &MembershipEvent) -> Result<()> {
        self.check_next(event)?;
        self.apply_checked(event)
    }

    fn check_signer(event: &MembershipEvent, devices: DeviceLookup<'_>) -> Result<()> {
        let (account, key) =
            devices(event.by_device).ok_or(Error::UnknownDevice(event.by_device))?;
        if account != event.by_account {
            return Err(Error::ChainBroken {
                seq: event.seq,
                why: "a membership change was signed by a device of another account".into(),
            });
        }
        event.verify(&key)
    }

    /// Takes one more event, after every check: it continues the chain,
    /// its signer is a device of an account that may manage the vault, and
    /// the change itself is allowed.
    pub fn apply(&mut self, event: &MembershipEvent, devices: DeviceLookup<'_>) -> Result<()> {
        self.check_next(event)?;
        Self::check_signer(event, devices)?;
        self.apply_checked(event)
    }

    /// The rules [`Self::apply`] checks besides the signature.
    pub fn check_next(&self, event: &MembershipEvent) -> Result<()> {
        let broken = |why: String| Error::ChainBroken {
            seq: event.seq,
            why,
        };
        if event.v > SHARING_V {
            return Err(broken(
                "a newer membership format than this app reads".into(),
            ));
        }
        if event.vault_id != self.vault_id {
            return Err(broken("a membership change for another vault".into()));
        }
        if event.seq != self.seq + 1 {
            return Err(broken(format!(
                "membership change {} doesn't follow {}",
                event.seq, self.seq
            )));
        }
        if event.prev_hash != self.head {
            return Err(broken(
                "a membership change doesn't continue the chain this device holds".into(),
            ));
        }
        if !self
            .right_of(event.by_account)
            .is_some_and(Right::can_manage)
        {
            return Err(broken(
                "a membership change was made by an account that can't manage the vault".into(),
            ));
        }
        match &event.change {
            Change::Create { .. } => Err(broken("the vault was shared twice".into())),
            Change::Add { member, .. } => {
                if member.account_id == self.owner.account_id
                    || self.members.contains_key(&member.account_id)
                {
                    return Err(broken("that person is already in the vault".into()));
                }
                if self.members.len() + 1 >= MAX_MEMBERS {
                    return Err(broken(format!(
                        "a shared vault holds at most {MAX_MEMBERS} people"
                    )));
                }
                self.same_key(event)
            }
            Change::SetRight { account_id, .. } => {
                if !self.members.contains_key(account_id) {
                    return Err(broken(
                        "a right was changed for someone who isn't a member".into(),
                    ));
                }
                self.same_key(event)
            }
            Change::Remove { account_id } => {
                if !self.members.contains_key(account_id) {
                    return Err(broken("someone who isn't a member was removed".into()));
                }
                if self.key_history.contains(&event.key_ref) {
                    return Err(broken(
                        "a removal didn't move the vault to a new key generation".into(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn same_key(&self, event: &MembershipEvent) -> Result<()> {
        if event.key_ref != self.key_ref {
            return Err(Error::ChainBroken {
                seq: event.seq,
                why: "only a removal moves the vault to a new key generation".into(),
            });
        }
        Ok(())
    }

    fn apply_checked(&mut self, event: &MembershipEvent) -> Result<()> {
        match &event.change {
            Change::Create { .. } => {}
            Change::Add { member, right } => {
                if right.can_edit() {
                    self.writers.insert(member.account_id);
                }
                self.former.remove(&member.account_id);
                self.members.insert(
                    member.account_id,
                    Membership {
                        party: *member,
                        right: *right,
                        since: event.seq,
                    },
                );
            }
            Change::SetRight { account_id, right } => {
                if right.can_edit() {
                    self.writers.insert(*account_id);
                }
                if let Some(m) = self.members.get_mut(account_id) {
                    m.right = *right;
                }
            }
            Change::Remove { account_id } => {
                if let Some(m) = self.members.remove(account_id) {
                    self.former.insert(*account_id, (m.party, event.seq));
                }
                self.key_ref = event.key_ref;
                self.key_history.push(event.key_ref);
            }
        }
        self.seq = event.seq;
        self.head = event.hash()?;
        Ok(())
    }

    /// The next event after this roster, unsigned: `seq`, `prev_hash` and
    /// `key_ref` filled in (a removal names `new_key_ref`).
    pub fn propose(
        &self,
        change: Change,
        new_key_ref: Option<Id>,
        by_account: Id,
        by_device: Id,
    ) -> MembershipEvent {
        MembershipEvent {
            v: SHARING_V,
            vault_id: self.vault_id,
            seq: self.seq + 1,
            prev_hash: self.head,
            change,
            key_ref: new_key_ref.unwrap_or(self.key_ref),
            by_account,
            by_device,
            at: Timestamp::now(),
            sig: Vec::new(),
        }
    }

    /// What `account` may do: the owner always manages; a former member
    /// or a stranger may do nothing.
    pub fn right_of(&self, account: Id) -> Option<Right> {
        if account == self.owner.account_id {
            return Some(Right::Manage);
        }
        self.members.get(&account).map(|m| m.right)
    }

    /// Everyone in the vault now, the owner first, then by account id.
    pub fn parties(&self) -> Vec<(Party, Right)> {
        std::iter::once((self.owner, Right::Manage))
            .chain(self.members.values().map(|m| (m.party, m.right)))
            .collect()
    }

    /// Whether `account` ever held a right that let it write here (its
    /// devices' old versions still verify; a member who could only ever
    /// view or fill never signs anything this vault takes).
    pub fn ever_wrote(&self, account: Id) -> bool {
        self.writers.contains(&account)
    }
}

/// What a share envelope carries a key for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SharePurpose {
    /// A Vault Key generation, for a member.
    Vault,
    /// One item's Item Key, for a recipient of that item alone.
    Item,
    /// A Vault Key the server holds back until an emergency request's
    /// waiting period has run out.
    Emergency,
}

/// Exactly where a shared key belongs. Signed with the envelope, and
/// checked by name before anything is opened.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SharePlace {
    /// What for.
    pub purpose: SharePurpose,
    /// The vault the key belongs to.
    pub vault_id: Id,
    /// For an item share, the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<Id>,
    /// The key: a Vault Key generation's id, or the item's id.
    pub key_id: Id,
    /// Who it is for.
    pub to: Party,
    /// The account that sent it.
    pub from_account: Id,
    /// For an item share or an emergency grant, which one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<Id>,
}

/// A key wrapped to another persona and signed by the sending device.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ShareEnvelope {
    /// Format version.
    pub v: u16,
    /// Where the key belongs.
    pub place: SharePlace,
    /// The identity crate's peer wrap (a `KeyEnvelope`, purpose `share`,
    /// wrapped by the recipient persona's receiving key), as bytes.
    #[serde(with = "serde_bytes")]
    pub wrap: Vec<u8>,
    /// The sending device.
    pub by_device: Id,
    /// When.
    pub at: Timestamp,
    /// Ed25519 by `by_device` over the prefix and the envelope with `sig` empty.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

fn inner_aad(place: &SharePlace) -> ecosystem_identity::records::Aad {
    identity_aad(
        place.to.profile_id,
        Some(place.to.persona_id),
        place.key_id,
        Purpose::Share,
    )
}

impl ShareEnvelope {
    /// Wraps `key` to the persona whose receiving key is `to_kem`, at
    /// `place`, and signs it with this device.
    pub fn seal<K: SymmetricKey>(
        key: &K,
        to_kem: &KemPublicKey,
        place: SharePlace,
        by_device: Id,
        signer: &dyn Signer,
    ) -> Result<Self> {
        crate::pq::require_hybrid(to_kem, "the person this goes to")?;
        let wrap = wrap_to_peer(
            key.expose(),
            to_kem,
            KeyRef::new(KeyRefKind::Peer, place.to.persona_id),
            &inner_aad(&place),
        )?;
        let mut envelope = Self {
            v: SHARING_V,
            place,
            wrap: wrap.to_bytes()?,
            by_device,
            at: Timestamp::now(),
            sig: Vec::new(),
        };
        envelope.sig = signer.sign(&envelope.signable()?)?.to_vec();
        Ok(envelope)
    }

    /// The bytes the signature covers.
    pub fn signable(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.sig.clear();
        let mut out = SHARE_SIG_PREFIX.to_vec();
        out.extend(to_canonical(&copy)?);
        Ok(out)
    }

    /// Checks the sending device's signature.
    pub fn verify(&self, sender: &VerifyingKey) -> Result<()> {
        sender
            .verify(&self.signable()?, &self.sig, "a shared key")
            .map_err(|_| {
                Error::KeyMoved(
                    "a shared key whose sender's signature didn't check out (it may have been swapped)"
                        .into(),
                )
            })
    }

    /// Opens the key with the recipient persona's private receiving key,
    /// after checking the place is `expected` and the sender's signature.
    pub fn open<K: SymmetricKey>(
        &self,
        with: &KemSecretKey,
        expected: &SharePlace,
        sender: &VerifyingKey,
    ) -> Result<K> {
        if self.v > SHARING_V {
            return Err(Error::SchemaTooNew {
                found: self.v,
                supported: SHARING_V,
            });
        }
        if self.place != *expected {
            return Err(Error::KeyMoved(format!(
                "a shared key for {:?} {} was offered where {:?} {} was expected",
                self.place.purpose, self.place.key_id, expected.purpose, expected.key_id
            )));
        }
        self.verify(sender)?;
        let wrap = KeyEnvelope::from_bytes(&self.wrap)?;
        Ok(unwrap_key_from_peer(&wrap, with, &inner_aad(expected))?)
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

/// Domain separation for emergency grants and steps.
const EMERGENCY_SIG_PREFIX: &[u8] = b"lockdown-emergency-v1\0";

/// The shortest waiting period an emergency contact may be given, in days.
pub const MIN_WAIT_DAYS: u32 = 1;
/// The longest, in days.
pub const MAX_WAIT_DAYS: u32 = 30;
/// The default, in days (04-security §8).
pub const DEFAULT_WAIT_DAYS: u32 = 7;

/// An owner names a trusted contact for emergency access (04-security §8
/// "trusted contacts"): which vaults the contact may read once a request
/// has waited `wait_secs` without the owner stopping it. Signed by the
/// owner's device; the Vault Keys themselves travel in [`ShareEnvelope`]s
/// of purpose `emergency` that the server holds back until then.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyGrant {
    /// Format version.
    pub v: u16,
    /// The grant.
    pub grant_id: Id,
    /// Whose vaults.
    pub owner: Party,
    /// Who may ask.
    pub contact: Party,
    /// How long a request waits before the keys are released.
    pub wait_secs: u64,
    /// The vaults covered, sorted and unique.
    pub vaults: Vec<Id>,
    /// The owner's device that signed it.
    pub by_device: Id,
    /// When.
    pub at: Timestamp,
    /// Ed25519 by `by_device`.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

/// One step of an emergency grant's life, signed by the person taking it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmergencyAction {
    /// The contact agrees to be one.
    Accept,
    /// The contact says no, or steps down.
    Decline,
    /// The contact asks for access; the waiting period starts.
    Request,
    /// The owner stops a waiting request.
    Stop,
    /// The owner ends the grant.
    Revoke,
}

impl EmergencyAction {
    /// The word on the wire and in the path.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Request => "request",
            Self::Stop => "stop",
            Self::Revoke => "revoke",
        }
    }

    /// The action a word names.
    pub fn parse(text: &str) -> Option<Self> {
        [
            Self::Accept,
            Self::Decline,
            Self::Request,
            Self::Stop,
            Self::Revoke,
        ]
        .into_iter()
        .find(|a| a.as_str() == text)
    }

    /// Whether the owner takes this step (the others are the contact's).
    pub fn by_owner(self) -> bool {
        matches!(self, Self::Stop | Self::Revoke)
    }
}

/// A signed step.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EmergencyStep {
    /// Format version.
    pub v: u16,
    /// The grant.
    pub grant_id: Id,
    /// What.
    pub action: EmergencyAction,
    /// Who.
    pub by_account: Id,
    /// With which device.
    pub by_device: Id,
    /// When.
    pub at: Timestamp,
    /// Ed25519 by `by_device`.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

fn emergency_signable<T: Serialize + Clone>(value: &T, clear: impl Fn(&mut T)) -> Result<Vec<u8>> {
    let mut copy = value.clone();
    clear(&mut copy);
    let mut out = EMERGENCY_SIG_PREFIX.to_vec();
    out.extend(to_canonical(&copy)?);
    Ok(out)
}

impl EmergencyGrant {
    /// A grant, unsigned: the wait in days within the allowed range, the
    /// vaults sorted.
    pub fn new(
        owner: Party,
        contact: Party,
        wait_days: u32,
        mut vaults: Vec<Id>,
        by_device: Id,
    ) -> Result<Self> {
        if !(MIN_WAIT_DAYS..=MAX_WAIT_DAYS).contains(&wait_days) {
            return Err(Error::Invalid(format!(
                "the waiting period is {MIN_WAIT_DAYS} to {MAX_WAIT_DAYS} days"
            )));
        }
        vaults.sort();
        vaults.dedup();
        if vaults.is_empty() {
            return Err(Error::Invalid("name at least one vault".into()));
        }
        if owner.account_id == contact.account_id {
            return Err(Error::Invalid(
                "an emergency contact is someone other than you".into(),
            ));
        }
        Ok(Self {
            v: SHARING_V,
            grant_id: Id::new(),
            owner,
            contact,
            wait_secs: u64::from(wait_days) * 86_400,
            vaults,
            by_device,
            at: Timestamp::now(),
            sig: Vec::new(),
        })
    }

    /// Signs with the owner's device.
    pub fn sign(&mut self, signer: &dyn Signer) -> Result<()> {
        self.sig = signer
            .sign(&emergency_signable(self, |c| c.sig.clear())?)?
            .to_vec();
        Ok(())
    }

    /// Checks the owner's device signature and the grant's shape.
    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        if self.v > SHARING_V {
            return Err(Error::SchemaTooNew {
                found: self.v,
                supported: SHARING_V,
            });
        }
        let mut sorted = self.vaults.clone();
        sorted.sort();
        sorted.dedup();
        if sorted != self.vaults || self.vaults.is_empty() {
            return Err(Error::Invalid(
                "an emergency grant's vaults aren't in order".into(),
            ));
        }
        let days = self.wait_secs / 86_400;
        if !self.wait_secs.is_multiple_of(86_400)
            || !(u64::from(MIN_WAIT_DAYS)..=u64::from(MAX_WAIT_DAYS)).contains(&days)
        {
            return Err(Error::Invalid(
                "an emergency grant's waiting period is out of range".into(),
            ));
        }
        key.verify(
            &emergency_signable(self, |c| c.sig.clear())?,
            &self.sig,
            "an emergency grant",
        )
        .map_err(|_| Error::Invalid("an emergency grant's signature didn't check out".into()))
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

impl EmergencyStep {
    /// A step, unsigned.
    pub fn new(grant_id: Id, action: EmergencyAction, by_account: Id, by_device: Id) -> Self {
        Self {
            v: SHARING_V,
            grant_id,
            action,
            by_account,
            by_device,
            at: Timestamp::now(),
            sig: Vec::new(),
        }
    }

    /// Signs with this device.
    pub fn sign(&mut self, signer: &dyn Signer) -> Result<()> {
        self.sig = signer
            .sign(&emergency_signable(self, |c| c.sig.clear())?)?
            .to_vec();
        Ok(())
    }

    /// Checks the device signature.
    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        key.verify(
            &emergency_signable(self, |c| c.sig.clear())?,
            &self.sig,
            "an emergency step",
        )
        .map_err(|_| Error::Invalid("an emergency step's signature didn't check out".into()))
    }

    /// As canonical CBOR.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_canonical(self)?)
    }

    /// From CBOR.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(lockdown_crypto::cbor::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::VaultKey;
    use lockdown_crypto::SigningKey;

    struct Person {
        party: Party,
        device: Id,
        key: SigningKey,
    }

    fn person() -> Person {
        Person {
            party: Party {
                account_id: Id::new(),
                profile_id: Id::new(),
                persona_id: Id::new(),
            },
            device: Id::new(),
            key: SigningKey::generate().unwrap(),
        }
    }

    fn lookup(people: &[&Person]) -> impl Fn(Id) -> Option<(Id, VerifyingKey)> + use<> {
        let table: Vec<(Id, Id, VerifyingKey)> = people
            .iter()
            .map(|p| (p.device, p.party.account_id, p.key.verifying_key()))
            .collect();
        move |device| {
            table
                .iter()
                .find(|(d, _, _)| *d == device)
                .map(|(_, a, k)| (*a, k.clone()))
        }
    }

    fn signed(mut e: MembershipEvent, by: &Person) -> MembershipEvent {
        e.sign(&by.key).unwrap();
        e
    }

    #[test]
    fn a_roster_follows_its_signed_chain_and_refuses_what_rights_forbid() {
        let (alice, bob, carol) = (person(), person(), person());
        let vault = Id::new();
        let k1 = Id::new();
        let devices = lookup(&[&alice, &bob, &carol]);
        let mut events = vec![signed(
            Roster::genesis(vault, alice.party, VaultKind::Shared, k1, alice.device),
            &alice,
        )];
        let roster = Roster::replay(&events, &devices).unwrap();
        events.push(signed(
            roster.propose(
                Change::Add {
                    member: bob.party,
                    right: Right::View,
                },
                None,
                alice.party.account_id,
                alice.device,
            ),
            &alice,
        ));
        let mut roster = Roster::replay(&events, &devices).unwrap();
        assert_eq!(roster.right_of(bob.party.account_id), Some(Right::View));
        assert_eq!(roster.right_of(alice.party.account_id), Some(Right::Manage));
        assert_eq!(roster.right_of(carol.party.account_id), None);

        // Bob can only view: he can't add Carol.
        let by_bob = signed(
            roster.propose(
                Change::Add {
                    member: carol.party,
                    right: Right::Edit,
                },
                None,
                bob.party.account_id,
                bob.device,
            ),
            &bob,
        );
        assert!(roster.apply(&by_bob, &devices).is_err());
        // A device signing for an account it isn't of.
        let forged = signed(
            roster.propose(
                Change::Add {
                    member: carol.party,
                    right: Right::Edit,
                },
                None,
                alice.party.account_id,
                bob.device,
            ),
            &bob,
        );
        assert!(roster.apply(&forged, &devices).is_err());
        // A removal that keeps the old key is refused; a fresh key is taken.
        let stale = signed(
            roster.propose(
                Change::Remove {
                    account_id: bob.party.account_id,
                },
                Some(k1),
                alice.party.account_id,
                alice.device,
            ),
            &alice,
        );
        assert!(roster.apply(&stale, &devices).is_err());
        let k2 = Id::new();
        let removal = signed(
            roster.propose(
                Change::Remove {
                    account_id: bob.party.account_id,
                },
                Some(k2),
                alice.party.account_id,
                alice.device,
            ),
            &alice,
        );
        roster.apply(&removal, &devices).unwrap();
        assert_eq!(roster.key_ref, k2);
        assert_eq!(roster.right_of(bob.party.account_id), None);
        // Bob only ever viewed, so no version of his counts as a writer's;
        // the owner always does.
        assert!(!roster.ever_wrote(bob.party.account_id));
        assert!(roster.ever_wrote(alice.party.account_id));
        events.push(removal);
        // Replayed from the start, the same roster.
        assert_eq!(Roster::replay(&events, &devices).unwrap(), roster);
        // Adding back must not return to an old generation either.
        let add_old = signed(
            roster.propose(
                Change::Add {
                    member: bob.party,
                    right: Right::View,
                },
                Some(k1),
                alice.party.account_id,
                alice.device,
            ),
            &alice,
        );
        assert!(roster.apply(&add_old, &devices).is_err());

        // A reordered or tampered chain is refused.
        let mut swapped = events.clone();
        swapped.swap(1, 2);
        assert!(Roster::replay(&swapped, &devices).is_err());
        let mut bent = events.clone();
        bent[1].change = Change::Add {
            member: bob.party,
            right: Right::Manage,
        };
        assert!(Roster::replay(&bent, &devices).is_err());
        // A chain that doesn't start with its owner sharing it.
        assert!(Roster::replay(&events[1..], &devices).is_err());
    }

    #[test]
    fn a_shared_key_opens_only_for_its_persona_at_its_place_from_its_sender() {
        let (alice, bob) = (person(), person());
        let bob_seed = lockdown_crypto::rand::random_32().unwrap();
        let (bob_sk, bob_pk) = ecosystem_identity::kem::kem(ecosystem_identity::kem::DEFAULT_KEM)
            .keypair(bob_seed)
            .unwrap();
        let key = VaultKey::from_bytes(lockdown_crypto::rand::random_32().unwrap());
        let place = SharePlace {
            purpose: SharePurpose::Vault,
            vault_id: Id::new(),
            item_id: None,
            key_id: Id::new(),
            to: bob.party,
            from_account: alice.party.account_id,
            grant_id: None,
        };
        let env = ShareEnvelope::seal(&key, &bob_pk, place, alice.device, &alice.key).unwrap();
        let env = ShareEnvelope::from_bytes(&env.to_bytes().unwrap()).unwrap();
        let back: VaultKey = env
            .open(&bob_sk, &place, &alice.key.verifying_key())
            .unwrap();
        assert_eq!(back.expose(), key.expose());

        // From someone else's device: refused.
        let mallory = SigningKey::generate().unwrap();
        assert!(
            env.open::<VaultKey>(&bob_sk, &place, &mallory.verifying_key())
                .is_err()
        );
        // Another place: refused by name.
        let mut other = place;
        other.purpose = SharePurpose::Emergency;
        assert!(matches!(
            env.open::<VaultKey>(&bob_sk, &other, &alice.key.verifying_key()),
            Err(Error::KeyMoved(_))
        ));
        // Re-addressed to another place with the signature kept: refused.
        let mut moved = env.clone();
        moved.place.vault_id = Id::new();
        assert!(
            moved
                .open::<VaultKey>(&bob_sk, &moved.place, &alice.key.verifying_key())
                .is_err()
        );
        // A server that wraps its own key to Bob can't sign as Alice.
        let fake = ShareEnvelope::seal(&key, &bob_pk, place, alice.device, &mallory).unwrap();
        assert!(
            fake.open::<VaultKey>(&bob_sk, &place, &alice.key.verifying_key())
                .is_err()
        );
        // Another persona's private key doesn't open it.
        let (carol_sk, _) = ecosystem_identity::kem::kem(ecosystem_identity::kem::DEFAULT_KEM)
            .keypair(lockdown_crypto::rand::random_32().unwrap())
            .unwrap();
        assert!(
            env.open::<VaultKey>(&carol_sk, &place, &alice.key.verifying_key())
                .is_err()
        );
    }

    #[test]
    fn an_emergency_grant_is_signed_bounded_and_in_order() {
        let (alice, bob) = (person(), person());
        let (v1, v2) = (Id::new(), Id::new());
        assert!(EmergencyGrant::new(alice.party, bob.party, 0, vec![v1], alice.device).is_err());
        assert!(EmergencyGrant::new(alice.party, bob.party, 31, vec![v1], alice.device).is_err());
        assert!(EmergencyGrant::new(alice.party, alice.party, 7, vec![v1], alice.device).is_err());
        let mut g =
            EmergencyGrant::new(alice.party, bob.party, 7, vec![v2, v1, v2], alice.device).unwrap();
        assert_eq!(g.vaults.len(), 2);
        g.sign(&alice.key).unwrap();
        let g = EmergencyGrant::from_bytes(&g.to_bytes().unwrap()).unwrap();
        g.verify(&alice.key.verifying_key()).unwrap();
        assert!(g.verify(&bob.key.verifying_key()).is_err());
        let mut bent = g.clone();
        bent.wait_secs = 86_400;
        assert!(bent.verify(&alice.key.verifying_key()).is_err());
        let mut step = EmergencyStep::new(
            g.grant_id,
            EmergencyAction::Request,
            bob.party.account_id,
            bob.device,
        );
        step.sign(&bob.key).unwrap();
        step.verify(&bob.key.verifying_key()).unwrap();
        assert!(step.verify(&alice.key.verifying_key()).is_err());
        assert_eq!(EmergencyAction::parse("stop"), Some(EmergencyAction::Stop));
        assert!(EmergencyAction::Revoke.by_owner() && !EmergencyAction::Request.by_owner());
    }

    #[test]
    fn rights_say_what_they_allow() {
        assert!(!Right::FillOnly.can_reveal());
        assert!(Right::View.can_reveal() && !Right::View.can_edit());
        assert!(Right::Edit.can_edit() && !Right::Edit.can_manage());
        assert!(Right::Manage.can_manage());
        for r in Right::ALL {
            assert_eq!(Right::parse(r.as_str()).unwrap(), r);
        }
        assert!(Right::parse("owner").is_err());
        assert!(Right::FillOnly < Right::Manage);
    }
}
