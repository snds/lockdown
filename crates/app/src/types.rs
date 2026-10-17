//! What crosses from the app to a shell: plain data a shell may show, and
//! nothing a shell must keep secret. Secrets travel as [`Revealed`] bytes
//! and only when asked for by name.

use serde::Serialize;

use crate::Revealed;
use crate::lock::LockReason;

/// Where the app reads and writes, and what is there before anything opens.
#[derive(Clone, Debug, Serialize)]
pub struct Probe {
    /// The ecosystem identity root (D15).
    pub root: String,
    /// Lockdown's own data directory (D47).
    pub data: String,
    /// Whether an identity exists at the root.
    pub identity_present: bool,
    /// Whether this app has a device key file here (so the identity, if
    /// present, is one this device may be enrolled in).
    pub device_key_present: bool,
    /// Whether an account is remembered (`sync.cbor`).
    pub account_present: bool,
    /// The device name this app enrols under (D74).
    pub device_name: String,
}

/// Locked or open, and why it locked last.
#[derive(Clone, Debug, Serialize)]
pub struct LockState {
    /// Whether the vault's keys are dropped.
    pub locked: bool,
    /// Why, when it locked without being asked.
    pub reason: Option<LockReason>,
    /// Seconds since the last call, while open.
    pub idle_secs: u64,
    /// The idle timeout in seconds; `None` means never.
    pub idle_timeout_secs: Option<u64>,
    /// The hidden-window timeout in seconds; `None` means never.
    pub hidden_timeout_secs: Option<u64>,
}

/// One row of the list: nothing hidden. The facts after `edited_at` are
/// what the sidebar's smart views and a row's method glyphs are drawn from
/// (`views`); they name what exists, never a value.
#[derive(Clone, Debug, Serialize)]
pub struct ItemSummary {
    pub id: String,
    pub kind: String,
    /// `live`, `archived` or `trashed` (a trashed item is in its grace
    /// period and shows only in the Deleted view).
    pub state: String,
    pub title: String,
    pub site: String,
    pub username: String,
    /// Seconds since the Unix epoch.
    pub edited_at: i64,
    /// Under the crown-jewel treatment (D98).
    pub crown_jewel: bool,
    /// The sign-in methods a Login holds, in order: `password`, `passkey`,
    /// `code` (each once, however many there are).
    pub methods: Vec<String>,
    /// How many passkeys are inside this item.
    pub passkeys: u32,
    /// How many one-time codes are inside this item.
    pub codes: u32,
    /// The `expires_at` date field's text, when the item has one.
    pub expires_at: Option<String>,
    /// A conflict copy (D58): the title carries the marker.
    pub conflict_copy: bool,
    /// Things the person should look at: `issuer-equals-login`.
    pub warnings: Vec<String>,
    /// The smart views this row is in (`views::classify`).
    pub views: Vec<String>,
}

/// One one-time code as the Codes view shows it: the current and next
/// values and how long the current one lasts. The set-up (the seed) never
/// crosses; a shell cannot re-derive a code from this.
#[derive(Clone, Debug, Serialize)]
pub struct CodeView {
    pub item_id: String,
    pub field_id: String,
    /// The item's title.
    pub title: String,
    pub issuer: Option<String>,
    pub account: Option<String>,
    /// `totp`, `hotp` or `steam`.
    pub variant: String,
    pub digits: u32,
    pub period: u32,
    /// The code valid now.
    pub current: String,
    /// The code after it (time-based kinds only).
    pub next: Option<String>,
    /// Seconds the current code stays valid; `None` for HOTP.
    pub seconds_left: Option<u64>,
    /// `issuer-equals-login` when the code's issuer is the sync login.
    pub warnings: Vec<String>,
}

/// One persona of the identity, for the sidebar's switcher.
#[derive(Clone, Debug, Serialize)]
pub struct PersonaInfo {
    pub id: String,
    pub label: String,
    /// `personal`, `work`, `business`, `custom`.
    pub kind: String,
    /// The one whose vault is open.
    pub active: bool,
}

/// What an import did.
#[derive(Clone, Debug, Serialize)]
pub struct ImportReport {
    /// Items created.
    pub imported: usize,
    /// Per item, what in it did not map and was left out (never silent).
    pub notes: Vec<String>,
}

/// One field of an item as a shell may show it. A hidden field carries a
/// mask and, only when the caller asked to reveal, the bytes.
#[derive(Debug, Serialize)]
pub struct FieldView {
    pub field_id: String,
    pub label: String,
    pub kind: String,
    pub hidden: bool,
    /// The value, for fields that are not hidden.
    pub value: Option<String>,
    /// Dots standing in for a hidden value.
    pub mask: Option<String>,
    /// The hidden value, when revealed. Not serialised: a shell that needs
    /// it takes the bytes out of the struct by hand.
    #[serde(skip)]
    pub secret: Option<Revealed>,
}

/// One item in full.
#[derive(Debug, Serialize)]
pub struct ItemDetail {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub version: u64,
    pub edited_at: i64,
    pub crown_jewel: bool,
    pub fields: Vec<FieldView>,
}

/// The server's key-transparency log as this device last verified it
/// (04-security §6; D78, D79): what a Status screen shows beside the
/// account, and what a sync reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TransparencyView {
    /// How many entries the log had under the newest tree head verified.
    pub log_size: u64,
    /// The first sixteen hex characters of that tree head's root.
    pub root_prefix: String,
    /// The log index this account's own entries were audited through.
    pub audited: u64,
    /// When this device last verified the tree head, seconds since the
    /// Unix epoch; `None` for a view remembered before the time was kept.
    pub checked_at: Option<i64>,
}

/// The account on the server, as the status shows it.
#[derive(Clone, Debug, Serialize)]
pub struct AccountStatus {
    pub server: String,
    pub login: String,
    pub synced_seq: u64,
    pub synced_rev: u64,
    /// When this device last finished a sync, seconds since the Unix epoch.
    pub last_sync_at: Option<i64>,
    /// When the session token runs out.
    pub session_expires_at: i64,
    /// The server's log as last verified here; `None` until the first
    /// sync checks it.
    pub transparency: Option<TransparencyView>,
}

/// `status`.
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub root: String,
    pub data: String,
    pub profile_id: String,
    pub manager: Option<String>,
    pub recovery: String,
    pub device_id: String,
    pub device_name: String,
    pub persona_id: String,
    pub persona_label: String,
    pub vault_id: String,
    pub head_seq: u64,
    pub head_hash: String,
    pub live_items: usize,
    pub devices: usize,
    pub devices_revoked: usize,
    pub identity_seq: u64,
    pub account: Option<AccountStatus>,
}

/// One device of the identity.
#[derive(Clone, Debug, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub enrolled_at: i64,
    pub revoked_at: Option<i64>,
    pub this_device: bool,
}

/// What a sync did. A sync that got this far verified the server's log
/// before and after; a refused check never produces one.
#[derive(Clone, Debug, Serialize)]
pub struct SyncOutcome {
    pub pushed: usize,
    pub pulled: usize,
    pub merged: usize,
    pub conflict_copies: Vec<String>,
    pub head_seq: u64,
    pub rounds: u32,
    /// The log as verified after the sync.
    pub transparency: TransparencyView,
    pub lines: Vec<String>,
}

/// What registering an account returned: notes, and the recovery kit's
/// page when one was made here (shown once; never kept).
#[derive(Debug)]
pub struct Registered {
    pub lines: Vec<String>,
    pub kit: Option<Revealed>,
}

/// A new recovery kit: its page and what changed around it.
#[derive(Debug)]
pub struct KitCreated {
    pub kit: Revealed,
    pub notes: Vec<String>,
}
