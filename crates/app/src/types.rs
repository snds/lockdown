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
    /// The template's plain field shown under the title (a network's name,
    /// a card's holder), or the username; empty for a crown jewel, whose
    /// summary carries its title only (02-model §3 "never in search previews").
    pub subtitle: String,
    /// What the row's quick copy takes, when the item holds it.
    pub copy: Option<CopyTarget>,
    /// The vault the item is in.
    pub vault_id: String,
    /// The folder it is filed in (an id of its vault's folders), if any.
    pub folder: Option<String>,
    /// Its tags, sorted; a `/` nests one under another.
    pub tags: Vec<String>,
    /// A favourite: listed first in the palette and in its own view.
    pub favourite: bool,
    /// When this device last used it (a copy, a reveal, its site opened),
    /// in seconds since the epoch; kept on this device only, never synced.
    pub last_used: Option<i64>,
    /// How many times this device has used it.
    pub uses: u32,
    /// Days until `expires_at` lapses (negative once it has).
    pub expires_in_days: Option<i64>,
    /// How many files are attached.
    pub attachments: u32,
}

/// The one value a row (and the quick-access palette) offers to copy: a
/// secret through `copy_secret` or a code through `copy_code`, by id and
/// field; never the value itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CopyTarget {
    pub field_id: String,
    /// `secret` or `code`.
    pub how: String,
    /// What is copied, in plain words: `password`, `code`, `Wi-Fi password`, `card number`.
    pub label: String,
}

/// One kind's template as a shell draws it (`lockdown_vault::template`).
#[derive(Clone, Debug, Serialize)]
pub struct TemplateView {
    /// `ItemKind::as_str`.
    pub kind: String,
    /// Singular, in plain words: "Wi-Fi network".
    pub name: String,
    /// One line for the "New item" list.
    pub summary: String,
    /// Whether a person makes one by hand (not a passkey).
    pub creatable: bool,
    /// Always a crown jewel (the person cannot opt down).
    pub crown_jewel: bool,
    /// Whether this kind may ever be filled into a page.
    pub autofill: bool,
    /// The quick copy's field.
    pub primary: Option<String>,
    /// The plain field shown under the title.
    pub subtitle: Option<String>,
    pub fields: Vec<FieldTemplateView>,
}

/// One field of a template.
#[derive(Clone, Debug, Serialize)]
pub struct FieldTemplateView {
    pub field_id: String,
    pub label: String,
    /// `text`, `multiline`, `secret`, `secret-multiline`, `url`, `email`,
    /// `phone`, `number`, `date`, `month`, `select`, `boolean`, `totp`, `passkey`.
    pub control: String,
    /// The value crosses only through `reveal`, and is sent as bytes.
    pub secret: bool,
    pub required: bool,
    /// Holding it makes the item a crown jewel.
    pub crown_jewel: bool,
    /// `fill`, `confirm` (a second deliberate action) or `never`.
    pub fill: String,
    /// The generator's rule, where it applies.
    pub generator: Option<GeneratorView>,
    /// A `select`'s choices.
    pub options: Vec<String>,
    /// The detail's heading for it.
    pub section: String,
    /// Shown in groups of four.
    pub grouped: bool,
    /// One line of help, or empty.
    pub hint: String,
    /// The field may be added more than once (`url.0`, `url.1`, …).
    pub repeats: bool,
}

/// A generator rule, with its plain words.
#[derive(Clone, Debug, Serialize)]
pub struct GeneratorView {
    pub length: u32,
    pub symbols: bool,
    pub unambiguous: bool,
    /// "20 characters: letters, digits and symbols".
    pub words: String,
}

/// A kind a person may give a field they add by hand.
#[derive(Clone, Debug, Serialize)]
pub struct CustomKindView {
    /// The storage kind's name (`text`, `hidden`, …), as `FieldView.kind`.
    pub kind: String,
    /// Its word: "Secret", "Web address".
    pub label: String,
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
    /// The counter `current` was made from (HOTP only; `None` for the
    /// time-based kinds). A count, not part of the set-up's secret.
    pub counter: Option<u64>,
    /// `issuer-equals-login` when the code's issuer is the sync login.
    pub warnings: Vec<String>,
}

/// What advancing a counter-based (HOTP) code gives back: the new code and
/// the counter it was made from. The set-up stays in the core.
#[derive(Clone, Debug, Serialize)]
pub struct CodeAdvanced {
    /// The code for the new counter.
    pub code: String,
    /// The counter now stored on the item.
    pub counter: u64,
    /// The item's version after the advance (a new version, so it syncs).
    pub version: u64,
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
    /// Items already here (from an earlier restore of the same backup)
    /// that got the file's copy as a new version, history kept.
    pub updated: usize,
    /// Items already here exactly as the file has them, left alone.
    pub unchanged: usize,
    /// About the file and per item, what did not map and was left out
    /// (never silent).
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
    /// A web address's match rule (`exact_host`, `base_domain`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_rule: Option<String>,
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
    /// A crown jewel the person opted in to exports and backups (D121).
    pub export_opt_in: bool,
    pub fields: Vec<FieldView>,
    /// Where it is filed and what is attached (flattened into the item).
    #[serde(flatten)]
    pub place: ItemPlace,
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

// --- import, export and backups (14-import-export.md) -------------------------

/// One format the import wizard offers.
#[derive(Clone, Debug, Serialize)]
pub struct ImportFormatView {
    /// The id a shell sends back (`1pux`, `bitwarden`, …).
    pub id: String,
    /// Its name in plain words.
    pub name: String,
    /// How to get the file out of the other app.
    pub how: String,
    /// Whether the file may have a password of its own.
    pub may_need_password: bool,
}

/// How many items of one kind a file holds.
#[derive(Clone, Debug, Serialize)]
pub struct KindCount {
    /// The kind's id.
    pub kind: String,
    /// The kind's name.
    pub name: String,
    /// How many.
    pub count: usize,
}

/// What a file holds, before anything is kept. Counts and words only:
/// no field value of the file crosses to a shell.
#[derive(Clone, Debug, Serialize)]
pub struct ImportPreview {
    /// Hand it back to `import_commit` to keep the items.
    pub token: u64,
    /// The format read.
    pub format: String,
    /// Its name in plain words.
    pub format_name: String,
    /// Items in the file.
    pub total: usize,
    /// Items per kind.
    pub by_kind: Vec<KindCount>,
    /// Attached files.
    pub attachments: usize,
    /// Items that will be crown jewels here.
    pub crown_jewels: usize,
    /// Items that will be new here.
    pub new: usize,
    /// A backup's items already here that will get the backup's copy.
    pub updated: usize,
    /// A backup's items already here as the backup has them.
    pub unchanged: usize,
    /// About the file as a whole.
    pub problems: Vec<String>,
    /// Per item ("title: what").
    pub item_notes: Vec<String>,
}

/// An item an export or a backup leaves out, and why.
#[derive(Clone, Debug, Serialize)]
pub struct LeftOutView {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub reason: String,
}

/// What an export carries and leaves, in counts and words.
#[derive(Clone, Debug, Serialize)]
pub struct ExportPreview {
    /// `lockdown`, `cxf`, `csv` or `kdbx`.
    pub format: String,
    /// Items the file carries.
    pub items: usize,
    /// Files it carries.
    pub attachments: usize,
    /// Crown jewels in it (each opted in by the person, D121).
    pub crown_jewels_included: usize,
    /// Crown jewels left out (not opted in).
    pub crown_jewels_left_out: usize,
    /// The items left out, by title.
    pub left_out: Vec<LeftOutView>,
    /// What the format itself can't carry, in words.
    pub dropped: Vec<String>,
    /// Whether the file is locked with a password.
    pub encrypted: bool,
}

/// An export written to the place the person chose.
#[derive(Clone, Debug, Serialize)]
pub struct ExportDone {
    /// Where.
    pub path: String,
    /// How big.
    pub bytes: u64,
    /// What it carried and left.
    pub preview: ExportPreview,
}

/// One backup file in the backup folder.
#[derive(Clone, Debug, Serialize)]
pub struct BackupFile {
    /// Its name in the folder.
    pub name: String,
    /// Its size.
    pub bytes: u64,
    /// When it was made, seconds since the Unix epoch.
    pub made_at: i64,
}

/// Automatic backups as they stand.
#[derive(Clone, Debug, Serialize)]
pub struct BackupStatus {
    /// Whether scheduled backups run.
    pub enabled: bool,
    /// The folder they go to.
    pub folder: Option<String>,
    /// Hours between backups.
    pub interval_hours: u32,
    /// How many to keep.
    pub keep_last: u32,
    /// Whether a backup password is set on this device.
    pub key_set: bool,
    /// When the last backup succeeded.
    pub last_success_at: Option<i64>,
    /// What went wrong last time, in words.
    pub last_error: Option<String>,
    /// When the next one is due (while the vault is open).
    pub next_due_at: Option<i64>,
    /// Crown jewels a backup leaves out (not opted in, D121).
    pub crown_jewels_left_out: usize,
    /// The backups in the folder, newest first.
    pub backups: Vec<BackupFile>,
}

/// An item's folder, tags and favourite, as `show` gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct OrganizationView {
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub favourite: bool,
}

/// One attached file's facts: never its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttachmentView {
    pub id: String,
    pub name: String,
    pub mime: String,
    /// Bytes.
    pub size: u64,
}

/// What `show` adds to an item beyond its fields: where it is filed, its
/// files, and whether it may leave the crown-jewel class.
#[derive(Clone, Debug, Serialize)]
pub struct ItemPlace {
    pub vault_id: String,
    pub organization: OrganizationView,
    pub attachments: Vec<AttachmentView>,
    /// A crown jewel by its kind or a field (a wallet's recovery phrase):
    /// it can never leave the class (02-model §3).
    pub crown_jewel_fixed: bool,
}

/// One folder of a vault, for the sidebar and the pickers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FolderView {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    /// The names from the top, joined by ` / `.
    pub path: String,
}

/// One vault of the open persona.
#[derive(Clone, Debug, Serialize)]
pub struct VaultView {
    pub id: String,
    /// The name the person gave it, or `Personal` for the first vault and
    /// `Vault` for any other until one is given.
    pub name: String,
    /// The first vault: where new items go unless another is picked.
    pub primary: bool,
    /// Items not in the trash.
    pub items: usize,
    /// Days a trashed item stays before it is purged on every device.
    pub trash_days: u32,
    pub folders: Vec<FolderView>,
}

/// Items that look like the same thing twice (after an import, say).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DuplicateGroup {
    /// `same-login` (same site and username), `same-password` (the same
    /// password on several items) or `same-code` (the same code set-up).
    pub reason: String,
    /// In plain words, for the list.
    pub words: String,
    /// The items, newest change first.
    pub ids: Vec<String>,
    /// Whether "Merge" applies (Logins of the same site and username).
    pub mergeable: bool,
}

/// One saved version of an item, for its history.
#[derive(Clone, Debug, Serialize)]
pub struct VersionView {
    pub n: u64,
    pub edited_at: i64,
    /// The device's name, or its id when this device doesn't know it.
    pub edited_by: String,
    /// The labels of the fields this version changed, added or removed
    /// against the one before it (never a value).
    pub changed: Vec<String>,
    /// False once its key was destroyed.
    pub readable: bool,
    /// The newest version.
    pub current: bool,
}

/// An item due to lapse, for reminders.
#[derive(Clone, Debug, Serialize)]
pub struct ExpiryReminder {
    pub id: String,
    pub title: String,
    pub kind: String,
    /// The date as the field holds it.
    pub date: String,
    /// Negative once lapsed.
    pub days: i64,
}
