//! The adoption path (01-identity §6–§8; D10, D11): how Lockdown gets from
//! "a root on disk and this device's keystore" to an open vault, asking
//! for nothing else.
//!
//! 1. [`open_or_create_identity`] attaches to the ecosystem identity at the
//!    root if one is there and this device is enrolled, else creates one.
//!    It never creates a second identity next to an existing one (D10):
//!    a root with an identity this device is not enrolled in is an error
//!    whose answer is the local handshake or the recovery kit, not a new
//!    identity.
//! 2. [`default_persona`] picks the persona when the identity has several.
//! 3. [`persona_keys`] finds, or makes once, the persona's Lockdown App
//!    Data Key and Crown-Jewel Key through the identity crate
//!    (`wrap_app_key(persona, "lockdown", …)`, `app_keys`), so both sit
//!    under the Persona Key and the recovery kit reaches them (D11).
//! 4. [`open_or_create_vault`] opens the persona's vault from a store with
//!    the Vault Key unwrapped under that App Data Key, or creates the
//!    first vault and saves it.
//!
//! Same device, no kit, no master password: the device keystore opens the
//! IRK, the IRK the Persona Key, the Persona Key the App Data Key, the App
//! Data Key the Vault Key (D11 "on the same device no kit is needed").

use std::collections::BTreeMap;
use std::path::Path;

use ecosystem_identity::keys::{AppDataKey, CrownJewelKey, SymmetricKey};
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::{KeyRef, KeyRefKind, Persona, PersonaKind, Purpose};
use ecosystem_identity::{Attach, Head, Identity};
use lockdown_crypto::VerifyingKey;

use crate::signer::Signer;
use crate::store::{Store, StoreError};
use crate::vault::{Unlock, Vault, VaultContext};
use crate::{Error, Id, Result};

/// The app label Lockdown's keys carry in the identity store (D28).
pub const APP: &str = "lockdown";

/// Attaches to the identity at `root` with this device's keystore, or
/// creates one if nothing is there (§6 steps 2, 4, 5). `remembered` is the
/// identity head this device last accepted, so a rolled-back identity
/// store is refused. When no app manages the identity yet, Lockdown takes
/// management (§7: `Profile.manager = lockdown`); an identity another app
/// manages is used as is and left to that app.
pub fn open_or_create_identity(
    root: &Path,
    device_name: &str,
    keystore: &dyn DeviceKeystore,
    remembered: Option<Head>,
) -> Result<Identity> {
    let mut identity = match Identity::attach(root, APP, keystore, remembered)? {
        Attach::Attached(identity) => *identity,
        Attach::Nothing => Identity::create(root, APP, device_name, keystore)?,
        Attach::NotEnrolled { .. } => {
            return Err(Error::Invalid(format!(
                "an ecosystem identity already exists at {} but this device isn't enrolled in it; \
                 enrol it from another ecosystem app running on this device, or with the recovery kit \
                 (a second identity is never created next to it)",
                root.display()
            )));
        }
    };
    if identity.profile().manager.is_none() {
        identity.adopt(APP)?;
    }
    Ok(identity)
}

/// Which persona Lockdown opens when nothing says. In order: the one
/// persona that already holds Lockdown keys; else the only persona; else
/// the only `personal`-kind persona. Anything else is a choice the person
/// makes, by id or label ([`find_persona`]).
pub fn default_persona(identity: &Identity) -> Result<Id> {
    let mut personas: Vec<&Persona> = identity.personas().collect();
    personas.sort_by_key(|p| p.persona_id);
    let with_keys: Vec<&&Persona> = personas
        .iter()
        .filter(|p| !identity.app_keys(p.persona_id, APP).is_empty())
        .collect();
    match with_keys.as_slice() {
        [one] => return Ok(one.persona_id),
        [_, _, ..] => {
            return Err(Error::Invalid(format!(
                "several personas already hold Lockdown data ({}); choose one by id or label",
                labels(&personas)
            )));
        }
        [] => {}
    }
    if let [only] = personas.as_slice() {
        return Ok(only.persona_id);
    }
    let personal: Vec<&&Persona> = personas
        .iter()
        .filter(|p| p.kind == PersonaKind::Personal)
        .collect();
    if let [one] = personal.as_slice() {
        return Ok(one.persona_id);
    }
    Err(Error::Invalid(format!(
        "this identity has {} personas ({}); choose one by id or label",
        personas.len(),
        labels(&personas)
    )))
}

/// A persona by its id, or by its label (exact, ignoring case) when the
/// label names exactly one.
pub fn find_persona(identity: &Identity, selector: &str) -> Result<Id> {
    if let Ok(id) = Id::parse(selector)
        && identity.personas().any(|p| p.persona_id == id)
    {
        return Ok(id);
    }
    let matches: Vec<&Persona> = identity
        .personas()
        .filter(|p| p.label.eq_ignore_ascii_case(selector))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.persona_id),
        [] => Err(Error::NotFound(format!(
            "a persona named or numbered '{selector}'"
        ))),
        _ => Err(Error::Invalid(format!(
            "'{selector}' names more than one persona; use the id"
        ))),
    }
}

fn labels(personas: &[&Persona]) -> String {
    personas
        .iter()
        .map(|p| format!("{} {:?} {}", p.label, p.kind, p.persona_id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A persona's Lockdown keys, as the identity holds them.
pub struct PersonaKeys {
    /// The persona.
    pub persona_id: Id,
    /// The id of the App Data Key envelope.
    pub app_data_key_id: Id,
    /// The App Data Key: what wraps every Vault Key of this persona.
    pub app_data_key: AppDataKey,
    /// The id of the Crown-Jewel Key envelope.
    pub crown_jewel_key_id: Id,
    /// The Crown-Jewel Key: what wraps crown-jewel Item Keys.
    pub crown_jewel_key: CrownJewelKey,
}

impl std::fmt::Debug for PersonaKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersonaKeys")
            .field("persona_id", &self.persona_id)
            .field("app_data_key_id", &self.app_data_key_id)
            .field("crown_jewel_key_id", &self.crown_jewel_key_id)
            .finish_non_exhaustive()
    }
}

/// The persona's current Lockdown keys, made on first use. When the
/// persona holds several App Data Keys (a rotation, later), the newest is
/// the one new vaults are wrapped under; an existing vault names the one
/// it was wrapped under in its record ([`keys_for`]).
pub fn persona_keys(identity: &mut Identity, persona_id: Id) -> Result<PersonaKeys> {
    let app_data_key_id = match identity.app_keys(persona_id, APP).last() {
        Some(id) => *id,
        None => identity.wrap_app_key(persona_id, APP, &AppDataKey::generate()?)?,
    };
    let crown_jewel_key_id = match crown_jewel_key_ids(identity, persona_id).last() {
        Some(id) => *id,
        None => identity.wrap_crown_jewel_key(persona_id, APP, &CrownJewelKey::generate()?)?,
    };
    keys_for(identity, persona_id, app_data_key_id, crown_jewel_key_id)
}

/// The persona's Lockdown keys by their ids: the ones a vault record names.
pub fn keys_for(
    identity: &Identity,
    persona_id: Id,
    app_data_key_id: Id,
    crown_jewel_key_id: Id,
) -> Result<PersonaKeys> {
    Ok(PersonaKeys {
        persona_id,
        app_data_key_id,
        app_data_key: identity.unwrap_app_key(persona_id, APP, app_data_key_id)?,
        crown_jewel_key_id,
        crown_jewel_key: identity.unwrap_crown_jewel_key(persona_id, APP, crown_jewel_key_id)?,
    })
}

/// The ids of every Crown-Jewel Key Lockdown wrapped under a persona,
/// oldest first (the identity crate lists app keys but not these).
fn crown_jewel_key_ids(identity: &Identity, persona_id: Id) -> Vec<Id> {
    let persona_ref = KeyRef::new(KeyRefKind::Persona, persona_id);
    let mut found: Vec<_> = identity
        .envelopes()
        .filter(|e| e.purpose == Purpose::CrownJewel && e.wrapped_by == persona_ref && e.app == APP)
        .collect();
    found.sort_by_key(|e| (e.created_at, e.key_id));
    found.into_iter().map(|e| e.key_id).collect()
}

/// The public key of every device enrolled in the identity, by device id:
/// what a manifest head or item version signed by another device is
/// verified against. (Revoked devices stay, so history they signed before
/// revocation still verifies; refusing their *new* heads is the sync
/// step's job, since only the server sees order between devices.)
pub fn device_keys(identity: &Identity) -> BTreeMap<Id, VerifyingKey> {
    identity
        .devices()
        .filter_map(|d| {
            VerifyingKey::from_bytes(&d.device_pub)
                .ok()
                .map(|k| (d.device_id, k))
        })
        .collect()
}

/// An open vault with the key that wraps it, so it can be saved again.
pub struct OpenVault {
    /// The vault.
    pub vault: Vault,
    /// The persona's keys the vault hangs from.
    pub keys: PersonaKeys,
    /// Whether this call created the vault (first run).
    pub created: bool,
}

impl std::fmt::Debug for OpenVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenVault")
            .field("vault", &self.vault)
            .field("keys", &self.keys)
            .field("created", &self.created)
            .finish()
    }
}

impl OpenVault {
    /// Writes the vault to `store` under its App Data Key.
    pub fn save(&self, store: &mut dyn Store) -> Result<()> {
        self.vault.save(store, &self.keys.app_data_key)
    }
}

/// The ids of every vault in `store` that belongs to `persona_id` of this
/// identity's profile, in id order.
pub fn persona_vaults(identity: &Identity, persona_id: Id, store: &dyn Store) -> Result<Vec<Id>> {
    let profile_id = identity.profile().profile_id;
    let mut out = Vec::new();
    for vault_id in store.list_vaults()? {
        let record = store.get_vault(vault_id)?;
        if record.context.profile_id == profile_id && record.context.persona_id == persona_id {
            out.push(vault_id);
        }
    }
    Ok(out)
}

/// Opens the persona's vault from `store` through the identity, or creates
/// the first one and saves it. `signer` is this device's (the keystore);
/// `last_accepted_seq` is what this device remembers for a vault, zero for
/// none. A persona with several vaults opens the first by id; picking
/// another is [`open_vault`].
pub fn open_or_create_vault(
    identity: &mut Identity,
    persona_id: Id,
    store: &mut dyn Store,
    signer: Box<dyn Signer>,
    last_accepted_seq: &dyn Fn(Id) -> u64,
) -> Result<OpenVault> {
    if let Some(vault_id) = persona_vaults(identity, persona_id, store)?.first() {
        return open_vault(
            identity,
            *vault_id,
            store,
            signer,
            last_accepted_seq(*vault_id),
        );
    }
    let keys = persona_keys(identity, persona_id)?;
    let ctx = VaultContext {
        profile_id: identity.profile().profile_id,
        persona_id,
        vault_id: Id::new(),
        app_data_key_id: keys.app_data_key_id,
        crown_jewel_key_id: keys.crown_jewel_key_id,
    };
    let vault = Vault::create(
        ctx,
        identity.device_id(),
        signer,
        Some(keys.crown_jewel_key.clone()),
    )?;
    let opened = OpenVault {
        vault,
        keys,
        created: true,
    };
    opened.save(store)?;
    Ok(opened)
}

/// Opens one vault from `store` through the identity: the record names the
/// persona and the App Data Key, the identity opens that key, the key opens
/// the vault. Heads are verified against every device the identity knows.
pub fn open_vault(
    identity: &Identity,
    vault_id: Id,
    store: &dyn Store,
    signer: Box<dyn Signer>,
    last_accepted_seq: u64,
) -> Result<OpenVault> {
    let record = store.get_vault(vault_id)?;
    let ctx = record.context;
    if ctx.profile_id != identity.profile().profile_id {
        return Err(Error::Store(StoreError::NotFound(format!(
            "vault {vault_id} in this identity (it belongs to profile {})",
            ctx.profile_id
        ))));
    }
    let keys = keys_for(
        identity,
        ctx.persona_id,
        ctx.app_data_key_id,
        ctx.crown_jewel_key_id,
    )?;
    let devices = device_keys(identity);
    let known = |id: Id| devices.get(&id).cloned();
    let vault = Vault::load(
        store,
        vault_id,
        Unlock {
            app_data_key: &keys.app_data_key,
            crown_jewel_key: Some(keys.crown_jewel_key.clone()),
            device_id: identity.device_id(),
            signer,
            known_devices: &known,
            last_accepted_seq,
        },
    )?;
    Ok(OpenVault {
        vault,
        keys,
        created: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    fn fresh() -> (tempfile::TempDir, FileKeystore) {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("device-key.cbor")).unwrap();
        (dir, keystore)
    }

    #[test]
    fn attaches_instead_of_creating_a_second_identity_and_adopts_it() {
        let (dir, keystore) = fresh();
        let root = dir.path().join("identity");
        // Another ecosystem app made the identity first (D10).
        let first = Identity::create(&root, "other-app", "Laptop", &keystore).unwrap();
        let profile_id = first.profile().profile_id;
        assert!(first.profile().manager.is_none());
        drop(first);

        let identity = open_or_create_identity(&root, "Laptop", &keystore, None).unwrap();
        assert_eq!(identity.profile().profile_id, profile_id);
        assert_eq!(identity.profile().manager.as_deref(), Some(APP));
        assert_eq!(identity.app(), APP);
        let head = identity.head();
        drop(identity);

        // Again, with the head remembered: same identity, still one.
        let again = open_or_create_identity(&root, "Laptop", &keystore, Some(head)).unwrap();
        assert_eq!(again.profile().profile_id, profile_id);
        assert_eq!(again.personas().count(), 1);

        // A device that isn't enrolled gets told what to do, not a new identity.
        let other = FileKeystore::open_or_create(&dir.path().join("other.cbor")).unwrap();
        let err = open_or_create_identity(&root, "Phone", &other, None).unwrap_err();
        assert!(err.to_string().contains("isn't enrolled"), "{err}");
        assert!(root.join("FORMAT").exists());
    }

    #[test]
    fn an_identity_managed_by_another_app_is_left_to_it() {
        let (dir, keystore) = fresh();
        let root = dir.path().join("identity");
        let mut first = Identity::create(&root, "other-app", "Laptop", &keystore).unwrap();
        first.adopt("other-app").unwrap();
        drop(first);
        let identity = open_or_create_identity(&root, "Laptop", &keystore, None).unwrap();
        assert_eq!(identity.profile().manager.as_deref(), Some("other-app"));
    }

    #[test]
    fn persona_keys_are_made_once_and_found_again() {
        let (dir, keystore) = fresh();
        let root = dir.path().join("identity");
        let mut identity = open_or_create_identity(&root, "Laptop", &keystore, None).unwrap();
        let persona = default_persona(&identity).unwrap();
        let first = persona_keys(&mut identity, persona).unwrap();
        let second = persona_keys(&mut identity, persona).unwrap();
        assert_eq!(first.app_data_key_id, second.app_data_key_id);
        assert_eq!(first.crown_jewel_key_id, second.crown_jewel_key_id);
        assert_eq!(first.app_data_key.expose(), second.app_data_key.expose());
        assert_eq!(identity.app_keys(persona, APP), vec![first.app_data_key_id]);
        assert_eq!(crown_jewel_key_ids(&identity, persona).len(), 1);
        // Another app's keys under the same persona are not ours.
        identity
            .wrap_app_key(persona, "other-app", &AppDataKey::generate().unwrap())
            .unwrap();
        assert_eq!(identity.app_keys(persona, APP).len(), 1);
        assert!(matches!(
            keys_for(&identity, persona, first.app_data_key_id, Id::new()),
            Err(Error::Crypto(_))
        ));
    }

    #[test]
    fn the_default_persona_rule() {
        let (dir, keystore) = fresh();
        let root = dir.path().join("identity");
        let mut identity = open_or_create_identity(&root, "Laptop", &keystore, None).unwrap();
        let personal = default_persona(&identity).unwrap();
        // One persona: it.
        assert_eq!(identity.personas().next().unwrap().persona_id, personal);
        // A second, of another kind: the personal one still wins.
        let work = identity.create_persona("Work", PersonaKind::Work).unwrap();
        assert_eq!(default_persona(&identity).unwrap(), personal);
        // Two personal ones and no Lockdown keys yet: ask.
        let second = identity
            .create_persona("Personal 2", PersonaKind::Personal)
            .unwrap();
        let err = default_persona(&identity).unwrap_err();
        assert!(err.to_string().contains("choose one"), "{err}");
        // Lockdown keys on exactly one persona: that one, whatever its kind.
        persona_keys(&mut identity, work.persona_id).unwrap();
        assert_eq!(default_persona(&identity).unwrap(), work.persona_id);
        // Keys on two: ask again.
        persona_keys(&mut identity, second.persona_id).unwrap();
        assert!(default_persona(&identity).is_err());
        // By id or by label.
        assert_eq!(
            find_persona(&identity, &work.persona_id.to_string()).unwrap(),
            work.persona_id
        );
        assert_eq!(find_persona(&identity, "work").unwrap(), work.persona_id);
        assert!(matches!(
            find_persona(&identity, "nobody"),
            Err(Error::NotFound(_))
        ));
    }
}
