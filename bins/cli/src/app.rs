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
//!   known-devices.cbor        other devices' signed records, learned through the server
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

use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::{
    AccountAuth, AccountCredentials, Backup, ExportKey, FileKeystore, Identity, default_root,
};
use lockdown_crypto::auth::{DeviceBinding, OpaqueAccountAuth};
use lockdown_store::{HeadMemory, KnownDevices, RememberedHead, SqliteStore, write_private};
use lockdown_sync::{HttpTransport, SyncReport, Syncer, Transport, VaultSyncState};
use lockdown_vault::identity::{
    self, APP, OpenVault, default_persona, device_keys, find_persona, open_or_create_identity,
    open_vault_trusting, persona_vaults,
};
use lockdown_vault::{FieldKind, FieldValue, Id, ItemState, PlainField, Vault};
use serde::{Deserialize, Serialize};

const KEYSTORE_FILE: &str = "device-key.cbor";
const STORE_FILE: &str = "vault.sqlite";
const MEMORY_FILE: &str = "remembered.cbor";
const KNOWN_DEVICES_FILE: &str = "known-devices.cbor";
const SYNC_FILE: &str = "sync.cbor";
const SYNC_V: u16 = 1;

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
/// copies.
pub fn device_name() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME"] {
        if let Some(name) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return name.to_string_lossy().into_owned();
        }
    }
    if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "this device".to_string()
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
    /// `sync`: open, after learning devices and pulling a vault this
    /// device has never held.
    Sync,
}

/// The open app: identity, store, memory, learned devices, the account,
/// and the one vault in use.
pub struct App {
    paths: Paths,
    keystore: Rc<FileKeystore>,
    identity: Identity,
    store: SqliteStore,
    memory: HeadMemory,
    known: KnownDevices,
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

    /// `lockdown sync`: opens, learning other devices from the server and
    /// pulling the vault first when this device holds none yet.
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

        let persona_id = match persona {
            Some(selector) => find_persona(&identity, selector),
            None => default_persona(&identity),
        }
        .map_err(|e| e.to_string())?;

        let mut store =
            SqliteStore::open(&paths.data.join(STORE_FILE)).map_err(|e| e.to_string())?;
        let mut known =
            KnownDevices::open(&paths.data.join(KNOWN_DEVICES_FILE)).map_err(|e| e.to_string())?;
        let mut sync = SyncConfig::load(&paths.sync_file())?;

        // With an account: learn the devices enrolled elsewhere, and pull
        // the vault when this device holds none. `init` after `account
        // login` on a new device must find the account's vault, not make
        // a second one.
        if let Some(cfg) = sync.as_mut()
            && matches!(mode, Mode::Sync | Mode::Init)
        {
            let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
            let token = cfg.session_token()?.to_string();
            learn_devices(&transport, &token, &identity, &mut known)?;
            if persona_vaults(&identity, persona_id, &store)
                .map_err(|e| e.to_string())?
                .is_empty()
            {
                pull_new_vaults(&transport, &token, &identity, &mut store, cfg)?;
                cfg.save(&paths.sync_file())?;
            }
        }
        let mut learned = known.keys();
        for (id, key) in device_keys(&identity) {
            learned.entry(id).or_insert(key);
        }

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
                    &learned,
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
                &learned,
            )
            .map_err(|e| e.to_string())?
        };
        let mut app = Self {
            paths,
            keystore,
            identity,
            store,
            memory,
            known,
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
                "devices        {} in the identity, {} learned",
                self.identity.devices().count(),
                self.known.devices().count()
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
            }
            None => lines.push("account        none (run `lockdown account register`)".into()),
        }
        Ok(lines)
    }

    // --- the account --------------------------------------------------------

    /// `lockdown account register`: makes the account on `server` under
    /// `login` with OPAQUE, writes the identity's account unlock slot,
    /// uploads the identity backup, and keeps the session.
    pub fn account_register(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
    ) -> Result<Vec<String>, String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
        // Registers the password with the server and writes the `account`
        // wrap of the root key under the Account Unlock Key (04-security §3).
        // The Secret Key half waits for the recovery-kit commands.
        self.identity
            .enable_account_unlock(&mut auth, password, None)
            .map_err(|e| e.to_string())?;
        let (_export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        let backup = self
            .identity
            .export_backup()
            .to_bytes()
            .map_err(|e| e.to_string())?;
        auth.rounds()
            .put_identity(&session.token, &backup)
            .map_err(|e| e.to_string())?;
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
        Ok(vec![
            format!("account {login} registered at {server}"),
            "session kept; run `lockdown sync` to push the vault".into(),
        ])
    }

    /// `lockdown account login` on a device that already holds the
    /// identity: a fresh session, and the account's devices learned.
    pub fn account_login(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
    ) -> Result<Vec<String>, String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
        let (_export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        if session.profile_id != self.identity.profile().profile_id {
            return Err(format!(
                "the account '{login}' belongs to another profile ({}), not the identity at {}",
                session.profile_id,
                self.paths.root.display()
            ));
        }
        learn_devices(
            auth.rounds(),
            &session.token,
            &self.identity,
            &mut self.known,
        )?;
        let backup = self
            .identity
            .export_backup()
            .to_bytes()
            .map_err(|e| e.to_string())?;
        auth.rounds()
            .put_identity(&session.token, &backup)
            .map_err(|e| e.to_string())?;
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
        Ok(vec![format!("logged in as {login} at {server}")])
    }

    /// `lockdown account login` on a device with no identity: logs in
    /// with the password, fetches the identity backup, opens the root key
    /// through the account slot (04-security §3) and enrols this device.
    /// The vault arrives with the first `lockdown sync`.
    pub fn enrol_from_account(
        paths: Paths,
        server: &str,
        login: &str,
        password: &str,
    ) -> Result<Vec<String>, String> {
        match ecosystem_identity::Store::at(&paths.root)
            .probe()
            .map_err(|e| e.to_string())?
        {
            ecosystem_identity::store::Probe::Missing => {}
            _ => {
                return Err(format!(
                    "an ecosystem identity already exists at {} but this device isn't enrolled in it; \
                     enrol it from another ecosystem app running on this device, or with the recovery kit",
                    paths.root.display()
                ));
            }
        }
        let keystore = Rc::new(
            FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE))
                .map_err(|e| e.to_string())?,
        );
        let signing = keystore.clone();
        let binding = DeviceBinding {
            device_id: keystore.device_id(),
            device_pub: keystore.device_pub(),
            sign: Box::new(move |m| {
                DeviceKeystore::sign(signing.as_ref(), m).map_err(lockdown_crypto::Error::from)
            }),
        };
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, binding);
        let (export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        let backup = auth
            .rounds()
            .get_identity(&session.token)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!("the account '{login}' has no identity backup on {server} yet")
            })?;
        let backup = Backup::from_bytes(&backup).map_err(|e| e.to_string())?;
        if backup.profile.profile_id != session.profile_id {
            return Err("the server's identity backup is for another profile".into());
        }
        let mut prepared = PreparedExportKey(Some(export_key));
        let identity = Identity::recover_with_account(
            &paths.root,
            APP,
            &device_name(),
            keystore.as_ref(),
            AccountCredentials {
                auth: &mut prepared,
                password,
                secret_key: None,
            },
            &backup,
        )
        .map_err(|e| e.to_string())?;
        let mut memory =
            HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
        memory
            .remember_identity(identity.head())
            .map_err(|e| e.to_string())?;
        let mut known =
            KnownDevices::open(&paths.data.join(KNOWN_DEVICES_FILE)).map_err(|e| e.to_string())?;
        learn_devices(auth.rounds(), &session.token, &identity, &mut known)?;
        // The backup on the server now lists this device too.
        let refreshed = identity
            .export_backup()
            .to_bytes()
            .map_err(|e| e.to_string())?;
        auth.rounds()
            .put_identity(&session.token, &refreshed)
            .map_err(|e| e.to_string())?;
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

    /// `lockdown sync`: pull, merge, push, head (03-architecture §6).
    pub fn sync(&mut self) -> Result<Vec<String>, String> {
        let Some(cfg) = self.sync.as_mut() else {
            return Err("no account on this device; run `lockdown account register` or `lockdown account login` first".into());
        };
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
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
        let state = cfg.vaults.entry(vault_id).or_default();
        let outcome = lockdown_sync::sync_vault(&syncer, &mut self.opened.vault, &record, state);
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
        if let Some(cfg) = &self.sync {
            cfg.save(&self.paths.sync_file())?;
        }
        Ok(sync_lines(&report))
    }
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

/// Learns the devices in the account's identity backup whose records the
/// profile's own signing key vouches for. The server only relays them.
fn learn_devices(
    transport: &dyn Transport,
    token: &str,
    identity: &Identity,
    known: &mut KnownDevices,
) -> Result<usize, String> {
    let Some(bytes) = transport.get_identity(token).map_err(|e| e.to_string())? else {
        return Ok(0);
    };
    let backup = Backup::from_bytes(&bytes).map_err(|e| e.to_string())?;
    if backup.profile.profile_id != identity.profile().profile_id {
        return Err("the server's identity backup is for another profile".into());
    }
    known
        .learn(&backup.devices, &identity.profile().isk_pub)
        .map_err(|e| e.to_string())
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

/// An [`AccountAuth`] that hands over one export key already obtained
/// from a login, so enrolling a new device needs one OPAQUE round, not two.
struct PreparedExportKey(Option<ExportKey>);

impl AccountAuth for PreparedExportKey {
    fn register(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        Err(ecosystem_identity::Error::Unsupported(
            "this login was already run; register through the server".into(),
        ))
    }

    fn login(
        &mut self,
        _profile_id: &Id,
        _password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        self.0.take().ok_or_else(|| {
            ecosystem_identity::Error::Invalid("the export key was already used".into())
        })
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
