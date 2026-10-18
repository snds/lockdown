//! One run of the command-line tool: where things live on this device,
//! the open identity, the store, what the device remembers, and the
//! account on the server when there is one.
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
//! The identity root is the shared, platform-fixed place every ecosystem
//! app reads. The data dir is Lockdown's own: next to the root
//! (`…/snds/lockdown` beside `…/snds/identity`) unless `--data` says
//! otherwise.

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
    HeadMemory, RememberedHead, SqliteStore, SyncedIdentity, TransparencyMemory, write_private,
};
use lockdown_sync::wire::ErrorCode;
use lockdown_sync::{
    AuditExpectations, DeviceExpectations, HttpTransport, KtSession, SyncReport, Syncer, Transport,
    VaultSyncState, pull_identity, push_identity,
};
use lockdown_vault::identity::{
    self, APP, DeviceTrust, OpenVault, default_persona, device_keys, find_persona,
    identity_signing_key, merge_records, open_or_create_identity, open_vault_trusting,
    persona_vaults, record_set,
};
use lockdown_vault::identity_set::{Revocation, VaultBoundary};
use lockdown_vault::transparency::{CHECKPOINT_V, Checkpoint};
use lockdown_vault::{FieldKind, FieldValue, Hash32, Id, ItemState, PlainField, Vault};
use serde::{Deserialize, Serialize};

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

/// Where this run reads and writes.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The identity root.
    pub root: PathBuf,
    /// Lockdown's data directory.
    pub data: PathBuf,
}

impl Paths {
    /// Resolves `--root` and `--data`. With neither, the platform's
    /// identity root (D15) and the `lockdown` directory beside it. With
    /// `--root` alone, the `lockdown` directory beside that root.
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
}

/// The `lockdown` directory next to an identity root.
fn beside(root: &Path) -> PathBuf {
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("lockdown"),
        _ => root.join("lockdown"),
    }
}

/// A name for this device in the identity's device list and on conflict
/// copies: `Lockdown on <hostname>` (D68), or plain `Lockdown` when the
/// hostname can't be read, so a laptop with two ecosystem apps shows as
/// two devices told apart by app.
pub fn device_name() -> String {
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
/// token is a bearer credential: the file is readable only by the user,
/// like the device key beside it.
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
}

impl SyncConfig {
    fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let cfg: Self = lockdown_crypto::cbor::from_slice(&bytes)
                    .map_err(|_| format!("{} didn't decode; log in again", path.display()))?;
                if cfg.v > SYNC_V {
                    return Err(format!(
                        "{} was written by a newer Lockdown; update the app",
                        path.display()
                    ));
                }
                Ok(Some(cfg))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
        }
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = lockdown_crypto::cbor::to_canonical(self).map_err(|e| e.to_string())?;
        write_private(path, &bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))
    }

    fn session_token(&self) -> Result<&str, String> {
        if self.expires_at <= lockdown_vault::Timestamp::now().0 {
            return Err(
                "the session with the server has run out; run `lockdown account login` again"
                    .into(),
            );
        }
        Ok(&self.token)
    }
}

/// What a command needs opened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// `init`: create what is missing.
    Init,
    /// Every local command: open what is there, create nothing.
    Open,
    /// `sync`: open, after syncing the identity's records and pulling a
    /// vault this device has never held.
    Sync,
}

/// The open app: identity, store, memory, the synced identity records,
/// the account, and the one vault in use.
pub struct App {
    paths: Paths,
    keystore: Rc<FileKeystore>,
    identity: Identity,
    store: SqliteStore,
    memory: HeadMemory,
    synced: SyncedIdentity,
    transparency: TransparencyMemory,
    sync: Option<SyncConfig>,
    opened: OpenVault,
    /// Whether this run created the identity.
    pub identity_created: bool,
}

impl App {
    /// `lockdown init`: opens what is there and creates what is not.
    pub fn init(paths: Paths, persona: Option<&str>) -> Result<Self, String> {
        Self::open(paths, persona, Mode::Init)
    }

    /// Every other local command: opens what is there and refuses to create.
    pub fn open_existing(paths: Paths, persona: Option<&str>) -> Result<Self, String> {
        Self::open(paths, persona, Mode::Open)
    }

    /// `lockdown sync`: opens, syncing the identity's records with the
    /// server first and pulling the vault when this device holds none yet.
    pub fn open_for_sync(paths: Paths, persona: Option<&str>) -> Result<Self, String> {
        Self::open(paths, persona, Mode::Sync)
    }

    fn open(paths: Paths, persona: Option<&str>, mode: Mode) -> Result<Self, String> {
        let keystore_path = paths.data.join(KEYSTORE_FILE);
        if mode != Mode::Init && !keystore_path.exists() && !paths.root.join("FORMAT").exists() {
            return Err(format!(
                "nothing is set up at {} yet; run `lockdown init` first",
                paths.root.display()
            ));
        }
        let keystore =
            Rc::new(FileKeystore::open_or_create(&keystore_path).map_err(|e| e.to_string())?);
        let mut memory =
            HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
        let remembered = memory.identity();
        let (identity, identity_created) = if mode == Mode::Init {
            let before = ecosystem_identity::Store::at(&paths.root)
                .probe()
                .map_err(|e| e.to_string())?;
            let identity =
                open_or_create_identity(&paths.root, &device_name(), keystore.as_ref(), remembered)
                    .map_err(|e| e.to_string())?;
            (
                identity,
                before == ecosystem_identity::store::Probe::Missing,
            )
        } else {
            match identity::open_identity(&paths.root, keystore.as_ref(), remembered)
                .map_err(|e| e.to_string())?
            {
                Some(identity) => (identity, false),
                None => {
                    return Err(format!(
                        "no ecosystem identity at {}; run `lockdown init` first, or `lockdown account login` on a new device",
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
        // <hostname>`. The crate has no rename call, so the record is
        // updated through the derived ISK (identity::rename_this_device);
        // best-effort, so a device whose identity another app manages, or
        // a read before the name settled, still works.
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
            SqliteStore::open(&paths.data.join(STORE_FILE)).map_err(|e| e.to_string())?;
        let isk_pub = identity.profile().isk_pub;
        let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), Some(&isk_pub))
            .map_err(|e| e.to_string())?;
        let transparency = TransparencyMemory::open(&paths.data.join(TRANSPARENCY_FILE))
            .map_err(|e| e.to_string())?;
        let mut sync = SyncConfig::load(&paths.sync_file())?;

        // With an account: bring the identity's records in step first (so
        // every device and every revocation is known before a signature
        // is judged), then pull the vault when this device holds none.
        // `init` after `account login` on a new device must find the
        // account's vault, not make a second one.
        if let Some(cfg) = sync.as_mut()
            && matches!(mode, Mode::Sync | Mode::Init)
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
                pull_new_vaults(&transport, &token, &identity, &mut store, cfg)?;
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
                    "this persona has no vault at {} yet; run `lockdown init`",
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
        let mut opened = opened;
        opened
            .vault
            .require_post_quantum_from(memory.pq_since(opened.vault.context().vault_id))
            .map_err(|e| e.to_string())?;
        let mut app = Self {
            paths,
            keystore,
            identity,
            store,
            memory,
            synced,
            transparency,
            sync,
            opened,
            identity_created,
        };
        app.remember()?;
        Ok(app)
    }

    /// Writes the vault and records its newest head.
    fn save(&mut self) -> Result<(), String> {
        self.opened
            .save(&mut self.store)
            .map_err(|e| e.to_string())?;
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
            .map_err(|e| e.to_string())
    }

    /// The vault in use.
    pub fn vault(&self) -> &Vault {
        &self.opened.vault
    }

    /// What this device knows, for the key-transparency audit: the shared
    /// [`DeviceExpectations`] over the identity, the synced records and the
    /// vault in use at the `seq` the server last confirmed.
    fn expectations(&self) -> DeviceExpectations<'_> {
        let vault = self.vault();
        let synced_seq = self
            .sync
            .as_ref()
            .and_then(|c| c.vaults.get(&vault.context().vault_id))
            .map(|s| s.synced_seq)
            .unwrap_or(0);
        DeviceExpectations {
            identity: &self.identity,
            synced: &self.synced,
            vault: Some((vault, synced_seq)),
        }
    }

    /// Whether `init` created the vault this run.
    pub fn vault_created(&self) -> bool {
        self.opened.created
    }

    fn device_binding(&self) -> DeviceBinding {
        let keystore = self.keystore.clone();
        DeviceBinding {
            device_id: self.identity.device_id(),
            device_pub: self.keystore.device_pub(),
            sign: Box::new(move |m| {
                DeviceKeystore::sign(keystore.as_ref(), m).map_err(lockdown_crypto::Error::from)
            }),
        }
    }

    // --- items -------------------------------------------------------------

    /// Adds a login and returns its id. The password arrives here already
    /// read from stdin or the environment, never from an argument.
    pub fn add_login(
        &mut self,
        site: &str,
        username: &str,
        password: &str,
        title: Option<&str>,
    ) -> Result<Id, String> {
        let origin = lockdown_matcher::Origin::parse(site).map_err(|e| e.to_string())?;
        let title = title.map(str::to_string).unwrap_or(origin.host.clone());
        let fields = vec![
            PlainField::text("title", "title", &title),
            PlainField::url("url.0", site, "exact_host"),
            PlainField::text("user", "username", username),
            PlainField::hidden("pw", "password", password),
        ];
        let id = self
            .opened
            .vault
            .create_item(
                lockdown_vault::ItemKind::Login,
                lockdown_vault::ItemClass::Standard,
                fields,
            )
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(id)
    }

    /// Changes a login's title, username or password (whichever are given)
    /// as a new version, and returns the version number.
    pub fn edit_login(
        &mut self,
        id: &str,
        title: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<u64, String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        let mut fields = self.vault().open_item(item_id).map_err(|e| e.to_string())?;
        let mut changed = false;
        for (field_id, value) in [("title", title), ("user", username), ("pw", password)] {
            let Some(value) = value else { continue };
            match fields.iter_mut().find(|f| f.field_id == field_id) {
                Some(f) => f.value = FieldValue::Text(value.to_string()),
                None => fields.push(match field_id {
                    "pw" => PlainField::hidden("pw", "password", value),
                    "user" => PlainField::text("user", "username", value),
                    _ => PlainField::text("title", "title", value),
                }),
            }
            changed = true;
        }
        if !changed {
            return Err("nothing to change: pass --title, --username or --password-stdin".into());
        }
        let n = self
            .opened
            .vault
            .update_item(item_id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(n)
    }

    // --- attachments ---------------------------------------------------------

    /// `lockdown attach <item> <file>`: seals the file under a fresh
    /// Attachment Key as a new version of the item and keeps the blob in
    /// the store. Returns the attachment id, its name and size.
    pub fn attach_file(&mut self, id: &str, file: &Path) -> Result<(Id, String, u64), String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        let bytes =
            std::fs::read(file).map_err(|e| format!("couldn't read {}: {e}", file.display()))?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("{} has no file name", file.display()))?;
        let mime = mime_for(&name);
        let (att_id, _) = self
            .opened
            .vault
            .attach(item_id, &name, mime, &bytes)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok((att_id, name, bytes.len() as u64))
    }

    /// `lockdown attachments <item>`: one line per attachment of the
    /// item's newest version.
    pub fn attachment_lines(&self, id: &str) -> Result<Vec<String>, String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        let listed = self
            .vault()
            .attachments(item_id)
            .map_err(|e| e.to_string())?;
        if listed.is_empty() {
            return Ok(vec!["no attachments".into()]);
        }
        Ok(listed
            .into_iter()
            .map(|a| match a.meta {
                Some(m) => format!(
                    "{}  {}  {} bytes  {}  {} chunk{}  blob {}",
                    a.att_id,
                    m.name,
                    m.size,
                    m.mime,
                    a.chunks,
                    if a.chunks == 1 { "" } else { "s" },
                    &a.blob.to_hex()[..16]
                ),
                None => format!("{}  (shredded)  blob {}", a.att_id, &a.blob.to_hex()[..16]),
            })
            .collect())
    }

    /// `lockdown fetch <item> <name> --out FILE`: opens the attachment
    /// named `name` (or by attachment id) from the local blob, fetching it
    /// from the server first when this device doesn't hold it, and writes
    /// it to `out`, readable only by the user and never over a file that
    /// exists.
    pub fn fetch_attachment(
        &mut self,
        id: &str,
        name: &str,
        out: &Path,
    ) -> Result<Vec<String>, String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        if out.exists() {
            return Err(format!(
                "{} exists already; choose another file",
                out.display()
            ));
        }
        let listed = self
            .vault()
            .attachments(item_id)
            .map_err(|e| e.to_string())?;
        let by_id = Id::parse(name).ok();
        let found: Vec<_> = listed
            .iter()
            .filter(|a| Some(a.att_id) == by_id || a.meta.as_ref().is_some_and(|m| m.name == name))
            .collect();
        let att = match found.as_slice() {
            [one] => (*one).clone(),
            [] => {
                return Err(format!(
                    "no attachment '{name}' on item {item_id}; `lockdown attachments {item_id}` lists them"
                ));
            }
            _ => {
                return Err(format!(
                    "'{name}' names more than one attachment; use the id"
                ));
            }
        };
        if att.meta.is_none() {
            return Err(format!(
                "attachment {} was shredded; its contents are gone for good",
                att.att_id
            ));
        }
        let blob = match lockdown_vault::Store::get_blob(&self.store, att.blob) {
            Ok(bytes) => bytes,
            Err(lockdown_store::StoreError::NotFound(_)) => {
                let Some(cfg) = self.sync.as_ref() else {
                    return Err(format!(
                        "this device doesn't hold the blob of attachment {} and has no account to fetch it from",
                        att.att_id
                    ));
                };
                let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
                let token = cfg.session_token()?.to_string();
                let bytes = transport
                    .get_blob(&token, self.vault().context().vault_id, att.blob)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| {
                        format!(
                            "the server doesn't hold the blob of attachment {} either",
                            att.att_id
                        )
                    })?;
                if lockdown_crypto::hash::hash(&bytes) != att.blob {
                    return Err(
                        "the server's copy of the blob doesn't match its name; it was not kept"
                            .into(),
                    );
                }
                lockdown_vault::Store::put_blob(&mut self.store, att.blob, &bytes)
                    .map_err(|e| e.to_string())?;
                bytes
            }
            Err(e) => return Err(e.to_string()),
        };
        let (meta, plain) = self
            .vault()
            .open_attachment(item_id, att.att_id, &blob)
            .map_err(|e| e.to_string())?;
        write_private(out, &plain).map_err(|e| format!("couldn't write {}: {e}", out.display()))?;
        Ok(vec![format!(
            "wrote {} ({} bytes, {}) to {} (readable only by you)",
            meta.name,
            meta.size,
            meta.mime,
            out.display()
        )])
    }

    /// One line per live, visible item: id, kind, title, site.
    pub fn list_lines(&self) -> Result<Vec<String>, String> {
        let vault = self.vault();
        let mut lines = Vec::new();
        for item in vault.items() {
            if item.kind.hidden() || !matches!(item.state, ItemState::Live | ItemState::Archived) {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let title = text_of(&fields, "title").unwrap_or_default();
            let site = fields
                .iter()
                .find(|f| f.kind == FieldKind::Url)
                .and_then(|f| match &f.value {
                    FieldValue::Url { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            lines.push(format!(
                "{}  {:<8}  {}  {}",
                item.item_id,
                item.kind.as_str(),
                title,
                site
            ));
        }
        Ok(lines)
    }

    /// The live items matching `query`, best first, through the same
    /// quick-access index the desktop, the web vault and the extension ask
    /// (`lockdown_vault::search`): title, website host, username and kind
    /// only, a crown jewel by title and kind; never a secret field.
    pub fn search_lines(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        use lockdown_vault::search::{Doc, Field, Index};
        let vault = self.vault();
        let mut rows = Vec::new();
        for item in vault.items() {
            if item.kind.hidden() || item.state != ItemState::Live {
                continue;
            }
            let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
            let title = text_of(&fields, "title").unwrap_or_default();
            let jewel = item.class == lockdown_vault::ItemClass::CrownJewel;
            let site = fields
                .iter()
                .find(|f| f.kind == FieldKind::Url)
                .and_then(|f| match &f.value {
                    FieldValue::Url { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .filter(|_| !jewel)
                .unwrap_or_default();
            let user = text_of(&fields, "user")
                .filter(|_| !jewel)
                .unwrap_or_default();
            rows.push((item.item_id, item.kind, title, site, user));
        }
        rows.sort_by(|a, b| {
            a.2.to_lowercase()
                .cmp(&b.2.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        let index = Index::build(rows.into_iter().map(|(id, kind, title, site, user)| {
            let host = lockdown_matcher::Origin::parse(&site)
                .map(|o| o.host)
                .unwrap_or_default();
            let template = lockdown_vault::template_for(kind);
            let line = format!(
                "{id}  {:<8}  {title}  {}",
                kind.as_str(),
                if user.is_empty() {
                    host.clone()
                } else {
                    format!("{user} · {host}")
                }
            );
            Doc::new(line, 0)
                .with(Field::Title, &title)
                .with(Field::Host, &host)
                .with(Field::Username, &user)
                .with(Field::Kind, kind.as_str())
                .with(Field::Kind, template.name)
        }));
        Ok(index
            .search(query, limit)
            .into_iter()
            .map(|h| {
                if h.exact {
                    h.key.clone()
                } else {
                    format!("{}  (close match)", h.key)
                }
            })
            .collect())
    }

    /// The fields of one item, hidden values masked unless `reveal`.
    pub fn show_lines(&self, id: &str, reveal: bool) -> Result<Vec<String>, String> {
        let item_id = Id::parse(id).map_err(|_| format!("'{id}' isn't an item id"))?;
        let vault = self.vault();
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        let latest = item.latest().map_err(|e| e.to_string())?;
        let fields = vault.open_item(item_id).map_err(|e| e.to_string())?;
        let mut lines = vec![
            format!("id:       {item_id}"),
            format!("kind:     {}", item.kind.as_str()),
            format!("state:    {:?}", item.state),
            format!("version:  {} (edited {})", latest.n, latest.edited_at.0),
        ];
        for f in &fields {
            let value = match (&f.value, f.kind) {
                (FieldValue::Text(t), FieldKind::Hidden) if !reveal => {
                    "•".repeat(t.chars().count().clamp(8, 16))
                }
                (FieldValue::Text(t), _) => t.clone(),
                (FieldValue::Boolean(b), _) => b.to_string(),
                (FieldValue::Url { value, match_rule }, _) => format!("{value} ({match_rule})"),
                (FieldValue::Bytes(b), _) => format!("<{} bytes>", b.len()),
                (FieldValue::Ref(r), _) => r.to_string(),
            };
            lines.push(format!("{:<9} {value}", format!("{}:", f.label)));
        }
        Ok(lines)
    }

    /// `lockdown status`.
    pub fn status_lines(&self) -> Result<Vec<String>, String> {
        let profile = self.identity.profile();
        let device_id = self.identity.device_id();
        let device = self
            .identity
            .devices()
            .find(|d| d.device_id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "this device".into());
        let persona_id = self.vault().context().persona_id;
        let persona = self
            .identity
            .personas()
            .find(|p| p.persona_id == persona_id)
            .map(|p| format!("{} ({:?})", p.label, p.kind))
            .unwrap_or_else(|| "unknown".into());
        let head = self.vault().head().map_err(|e| e.to_string())?;
        let hash = head.hash().map_err(|e| e.to_string())?;
        let live = self
            .vault()
            .items()
            .filter(|i| i.state == ItemState::Live && !i.kind.hidden())
            .count();
        let vault_id = self.vault().context().vault_id;
        let mut lines = vec![
            format!("identity root  {}", self.paths.root.display()),
            format!("data dir       {}", self.paths.data.display()),
            format!(
                "profile        {} (managed by {}, recovery kit: {:?})",
                profile.profile_id,
                profile.manager.as_deref().unwrap_or("no app"),
                profile.recovery
            ),
            format!("device         {device} ({device_id})"),
            format!("persona        {persona} {persona_id}"),
            format!("vault          {vault_id}"),
            format!("vault head     seq {} hash {}", head.seq, hash.to_hex()),
            format!("items          {live} live"),
            format!(
                "devices        {} in the identity ({} revoked); identity manifest seq {}",
                self.identity.devices().count(),
                self.identity
                    .devices()
                    .filter(|d| d.revoked_at.is_some())
                    .count(),
                self.synced.seq()
            ),
        ];
        match &self.sync {
            Some(cfg) => {
                let state = cfg.vaults.get(&vault_id).copied().unwrap_or_default();
                lines.push(format!("account        {} at {}", cfg.login, cfg.server));
                lines.push(format!(
                    "synced         seq {} (item revision {})",
                    state.synced_seq, state.synced_rev
                ));
                match self.transparency.view(&cfg.server) {
                    Some(view) => lines.push(format!(
                        "transparency   log of {} entries, root {}, audited through {}",
                        view.sth.size,
                        &view.sth.root.to_hex()[..16],
                        view.audited
                    )),
                    None => {
                        lines.push("transparency   not checked yet (run `lockdown sync`)".into())
                    }
                }
            }
            None => lines.push("account        none (run `lockdown account register`)".into()),
        }
        Ok(lines)
    }

    // --- the account --------------------------------------------------------

    /// `lockdown account register`: makes the account on `server` under
    /// `login` with OPAQUE, writes the identity's account unlock slot with
    /// the recovery kit's Secret Key mixed in (04-security §3; D70),
    /// publishes the identity's records, and keeps the session. When the
    /// identity has no kit yet one is made here and returned as text, to
    /// be shown or saved once; when it has one, its Secret Key must be
    /// given (`secret_key`).
    pub fn account_register(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
        let profile_id = self.identity.profile().profile_id;
        let (sk, kit_text) = if self.identity.profile().recovery == RecoveryState::None {
            if secret_key.is_some() {
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
            let Some(text) = secret_key else {
                return Err(
                    "this identity already has a recovery kit; give its Secret Key with --secret-key-stdin or LOCKDOWN_SECRET_KEY so the account is protected by it"
                        .into(),
                );
            };
            (secret_key_bytes(profile_id, text)?, None)
        };
        // Registers the password with the server and writes the `account`
        // wrap of the root key under the Account Unlock Key: HKDF over the
        // OPAQUE export key and the Secret Key (04-security §3 step 4).
        self.identity
            .enable_account_unlock(&mut auth, password, Some(&sk))
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
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
        lines.push("session kept; run `lockdown sync` to push the vault".into());
        Ok((lines, kit_text))
    }

    /// `lockdown account login` on a device that already holds the
    /// identity: a fresh session, and the identity's records brought in
    /// step with the server. An account slot written without a Secret Key
    /// (D60) is upgraded here when the kit exists and its Secret Key is
    /// given; otherwise the person is told what is still open (D70).
    pub fn account_login(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
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
        if slot_opens_without_secret_key(&self.identity, &export_key) {
            match (self.identity.profile().recovery, secret_key) {
                (RecoveryState::None, _) => lines.push(
                    "notice: this account can still be opened with the password alone; run `lockdown kit create` to make the recovery kit and protect the account with its Secret Key"
                        .into(),
                ),
                (_, None) => lines.push(
                    "notice: this account can still be opened with the password alone; run `lockdown account login --secret-key-stdin` with the Secret Key from your recovery kit to finish protecting it"
                        .into(),
                ),
                (_, Some(text)) => {
                    let sk = secret_key_bytes(self.identity.profile().profile_id, text)?;
                    self.upgrade_account_slot(export_key, password, &sk)?;
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
        let vaults = match &self.sync {
            Some(old) if old.server == auth.rounds().base() && old.login == login => {
                old.vaults.clone()
            }
            _ => BTreeMap::new(),
        };
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults,
        };
        cfg.save(&self.paths.sync_file())?;
        self.sync = Some(cfg);
        Ok(lines)
    }

    /// Rewrites the account slot with the Secret Key mixed in (D70): the
    /// old `account` envelope is retired, which the identity store records
    /// as a tombstone (D29), and a new one is written under the Account
    /// Unlock Key over the same OPAQUE registration: `export_key` is the
    /// one the login just produced, so no second round trip and no second
    /// registration.
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

    /// `lockdown kit create`: makes the recovery kit (04-security §8) and
    /// returns its printable text, shown or saved once and never kept by
    /// Lockdown. With an account, the account password is needed: the
    /// slot is rewritten with the new Secret Key (D70). A kit that exists
    /// is replaced only with `replace`, and the old page stops working.
    pub fn kit_create(
        &mut self,
        replace: bool,
        password: Option<&str>,
        check_target: impl FnOnce() -> Result<(), String>,
    ) -> Result<(String, Vec<String>), String> {
        // Whatever the kit will be delivered to must be free *before* the
        // account slot is rewritten: a kit that is made but not delivered
        // would leave the slot on a Secret Key no page carries.
        check_target()?;
        if self.identity.profile().recovery != RecoveryState::None && !replace {
            return Err(
                "this identity already has a recovery kit; `lockdown kit verify` checks it, `lockdown kit create --replace` makes a new one (the old page stops working)"
                    .into(),
            );
        }
        let mut auth = match &self.sync {
            Some(cfg) => {
                let Some(password) = password else {
                    return Err(
                        "this device has an account, so the new Secret Key must go into its slot: pass the account password with --password-stdin or LOCKDOWN_PASSWORD"
                            .into(),
                    );
                };
                let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
                let mut auth = OpaqueAccountAuth::new(transport, &cfg.login, self.device_binding());
                // The password is checked before anything changes; the
                // export key from this one login rewrites the slot.
                let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
                Some((auth, session, export_key))
            }
            None => None,
        };
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
            let auth = auth;
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
            notes.push(
                "no account on this device yet; `lockdown account register` will ask for this Secret Key"
                    .into(),
            );
        }
        let text = render_kit(
            &kit,
            login.as_ref().map(|(l, s)| (l.as_str(), s.as_str())),
            &device_name(),
        );
        Ok((text, notes))
    }

    /// `lockdown kit verify`: types the kit back in and checks it opens
    /// this identity, changing nothing (04-security §9). The recovery code
    /// is checked against the kit wrap of the root key; the Secret Key
    /// against the account slot when the account password is given too.
    /// With `drill`, a passing check is recorded on the profile.
    pub fn kit_verify(
        &mut self,
        secret_key: &str,
        recovery_code: &str,
        password: Option<&str>,
        drill: bool,
    ) -> Result<Vec<String>, String> {
        let profile = self.identity.profile().clone();
        if profile.recovery == RecoveryState::None {
            return Err("this identity has no recovery kit yet; run `lockdown kit create`".into());
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

    /// `lockdown recover` on a device with no identity (01-identity §8 row
    /// 3): logs in with the password (for the session only), pulls the
    /// identity's record set, rebuilds the root key from the kit, enrols
    /// this device, publishes the records that list it, and pulls the
    /// vault. A wrong kit changes nothing here and tells the server
    /// nothing: the only round trip is the login.
    pub fn recover(
        paths: Paths,
        server: &str,
        login: &str,
        password: &str,
        secret_key: &str,
        recovery_code: &str,
    ) -> Result<Vec<String>, String> {
        Self::check_root_empty(&paths)?;
        let keystore = Rc::new(
            FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE))
                .map_err(|e| e.to_string())?,
        );
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, binding_of(&keystore));
        let (_export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), None)
            .map_err(|e| e.to_string())?;
        pull_identity(auth.rounds(), &session.token, &mut synced, None)
            .map_err(|e| e.to_string())?;
        let verified = synced.verified().cloned().ok_or_else(|| {
            format!("the account '{login}' has no identity records on {server} yet")
        })?;
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
        let mut memory =
            HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
        memory
            .remember_identity(identity.head())
            .map_err(|e| e.to_string())?;
        sync_identity_records(
            auth.rounds(),
            &session.token,
            &mut identity,
            keystore.as_ref(),
            &mut synced,
            &mut memory,
        )?;
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults: BTreeMap::new(),
        };
        cfg.save(&paths.sync_file())?;
        drop(identity);
        // The vault: pulled the way `init` does on a device with an account.
        let app = Self::init(paths.clone(), None)?;
        let mut lines = vec![format!(
            "recovered: this device ({}) is enrolled in the identity at {} through the recovery kit; logged in as {login} at {server}",
            device_name(),
            paths.root.display()
        )];
        lines.push(format!(
            "vault {}: {} ({} live items)",
            app.vault().context().vault_id,
            if app.vault_created() {
                "created (the account had none)"
            } else {
                "pulled"
            },
            app.vault()
                .items()
                .filter(|i| i.state == ItemState::Live && !i.kind.hidden())
                .count()
        ));
        Ok(lines)
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

    /// `lockdown account login` on a device with no identity: logs in
    /// with the password, pulls the identity's record set, opens the root
    /// key through the account slot with the kit's Secret Key (04-security
    /// §3) and enrols this device, then publishes the record set that
    /// lists it. The vault arrives with the first `lockdown sync`.
    pub fn enrol_from_account(
        paths: Paths,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> Result<Vec<String>, String> {
        Self::check_root_empty(&paths)?;
        let keystore = Rc::new(
            FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE))
                .map_err(|e| e.to_string())?,
        );
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, binding_of(&keystore));
        let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), None)
            .map_err(|e| e.to_string())?;
        pull_identity(auth.rounds(), &session.token, &mut synced, None)
            .map_err(|e| e.to_string())?;
        let verified = synced.verified().cloned().ok_or_else(|| {
            format!("the account '{login}' has no identity records on {server} yet")
        })?;
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
                password,
                secret_key: sk.as_ref(),
            },
            &backup,
        )
        .map_err(|e| match (e, sk.is_some()) {
            (ecosystem_identity::Error::WrongKey, false) => {
                "this account's slot needs the Secret Key from the recovery kit beside the password: pass it with --secret-key-stdin or LOCKDOWN_SECRET_KEY (or use `lockdown recover` with the whole kit)"
                    .to_string()
            }
            (ecosystem_identity::Error::WrongKey, true) => {
                "the password and Secret Key didn't open the account slot: check the Secret Key against the page"
                    .to_string()
            }
            (other, _) => other.to_string(),
        })?;
        let mut memory =
            HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
        memory
            .remember_identity(identity.head())
            .map_err(|e| e.to_string())?;
        // The record set on the server now lists this device too.
        sync_identity_records(
            auth.rounds(),
            &session.token,
            &mut identity,
            keystore.as_ref(),
            &mut synced,
            &mut memory,
        )?;
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults: BTreeMap::new(),
        };
        cfg.save(&paths.sync_file())?;
        Ok(vec![
            format!(
                "logged in as {login} at {server}; this device ({}) is now enrolled in the identity at {}",
                device_name(),
                paths.root.display()
            ),
            "run `lockdown sync` to pull the vault".into(),
        ])
    }

    /// `lockdown device list`: every device of the identity, this one and
    /// revoked ones marked.
    pub fn device_lines(&self) -> Result<Vec<String>, String> {
        let mut devices: Vec<_> = self.identity.devices().collect();
        devices.sort_by_key(|d| (d.enrolled_at, d.device_id));
        let mine = self.identity.device_id();
        Ok(devices
            .into_iter()
            .map(|d| {
                let mut flags = Vec::new();
                if d.device_id == mine {
                    flags.push("this device".to_string());
                }
                if let Some(at) = d.revoked_at {
                    flags.push(format!(
                        "revoked {}",
                        lockdown_sync::engine::civil_date(at.0)
                    ));
                }
                format!(
                    "{}  {:<10}  {}  enrolled {}{}",
                    d.device_id,
                    format!("{:?}", d.platform).to_lowercase(),
                    d.name,
                    lockdown_sync::engine::civil_date(d.enrolled_at.0),
                    if flags.is_empty() {
                        String::new()
                    } else {
                        format!("  [{}]", flags.join(", "))
                    }
                )
            })
            .collect())
    }

    /// `lockdown device revoke <id>`: revokes another device of this
    /// identity (manager only, D73). The device's record is marked revoked
    /// and signed, the identity manifest that carries it names the
    /// server's current head of every vault as the point after which
    /// nothing that device signs is taken, and the server and every other
    /// device refuse its writes from then on. Needs the account: the order
    /// between devices is the server's to fix.
    pub fn device_revoke(&mut self, selector: &str) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("revoking a device needs the account on this device; run `lockdown account login` first".into());
        };
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        // Be current first, so the device to revoke is known here.
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
                            "  vault {}: its heads after seq {} are refused",
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
            ([], []) => Err(format!(
                "no device '{selector}' in this identity; `lockdown device list` shows them"
            )),
            _ => Err(format!(
                "'{selector}' names more than one device; use the full id"
            )),
        }
    }

    /// `lockdown sync`: key transparency first (04-security §6: the
    /// server's tree head must continue the one this device saw, and the
    /// account's own log entries must be what this device knows), then
    /// pull, merge, push, head (03-architecture §6) with the verified tree
    /// head cited in the new head and every pulled head's citation checked
    /// against it, then the account's entries once more for what the sync
    /// added. A failed proof or a split view stops everything before a
    /// byte is written.
    pub fn sync(&mut self) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("no account on this device; run `lockdown account register` or `lockdown account login` first".into());
        };
        let server = cfg.server.clone();
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let remembered = self.transparency.view(&server).cloned();
        let mut kt =
            KtSession::begin(&transport, &token, remembered.as_ref()).map_err(|e| e.to_string())?;
        kt.audit(&self.expectations()).map_err(|e| e.to_string())?;
        let label = device_name();
        let syncer = Syncer {
            transport: &transport,
            token: &token,
            device_label: &label,
        };
        let vault_id = self.opened.vault.context().vault_id;
        let record = self
            .opened
            .vault
            .record(&self.opened.keys.app_data_key)
            .and_then(|r| r.to_bytes())
            .map_err(|e| e.to_string())?;
        let state = self
            .sync
            .as_mut()
            .expect("checked above")
            .vaults
            .entry(vault_id)
            .or_default();
        let outcome = lockdown_sync::sync_vault_with(
            &syncer,
            &mut self.opened.vault,
            &record,
            state,
            &mut self.store,
            Some(&kt),
        );
        // Whatever happened on the wire, what is in memory is saved and
        // remembered, so a half-finished round is picked up next time.
        self.opened
            .save(&mut self.store)
            .map_err(|e| e.to_string())?;
        let report = match outcome {
            Ok(report) => report,
            Err(e) => {
                let _ = self.remember();
                if let Some(cfg) = &self.sync {
                    cfg.save(&self.paths.sync_file())?;
                }
                return Err(e.to_string());
            }
        };
        let head = self.remembered_head()?;
        if report.chain_rewritten {
            self.memory
                .replace_vault(vault_id, head)
                .map_err(|e| e.to_string())?;
        } else {
            self.remember()?;
        }
        // The first post-quantum head the server confirmed: from there on
        // every head must carry one (docs/plan/28-pq-signatures.md).
        if let Some(since) = self.opened.vault.post_quantum_since(report.head_seq) {
            self.memory
                .remember_pq_since(vault_id, since)
                .map_err(|e| e.to_string())?;
            self.opened
                .vault
                .require_post_quantum_from(self.memory.pq_since(vault_id))
                .map_err(|e| e.to_string())?;
        }
        if let Some(cfg) = &self.sync {
            cfg.save(&self.paths.sync_file())?;
        }
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
        let mut lines = sync_lines(&report);
        lines.push(format!(
            "transparency: log of {} entries verified (root {}), your entries audited through {}",
            view.sth.size,
            &view.sth.root.to_hex()[..16],
            view.audited
        ));
        Ok(lines)
    }

    // --- key transparency ---------------------------------------------------

    /// `lockdown audit`: brings the identity's records in step, then prints
    /// and verifies every log entry of the account from the start, each
    /// under an inclusion proof against a tree head that must continue the
    /// one this device remembers. Nothing in the vault changes.
    pub fn audit(&mut self) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("no account on this device; run `lockdown account register` or `lockdown account login` first".into());
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
        // From the start, so the whole account history is printed.
        kt.view.audited = 0;
        kt.view.identity_logged = None;
        kt.view.own_device_logged = false;
        let sth = kt.sth().clone();
        let mut lines = vec![
            format!("server          {server}"),
            format!("log key         {}", Hash32(kt.view.log_key).to_hex()),
            format!(
                "tree head       {} entries, root {}, signed {}",
                sth.size,
                sth.root.to_hex(),
                lockdown_sync::engine::civil_date(sth.time)
            ),
            match &remembered {
                Some(r) => format!(
                    "remembered      {} entries, root {}: the log continues it",
                    r.sth.size,
                    &r.sth.root.to_hex()[..16]
                ),
                None => "remembered      nothing yet; this tree head is the first this device pins"
                    .into(),
            },
            String::new(),
        ];
        let report = kt.audit(&self.expectations()).map_err(|e| {
            format!(
                "{e}
(the entries before it were fine; nothing was changed)"
            )
        })?;
        for e in &report.checked {
            lines.push(format!(
                "#{:<6} {}  {}  verified",
                e.index,
                lockdown_sync::engine::civil_date(e.leaf.time),
                e.leaf.describe()
            ));
        }
        lines.push(String::new());
        lines.push(format!(
            "{} entries of this account verified under the tree head; every device key is one your identity lists, your identity manifest and this vault's heads are the ones this device holds{}",
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

    /// `lockdown checkpoint`: verifies the server's current tree head and
    /// exports it with the account's newest heads as this device holds
    /// them, signed by this device, as one text page a person can keep or
    /// hand to another device.
    pub fn checkpoint_export(&mut self) -> Result<(Checkpoint, String), String> {
        let Some(cfg) = self.sync.as_ref() else {
            return Err("no account on this device; run `lockdown account register` or `lockdown account login` first".into());
        };
        let server = cfg.server.clone();
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let remembered = self.transparency.view(&server).cloned();
        let kt =
            KtSession::begin(&transport, &token, remembered.as_ref()).map_err(|e| e.to_string())?;
        let view = kt.view.clone();
        drop(kt);
        let vault = self.vault();
        let head = vault.head().map_err(|e| e.to_string())?;
        // Post-quantum parts ride only once every device reads them (the
        // rollout rule): an older build re-encoding them would break its
        // own check of the device signature.
        let post_quantum =
            lockdown_vault::pq::heads_ready(&self.identity).map_err(|e| e.to_string())?;
        let mut sth = view.sth.clone();
        if !post_quantum {
            sth.pq_sig = None;
        }
        let mut cp = Checkpoint {
            v: CHECKPOINT_V,
            server: server.clone(),
            log_key: view.log_key,
            sth,
            identity_head: self
                .synced
                .manifest()
                .and_then(|m| m.hash().ok().map(|h| (m.seq, h))),
            vault_heads: vec![(
                vault.context().vault_id,
                head.seq,
                head.hash().map_err(|e| e.to_string())?,
            )],
            exported_at: lockdown_vault::Timestamp::now().0,
            device_id: self.identity.device_id(),
            pq_log_key: view.pq_log_key.clone().filter(|_| post_quantum),
            sig: Vec::new(),
            pq_sig: None,
        };
        let signer: Box<dyn lockdown_vault::Signer> = Box::new(self.keystore.clone());
        cp.sign(signer.as_ref()).map_err(|e| e.to_string())?;
        if post_quantum {
            let pq = lockdown_vault::pq::identity_pq_key(&self.identity, self.keystore.as_ref())
                .map_err(|e| e.to_string())?;
            cp.sign_post_quantum(&pq).map_err(|e| e.to_string())?;
        }
        self.transparency
            .remember(&server, view)
            .map_err(|e| e.to_string())?;
        let text = render_checkpoint(&cp, &device_name())?;
        Ok((cp, text))
    }

    /// `lockdown checkpoint --verify FILE`: reads a checkpoint another
    /// device exported and checks it against this device's view: same
    /// server and log key, the tree head signed by that key and provably a
    /// prefix (or continuation) of the one this device verifies now, the
    /// exporting device one the identity lists and its signature good,
    /// and every head it names the one this device holds at that `seq`.
    pub fn checkpoint_verify(&mut self, text: &str) -> Result<Vec<String>, String> {
        let cp = parse_checkpoint(text)?;
        let Some(cfg) = self.sync.as_ref() else {
            return Err("no account on this device; run `lockdown account login` first".into());
        };
        let server = cfg.server.clone();
        let mut lines = Vec::new();
        if cp.server.trim_end_matches('/') != server.trim_end_matches('/') {
            return Err(format!(
                "the checkpoint is for {} but this device uses {server}; the two can't be compared",
                cp.server
            ));
        }
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let remembered = self.transparency.view(&server).cloned();
        let kt =
            KtSession::begin(&transport, &token, remembered.as_ref()).map_err(|e| e.to_string())?;
        if cp.log_key != kt.view.log_key {
            return Err(
                "the checkpoint names a different log key from the one this device pinned; one of the two devices was shown another server"
                    .into(),
            );
        }
        if cp.pq_log_key.is_some() && cp.pq_log_key != kt.view.pq_log_key {
            return Err(
                "the checkpoint names a different post-quantum log key from the one this device pinned; one of the two devices was shown another server"
                    .into(),
            );
        }
        let key =
            lockdown_crypto::VerifyingKey::from_bytes(&cp.log_key).map_err(|e| e.to_string())?;
        let pq_log_key = cp
            .pq_log_key
            .as_ref()
            .map(|k| lockdown_crypto::pq_sign::PqVerifyingKey::from_bytes(k))
            .transpose()
            .map_err(|e| e.to_string())?;
        cp.sth
            .verify_hybrid(&key, pq_log_key.as_ref())
            .map_err(|e| e.to_string())?;
        lines.push(format!(
            "tree head       {} entries, root {}: signed by the server's log key",
            cp.sth.size,
            &cp.sth.root.to_hex()[..16]
        ));
        kt.verify_citation(cp.sth.as_ref())
            .map_err(|e| e.to_string())?;
        lines.push(format!(
            "consistency     connects to this device's view of {} entries: one log, no fork",
            kt.view.sth.size
        ));
        let device_key = self
            .expectations()
            .device_key(cp.device_id)
            .ok_or_else(|| {
                format!(
                    "the checkpoint was exported by device {}, which this identity doesn't list",
                    cp.device_id
                )
            })?;
        let identity_pq = lockdown_vault::pq::published_key(self.identity.profile())
            .map_err(|e| e.to_string())?;
        cp.verify_hybrid(
            &lockdown_crypto::VerifyingKey::from_bytes(&device_key).map_err(|e| e.to_string())?,
            identity_pq.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        let name = self
            .identity
            .devices()
            .find(|d| d.device_id == cp.device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| cp.device_id.to_string());
        lines.push(format!(
            "exported by     {name} on {}: signature good",
            lockdown_sync::engine::civil_date(cp.exported_at)
        ));
        if let (Some((seq, hash)), Some(mine)) = (cp.identity_head, self.synced.manifest()) {
            if mine.seq == seq {
                if mine.hash().ok() != Some(hash) {
                    return Err(format!(
                        "the checkpoint's identity manifest {seq} isn't the one this device holds at that seq; the two devices were shown different identities"
                    ));
                }
                lines.push(format!("identity        manifest {seq}: the same as here"));
            } else {
                lines.push(format!(
                    "identity        manifest {seq} there, {} here (the other device synced at another time)",
                    mine.seq
                ));
            }
        }
        for (vault_id, seq, hash) in &cp.vault_heads {
            match self.expectations().vault_head(*vault_id, *seq) {
                Some(mine) if mine != *hash => {
                    return Err(format!(
                        "the checkpoint's head {seq} of vault {vault_id} isn't the one this device holds; the two devices were shown different vault histories"
                    ));
                }
                Some(_) => lines.push(format!("vault {vault_id}  head {seq}: the same as here")),
                None => lines.push(format!(
                    "vault {vault_id}  head {seq}: not held here (another vault, or not pulled yet)"
                )),
            }
        }
        lines.push(format!(
            "fingerprint     {}",
            cp.fingerprint().map_err(|e| e.to_string())?.to_hex()
        ));
        lines.push("the checkpoint agrees with this device".into());
        let view = kt.view.clone();
        drop(kt);
        self.transparency
            .remember(&server, view)
            .map_err(|e| e.to_string())?;
        Ok(lines)
    }
}

/// The checkpoint as one text page (D81): the facts a person can read and
/// the signed bytes a device can check, in lowercase hex.
pub fn render_checkpoint(cp: &Checkpoint, device: &str) -> Result<String, String> {
    let mut page = format!(
        "LOCKDOWN CHECKPOINT
===================
What the server's public log said when this device checked it, and the
newest heads this device holds. Compare the fingerprint with another
device's, or give this page to `lockdown checkpoint --verify FILE` there.

Server:          {server}
Log key:         {log_key}
Tree head:       {size} entries
Tree root:       {root}
Signed at:       {signed}
",
        server = cp.server,
        log_key = Hash32(cp.log_key).to_hex(),
        size = cp.sth.size,
        root = cp.sth.root.to_hex(),
        signed = lockdown_sync::engine::civil_date(cp.sth.time),
    );
    match cp.identity_head {
        Some((seq, hash)) => page.push_str(&format!(
            "Identity head:   manifest {seq}, {}
",
            hash.to_hex()
        )),
        None => page.push_str(
            "Identity head:   none synced yet
",
        ),
    }
    for (vault_id, seq, hash) in &cp.vault_heads {
        page.push_str(&format!(
            "Vault {vault_id}:
                 head {seq}, {}
",
            hash.to_hex()
        ));
    }
    page.push_str(&format!(
        "Exported:        {} on {device} ({})
Fingerprint:     {}

Checkpoint:      {}
",
        lockdown_sync::engine::civil_date(cp.exported_at),
        cp.device_id,
        cp.fingerprint().map_err(|e| e.to_string())?.to_hex(),
        hex(&cp.to_bytes().map_err(|e| e.to_string())?),
    ));
    Ok(page)
}

/// The checkpoint back from its page: the `Checkpoint:` line holds the
/// signed bytes; everything else is for the person.
pub fn parse_checkpoint(text: &str) -> Result<Checkpoint, String> {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("Checkpoint:"))
        .ok_or("this isn't a Lockdown checkpoint page: no `Checkpoint:` line")?;
    let bytes = unhex(line.trim()).ok_or("the checkpoint's bytes aren't readable")?;
    Checkpoint::from_bytes(&bytes).map_err(|e| format!("the checkpoint didn't decode: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

/// What `sync` prints.
pub fn sync_lines(report: &SyncReport) -> Vec<String> {
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
    lines
}

/// Brings the identity's records in step with the server (D71): pulls
/// every identity manifest after the one this device accepted, merges the
/// records into the local identity store, then publishes this device's
/// records when they differ from what the server holds. A device that
/// learns it was revoked stops here.
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
/// `new_revocations`. When another device moved first, pulls, merges and
/// tries again, a few times. Returns whether anything was published.
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
    // The post-quantum rollout, as the app runs it (D248, D318; doc 28):
    // this device says what it reads, and the key goes on the profile once
    // every device does.
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
fn pull_new_vaults(
    transport: &dyn Transport,
    token: &str,
    identity: &Identity,
    store: &mut SqliteStore,
    cfg: &mut SyncConfig,
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
        if let Some(state) =
            lockdown_sync::bootstrap_vault(&syncer, store, summary).map_err(|e| e.to_string())?
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
pub fn render_kit(kit: &RecoveryKit, account: Option<(&str, &str)>, device: &str) -> String {
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

To get back in on a new device:
  lockdown recover --server {server} --login {login} --password-stdin --kit-stdin
and give it your password, the Secret Key and the recovery code, one per line.
Lockdown never asks for this page in an email, a message or a web form.
",
        sk = kit.secret_key_text(),
        code = kit.recovery_code_text(),
        profile = kit.profile_id(),
    )
}

/// The Secret Key's bytes from its printed form. The crate parses a kit
/// as a whole, so a well-formed recovery code of zeros stands in for the
/// half that isn't needed here (08-identity-proposal §8).
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

/// Whether the account slot opens from the login's export key alone, with
/// no Secret Key: the shape D60 wrote, to be upgraded (D70).
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
/// on one that is enrolled, needs one OPAQUE round, not two. Its
/// `register` is the same hand-over: the account exists on the server
/// already, and the slot is what is being (re)written.
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

/// A media type from a file name's extension; `application/octet-stream`
/// when it isn't one of the few known here. It is sealed with the file,
/// so a wrong guess costs nothing but a label.
pub fn mime_for(name: &str) -> &'static str {
    match name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("txt" | "md") => "text/plain",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
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

/// Whether the identity root holds nothing yet (so `account login` enrols
/// this device from the server rather than opening a local identity).
pub fn root_is_empty(paths: &Paths) -> Result<bool, String> {
    Ok(ecosystem_identity::Store::at(&paths.root)
        .probe()
        .map_err(|e| e.to_string())?
        == ecosystem_identity::store::Probe::Missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sync_config_round_trips_and_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.cbor");
        let cfg = SyncConfig {
            v: SYNC_V,
            server: "https://example.invalid".into(),
            login: "me@example.com".into(),
            profile_id: Id::new(),
            token: "lk1.a.b".into(),
            expires_at: i64::MAX,
            vaults: BTreeMap::from([(
                Id::new(),
                VaultSyncState {
                    synced_seq: 3,
                    synced_rev: 7,
                },
            )]),
        };
        cfg.save(&path).unwrap();
        let back = SyncConfig::load(&path).unwrap().unwrap();
        assert_eq!(back.login, cfg.login);
        assert_eq!(back.vaults, cfg.vaults);
        assert!(back.session_token().is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let expired = SyncConfig {
            expires_at: 1,
            ..cfg
        };
        assert!(
            expired
                .session_token()
                .unwrap_err()
                .contains("account login")
        );
        assert!(
            SyncConfig::load(&dir.path().join("missing"))
                .unwrap()
                .is_none()
        );
    }
}
