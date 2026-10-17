//! One run of the command-line tool: where things live on this device,
//! the open identity, the store, and what the device remembers.
//!
//! ```text
//! <identity root>/            the ecosystem identity (01-identity §5; D15)
//! <data dir>/
//!   device-key.cbor           this device's keys (the identity crate's file keystore)
//!   vault.sqlite              the ciphertext store (lockdown-store)
//!   remembered.cbor           the newest heads this device accepted (rollback memory)
//! ```
//!
//! The identity root is the shared, platform-fixed place every ecosystem
//! app reads. The data dir is Lockdown's own: next to the root
//! (`…/snds/lockdown` beside `…/snds/identity`) unless `--data` says
//! otherwise.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use ecosystem_identity::{FileKeystore, Identity, default_root};
use lockdown_store::{HeadMemory, RememberedHead, SqliteStore};
use lockdown_vault::identity::{
    self, OpenVault, default_persona, find_persona, open_or_create_identity, open_vault,
    persona_vaults,
};
use lockdown_vault::{FieldKind, FieldValue, Id, ItemState, PlainField, Vault};

const KEYSTORE_FILE: &str = "device-key.cbor";
const STORE_FILE: &str = "vault.sqlite";
const MEMORY_FILE: &str = "remembered.cbor";

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
}

/// The `lockdown` directory next to an identity root.
fn beside(root: &Path) -> PathBuf {
    match root.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("lockdown"),
        _ => root.join("lockdown"),
    }
}

/// A name for this device in the identity's device list.
fn device_name() -> String {
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

/// The open app: identity, store, memory, and the one vault in use.
pub struct App {
    paths: Paths,
    identity: Identity,
    store: SqliteStore,
    memory: HeadMemory,
    opened: OpenVault,
    /// Whether this run created the identity.
    pub identity_created: bool,
}

impl App {
    /// `lockdown init`: opens what is there and creates what is not.
    pub fn init(paths: Paths, persona: Option<&str>) -> Result<Self, String> {
        Self::open(paths, persona, true)
    }

    /// Every other command: opens what is there and refuses to create.
    pub fn open_existing(paths: Paths, persona: Option<&str>) -> Result<Self, String> {
        Self::open(paths, persona, false)
    }

    fn open(paths: Paths, persona: Option<&str>, create: bool) -> Result<Self, String> {
        let keystore_path = paths.data.join(KEYSTORE_FILE);
        if !create && !keystore_path.exists() && !paths.root.join("FORMAT").exists() {
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
        let (identity, identity_created) = if create {
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
                        "no ecosystem identity at {}; run `lockdown init` first",
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
        let opened = if create {
            let last = |vault_id: Id| memory.last_accepted_seq(vault_id);
            identity::open_or_create_vault(
                &mut identity,
                persona_id,
                &mut store,
                Box::new(keystore.clone()),
                &last,
            )
            .map_err(|e| e.to_string())?
        } else {
            let vaults =
                persona_vaults(&identity, persona_id, &store).map_err(|e| e.to_string())?;
            let Some(vault_id) = vaults.first() else {
                return Err(format!(
                    "this persona has no vault at {} yet; run `lockdown init`",
                    paths.data.display()
                ));
            };
            open_vault(
                &identity,
                *vault_id,
                &store,
                Box::new(keystore.clone()),
                memory.last_accepted_seq(*vault_id),
            )
            .map_err(|e| e.to_string())?
        };
        let mut app = Self {
            paths,
            identity,
            store,
            memory,
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

    fn remember(&mut self) -> Result<(), String> {
        let head = self.vault().head().map_err(|e| e.to_string())?;
        let hash = head.hash().map_err(|e| e.to_string())?;
        self.memory
            .remember_vault(
                self.vault().context().vault_id,
                RememberedHead {
                    seq: head.seq,
                    hash,
                },
            )
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
        Ok(vec![
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
            format!("vault          {}", self.vault().context().vault_id),
            format!("vault head     seq {} hash {}", head.seq, hash.to_hex()),
            format!("items          {live} live"),
        ])
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
