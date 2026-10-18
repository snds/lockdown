//! One open session: where things live on this device, the open identity,
//! the store, what the device remembers, the account when there is one,
//! and the one vault in use. The desktop shell drives this today and the
//! phone shells will; `bins/cli/src/app.rs` still carries its own copy of
//! the same flows and is to move onto this crate in a later pass.
//!
//! ```text
//! <identity root>/            the ecosystem identity (01-identity §5; D15)
//! <data dir>/
//!   device-key.cbor           this device's keys (the identity crate's file keystore)
//!   vault.sqlite              the ciphertext store (lockdown-store)
//!   remembered.cbor           the newest heads this device accepted (rollback memory)
//!   identity-sync.cbor        the identity's signed record set as last accepted from the server
//!   transparency.cbor         per server: the pinned log key and the newest tree head verified here
//!   sync.cbor                 the server, the login name, the session, how far each vault is synced
//! ```
//!
//! The data dir is shared with the `lockdown` command line, so the desktop
//! app and the CLI on one machine are one device (D68: one device record
//! per app, and they are the same app), and the two share one memory of
//! the server's key-transparency log.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ecosystem_identity::keys::Irk;
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::{KeyEnvelope, KeyRef, KeyRefKind, Purpose, RecoveryState};
use ecosystem_identity::{
    AccountAuth, AccountCredentials, ExportKey, FileKeystore, Identity, RecoveryKit, default_root,
};
use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_store::{
    DeviceStore, HeadMemory, RememberedHead, ServerView, SyncedIdentity, TransparencyMemory,
    open_device_store, write_private,
};
use lockdown_sync::wire::ErrorCode;
use lockdown_sync::{
    DeviceExpectations, HttpTransport, KtSession, SyncReport, Syncer, Transport, VaultSyncState,
    pull_identity, push_identity,
};
use lockdown_vault::identity::{
    self, APP, DeviceTrust, OpenVault, default_persona, device_keys, find_persona,
    identity_signing_key, merge_records, open_or_create_identity, open_vault_trusting,
    persona_vaults, record_set,
};
use lockdown_vault::identity_set::{Revocation, VaultBoundary};
use lockdown_vault::{
    FieldChange, FieldKind, FieldValue, Id, ItemClass, ItemState, PlainField, Vault,
};
use serde::{Deserialize, Serialize};

use crate::Revealed;
use crate::secrets;
use crate::types::{
    AccountStatus, DeviceInfo, FieldView, ItemDetail, ItemSummary, KitCreated, PersonaInfo, Probe,
    Registered, Status, SyncOutcome, TransparencyView,
};
use crate::views;

mod backup;
pub(crate) mod bridges;
mod codes;
pub(crate) mod depth;
pub(crate) mod devices_link;
mod duplicates;
pub(crate) mod exposure;
mod history;
pub mod items;
mod organize;
pub(crate) mod passkeys;
pub(crate) mod quick;
pub(crate) mod recovery;
pub(crate) mod second_factor;
pub(crate) mod secret_key;
pub(crate) mod security;
pub(crate) mod sharing;
pub(crate) mod transfer;

const KEYSTORE_FILE: &str = "device-key.cbor";
const STORE_FILE: &str = "vault.sqlite";
const MEMORY_FILE: &str = "remembered.cbor";
const IDENTITY_SYNC_FILE: &str = "identity-sync.cbor";
const TRANSPARENCY_FILE: &str = "transparency.cbor";
const SYNC_FILE: &str = "sync.cbor";
const SYNC_V: u16 = 1;
/// How many times a push of the identity manifest is retried after another
/// device moved first.
const IDENTITY_ROUNDS: u32 = 6;

/// Where the app reads and writes.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The identity root.
    pub root: PathBuf,
    /// Lockdown's data directory.
    pub data: PathBuf,
}

impl Paths {
    /// Resolves an explicit root and data dir. With neither, the platform's
    /// identity root (D15) and the `lockdown` directory beside it. With a
    /// root alone, the `lockdown` directory beside that root.
    pub fn resolve(root: Option<PathBuf>, data: Option<PathBuf>) -> Result<Self, String> {
        let root = match root {
            Some(r) => r,
            None => default_root().map_err(|e| e.to_string())?,
        };
        let data = match data {
            Some(d) => d,
            None => beside(&root),
        };
        Ok(Self { root, data })
    }

    fn sync_file(&self) -> PathBuf {
        self.data.join(SYNC_FILE)
    }

    /// The settings file (D100), beside `remembered.cbor`.
    pub fn settings_file(&self) -> PathBuf {
        self.data.join(crate::settings::SETTINGS_FILE)
    }

    /// The rollback memory (`remembered.cbor`: the newest heads accepted).
    pub fn memory_file(&self) -> PathBuf {
        self.data.join(MEMORY_FILE)
    }

    /// The log memory (`transparency.cbor`: per server, the pinned log key
    /// and the newest tree head verified).
    pub fn transparency_file(&self) -> PathBuf {
        self.data.join(TRANSPARENCY_FILE)
    }

    /// What is here before anything opens.
    pub fn probe(&self) -> Result<Probe, String> {
        let identity_present = ecosystem_identity::Store::at(&self.root)
            .probe()
            .map_err(|e| e.to_string())?
            == ecosystem_identity::store::Probe::Present;
        Ok(Probe {
            root: self.root.display().to_string(),
            data: self.data.display().to_string(),
            identity_present,
            device_key_present: self.data.join(KEYSTORE_FILE).exists(),
            account_present: self.sync_file().exists(),
            device_name: device_name(),
        })
    }
}

/// The `lockdown` directory next to an identity root.
fn beside(root: &Path) -> PathBuf {
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("lockdown"),
        _ => root.join("lockdown"),
    }
}

/// A name for this device in the identity's device list and on conflict
/// copies: `Lockdown on <hostname>` (D68, D74), or plain `Lockdown` when
/// the hostname can't be read. A host with a better name for itself (the
/// web vault names the browser it runs in) sets `LOCKDOWN_DEVICE_NAME`.
pub fn device_name() -> String {
    if let Some(name) = std::env::var_os("LOCKDOWN_DEVICE_NAME").filter(|v| !v.is_empty()) {
        return name.to_string_lossy().into_owned();
    }
    match hostname() {
        Some(host) => format!("Lockdown on {host}"),
        None => "Lockdown".to_string(),
    }
}

/// The machine's hostname, if it can be read.
fn hostname() -> Option<String> {
    for var in ["HOSTNAME", "COMPUTERNAME"] {
        if let Some(name) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return Some(name.to_string_lossy().into_owned());
        }
    }
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

/// The account this device uses, as kept in `sync.cbor`. The session
/// token is a bearer credential: with a [`secrets::SecretStore`] installed
/// it lives there and the file holds an empty string in its place;
/// otherwise the file is readable only by the user, like the device key
/// beside it. The field order and names are the CLI's, so both read it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConfig {
    v: u16,
    /// The server's base URL.
    pub server: String,
    /// The login name the account is filed under.
    pub login: String,
    /// The profile the account was registered for.
    pub profile_id: Id,
    /// The session token, until `expires_at`.
    pub token: String,
    /// When the token stops working (seconds since the Unix epoch).
    pub expires_at: i64,
    /// How far each vault is synced on this device.
    #[serde(default)]
    pub vaults: BTreeMap<Id, VaultSyncState>,
    /// When this device last finished a sync (added by the desktop shell;
    /// the CLI ignores and drops it).
    #[serde(default)]
    pub last_sync_at: Option<i64>,
}

impl SyncConfig {
    fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut cfg: Self = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| format!("{} didn't decode; log in again", path.display()))?;
                if cfg.v > SYNC_V {
                    return Err(format!(
                        "{} was written by a newer Lockdown; update the app",
                        path.display()
                    ));
                }
                if cfg.token.is_empty()
                    && let Some(store) = secrets::store()
                    && let Some(token) =
                        store.get(&secrets::session_key(&cfg.server, &cfg.login))?
                {
                    cfg.token = token;
                }
                Ok(Some(cfg))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let mut on_disk = self.clone();
        if let Some(store) = secrets::store() {
            store.set(
                &secrets::session_key(&self.server, &self.login),
                &self.token,
            )?;
            on_disk.token = String::new();
        }
        let bytes = lockdown_crypto::cbor::to_canonical(&on_disk).map_err(|e| e.to_string())?;
        write_private(path, &bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))
    }

    fn session_token(&self) -> Result<&str, String> {
        if self.token.is_empty() || self.expires_at <= lockdown_vault::Timestamp::now().0 {
            return Err("the session with the server has run out; log in again".into());
        }
        Ok(&self.token)
    }
}

/// What opening needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// First run: create what is missing.
    Init,
    /// Every later open: open what is there, create nothing.
    Open,
}

/// The open session: identity, store, memory, the synced identity records,
/// the memory of the server's log, the account, and the one vault in use.
/// Dropping it drops every key.
pub struct Session {
    paths: Paths,
    keystore: Rc<FileKeystore>,
    identity: Identity,
    store: DeviceStore,
    memory: HeadMemory,
    synced: SyncedIdentity,
    transparency: TransparencyMemory,
    sync: Option<SyncConfig>,
    /// The persona's first vault: where new items go unless another is named.
    opened: OpenVault,
    /// A parsed import waiting for the person's yes (dropped on lock).
    transfer: transfer::Pending,
    /// The persona's other vaults, oldest first (`organize`).
    more: Vec<OpenVault>,
    /// What this device used, and when (never synced).
    recency: crate::recency::Recency,
    /// Who this persona pinned and what is shared with it (`sharing`).
    sharing: crate::sharing::SharingState,
    /// Each shared vault's membership as its signed chain says.
    rosters: BTreeMap<Id, lockdown_vault::sharing::Roster>,
    /// Items others shared on their own, held in memory only, read only.
    received: Vec<Vault>,
    /// The share link or answer opened last, until the next or the lock.
    opened_send: Option<lockdown_vault::send::SendPayload>,
    /// A link this device opened to approve a new one (`devices_link`).
    link: Option<devices_link::OpenLink>,
    /// Phrase checks of someone's keys under way (`sharing::phrase`).
    phrase_checks: BTreeMap<Id, sharing::phrase::Pending>,
    /// The quick-access index, until what it was built from changes (`quick`).
    quick: Option<quick::Cache>,
}

impl Session {
    /// Opens the identity at the root and the persona's vault. `Init`
    /// creates what is missing and, with an account, brings the identity's
    /// records in step and pulls a vault this device never held (so a
    /// device that just joined finds the account's vault, not a new one),
    /// with the server's log verified and the account's entries audited
    /// first, as a sync does; `Open` touches the network not at all, so
    /// unlock works offline.
    pub fn open(paths: Paths, persona: Option<&str>, mode: Mode) -> Result<Self, String> {
        let keystore_path = paths.data.join(KEYSTORE_FILE);
        if mode != Mode::Init && !keystore_path.exists() && !paths.root.join("FORMAT").exists() {
            return Err(format!("nothing is set up at {} yet", paths.root.display()));
        }
        let keystore =
            Rc::new(FileKeystore::open_or_create(&keystore_path).map_err(|e| e.to_string())?);
        let mut memory =
            HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
        let remembered = memory.identity();
        let identity = if mode == Mode::Init {
            open_or_create_identity(&paths.root, &device_name(), keystore.as_ref(), remembered)
                .map_err(|e| e.to_string())?
        } else {
            match identity::open_identity(&paths.root, keystore.as_ref(), remembered)
                .map_err(|e| e.to_string())?
            {
                Some(identity) => identity,
                None => {
                    return Err(format!(
                        "no ecosystem identity at {}; create one, or log in to an account on a new device",
                        paths.root.display()
                    ));
                }
            }
        };
        memory
            .remember_identity(identity.head())
            .map_err(|e| e.to_string())?;
        let mut identity = identity;

        // D68: carry an older `<hostname>` record forward to `Lockdown on
        // <hostname>`, best-effort (see the CLI's note).
        let want = device_name();
        let current = identity
            .devices()
            .find(|d| d.device_id == identity.device_id())
            .map(|d| d.name.clone());
        if current.as_deref() != Some(want.as_str())
            && matches!(identity.profile().manager.as_deref(), None | Some(APP))
            && let Ok(Some(head)) =
                identity::rename_this_device(&mut identity, keystore.as_ref(), &want)
        {
            let _ = memory.remember_identity(head);
        }

        let persona_id = match persona {
            Some(selector) => find_persona(&identity, selector),
            None => default_persona(&identity),
        }
        .map_err(|e| e.to_string())?;

        let mut store =
            open_device_store(&paths.data.join(STORE_FILE)).map_err(|e| e.to_string())?;
        let isk_pub = identity.profile().isk_pub;
        let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), Some(&isk_pub))
            .map_err(|e| e.to_string())?;
        let mut transparency = TransparencyMemory::open(&paths.data.join(TRANSPARENCY_FILE))
            .map_err(|e| e.to_string())?;
        let mut sync = SyncConfig::load(&paths.sync_file())?;

        if let Some(cfg) = sync.as_mut()
            && mode == Mode::Init
        {
            let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
            let token = cfg.session_token()?.to_string();
            sync_identity_records(
                &transport,
                &token,
                &mut identity,
                keystore.as_ref(),
                &mut synced,
                &mut memory,
            )?;
            if persona_vaults(&identity, persona_id, &store)
                .map_err(|e| e.to_string())?
                .is_empty()
            {
                // The first pull is held to the same log as every sync
                // (04-security §6, D78, D80): the tree head must continue
                // the one remembered, the account's entries must be what
                // this device knows, and every head taken must cite a
                // tree head that connects to this view.
                let remembered = transparency.view(&cfg.server).cloned();
                let mut kt = KtSession::begin(&transport, &token, remembered.as_ref())
                    .map_err(|e| e.to_string())?;
                kt.audit(&DeviceExpectations {
                    identity: &identity,
                    synced: &synced,
                    vault: None,
                })
                .map_err(|e| e.to_string())?;
                pull_new_vaults(&transport, &token, &identity, &mut store, cfg, &kt)?;
                let view = kt.view.clone();
                drop(kt);
                transparency
                    .remember(&cfg.server, view)
                    .map_err(|e| e.to_string())?;
                cfg.save(&paths.sync_file())?;
            }
        }
        let trust_for = |vault_id: Id| {
            let mut keys = synced.device_keys();
            for (id, key) in device_keys(&identity) {
                keys.entry(id).or_insert(key);
            }
            DeviceTrust {
                keys,
                revoked_after: synced.revoked_after(vault_id),
            }
        };

        let opened = if mode == Mode::Init {
            let last = |vault_id: Id| memory.last_accepted_seq(vault_id);
            match persona_vaults(&identity, persona_id, &store)
                .map_err(|e| e.to_string())?
                .first()
            {
                Some(vault_id) => open_vault_trusting(
                    &identity,
                    *vault_id,
                    &store,
                    Box::new(keystore.clone()),
                    last(*vault_id),
                    &trust_for(*vault_id),
                )
                .map_err(|e| e.to_string())?,
                None => identity::open_or_create_vault(
                    &mut identity,
                    persona_id,
                    &mut store,
                    Box::new(keystore.clone()),
                    &last,
                )
                .map_err(|e| e.to_string())?,
            }
        } else {
            let vaults =
                persona_vaults(&identity, persona_id, &store).map_err(|e| e.to_string())?;
            let Some(vault_id) = vaults.first() else {
                return Err(format!(
                    "this persona has no vault at {} yet",
                    paths.data.display()
                ));
            };
            open_vault_trusting(
                &identity,
                *vault_id,
                &store,
                Box::new(keystore.clone()),
                memory.last_accepted_seq(*vault_id),
                &trust_for(*vault_id),
            )
            .map_err(|e| e.to_string())?
        };
        // Every other vault of the persona opens too, under the same checks;
        // the oldest is the first vault, whichever opened above.
        let mut all = vec![opened];
        let trust_for = |vault_id: Id| {
            let mut keys = synced.device_keys();
            for (id, key) in device_keys(&identity) {
                keys.entry(id).or_insert(key);
            }
            DeviceTrust {
                keys,
                revoked_after: synced.revoked_after(vault_id),
            }
        };
        for vault_id in persona_vaults(&identity, persona_id, &store).map_err(|e| e.to_string())? {
            if all.iter().any(|o| o.vault.context().vault_id == vault_id) {
                continue;
            }
            all.push(
                open_vault_trusting(
                    &identity,
                    vault_id,
                    &store,
                    Box::new(keystore.clone()),
                    memory.last_accepted_seq(vault_id),
                    &trust_for(vault_id),
                )
                .map_err(|e| e.to_string())?,
            );
        }
        organize::order_vaults(&mut all);
        let opened = all.remove(0);
        let recency = crate::recency::Recency::open(
            crate::recency::Recency::path_in(&paths.data, persona_id),
            &opened.keys.app_data_key,
        );
        let sharing_state = crate::sharing::SharingState::open(
            crate::sharing::SharingState::path_in(&paths.data, persona_id),
            persona_id,
            &opened.keys.app_data_key,
        )?;
        let mut session = Self {
            paths,
            keystore,
            identity,
            store,
            memory,
            synced,
            transparency,
            sync,
            opened,
            transfer: transfer::Pending::default(),
            more: all,
            recency,
            sharing: sharing_state,
            rosters: BTreeMap::new(),
            received: Vec::new(),
            opened_send: None,
            link: None,
            phrase_checks: BTreeMap::new(),
            quick: None,
        };
        session.load_sharing()?;
        session.remember()?;
        session.sweep_trash()?;
        Ok(session)
    }

    /// Writes every vault and records each one's newest head.
    fn save(&mut self) -> Result<(), String> {
        self.opened
            .save(&mut self.store)
            .map_err(|e| e.to_string())?;
        for o in &mut self.more {
            o.save(&mut self.store).map_err(|e| e.to_string())?;
        }
        self.remember()
    }

    fn remembered_head(&self) -> Result<RememberedHead, String> {
        let head = self.vault().head().map_err(|e| e.to_string())?;
        Ok(RememberedHead {
            seq: head.seq,
            hash: head.hash().map_err(|e| e.to_string())?,
        })
    }

    fn remember(&mut self) -> Result<(), String> {
        let head = self.remembered_head()?;
        self.memory
            .remember_vault(self.vault().context().vault_id, head)
            .map_err(|e| e.to_string())?;
        for o in &self.more {
            let head = o.vault.head().map_err(|e| e.to_string())?;
            let remembered = RememberedHead {
                seq: head.seq,
                hash: head.hash().map_err(|e| e.to_string())?,
            };
            self.memory
                .remember_vault(o.vault.context().vault_id, remembered)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Has every open vault trust the devices the identity's signed records
    /// list now (a device enrolled since unlock, learned in this sync's
    /// record pull), and refuse what revoked ones sign past their boundary.
    fn refresh_trust(&mut self) {
        let mut keys = self.synced.device_keys();
        for (id, key) in device_keys(&self.identity) {
            keys.entry(id).or_insert(key);
        }
        let mut vaults: Vec<&mut Vault> = std::iter::once(&mut self.opened.vault)
            .chain(self.more.iter_mut().map(|o| &mut o.vault))
            .collect();
        for vault in vaults.iter_mut() {
            for (id, key) in &keys {
                vault.trust_device(*id, key.clone());
            }
            for (id, after) in self.synced.revoked_after(vault.context().vault_id) {
                vault.revoke_device(id, after);
            }
        }
    }

    /// The persona's first vault (where new items go by default).
    pub fn vault(&self) -> &Vault {
        &self.opened.vault
    }

    /// Every open vault of the persona, the first one first, then the
    /// items others shared with it on their own (read only).
    pub fn vaults(&self) -> impl Iterator<Item = &Vault> {
        std::iter::once(&self.opened.vault)
            .chain(self.more.iter().map(|o| &o.vault))
            .chain(self.received.iter())
    }

    /// The vault holding `item_id`.
    pub(crate) fn holding(&self, item_id: Id) -> Result<&Vault, String> {
        self.vaults()
            .find(|v| v.item(item_id).is_ok())
            .ok_or_else(|| format!("item {item_id} wasn't found in this persona's vaults"))
    }

    /// The vault holding `item_id`, to change it: refused for a vault this
    /// persona may only view or fill, and for an item shared on its own.
    pub(crate) fn holding_mut(&mut self, item_id: Id) -> Result<&mut Vault, String> {
        if let Ok(vault) = self.holding(item_id) {
            let vault_id = vault.context().vault_id;
            if self.is_received_vault(vault_id) {
                return Err("this was shared with you on its own; you can't change it".into());
            }
            self.require_edit(vault_id)?;
        }
        if self.opened.vault.item(item_id).is_ok() {
            return Ok(&mut self.opened.vault);
        }
        self.more
            .iter_mut()
            .map(|o| &mut o.vault)
            .find(|v| v.item(item_id).is_ok())
            .ok_or_else(|| format!("item {item_id} wasn't found in this persona's vaults"))
    }

    /// What this device knows, for the key-transparency audit: the
    /// identity, the synced records, and every vault of the persona at the
    /// `seq` the server last confirmed for it.
    fn expectations(&self) -> AllVaults<'_> {
        let synced = |vault: &Vault| {
            self.sync
                .as_ref()
                .and_then(|c| c.vaults.get(&vault.context().vault_id))
                .map(|s| s.synced_seq)
                .unwrap_or(0)
        };
        let vault = self.vault();
        AllVaults {
            base: DeviceExpectations {
                identity: &self.identity,
                synced: &self.synced,
                vault: Some((vault, synced(vault))),
            },
            more: self
                .more
                .iter()
                .map(|o| (&o.vault, synced(&o.vault)))
                .collect(),
        }
    }

    fn device_binding(&self) -> DeviceBinding {
        binding_of(&self.keystore)
    }

    // --- items -------------------------------------------------------------

    /// Adds a login and returns its id. The password arrives as bytes a
    /// shell read from its own field and wipes afterwards. The narrow form
    /// of [`Self::add_item`]: the site is checked as an origin first, and
    /// left without a title the login takes the site's host.
    pub fn add_login(
        &mut self,
        site: &str,
        username: &str,
        password: &[u8],
        title: Option<&str>,
    ) -> Result<Id, String> {
        lockdown_matcher::Origin::parse(site).map_err(|e| e.to_string())?;
        let changes = vec![
            FieldChange::text("url.0", site),
            FieldChange::text("user", username),
            FieldChange::secret("pw", password),
        ];
        self.add_item("login", title, false, changes)
    }

    /// Moves an item to the trash (its grace period starts; nothing is
    /// destroyed).
    pub fn trash(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        self.holding_mut(item_id)?
            .trash_item(item_id)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// Brings a trashed item back from the Deleted view, as it was.
    pub fn restore(&mut self, id: &str) -> Result<(), String> {
        let item_id = parse_id(id)?;
        self.holding_mut(item_id)?
            .restore_item(item_id)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// Every visible item, newest edit first, trashed ones included (their
    /// `state` says so, and `views` puts them in Deleted alone): nothing
    /// hidden is in it. The facts a row and the smart views are drawn from
    /// (`views::classify`) are decided here, once, for every shell.
    pub fn list(&self) -> Result<Vec<ItemSummary>, String> {
        let login = self.sync.as_ref().map(|c| c.login.as_str());
        let now = now_secs();
        let mut rows = Vec::new();
        for (vault, item) in self.vaults().flat_map(|v| v.items().map(move |i| (v, i))) {
            if item.kind.hidden()
                || !matches!(
                    item.state,
                    ItemState::Live | ItemState::Archived | ItemState::Trashed
                )
            {
                continue;
            }
            // A crown jewel an emergency release didn't hand over stays
            // its owner's alone: not listed (`legacy`).
            if !vault.can_open(item.item_id) {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let latest = item.latest().map_err(|e| e.to_string())?;
            let title = text_of(&fields, "title").unwrap_or_default();
            let mut row = ItemSummary {
                id: item.item_id.to_string(),
                kind: item.kind.as_str().to_string(),
                state: state_name(item.state).to_string(),
                conflict_copy: views::is_conflict_copy(&title),
                title,
                site: fields
                    .iter()
                    .find(|f| f.kind == FieldKind::Url)
                    .and_then(|f| match &f.value {
                        FieldValue::Url { value, .. } => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
                username: text_of(&fields, "user").unwrap_or_default(),
                edited_at: latest.edited_at.0,
                crown_jewel: item.class == ItemClass::CrownJewel,
                methods: Vec::new(),
                passkeys: 0,
                codes: 0,
                expires_at: None,
                warnings: Vec::new(),
                views: Vec::new(),
                subtitle: String::new(),
                copy: items::copy_target(item.kind, &fields),
                vault_id: vault.context().vault_id.to_string(),
                folder: None,
                tags: Vec::new(),
                favourite: false,
                last_used: None,
                uses: 0,
                expires_in_days: None,
                attachments: latest
                    .attachments
                    .iter()
                    .filter(|a| !a.is_shredded())
                    .count() as u32,
                access: self
                    .item_right(item.item_id)
                    .map(|r| r.as_str().to_string()),
                template: depth::template_id(item.kind, &fields),
            };
            if depth::away_of(&fields).iter().any(|a| a.remind_at <= now) {
                row.warnings.push(depth::AWAY_WARNING.into());
            }
            if row.access.as_deref() == Some("fill-only") {
                // Nothing to copy: a fill-only item only fills.
                row.copy = None;
            }
            let org = lockdown_vault::Organization::of(&fields);
            row.folder = org.folder.map(|f| f.to_string());
            row.tags = org.tags;
            row.favourite = org.favourite;
            if let Some(u) = self.recency.get(item.item_id) {
                row.last_used = Some(u.last);
                row.uses = u.count;
            }
            let template = lockdown_vault::template_of(item.kind, &fields);
            row.subtitle = template
                .subtitle
                .and_then(|s| text_of(&fields, s))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| row.username.clone());
            if row.crown_jewel {
                // Never in search previews (02-model §3): a crown jewel's
                // summary is its title, kind and facts, no field.
                row.site.clear();
                row.username.clear();
                row.subtitle.clear();
            }
            let mut has_password = false;
            let held = passkeys::groups(&fields);
            row.passkeys = held.len() as u32;
            if held.iter().any(|p| p.hidden.is_some()) {
                row.warnings.push(PASSKEY_HIDDEN_WARNING.into());
            }
            for f in &fields {
                match f.kind {
                    FieldKind::Totp => {
                        row.codes += 1;
                        if let FieldValue::Text(uri) = &f.value
                            && let Ok(code) = lockdown_codes::parse_uri(uri)
                            && codes::issuer_equals_login(&code, login)
                            && !row.warnings.iter().any(|w| w == codes::ISSUER_WARNING)
                        {
                            row.warnings.push(codes::ISSUER_WARNING.into());
                        }
                    }
                    FieldKind::Hidden if f.field_id == "pw" => has_password = true,
                    FieldKind::Date
                        if f.field_id == "expires_at"
                            || f.field_id == "card.expiry"
                            || f.label.to_ascii_lowercase().contains("expir") =>
                    {
                        if let FieldValue::Text(t) = &f.value {
                            row.expires_at = Some(t.clone());
                        }
                    }
                    _ => {}
                }
            }
            if item.kind == lockdown_vault::ItemKind::Login {
                if has_password {
                    row.methods.push("password".into());
                }
                if row.passkeys > 0 {
                    row.methods.push("passkey".into());
                }
                if row.codes > 0 {
                    row.methods.push("code".into());
                }
            }
            row.expires_in_days = row
                .expires_at
                .as_deref()
                .and_then(|d| views::days_until(d, now));
            row.views = views::classify(&row, now)
                .into_iter()
                .map(|v| v.as_str().to_string())
                .collect();
            rows.push(row);
        }
        rows.sort_by(|a, b| b.edited_at.cmp(&a.edited_at).then(a.title.cmp(&b.title)));
        Ok(rows)
    }

    /// Whether an item is under the crown-jewel treatment (D98), so a
    /// reveal or a use of it goes through the gate first.
    pub fn is_crown_jewel(&self, id: &str) -> Result<bool, String> {
        let item_id = parse_id(id)?;
        let item = self
            .holding(item_id)?
            .item(item_id)
            .map_err(|e| e.to_string())?;
        Ok(item.class == ItemClass::CrownJewel)
    }

    /// Every persona of the identity, the open one marked.
    pub fn personas(&self) -> Vec<PersonaInfo> {
        let active = self.vault().context().persona_id;
        self.identity
            .personas()
            .map(|p| PersonaInfo {
                id: p.persona_id.to_string(),
                label: p.label.clone(),
                kind: format!("{:?}", p.kind).to_ascii_lowercase(),
                active: p.persona_id == active,
            })
            .collect()
    }

    /// Whether an account (and so an account password) exists here.
    pub fn has_account(&self) -> bool {
        self.sync.is_some()
    }

    /// Forgets the session token when the server ended this session from
    /// another device (`live`): the account stays, and the next sync asks
    /// for a login instead of failing on a dead token.
    #[cfg(all(feature = "native", not(target_os = "wasi")))]
    pub(crate) fn forget_session_token(&mut self, token: &str) -> Result<(), String> {
        let path = self.paths.sync_file();
        if let Some(cfg) = self.sync.as_mut()
            && cfg.token == token
        {
            cfg.token = String::new();
            cfg.expires_at = 0;
            cfg.save(&path)?;
        }
        Ok(())
    }

    /// Checks the account password with an OPAQUE sign-in (the password
    /// never leaves the device; the server learns only that a sign-in
    /// happened). `Ok(false)` is a wrong password; `Err` is a server that
    /// couldn't be reached or refused for another reason.
    pub fn check_password(&self, password: &str) -> Result<bool, String> {
        let cfg = self.sync.as_ref().ok_or_else(|| {
            "no account on this device, so there is no password to check".to_string()
        })?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, cfg.login.as_str(), self.device_binding())
            .at_least(self.account_ksf_floor());
        match auth.sign_in(password) {
            Ok(_) => Ok(true),
            Err(lockdown_crypto::Error::LoginFailed) => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    /// One item. Hidden values come back masked; with `reveal`, each hidden
    /// field also carries its bytes (wiped when the view drops).
    pub fn show(&self, id: &str, reveal: bool) -> Result<ItemDetail, String> {
        let item_id = parse_id(id)?;
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        if item.kind.hidden() {
            return Err(format!("'{id}' isn't an item you can open"));
        }
        let latest = item.latest().map_err(|e| e.to_string())?;
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        // A fill-only item is never shown, whatever the caller asked.
        let reveal =
            reveal && self.item_right(item_id) != Some(lockdown_vault::sharing::Right::FillOnly);
        let vault_id = vault.context().vault_id;
        let mut shown = shown_fields(&fields, reveal);
        for f in shown.iter_mut().filter(|f| f.kind == "linked") {
            f.link = Some(self.link_view(vault_id, f.value.as_deref().unwrap_or("")));
        }
        Ok(ItemDetail {
            id: item_id.to_string(),
            kind: item.kind.as_str().to_string(),
            state: state_name(item.state).to_string(),
            version: latest.n,
            edited_at: latest.edited_at.0,
            crown_jewel: item.class == ItemClass::CrownJewel,
            export_opt_in: lockdown_vault::export_opted_in(&fields),
            fields: shown,
            place: self.place_of(vault, item_id, &fields)?,
            template: depth::template_id(item.kind, &fields),
            away: depth::away_views(&fields, now_secs()),
            reprompt: depth::reprompts(item.kind, &fields),
        })
    }

    /// One hidden field's bytes, for a reveal or a copy. Refused for a
    /// field that isn't hidden (those come with `show`).
    pub fn reveal(&self, id: &str, field_id: &str) -> Result<Revealed, String> {
        let item_id = parse_id(id)?;
        let fields = self
            .holding(item_id)?
            .open_item(item_id)
            .map_err(|e| e.to_string())?;
        let field = fields
            .iter()
            .find(|f| f.field_id == field_id)
            .ok_or_else(|| format!("no field '{field_id}' on this item"))?;
        if field.kind == FieldKind::Passkey {
            return Err("a passkey's material is never shown; it signs inside Lockdown".into());
        }
        match &field.value {
            FieldValue::Text(t) if is_secret_kind(field.kind) => Ok(Revealed::from_text(t)),
            FieldValue::Bytes(b) if is_secret_kind(field.kind) => Ok(Revealed::new(b.clone())),
            _ => Err(format!("'{field_id}' isn't a hidden field")),
        }
    }

    /// `status`.
    pub fn status(&self) -> Result<Status, String> {
        let profile = self.identity.profile();
        let device_id = self.identity.device_id();
        let device_name = self
            .identity
            .devices()
            .find(|d| d.device_id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "this device".into());
        let persona_id = self.vault().context().persona_id;
        let persona_label = self
            .identity
            .personas()
            .find(|p| p.persona_id == persona_id)
            .map(|p| format!("{} ({:?})", p.label, p.kind))
            .unwrap_or_else(|| "unknown".into());
        let head = self.vault().head().map_err(|e| e.to_string())?;
        let hash = head.hash().map_err(|e| e.to_string())?;
        let live = self
            .vaults()
            .flat_map(|v| v.items())
            .filter(|i| i.state == ItemState::Live && !i.kind.hidden())
            .count();
        let vault_id = self.vault().context().vault_id;
        let account = self.sync.as_ref().map(|cfg| {
            let state = cfg.vaults.get(&vault_id).copied().unwrap_or_default();
            AccountStatus {
                server: cfg.server.clone(),
                login: cfg.login.clone(),
                synced_seq: state.synced_seq,
                synced_rev: state.synced_rev,
                last_sync_at: cfg.last_sync_at,
                session_expires_at: cfg.expires_at,
                transparency: self.transparency.view(&cfg.server).map(transparency_view),
            }
        });
        Ok(Status {
            root: self.paths.root.display().to_string(),
            data: self.paths.data.display().to_string(),
            profile_id: profile.profile_id.to_string(),
            manager: profile.manager.clone(),
            recovery: format!("{:?}", profile.recovery).to_lowercase(),
            device_id: device_id.to_string(),
            device_name,
            persona_id: persona_id.to_string(),
            persona_label,
            vault_id: vault_id.to_string(),
            head_seq: head.seq,
            head_hash: hash.to_hex(),
            live_items: live,
            devices: self.identity.devices().count(),
            devices_revoked: self
                .identity
                .devices()
                .filter(|d| d.revoked_at.is_some())
                .count(),
            identity_seq: self.synced.seq(),
            account,
        })
    }

    // --- the account --------------------------------------------------------

    /// Makes the account on `server` under `login` with OPAQUE, writes the
    /// identity's account unlock slot with the recovery kit's Secret Key
    /// mixed in (04-security §3; D70), publishes the identity's records,
    /// and keeps the session. When the identity has no kit yet one is made
    /// here and returned as its page, to be shown or saved once; when it
    /// has one, its Secret Key must be given.
    pub fn account_register(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> Result<Registered, String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
        let profile_id = self.identity.profile().profile_id;
        let (sk, kit_text) = if self.identity.profile().recovery == RecoveryState::None {
            if secret_key.is_some_and(|s| !s.trim().is_empty()) {
                return Err(
                    "this identity has no recovery kit yet, so there is no Secret Key to give; leave it out and the kit is made here"
                        .into(),
                );
            }
            let kit = self
                .identity
                .create_recovery_kit()
                .map_err(|e| e.to_string())?;
            self.memory
                .remember_identity(self.identity.head())
                .map_err(|e| e.to_string())?;
            let text = render_kit(&kit, Some((login, server)), &device_name());
            (*kit.secret_key().expose(), Some(text))
        } else {
            let Some(text) = secret_key.filter(|s| !s.trim().is_empty()) else {
                return Err(
                    "this identity already has a recovery kit; give its Secret Key so the account is protected by it"
                        .into(),
                );
            };
            (secret_key_bytes(profile_id, text)?, None)
        };
        self.identity
            .enable_account_unlock(&mut auth, password, Some(&sk))
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
        self.keep_secret_key(&sk)?;
        let (_export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        sync_identity_records(
            auth.rounds(),
            &session.token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults: BTreeMap::new(),
            last_sync_at: None,
        };
        cfg.save(&self.paths.sync_file())?;
        self.sync = Some(cfg);
        let mut lines = vec![format!("account {login} registered at {server}")];
        if kit_text.is_some() {
            lines.push(
                "a recovery kit was made; its Secret Key now protects the account together with the password"
                    .into(),
            );
        }
        lines.push("session kept; sync now to push the vault".into());
        Ok(Registered {
            lines,
            kit: kit_text.map(|t| Revealed::from_text(&t)),
        })
    }

    /// Logs in on a device that already holds the identity: a fresh
    /// session, and the identity's records brought in step with the
    /// server. An account slot written without a Secret Key (D60) is
    /// upgraded here when the kit exists and its Secret Key is given.
    pub fn account_login(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let secret_key = secret_key.filter(|s| !s.trim().is_empty());
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding())
            .at_least(self.account_ksf_floor());
        let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        if session.profile_id != self.identity.profile().profile_id {
            return Err(format!(
                "the account '{login}' belongs to another profile ({}), not the identity at {}",
                session.profile_id,
                self.paths.root.display()
            ));
        }
        sync_identity_records(
            auth.rounds(),
            &session.token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        let mut lines = vec![format!("logged in as {login} at {server}")];
        if let Some(text) = secret_key {
            let sk = secret_key_bytes(self.identity.profile().profile_id, text)?;
            if secret_key::opens_account_slot(&self.identity, &export_key, Some(&sk)) {
                self.keep_secret_key(&sk)?;
            }
        }
        if slot_opens_without_secret_key(&self.identity, &export_key) {
            match (self.identity.profile().recovery, secret_key) {
                (RecoveryState::None, _) => lines.push(
                    "notice: this account can still be opened with the password alone; make the recovery kit to protect it with a Secret Key"
                        .into(),
                ),
                (_, None) => lines.push(
                    "notice: this account can still be opened with the password alone; log in again with the Secret Key from your recovery kit to finish protecting it"
                        .into(),
                ),
                (_, Some(text)) => {
                    let sk = secret_key_bytes(self.identity.profile().profile_id, text)?;
                    self.upgrade_account_slot(export_key, password, &sk)?;
                    self.keep_secret_key(&sk)?;
                    sync_identity_records(
                        auth.rounds(),
                        &session.token,
                        &mut self.identity,
                        self.keystore.as_ref(),
                        &mut self.synced,
                        &mut self.memory,
                    )?;
                    lines.push(
                        "account slot upgraded: the Secret Key from the kit is now needed beside the password"
                            .into(),
                    );
                }
            }
        }
        let (vaults, last_sync_at) = match &self.sync {
            Some(old) if old.server == auth.rounds().base() && old.login == login => {
                (old.vaults.clone(), old.last_sync_at)
            }
            _ => (BTreeMap::new(), None),
        };
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults,
            last_sync_at,
        };
        cfg.save(&self.paths.sync_file())?;
        self.sync = Some(cfg);
        Ok(lines)
    }

    /// Takes a fresh session from a sign-in the shell ran itself for this
    /// account (the web vault's unlock, docs/plan/17-web-vault.md), so the
    /// next sync uses it.
    pub fn renew_session(
        &mut self,
        server: &str,
        login: &str,
        session: lockdown_crypto::auth::Session,
    ) -> Result<(), String> {
        let Some(cfg) = self.sync.as_mut() else {
            return Err("no account on this device; register or log in first".into());
        };
        if cfg.server != server.trim_end_matches('/') || cfg.login != login {
            return Err(format!(
                "this device's account is {} at {}, not {login} at {server}",
                cfg.login, cfg.server
            ));
        }
        if session.profile_id != cfg.profile_id {
            return Err("that session is for another profile".into());
        }
        cfg.token = session.token;
        cfg.expires_at = session.expires_at;
        cfg.save(&self.paths.sync_file())
    }

    /// Rewrites the account slot with the Secret Key mixed in (D70).
    fn upgrade_account_slot(
        &mut self,
        export_key: ExportKey,
        password: &str,
        secret_key: &[u8; 16],
    ) -> Result<(), String> {
        let profile_id = self.identity.profile().profile_id;
        if account_envelope(&self.identity).is_some() {
            let irk_id = self.identity.irk_id().map_err(|e| e.to_string())?;
            self.identity
                .retire_envelope(irk_id, KeyRef::new(KeyRefKind::Account, profile_id))
                .map_err(|e| e.to_string())?;
        }
        let mut prepared = PreparedExportKey(Some(export_key));
        self.identity
            .enable_account_unlock(&mut prepared, password, Some(secret_key))
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())
    }

    // --- the recovery kit ---------------------------------------------------

    /// Makes the recovery kit (04-security §8) and returns its page, shown
    /// or saved once and never kept. With an account, the account password
    /// is needed: the slot is rewritten with the new Secret Key (D70). A
    /// kit that exists is replaced only with `replace`.
    pub fn kit_create(
        &mut self,
        replace: bool,
        password: Option<&str>,
    ) -> Result<KitCreated, String> {
        if self.identity.profile().recovery != RecoveryState::None && !replace {
            return Err(
                "this identity already has a recovery kit; verify it, or replace it (the old page stops working)"
                    .into(),
            );
        }
        let password = password.filter(|p| !p.is_empty());
        let mut auth = match &self.sync {
            Some(cfg) => {
                let Some(password) = password else {
                    return Err(
                        "this device has an account, so the new Secret Key must go into its slot: give the account password"
                            .into(),
                    );
                };
                let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
                let mut auth = OpaqueAccountAuth::new(transport, &cfg.login, self.device_binding());
                let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
                Some((auth, session, export_key))
            }
            None => None,
        };
        self.raise_profile_kdf()?;
        let kit = self
            .identity
            .create_recovery_kit()
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
        let mut notes = Vec::new();
        let login = self
            .sync
            .as_ref()
            .map(|c| (c.login.clone(), c.server.clone()));
        if let Some((auth, session, export_key)) = auth.take() {
            let password = password.expect("checked above");
            self.upgrade_account_slot(export_key, password, kit.secret_key().expose())?;
            self.keep_secret_key(kit.secret_key().expose())?;
            sync_identity_records(
                auth.rounds(),
                &session.token,
                &mut self.identity,
                self.keystore.as_ref(),
                &mut self.synced,
                &mut self.memory,
            )?;
            notes.push(
                "the account's slot now needs this Secret Key beside the password; other devices learn it at their next sync"
                    .into(),
            );
        } else {
            secret_key::forget(&self.paths.data);
            notes.push(
                "no account on this device yet; registering one will ask for this Secret Key"
                    .into(),
            );
        }
        let text = render_kit(
            &kit,
            login.as_ref().map(|(l, s)| (l.as_str(), s.as_str())),
            &device_name(),
        );
        Ok(KitCreated {
            kit: Revealed::from_text(&text),
            notes,
        })
    }

    /// Types the kit back in and checks it opens this identity, changing
    /// nothing (04-security §9). With `drill`, a passing check is recorded.
    pub fn kit_verify(
        &mut self,
        secret_key: &str,
        recovery_code: &str,
        password: Option<&str>,
        drill: bool,
    ) -> Result<Vec<String>, String> {
        let password = password.filter(|p| !p.is_empty());
        let profile = self.identity.profile().clone();
        if profile.recovery == RecoveryState::None {
            return Err("this identity has no recovery kit yet".into());
        }
        let kit = RecoveryKit::from_text(profile.profile_id, secret_key, recovery_code)
            .map_err(|e| e.to_string())?;
        let kit_ref = KeyRef::new(KeyRefKind::Kit, profile.profile_id);
        let envelope = self
            .identity
            .envelopes()
            .find(|e| e.purpose == Purpose::Irk && e.wrapped_by == kit_ref)
            .ok_or("this identity holds no kit wrap of its root key")?;
        let kit_key = kit.kit_key(&profile.kdf).map_err(|e| e.to_string())?;
        let opened: Result<Irk, _> = ecosystem_identity::keys::unwrap_key(
            envelope,
            &kit_key,
            &ecosystem_identity::keys::aad(profile.profile_id, None, envelope.key_id, Purpose::Irk),
        );
        let irk = opened.map_err(|_| {
            "the recovery code didn't open this identity: check it against the page, letter by letter"
                .to_string()
        })?;
        let derived = ecosystem_identity::signing_key(&irk).map_err(|e| e.to_string())?;
        if derived.verifying_key().to_bytes() != profile.isk_pub {
            return Err("the recovery code opened a key, but not this identity's".into());
        }
        drop(irk);
        let mut lines = vec!["recovery code: opens this identity".to_string()];
        match (&self.sync, password) {
            (Some(cfg), Some(password)) => {
                let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
                let mut auth = OpaqueAccountAuth::new(transport, &cfg.login, self.device_binding());
                let (export_key, _) = auth.sign_in(password).map_err(|e| e.to_string())?;
                let Some(account) = account_envelope(&self.identity) else {
                    return Err("this identity has no account slot".into());
                };
                let auk = ecosystem_identity::kdf::account_unlock_key(
                    export_key.expose(),
                    Some(kit.secret_key().expose()),
                    &profile.profile_id,
                )
                .map_err(|e| e.to_string())?;
                let opened: Result<Irk, _> = ecosystem_identity::keys::unwrap_key(
                    account,
                    &auk,
                    &ecosystem_identity::keys::aad(
                        profile.profile_id,
                        None,
                        account.key_id,
                        Purpose::Irk,
                    ),
                );
                if opened.is_err() {
                    return Err(
                        "the Secret Key and password didn't open the account slot: check the Secret Key against the page"
                            .into(),
                    );
                }
                self.keep_secret_key(kit.secret_key().expose())?;
                lines.push("Secret Key: opens the account slot with the password".into());
            }
            _ => lines.push(
                "Secret Key: well formed (give the account password too to check it against the account slot)"
                    .into(),
            ),
        }
        if drill {
            self.identity
                .mark_recovery_drilled()
                .map_err(|e| e.to_string())?;
            self.memory
                .remember_identity(self.identity.head())
                .map_err(|e| e.to_string())?;
            // The date of this drill and, with the code in hand, the kit's
            // stretching raised to what this device can afford (`recovery`).
            lines.extend(self.after_drill(&kit)?);
            if let Some(cfg) = &self.sync {
                let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
                let token = cfg.session_token()?.to_string();
                sync_identity_records(
                    &transport,
                    &token,
                    &mut self.identity,
                    self.keystore.as_ref(),
                    &mut self.synced,
                    &mut self.memory,
                )?;
            }
            lines.push("drill recorded: the profile now says the kit was tried and works".into());
        }
        Ok(lines)
    }

    // --- devices ------------------------------------------------------------

    /// Every device of the identity, oldest first.
    pub fn device_list(&self) -> Vec<DeviceInfo> {
        let mut devices: Vec<_> = self.identity.devices().collect();
        devices.sort_by_key(|d| (d.enrolled_at, d.device_id));
        let mine = self.identity.device_id();
        devices
            .into_iter()
            .map(|d| DeviceInfo {
                id: d.device_id.to_string(),
                name: d.name.clone(),
                platform: format!("{:?}", d.platform).to_lowercase(),
                enrolled_at: d.enrolled_at.0,
                revoked_at: d.revoked_at.map(|t| t.0),
                this_device: d.device_id == mine,
            })
            .collect()
    }

    /// Revokes another device of this identity (D73). Needs the account:
    /// the order between devices is the server's to fix.
    pub fn device_revoke(&mut self, selector: &str) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("revoking a device needs the account on this device; log in first".into());
        };
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        let device = self.find_device(selector)?;
        let (record, head) =
            identity::revoke_device(&mut self.identity, self.keystore.as_ref(), device)
                .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(head)
            .map_err(|e| e.to_string())?;
        for _ in 0..IDENTITY_ROUNDS {
            let vaults = transport.list_vaults(&token).map_err(|e| e.to_string())?;
            let mut boundaries: Vec<VaultBoundary> = vaults
                .vaults
                .iter()
                .map(|v| VaultBoundary {
                    vault_id: v.vault_id,
                    seq: v.head_seq,
                })
                .collect();
            boundaries.sort_by_key(|b| b.vault_id);
            let revocation = Revocation {
                device_id: device,
                vaults: boundaries.clone(),
            };
            match push_identity_records(
                &transport,
                &token,
                &mut self.identity,
                self.keystore.as_ref(),
                &mut self.synced,
                &mut self.memory,
                vec![revocation],
            ) {
                Ok(_) => {
                    let mut lines = vec![format!(
                        "device {device} ({}) revoked; nothing it signs from now on is accepted",
                        record.name
                    )];
                    for b in boundaries {
                        lines.push(format!(
                            "vault {}: its heads after seq {} are refused",
                            b.vault_id, b.seq
                        ));
                    }
                    return Ok(lines);
                }
                Err(e) if e.contains("list the vaults again") => continue,
                Err(e) => return Err(e),
            }
        }
        Err("the vaults kept moving while the revocation was being written; try again".into())
    }

    /// A device by its full id or a unique prefix of at least eight
    /// characters, or by its exact name when that names one device.
    fn find_device(&self, selector: &str) -> Result<Id, String> {
        if let Ok(id) = Id::parse(selector)
            && self.identity.devices().any(|d| d.device_id == id)
        {
            return Ok(id);
        }
        let by_prefix: Vec<Id> = if selector.len() >= 8 {
            self.identity
                .devices()
                .filter(|d| d.device_id.to_string().starts_with(selector))
                .map(|d| d.device_id)
                .collect()
        } else {
            Vec::new()
        };
        let by_name: Vec<Id> = self
            .identity
            .devices()
            .filter(|d| d.name == selector)
            .map(|d| d.device_id)
            .collect();
        match (by_prefix.as_slice(), by_name.as_slice()) {
            ([one], _) | ([], [one]) => Ok(*one),
            ([], []) => Err(format!("no device '{selector}' in this identity")),
            _ => Err(format!(
                "'{selector}' names more than one device; use the full id"
            )),
        }
    }

    // --- sync ---------------------------------------------------------------

    /// The identity's records first (every device and revocation known
    /// before a signature is judged), then key transparency (04-security
    /// §6: the server's tree head must continue the one this device saw,
    /// and the account's own log entries must be what this device knows),
    /// then the vault: pull, merge, push, head (03-architecture §6) with
    /// the verified tree head cited in the new head and every pulled
    /// head's citation checked against it, then the account's entries once
    /// more for what the sync added. A failed proof or a split view stops
    /// everything before a byte is written, with the audit's own words.
    pub fn sync(&mut self) -> Result<SyncOutcome, String> {
        let Some(cfg) = self.sync.as_mut() else {
            return Err("no account on this device; register or log in first".into());
        };
        let server = cfg.server.clone();
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        self.refresh_trust();
        let remembered = self.transparency.view(&server).cloned();
        let mut kt =
            KtSession::begin(&transport, &token, remembered.as_ref()).map_err(|e| e.to_string())?;
        kt.audit(&self.expectations()).map_err(|e| e.to_string())?;
        let cfg = self
            .sync
            .as_mut()
            .ok_or("no account on this device; register or log in first")?;
        let label = device_name();
        // Vaults another device of this persona made since: pulled under
        // the same log, then opened like the rest.
        let pulled = pull_new_vaults(
            &transport,
            &token,
            &self.identity,
            &mut self.store,
            cfg,
            &kt,
        )?;
        if pulled > 0 {
            self.open_new_vaults()?;
        }
        // Sharing in step first: vaults shared with this persona, new key
        // generations, items shared on their own (docs/plan/18-sharing.md).
        self.sync_sharing(&transport, &token, kt.sth())?;
        let syncer = Syncer {
            transport: &transport,
            token: &token,
            device_label: &label,
        };
        let mut report = SyncReport::default();
        for idx in 0..=self.more.len() {
            let cfg = self
                .sync
                .as_mut()
                .ok_or("no account on this device; register or log in first")?;
            let open = if idx == 0 {
                &mut self.opened
            } else {
                &mut self.more[idx - 1]
            };
            let vault_id = open.vault.context().vault_id;
            let record = open
                .vault
                .record(&open.keys.app_data_key)
                .and_then(|r| r.to_bytes())
                .map_err(|e| e.to_string())?;
            let state = cfg.vaults.entry(vault_id).or_default();
            let outcome = lockdown_sync::sync_vault_with(
                &syncer,
                &mut open.vault,
                &record,
                state,
                &mut self.store,
                Some(&kt),
            );
            // Whatever happened on the wire, what is in memory is saved and
            // remembered, so a half-finished round is picked up next time.
            open.save(&mut self.store).map_err(|e| e.to_string())?;
            let one = match outcome {
                Ok(one) => one,
                Err(e) => {
                    let _ = self.remember();
                    if let Some(cfg) = &self.sync {
                        cfg.save(&self.paths.sync_file())?;
                    }
                    return Err(e.to_string());
                }
            };
            let head = open.vault.head().map_err(|e| e.to_string())?;
            let remembered = RememberedHead {
                seq: head.seq,
                hash: head.hash().map_err(|e| e.to_string())?,
            };
            if one.chain_rewritten {
                self.memory
                    .replace_vault(vault_id, remembered)
                    .map_err(|e| e.to_string())?;
            } else {
                self.memory
                    .remember_vault(vault_id, remembered)
                    .map_err(|e| e.to_string())?;
            }
            if idx == 0 {
                report.head_seq = one.head_seq;
            }
            report.pushed += one.pushed;
            report.pulled += one.pulled;
            report.merged += one.merged;
            report.conflict_copies.extend(one.conflict_copies);
            report.blobs_pushed += one.blobs_pushed;
            report.blobs_pulled += one.blobs_pulled;
            report.rounds = report.rounds.max(one.rounds);
        }
        if let Some(cfg) = &self.sync {
            cfg.save(&self.paths.sync_file())?;
        }
        // A purge whose grace ran out on another device's clock, or here
        // while offline, happens now (and syncs next time).
        self.sweep_trash()?;
        // The entries this sync added (the new head, at least) are audited
        // under a fresh tree head that must continue the one just verified.
        let mut view = kt.view.clone();
        drop(kt);
        let mut kt =
            KtSession::begin(&transport, &token, Some(&view)).map_err(|e| e.to_string())?;
        kt.audit(&self.expectations()).map_err(|e| e.to_string())?;
        view = kt.view.clone();
        drop(kt);
        self.transparency
            .remember(&server, view.clone())
            .map_err(|e| e.to_string())?;
        if let Some(cfg) = self.sync.as_mut() {
            cfg.last_sync_at = Some(lockdown_vault::Timestamp::now().0);
            cfg.save(&self.paths.sync_file())?;
        }
        Ok(sync_outcome(&report, &view))
    }

    /// `lockdown audit`, for the Status screen's "Check now": verifies the
    /// server's current tree head continues the one remembered, audits
    /// every entry of this account from the start, and remembers the view.
    /// A failed check comes back as the audit's own sentence; nothing is
    /// changed by it.
    pub fn audit(&mut self) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("no account on this device; register or log in first".into());
        };
        let server = cfg.server.clone();
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        let remembered = self.transparency.view(&server).cloned();
        let mut kt =
            KtSession::begin(&transport, &token, remembered.as_ref()).map_err(|e| e.to_string())?;
        // From the start, so the whole account history is checked.
        kt.view.audited = 0;
        kt.view.identity_logged = None;
        kt.view.own_device_logged = false;
        let sth = kt.sth().clone();
        let mut lines = vec![
            format!(
                "tree head: {} entries, root {}, signed {}",
                sth.size,
                &sth.root.to_hex()[..16],
                lockdown_sync::engine::civil_date(sth.time)
            ),
            match &remembered {
                Some(r) => format!(
                    "continues the {} entries remembered here (root {})",
                    r.sth.size,
                    &r.sth.root.to_hex()[..16]
                ),
                None => "nothing was remembered yet; this tree head is the first this device pins"
                    .into(),
            },
        ];
        let report = kt
            .audit(&self.expectations())
            .map_err(|e| format!("{e} (the entries before it were fine; nothing was changed)"))?;
        for e in &report.checked {
            lines.push(format!(
                "#{} {} {}: verified",
                e.index,
                lockdown_sync::engine::civil_date(e.leaf.time),
                e.leaf.describe()
            ));
        }
        lines.push(format!(
            "{} entries of this account verified: every device key is one your identity lists, and your identity manifest and this vault's heads are the ones this device holds{}",
            report.checked.len(),
            if report.pending > 0 {
                format!("; {} newer entries wait for the next tree head", report.pending)
            } else {
                String::new()
            }
        ));
        let view = kt.view.clone();
        drop(kt);
        self.transparency
            .remember(&server, view)
            .map_err(|e| e.to_string())?;
        Ok(lines)
    }
}

// --- flows on a device with no identity -------------------------------------

/// Logs in on a device with no identity: pulls the identity's record set,
/// opens the root key through the account slot with the kit's Secret Key
/// (04-security §3) and enrols this device, then publishes the record set
/// that lists it. The vault arrives with the first open. The sign-in itself
/// is `second_factor::sign_in_new_device`'s, which waits for a security key
/// when the account has one.
/// Enrolment after a sign-in the shell ran itself with this
/// device's keystore (the web vault, which must see the export key to seal
/// its device key, docs/plan/17-web-vault.md): no second password round.
pub fn enrol_from_sign_in(
    paths: &Paths,
    server: &str,
    login: &str,
    export_key: ExportKey,
    session: lockdown_crypto::auth::Session,
    secret_key: Option<&str>,
) -> Result<Vec<String>, String> {
    check_root_empty(paths)?;
    let keystore_path = paths.data.join(KEYSTORE_FILE);
    if !keystore_path.exists() {
        return Err("the sign-in was made without this device's key; sign in again".into());
    }
    let keystore =
        Rc::new(FileKeystore::open_or_create(&keystore_path).map_err(|e| e.to_string())?);
    let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
    enrol_signed_in(
        paths, &transport, &keystore, login, export_key, session, secret_key,
    )
}

fn enrol_signed_in(
    paths: &Paths,
    transport: &HttpTransport,
    keystore: &Rc<FileKeystore>,
    login: &str,
    export_key: ExportKey,
    session: lockdown_crypto::auth::Session,
    secret_key: Option<&str>,
) -> Result<Vec<String>, String> {
    let server = transport.base().to_string();
    let secret_key = secret_key.filter(|s| !s.trim().is_empty());
    let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), None)
        .map_err(|e| e.to_string())?;
    pull_identity(transport, &session.token, &mut synced, None).map_err(|e| e.to_string())?;
    let verified = synced
        .verified()
        .cloned()
        .ok_or_else(|| format!("the account '{login}' has no identity records on {server} yet"))?;
    if verified.profile.profile_id != session.profile_id {
        return Err("the server's identity records are for another profile".into());
    }
    let backup = verified.to_backup();
    let sk = match secret_key {
        Some(text) => Some(secret_key_bytes(verified.profile.profile_id, text)?),
        None => None,
    };
    let mut prepared = PreparedExportKey(Some(export_key));
    let mut identity = Identity::recover_with_account(
        &paths.root,
        APP,
        &device_name(),
        keystore.as_ref(),
        AccountCredentials {
            auth: &mut prepared,
            password: "",
            secret_key: sk.as_ref(),
        },
        &backup,
    )
    .map_err(|e| match (e, sk.is_some()) {
        (ecosystem_identity::Error::WrongKey, false) => {
            "this account's slot needs the Secret Key from the recovery kit beside the password"
                .to_string()
        }
        (ecosystem_identity::Error::WrongKey, true) => {
            "the password and Secret Key didn't open the account slot: check the Secret Key against the page"
                .to_string()
        }
        (other, _) => other.to_string(),
    })?;
    let mut memory = HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
    memory
        .remember_identity(identity.head())
        .map_err(|e| e.to_string())?;
    // It opened the account slot, so it is the account's Secret Key.
    match &sk {
        Some(sk) => secret_key::keep(
            &paths.data,
            keystore.as_ref(),
            verified.profile.profile_id,
            sk,
        )?,
        None => {
            secret_key::keep_opted_out(&paths.data, keystore.as_ref(), verified.profile.profile_id)?
        }
    }
    sync_identity_records(
        transport,
        &session.token,
        &mut identity,
        keystore.as_ref(),
        &mut synced,
        &mut memory,
    )?;
    let cfg = SyncConfig {
        v: SYNC_V,
        server: server.clone(),
        login: login.to_string(),
        profile_id: session.profile_id,
        token: session.token,
        expires_at: session.expires_at,
        vaults: BTreeMap::new(),
        last_sync_at: None,
    };
    cfg.save(&paths.sync_file())?;
    Ok(vec![format!(
        "logged in as {login} at {server}; this device ({}) is now enrolled in the identity at {}",
        device_name(),
        paths.root.display()
    )])
}

/// On a device with no identity (01-identity §8 row 3), after the sign-in
/// (the password, for the session only; the second factor's wait may sit
/// between, `second_factor`): pulls the identity's record set, rebuilds the
/// root key from the kit, enrols this device and publishes the records that
/// list it. A wrong kit changes nothing and tells the server nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn recover_signed_in(
    paths: &Paths,
    transport: &HttpTransport,
    keystore: &Rc<FileKeystore>,
    login: &str,
    export_key: ExportKey,
    session: lockdown_crypto::auth::Session,
    secret_key: &str,
    recovery_code: &str,
) -> Result<Vec<String>, String> {
    let server = transport.base().to_string();
    if session.token.is_empty() {
        return Err("this sign-in still waits for a security key".into());
    }
    let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), None)
        .map_err(|e| e.to_string())?;
    pull_identity(transport, &session.token, &mut synced, None).map_err(|e| e.to_string())?;
    let verified = synced
        .verified()
        .cloned()
        .ok_or_else(|| format!("the account '{login}' has no identity records on {server} yet"))?;
    if verified.profile.profile_id != session.profile_id {
        return Err("the server's identity records are for another profile".into());
    }
    let kit = RecoveryKit::from_text(verified.profile.profile_id, secret_key, recovery_code)
        .map_err(|e| e.to_string())?;
    let backup = verified.to_backup();
    let mut identity = Identity::recover_with_kit(
        &paths.root,
        APP,
        &device_name(),
        keystore.as_ref(),
        &kit,
        &backup,
    )
    .map_err(|e| match e {
        ecosystem_identity::Error::KitRejected => {
            "the recovery kit didn't open this identity: check the recovery code against the page, letter by letter; nothing was changed"
                .to_string()
        }
        other => other.to_string(),
    })?;
    let mut memory = HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
    memory
        .remember_identity(identity.head())
        .map_err(|e| e.to_string())?;
    if secret_key::opens_account_slot(&identity, &export_key, Some(kit.secret_key().expose())) {
        secret_key::keep(
            &paths.data,
            keystore.as_ref(),
            identity.profile().profile_id,
            kit.secret_key().expose(),
        )?;
    }
    sync_identity_records(
        transport,
        &session.token,
        &mut identity,
        keystore.as_ref(),
        &mut synced,
        &mut memory,
    )?;
    let cfg = SyncConfig {
        v: SYNC_V,
        server: transport.base().to_string(),
        login: login.to_string(),
        profile_id: session.profile_id,
        token: session.token,
        expires_at: session.expires_at,
        vaults: BTreeMap::new(),
        last_sync_at: None,
    };
    cfg.save(&paths.sync_file())?;
    Ok(vec![format!(
        "recovered: this device ({}) is enrolled in the identity at {} through the recovery kit; logged in as {login} at {server}",
        device_name(),
        paths.root.display()
    )])
}

fn check_root_empty(paths: &Paths) -> Result<(), String> {
    match ecosystem_identity::Store::at(&paths.root)
        .probe()
        .map_err(|e| e.to_string())?
    {
        ecosystem_identity::store::Probe::Missing => Ok(()),
        _ => Err(format!(
            "an ecosystem identity already exists at {} but this device isn't enrolled in it; \
             enrol it from another ecosystem app running on this device, or with the recovery kit",
            paths.root.display()
        )),
    }
}

// --- shared pieces ------------------------------------------------------------

/// The server's log as a shell shows it.
fn transparency_view(view: &ServerView) -> TransparencyView {
    TransparencyView {
        log_size: view.sth.size,
        root_prefix: view.sth.root.to_hex()[..16].to_string(),
        audited: view.audited,
        checked_at: view.checked_at,
    }
}

/// What `sync` reports.
fn sync_outcome(report: &SyncReport, view: &ServerView) -> SyncOutcome {
    let mut lines = vec![format!(
        "synced: pushed {}, pulled {}, merged {}; vault head seq {} ({} round{})",
        report.pushed,
        report.pulled,
        report.merged,
        report.head_seq,
        report.rounds,
        if report.rounds == 1 { "" } else { "s" }
    )];
    for id in &report.conflict_copies {
        lines.push(format!(
            "conflict copy {id}: both devices changed the same field; both versions are kept"
        ));
    }
    let transparency = transparency_view(view);
    lines.push(format!(
        "transparency: log of {} entries verified (root {}), your entries audited through {}",
        transparency.log_size, transparency.root_prefix, transparency.audited
    ));
    SyncOutcome {
        pushed: report.pushed,
        pulled: report.pulled,
        merged: report.merged,
        conflict_copies: report
            .conflict_copies
            .iter()
            .map(|i| i.to_string())
            .collect(),
        head_seq: report.head_seq,
        rounds: report.rounds,
        transparency,
        lines,
    }
}

/// Brings the identity's records in step with the server (D71).
fn sync_identity_records(
    transport: &dyn Transport,
    token: &str,
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    synced: &mut SyncedIdentity,
    memory: &mut HeadMemory,
) -> Result<(), String> {
    pull_and_merge_identity(transport, token, identity, keystore, synced, memory)?;
    push_identity_records(
        transport,
        token,
        identity,
        keystore,
        synced,
        memory,
        Vec::new(),
    )?;
    Ok(())
}

fn pull_and_merge_identity(
    transport: &dyn Transport,
    token: &str,
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    synced: &mut SyncedIdentity,
    memory: &mut HeadMemory,
) -> Result<bool, String> {
    let before = synced.manifest().cloned();
    let isk_pub = identity.profile().isk_pub;
    let changed =
        pull_identity(transport, token, synced, Some(&isk_pub)).map_err(|e| e.to_string())?;
    let Some(verified) = synced.verified().cloned() else {
        return Ok(false);
    };
    let report =
        merge_records(identity, keystore, &verified, before.as_ref()).map_err(|e| e.to_string())?;
    if let Some(head) = report.head {
        memory.remember_identity(head).map_err(|e| e.to_string())?;
    }
    if report.this_device_revoked {
        return Err(
            "this device was revoked from the identity by another device; it can't write to the account any more. \
             To use Lockdown here again, enrol it afresh with the recovery kit or from another device"
                .into(),
        );
    }
    Ok(changed)
}

/// Publishes this identity's records when they differ from the set the
/// server holds, carrying every revocation already in force plus
/// `new_revocations`. Returns whether anything was published.
fn push_identity_records(
    transport: &dyn Transport,
    token: &str,
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    synced: &mut SyncedIdentity,
    memory: &mut HeadMemory,
    new_revocations: Vec<Revocation>,
) -> Result<bool, String> {
    let isk_pub = identity.profile().isk_pub;
    // The identity's post-quantum key goes on the profile once (D248), and
    // only when every device says it keeps the second signature (the
    // rollout rule, docs/plan/26-desktop-finish.md §10); an identity another
    // app manages keeps signing with Ed25519 alone.
    if lockdown_vault::identity::check_manager_public(identity).is_ok() {
        lockdown_vault::pq::roll_out(identity, keystore).map_err(|e| e.to_string())?;
    }
    for _ in 0..IDENTITY_ROUNDS {
        let prev = synced.manifest().cloned();
        let mut revocations: Vec<Revocation> = prev
            .as_ref()
            .map(|p| p.revocations.clone())
            .unwrap_or_default();
        for r in &new_revocations {
            if !revocations.iter().any(|have| have.device_id == r.device_id) {
                revocations.push(r.clone());
            }
        }
        let mut set =
            record_set(identity, prev.as_ref(), revocations).map_err(|e| e.to_string())?;
        if let Some(p) = &prev
            && set.same_contents(p)
        {
            return Ok(false);
        }
        let isk = identity_signing_key(identity, keystore).map_err(|e| e.to_string())?;
        if lockdown_vault::pq::published_key(identity.profile())
            .map_err(|e| e.to_string())?
            .is_some()
        {
            let pq = lockdown_vault::pq::identity_pq_key(identity, keystore)
                .map_err(|e| e.to_string())?;
            set.sign_hybrid(&isk, &pq).map_err(|e| e.to_string())?;
        } else {
            set.sign(&isk).map_err(|e| e.to_string())?;
        }
        drop(isk);
        match push_identity(transport, token, &set, synced, Some(&isk_pub))
            .map_err(|e| e.to_string())?
        {
            Ok(_) => return Ok(true),
            Err(refused) => match refused.code {
                ErrorCode::Fork | ErrorCode::Gap | ErrorCode::OlderSeq => {
                    pull_and_merge_identity(transport, token, identity, keystore, synced, memory)?;
                }
                _ => return Err(format!("the server refused: {}", refused.message)),
            },
        }
    }
    Err(format!(
        "the identity's records didn't settle after {IDENTITY_ROUNDS} rounds (another device kept writing); try again"
    ))
}

/// Pulls every vault of this profile the server holds and this store does
/// not, so a new device finds the account's vaults instead of making one.
/// Every head taken must cite a tree head that connects to `kt`'s view
/// (D80); a chain that doesn't is refused before anything is written.
fn pull_new_vaults(
    transport: &dyn Transport,
    token: &str,
    identity: &Identity,
    store: &mut DeviceStore,
    cfg: &mut SyncConfig,
    kt: &KtSession<'_>,
) -> Result<usize, String> {
    let profile_id = identity.profile().profile_id;
    let have = lockdown_vault::Store::list_vaults(store).map_err(|e| e.to_string())?;
    let list = transport.list_vaults(token).map_err(|e| e.to_string())?;
    let syncer = Syncer {
        transport,
        token,
        device_label: "",
    };
    let mut pulled = 0;
    for summary in &list.vaults {
        if have.contains(&summary.vault_id) {
            continue;
        }
        let record =
            lockdown_vault::VaultRecord::from_bytes(&summary.record).map_err(|e| e.to_string())?;
        if record.context.profile_id != profile_id {
            continue;
        }
        if let Some(state) = lockdown_sync::bootstrap_vault_with(&syncer, store, summary, Some(kt))
            .map_err(|e| e.to_string())?
        {
            cfg.vaults.insert(summary.vault_id, state);
            pulled += 1;
        }
    }
    Ok(pulled)
}

/// This device's binding for the OPAQUE client, from its keystore.
fn binding_of(keystore: &Rc<FileKeystore>) -> DeviceBinding {
    let signing = keystore.clone();
    DeviceBinding {
        device_id: keystore.device_id(),
        device_pub: keystore.device_pub(),
        sign: Box::new(move |m| {
            DeviceKeystore::sign(signing.as_ref(), m).map_err(lockdown_crypto::Error::from)
        }),
    }
}

/// The printable recovery kit (D69). Shown once; Lockdown keeps no copy.
fn render_kit(kit: &RecoveryKit, account: Option<(&str, &str)>, device: &str) -> String {
    let (login, server) = account.unwrap_or(("(none yet)", "(none yet)"));
    let today = lockdown_sync::engine::civil_date(lockdown_vault::Timestamp::now().0);
    format!(
        "\
LOCKDOWN RECOVERY KIT
=====================
Keep this page somewhere safe, away from your devices. You need it only
when every device is gone, or a brand-new device has no old device nearby.

Secret Key:     {sk}
Recovery code:  {code}

Login name:     {login}
Server:         {server}
Profile:        {profile}
Made:           {today} on {device}

To get back in on a new device, choose \"Recover with the kit\" and give
your password, the Secret Key and the recovery code.
Lockdown never asks for this page in an email, a message or a web form.
",
        sk = kit.secret_key_text(),
        code = kit.recovery_code_text(),
        profile = kit.profile_id(),
    )
}

/// The Secret Key's bytes from its printed form.
fn secret_key_bytes(profile_id: Id, text: &str) -> Result<[u8; 16], String> {
    let kit = RecoveryKit::from_text(profile_id, text, &"0".repeat(32))
        .map_err(|e| format!("the Secret Key isn't readable: {e}"))?;
    Ok(*kit.secret_key().expose())
}

/// The identity's `account` wrap of the root key, if it has one.
fn account_envelope(identity: &Identity) -> Option<&KeyEnvelope> {
    let account_ref = KeyRef::new(KeyRefKind::Account, identity.profile().profile_id);
    identity
        .envelopes()
        .find(|e| e.purpose == Purpose::Irk && e.wrapped_by == account_ref)
}

/// Whether the account slot opens from the login's export key alone (the
/// shape D60 wrote, to be upgraded under D70).
fn slot_opens_without_secret_key(identity: &Identity, export_key: &ExportKey) -> bool {
    let profile_id = identity.profile().profile_id;
    let Some(envelope) = account_envelope(identity) else {
        return false;
    };
    let Ok(auk) =
        ecosystem_identity::kdf::account_unlock_key(export_key.expose(), None, &profile_id)
    else {
        return false;
    };
    let opened: Result<Irk, _> = ecosystem_identity::keys::unwrap_key(
        envelope,
        &auk,
        &ecosystem_identity::keys::aad(profile_id, None, envelope.key_id, Purpose::Irk),
    );
    opened.is_ok()
}

/// An [`AccountAuth`] that hands over one export key already obtained
/// from a login, so enrolling a new device, or rewriting the account slot
/// on one that is enrolled, needs one OPAQUE round, not two.
struct PreparedExportKey(Option<ExportKey>);

impl PreparedExportKey {
    fn take(&mut self) -> ecosystem_identity::Result<ExportKey> {
        self.0.take().ok_or_else(|| {
            ecosystem_identity::Error::Invalid("the export key was already used".into())
        })
    }
}

impl AccountAuth for PreparedExportKey {
    fn register(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        self.take()
    }

    fn login(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        self.take()
    }
}

/// Seconds since the Unix epoch, now.
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// [`DeviceExpectations`] over every vault of the persona: the audit
/// checks each vault's logged heads against the copy this device holds.
struct AllVaults<'a> {
    base: DeviceExpectations<'a>,
    more: Vec<(&'a Vault, u64)>,
}

impl lockdown_sync::AuditExpectations for AllVaults<'_> {
    fn device_key(&self, device_id: Id) -> Option<[u8; 32]> {
        self.base.device_key(device_id)
    }

    fn own_device(&self) -> Id {
        self.base.own_device()
    }

    fn identity_head(&self) -> Option<(u64, lockdown_vault::Hash32)> {
        self.base.identity_head()
    }

    fn vault_head(&self, vault_id: Id, seq: u64) -> Option<lockdown_vault::Hash32> {
        if let Some((vault, synced_seq)) = self
            .more
            .iter()
            .find(|(v, _)| v.context().vault_id == vault_id)
        {
            if seq > *synced_seq {
                return None;
            }
            return vault.head_at(seq).and_then(|h| h.hash().ok());
        }
        self.base.vault_head(vault_id, seq)
    }
}

/// The warning a row carries when a site's Signal hid one of its passkeys
/// (Needs attention, with Undo in the item).
pub const PASSKEY_HIDDEN_WARNING: &str = "passkey-hidden-by-site";

fn parse_id(id: &str) -> Result<Id, String> {
    Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))
}

fn state_name(state: ItemState) -> &'static str {
    match state {
        ItemState::Live => "live",
        ItemState::Archived => "archived",
        ItemState::Trashed => "trashed",
        ItemState::Tombstone => "tombstone",
    }
}

fn text_of(fields: &[PlainField], field_id: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.field_id == field_id)
        .and_then(|f| match &f.value {
            FieldValue::Text(t) => Some(t.clone()),
            _ => None,
        })
}

/// Whether a field's value is a secret a shell gets only on `reveal`: a
/// hidden text, a code's set-up (the seed is in the URI), passkey material.
fn is_secret_kind(kind: FieldKind) -> bool {
    matches!(
        kind,
        FieldKind::Hidden | FieldKind::Totp | FieldKind::Passkey
    )
}

/// An item's fields as a shell may show them: the organization fields
/// leave the list (they come as the item's `organization`), and Lockdown's
/// own reserved fields (the security center's settings) are never shown.
fn shown_fields(fields: &[PlainField], reveal: bool) -> Vec<FieldView> {
    fields
        .iter()
        .filter(|f| {
            !lockdown_vault::is_org_field(&f.field_id) && !lockdown_vault::is_reserved(&f.field_id)
        })
        .map(|f| field_view(f, reveal))
        .collect()
}

/// One field as a shell may show it.
fn field_view(f: &PlainField, reveal: bool) -> FieldView {
    let kind = format!("{:?}", f.kind).to_lowercase();
    let hidden = is_secret_kind(f.kind);
    let (value, mask, secret) = match (&f.value, hidden) {
        (FieldValue::Text(t), true) => (
            None,
            Some("•".repeat(t.chars().count().clamp(8, 16))),
            reveal.then(|| Revealed::from_text(t)),
        ),
        (FieldValue::Text(t), false) => (Some(t.clone()), None, None),
        (FieldValue::Boolean(b), _) => (Some(b.to_string()), None, None),
        (FieldValue::Url { value, .. }, _) => (Some(value.clone()), None, None),
        (FieldValue::Bytes(b), _) => (Some(format!("<{} bytes>", b.len())), None, None),
        (FieldValue::Ref(r), _) => (Some(r.to_string()), None, None),
    };
    FieldView {
        field_id: f.field_id.clone(),
        label: f.label.clone(),
        kind,
        hidden,
        value,
        mask,
        secret,
        match_rule: match &f.value {
            FieldValue::Url { match_rule, .. } => Some(match_rule.clone()),
            _ => None,
        },
        link: None,
    }
}
