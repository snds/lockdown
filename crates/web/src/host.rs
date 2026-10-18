//! [`WebVault`]: the session host around `lockdown-app` in the browser.
//!
//! The app itself is the desktop's. What this adds is the part a tab has
//! to do differently from a machine with a keystore:
//!
//! - **Every sign-in enrols this tab as a device.** The tab starts with
//!   nothing: an empty identity root and data directory in the worker's
//!   memory. Signing in is OPAQUE with the password, then the Secret Key
//!   and the export key open the account slot (04-security §3) exactly as
//!   `lockdown account login` does on a new machine, and the device is
//!   added to the identity's signed records so its writes verify on every
//!   other device.
//! - **The OPAQUE round is run here, not inside the app,** because the
//!   export key it yields is what seals this device's key file between
//!   unlocks: `K = HKDF-SHA-512(export_key, salt = device id,
//!   "lockdown/web/device-key/v1")`, XChaCha20-Poly1305 bound to the device
//!   id and the login. The app then enrols from that same sign-in
//!   (`App::account_login_signed_in`), so the password is stretched once.
//! - **Lock wipes the device key file** (overwritten, then removed) and
//!   forgets the session token; the identity's device wrap of the root key
//!   and every vault stay sealed under it. Whichever timer locked the app,
//!   the next call or tick finds it locked and wipes the file.
//! - **Unlock is a fresh OPAQUE sign-in** with the same device: the export
//!   key re-derives `K`, opens the sealed file, the device proves it holds
//!   its key, and the server's fresh session replaces the old one. A wrong
//!   password fails inside OPAQUE before anything is sent.
//! - **Accounts are made elsewhere.** An identity created inside the
//!   WebAssembly build takes the identity crate's WASM Argon2id floor for
//!   its kit, which the server and every desktop refuse as below theirs;
//!   so the web vault signs in to accounts the desktop app or the CLI made
//!   and never creates one (docs/plan/17-web-vault.md, D176).
//! - **Sign out forgets everything** the tab holds. The device record
//!   stays in the identity, where another device can revoke it; its key
//!   is gone, so it can never sign again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::{ExportKey, FileKeystore};
use lockdown_app::secrets::{self, SecretStore};
use lockdown_app::{App, AppError, AppResult, LockPolicy, Paths, Status};
use lockdown_crypto::aead::{self, Sealed};
use lockdown_crypto::auth::{AuthRounds, DeviceProof, LoginClient, Session, device_pop_message};
use lockdown_crypto::identity::SymmetricKey;
use lockdown_crypto::{Error as CryptoError, Key32};
use lockdown_store::{HeadMemory, RememberedHead, ServerView, TransparencyMemory};
use lockdown_sync::HttpTransport;
use lockdown_vault::{Id, SignedTreeHead};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// The file the identity crate's keystore keeps this device's keys in,
/// inside the data directory (the app's own name for it).
const KEYSTORE_FILE: &str = "device-key.cbor";

/// The label of the device-key seal: the HKDF info and the start of the
/// associated data.
const SEAL_LABEL: &[u8] = b"lockdown/web/device-key/v1";

/// Where the tab keeps the identity and Lockdown's files (inside the
/// worker's in-memory file system in the browser; a temporary directory in
/// the native tests).
#[derive(Clone, Debug)]
pub struct WebPaths {
    /// The identity root.
    pub root: PathBuf,
    /// Lockdown's data directory.
    pub data: PathBuf,
}

impl WebPaths {
    /// The places the worker uses: `/lockdown/identity` and `/lockdown/data`.
    pub fn in_worker() -> Self {
        Self {
            root: PathBuf::from("/lockdown/identity"),
            data: PathBuf::from("/lockdown/data"),
        }
    }

    fn keystore(&self) -> PathBuf {
        self.data.join(KEYSTORE_FILE)
    }

    fn app_paths(&self) -> Paths {
        Paths {
            root: self.root.clone(),
            data: self.data.clone(),
        }
    }
}

/// What the page shows before anything else: whether someone is signed in
/// in this tab, and whether the vault is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WebState {
    /// A sign-in happened in this tab and has not been signed out.
    pub signed_in: bool,
    /// The vault is closed (always, before a sign-in).
    pub locked: bool,
    /// The server signed in to.
    pub server: Option<String>,
    /// The login name signed in with.
    pub login: Option<String>,
}

/// The account this tab signed in to.
#[derive(Clone, Debug)]
struct Account {
    server: String,
    login: String,
}

/// This device's key file, sealed under the export key's sub-key.
#[derive(Clone, Debug)]
struct SealedDevice {
    device_id: Id,
    sealed: Sealed,
}

/// The session token's home: memory only, never the file system, so a
/// lock can forget it (lockdown-app's `secrets` hook).
#[derive(Default)]
#[cfg_attr(not(target_os = "wasi"), allow(dead_code))]
struct MemorySecrets(Mutex<BTreeMap<String, Zeroizing<String>>>);

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))]
impl SecretStore for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let held = self
            .0
            .lock()
            .map_err(|_| "the token store failed".to_string())?;
        Ok(held.get(key).map(|v| v.as_str().to_string()))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| "the token store failed".to_string())?;
        held.insert(key.to_string(), Zeroizing::new(value.to_string()));
        Ok(())
    }

    fn delete(&self, key: &str) {
        if let Ok(mut held) = self.0.lock() {
            held.remove(key);
        }
    }
}

/// The web vault's session: the app, and what a tab adds around it.
pub struct WebVault {
    paths: WebPaths,
    app: App,
    account: Option<Account>,
    sealed: Option<SealedDevice>,
    /// The persona's vaults as last listed, for [`WebVault::carried`].
    known_vaults: Vec<Id>,
    /// The page's own origin (`https://host[:port]`), the one server a
    /// share or request link opened here may name.
    origin: Option<String>,
    /// What the last share link opened here holds, until the next one or
    /// the tab's sign-out.
    opened_link: Option<lockdown_vault::send::SendPayload>,
}

impl std::fmt::Debug for WebVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebVault")
            .field("paths", &self.paths)
            .field("signed_in", &self.account.is_some())
            .finish_non_exhaustive()
    }
}

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

/// The words for a failed OPAQUE round.
fn login_error(e: CryptoError) -> AppError {
    match e {
        CryptoError::LoginFailed => {
            refused("the login name and password didn't match an account on this server")
        }
        other => refused(other.to_string()),
    }
}

fn trimmed(server: &str) -> String {
    server.trim().trim_end_matches('/').to_string()
}

/// The seal's key: a sub-key of the export key, salted with the device id.
fn seal_key(export_key: &ExportKey, device_id: Id) -> AppResult<Key32> {
    let mut out = Zeroizing::new([0u8; 32]);
    lockdown_crypto::kdf::derive(
        export_key.expose(),
        Some(device_id.as_bytes()),
        SEAL_LABEL,
        out.as_mut(),
    )
    .map_err(|e| refused(e.to_string()))?;
    Ok(Key32::from_bytes(*out))
}

/// The seal's associated data: the label, the device and the login, so a
/// sealed file moved to another device id or login fails to open.
fn seal_aad(device_id: Id, login: &str) -> Vec<u8> {
    let mut aad = SEAL_LABEL.to_vec();
    aad.extend_from_slice(device_id.as_bytes());
    aad.extend_from_slice(login.as_bytes());
    aad
}

/// Overwrites a file with zeros, then removes it. Missing is fine.
fn wipe_file(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let zeros = vec![0u8; usize::try_from(meta.len()).unwrap_or(0)];
        let _ = std::fs::write(path, &zeros);
        let _ = std::fs::remove_file(path);
    }
}

fn remove_dir(path: &Path) -> AppResult<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(refused(format!("couldn't clear {}: {e}", path.display()))),
    }
}

/// One OPAQUE sign-in with this device's key. With `sealed`, the key file
/// is first opened from its seal (unlock); without, the file is made when
/// missing (sign-in). Either way the file is sealed afresh
/// under this sign-in's export key before the device proves itself.
fn sign_in_round(
    paths: &WebPaths,
    server: &str,
    login: &str,
    password: &str,
    sealed: Option<&SealedDevice>,
) -> AppResult<(ExportKey, Session, SealedDevice)> {
    let mut rounds = HttpTransport::new(server).map_err(|e| refused(e.to_string()))?;
    let (client, request) = LoginClient::start(password).map_err(login_error)?;
    let started = rounds.login_start(login, &request).map_err(login_error)?;
    let finished = client
        .finish(password, &started.response)
        .map_err(login_error)?;
    let path = paths.keystore();
    if let Some(s) = sealed {
        let key = seal_key(&finished.export_key, s.device_id)?;
        let bytes = aead::open(&key, &seal_aad(s.device_id, login), &s.sealed).map_err(|_| {
            refused(
                "the password opened the account but not this tab's key; sign out and sign in again",
            )
        })?;
        std::fs::create_dir_all(&paths.data)
            .and_then(|()| std::fs::write(&path, bytes.as_slice()))
            .map_err(|e| refused(format!("couldn't restore this tab's key: {e}")))?;
    }
    let keystore = FileKeystore::open_or_create(&path).map_err(|e| refused(e.to_string()))?;
    let device_id = keystore.device_id();
    if sealed.is_some_and(|s| s.device_id != device_id) {
        wipe_file(&path);
        return Err(refused("this tab's key belongs to another device"));
    }
    let bytes = Zeroizing::new(
        std::fs::read(&path).map_err(|e| refused(format!("couldn't read this tab's key: {e}")))?,
    );
    let key = seal_key(&finished.export_key, device_id)?;
    let resealed = aead::seal(&key, &seal_aad(device_id, login), &bytes)
        .map_err(|e| refused(e.to_string()))?;
    drop(bytes);
    let pop = keystore
        .sign(&device_pop_message(login, &started.login_id))
        .map_err(|e| refused(e.to_string()))?;
    let proof = DeviceProof {
        device_id,
        device_pub: keystore.device_pub(),
        pop: pop.to_vec(),
    };
    let session = rounds
        .login_finish(&started.login_id, &finished.finalization, &proof)
        .map_err(login_error)?;
    Ok((
        finished.export_key,
        session,
        SealedDevice {
            device_id,
            sealed: resealed,
        },
    ))
}

impl WebVault {
    /// A tab with nothing in it. In the worker it installs the in-memory
    /// token store, so the session token never touches a file and a lock
    /// forgets it; natively (the tests, where a desktop app shares the
    /// process) the token stays in `sync.cbor`, as the CLI keeps it.
    pub fn new(paths: WebPaths) -> Self {
        #[cfg(target_os = "wasi")]
        secrets::install(Box::new(MemorySecrets::default()));
        let app = App::start(paths.app_paths(), None, LockPolicy::default());
        Self {
            paths,
            app,
            account: None,
            sealed: None,
            known_vaults: Vec::new(),
            origin: None,
            opened_link: None,
        }
    }

    /// The page's origin, which share and request links must name.
    pub fn serving(mut self, origin: &str) -> Self {
        self.origin = Some(trimmed(origin));
        self
    }

    fn origin(&self) -> AppResult<&str> {
        self.origin
            .as_deref()
            .ok_or_else(|| refused("the web vault doesn't know which server it came from"))
    }

    /// A share link's metadata (does it need a password, how long it
    /// lasts, how many opens are left) without opening it.
    pub fn link_meta(&self, link: &str) -> AppResult<crate::links::LinkMeta> {
        crate::links::link_meta(link, self.origin()?)
    }

    /// Opens a share link (one view counted); its values stay here.
    pub fn link_open(
        &mut self,
        link: &str,
        password: Option<&[u8]>,
    ) -> AppResult<lockdown_app::sharing::OpenedView> {
        self.opened_link = None;
        let (view, payload) = crate::links::open_link(link, password, self.origin()?)?;
        self.opened_link = Some(payload);
        Ok(view)
    }

    /// One value of the opened link, as bytes.
    pub fn link_value(&self, index: usize) -> AppResult<Zeroizing<Vec<u8>>> {
        let payload = self
            .opened_link
            .as_ref()
            .ok_or_else(|| refused("no link is open"))?;
        crate::links::value_of(payload, index)
    }

    /// The opened link's file, as bytes.
    pub fn link_file(&self) -> AppResult<Zeroizing<Vec<u8>>> {
        let payload = self
            .opened_link
            .as_ref()
            .ok_or_else(|| refused("no link is open"))?;
        crate::links::file_of(payload)
    }

    /// Forgets the opened link.
    pub fn link_close(&mut self) {
        self.opened_link = None;
    }

    /// What a request link asks for.
    pub fn request_label(&self, link: &str) -> AppResult<String> {
        crate::links::check_request_server(link, self.origin()?)?;
        self.app.request_label(link.as_bytes())
    }

    /// Answers a request link with `secret`, sealed here to the key the
    /// link pins; the asker opens it on their device.
    pub fn request_answer(&self, link: &str, title: &str, secret: Vec<u8>) -> AppResult<()> {
        crate::links::check_request_server(link, self.origin()?)?;
        self.app.answer_request(link.as_bytes(), title, secret)
    }

    /// The app, for the command table.
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Where this tab keeps its files.
    pub fn paths(&self) -> &WebPaths {
        &self.paths
    }

    /// Whether someone is signed in, and whether the vault is open.
    pub fn state(&self) -> WebState {
        let locked = self.app.lock_state().map(|s| s.locked).unwrap_or(true);
        WebState {
            signed_in: self.account.is_some(),
            locked,
            server: self.account.as_ref().map(|a| a.server.clone()),
            login: self.account.as_ref().map(|a| a.login.clone()),
        }
    }

    /// Signs in to an account: this tab is enrolled as a device of the
    /// account's identity and the persona's vaults are pulled and opened.
    /// The Secret Key from the recovery kit is needed beside the password
    /// on an account protected by one (every account the apps make now).
    /// `carried` is what an earlier visit to this account in this browser
    /// handed on ([`WebVault::carried`]), when the worker kept it.
    pub fn sign_in(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
        carried: Option<&[u8]>,
    ) -> AppResult<Status> {
        let server = trimmed(server);
        self.start_empty(true)?;
        if let Some(bytes) = carried {
            self.adopt_carried(&server, bytes)?;
        }
        let opened = self.sign_in_inner(&server, login, password, secret_key);
        if opened.is_err() {
            let _ = self.start_empty(true);
        }
        opened
    }

    fn sign_in_inner(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
        secret_key: Option<&str>,
    ) -> AppResult<Status> {
        let (export_key, session, sealed) =
            sign_in_round(&self.paths, server, login, password, None)?;
        self.app
            .account_login_signed_in(server, login, export_key, session, secret_key)?;
        let status = self.app.open_or_create(true)?;
        self.account = Some(Account {
            server: server.to_string(),
            login: login.to_string(),
        });
        self.sealed = Some(sealed);
        Ok(status)
    }

    /// Opens the vault again after a lock: a fresh OPAQUE sign-in with this
    /// tab's device, whose export key opens the sealed key file.
    pub fn unlock(&mut self, password: &str) -> AppResult<Status> {
        let (Some(account), Some(sealed)) = (self.account.clone(), self.sealed.clone()) else {
            return Err(refused("nobody is signed in in this tab; sign in first"));
        };
        if !self.state().locked {
            return self.app.status();
        }
        let opened = (|| {
            let (_export_key, session, resealed) = sign_in_round(
                &self.paths,
                &account.server,
                &account.login,
                password,
                Some(&sealed),
            )?;
            let status = self.app.unlock()?;
            self.app
                .renew_session(&account.server, &account.login, session)?;
            Ok((status, resealed))
        })();
        match opened {
            Ok((status, resealed)) => {
                self.sealed = Some(resealed);
                Ok(status)
            }
            Err(e) => {
                let _ = self.app.lock();
                self.forget_open_state();
                Err(e)
            }
        }
    }

    /// Locks now: the app drops every key, the key file is wiped and the
    /// session token forgotten.
    pub fn lock(&mut self) -> AppResult<()> {
        self.app.lock()?;
        self.forget_open_state();
        Ok(())
    }

    /// Forgets everything this tab holds: the vault, the identity, the
    /// sealed key, the account. The device stays listed in the identity
    /// until another device revokes it; its key no longer exists anywhere.
    pub fn sign_out(&mut self) -> AppResult<()> {
        let _ = self.app.lock();
        self.forget_open_state();
        self.start_empty(false)
    }

    /// Runs the lock timers (the worker calls this every second) and, when
    /// any of them locked the vault, wipes what an open vault needed.
    /// Returns whether the vault is locked.
    pub fn tick(&mut self) -> bool {
        #[cfg(target_os = "wasi")]
        let locked = self.app.tick();
        #[cfg(not(target_os = "wasi"))]
        let locked = self.app.lock_state().map(|s| s.locked).unwrap_or(true);
        if locked && self.account.is_some() {
            self.forget_open_state();
        }
        locked
    }

    /// After every call: a vault the core locked (a timer, the hidden-tab
    /// rule) leaves no key file and no token behind.
    pub fn after_call(&mut self) {
        if self.account.is_some() && self.app.lock_state().map(|s| s.locked).unwrap_or(true) {
            self.forget_open_state();
        }
    }

    /// What an open vault needed, gone: the key file and the token. Only
    /// for a signed-in tab: before a sign-in succeeds the key file is the
    /// pending device key, which [`Self::start_empty`] keeps.
    fn forget_open_state(&mut self) {
        if self.account.is_none() {
            return;
        }
        wipe_file(&self.paths.keystore());
        if let (Some(account), Some(store)) = (self.account.as_ref(), secrets::store()) {
            store.delete(&secrets::session_key(&account.server, &account.login));
        }
    }

    /// Clears the tab's files and starts a fresh app over them. Until a
    /// sign-in succeeds the tab keeps one pending device key, so every
    /// attempt (a Secret Key forgotten, then typed) is the same device: a
    /// sign-in the server logged under a key that then vanished would
    /// stand in the account's log as a device the identity never listed,
    /// which every other device rightly reports as a stranger (D78).
    /// Signing out drops it with everything else.
    fn start_empty(&mut self, keep_pending_key: bool) -> AppResult<()> {
        let _ = self.app.lock();
        let pending = if keep_pending_key && self.account.is_none() {
            std::fs::read(self.paths.keystore())
                .ok()
                .map(Zeroizing::new)
        } else {
            None
        };
        wipe_file(&self.paths.keystore());
        if let (Some(account), Some(store)) = (self.account.as_ref(), secrets::store()) {
            store.delete(&secrets::session_key(&account.server, &account.login));
        }
        self.account = None;
        if let Some(mut s) = self.sealed.take() {
            s.sealed.ct.zeroize();
        }
        self.known_vaults.clear();
        self.opened_link = None;
        remove_dir(&self.paths.root)?;
        remove_dir(&self.paths.data)?;
        if let Some(bytes) = pending {
            std::fs::create_dir_all(&self.paths.data)
                .and_then(|()| std::fs::write(self.paths.keystore(), bytes.as_slice()))
                .map_err(|e| refused(format!("couldn't keep this tab's key: {e}")))?;
        }
        self.app = App::start(self.paths.app_paths(), None, LockPolicy::default());
        Ok(())
    }

    /// What this tab remembers that a later visit to the same account in
    /// this browser should hold the server to (D181): the server's pinned
    /// log key and newest verified tree head, and each vault's newest
    /// accepted head. Hashes, sizes and public keys only: no key, no value,
    /// no title. Canonical CBOR, for the worker to keep in IndexedDB.
    /// `None` before a sign-in.
    pub fn carried(&mut self) -> AppResult<Option<Vec<u8>>> {
        let Some(account) = self.account.clone() else {
            return Ok(None);
        };
        if let Ok(vaults) = self.app.vaults() {
            let mut ids: Vec<Id> = vaults
                .iter()
                .filter_map(|v| Id::parse(&v.id).ok())
                .collect();
            ids.sort();
            self.known_vaults = ids;
        }
        let paths = self.paths.app_paths();
        let log = TransparencyMemory::open(&paths.transparency_file())
            .map_err(|e| refused(e.to_string()))?
            .view(&account.server)
            .map(|v| LogPin {
                log_key: v.log_key,
                sth: v.sth.clone(),
            });
        let memory = HeadMemory::open(&paths.memory_file()).map_err(|e| refused(e.to_string()))?;
        let vaults = self
            .known_vaults
            .iter()
            .filter_map(|id| memory.vault(*id).map(|h| (*id, h)))
            .collect();
        let carried = Carried {
            v: CARRIED_V,
            server: account.server.clone(),
            log,
            vaults,
        };
        lockdown_crypto::cbor::to_canonical(&carried)
            .map(Some)
            .map_err(|e| refused(e.to_string()))
    }

    /// Puts what an earlier visit carried (`Carried`, from
    /// [`Self::carried`]) in place before a sign-in, so the server's log
    /// must continue the tree head seen then and each vault's chain must
    /// reach the head accepted then. Something unreadable, or for another
    /// server, is ignored: it only ever adds checks.
    fn adopt_carried(&self, server: &str, bytes: &[u8]) -> AppResult<()> {
        let Ok(carried) = lockdown_crypto::cbor::from_slice::<Carried>(bytes) else {
            return Ok(());
        };
        if carried.v != CARRIED_V || carried.server != server {
            return Ok(());
        }
        let paths = self.paths.app_paths();
        if let Some(pin) = carried.log {
            let mut log = TransparencyMemory::open(&paths.transparency_file())
                .map_err(|e| refused(e.to_string()))?;
            log.remember(
                server,
                ServerView {
                    log_key: pin.log_key,
                    sth: pin.sth,
                    audited: 0,
                    identity_logged: None,
                    own_device_logged: false,
                    checked_at: None,
                },
            )
            .map_err(|e| refused(e.to_string()))?;
        }
        let mut memory =
            HeadMemory::open(&paths.memory_file()).map_err(|e| refused(e.to_string()))?;
        for (vault_id, head) in carried.vaults {
            memory
                .remember_vault(vault_id, head)
                .map_err(|e| refused(e.to_string()))?;
        }
        Ok(())
    }
}

/// The version of [`Carried`].
const CARRIED_V: u16 = 1;

/// What one visit hands the next in this browser (see `WebVault::carried`).
#[derive(Serialize, Deserialize)]
struct Carried {
    v: u16,
    server: String,
    log: Option<LogPin>,
    vaults: BTreeMap<Id, RememberedHead>,
}

/// A server's log as last verified: its key and its newest tree head.
#[derive(Serialize, Deserialize)]
struct LogPin {
    #[serde(with = "serde_bytes")]
    log_key: [u8; 32],
    sth: SignedTreeHead,
}
