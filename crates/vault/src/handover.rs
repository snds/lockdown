//! The record changes a device approval and the security features need,
//! for which the identity crate has no call yet (`08-identity-proposal.md`
//! §8; D72's rule: only the crate's own public functions, nothing
//! re-implemented, every key dropped as soon as the change is made).
//!
//! - [`root_key`]: the Identity Root Key as this device holds it, for the
//!   one moment an approval hands it to a new device.
//! - [`add_linked_device`]: the existing device writes and signs the new
//!   device's record (`enrolled_by: device`), as the crate's local
//!   `Responder` does for a second app on the same machine.
//! - [`restore_from_root_key`]: the new device builds its identity store
//!   from the root key it was handed and the records that came with it,
//!   the way the crate's `recover_with_*` calls do after their unwrap.
//! - [`profile_extra`] / [`set_profile_extra`]: Lockdown's own fields on the
//!   signed profile (`lockdown.*`), which the crate keeps as read (01-identity
//!   §9: unknown fields are preserved): the last recovery drill and the
//!   account's password stretching.

use std::path::Path;

use ciborium::Value;
use ecosystem_identity::keys::{Irk, SymmetricKey};
use ecosystem_identity::keystore::DeviceKeystore;
use ecosystem_identity::records::{
    Device, EnrolledBy, KeyRef, KeyRefKind, Purpose, RECORD_V, Timestamp as IdTimestamp,
};
use ecosystem_identity::store::Contents;
use ecosystem_identity::{Attach, Backup, Head, Identity, Record};

use crate::identity::{check_manager, write_identity_store};
use crate::{Error, Id, Result};

/// The root key and its key id, opened through this device's keystore and
/// checked against the profile's signing key. Drop it as soon as it is used.
pub fn root_key(identity: &Identity, keystore: &dyn DeviceKeystore) -> Result<(Irk, Id)> {
    let profile = identity.profile();
    let device_ref = KeyRef::new(KeyRefKind::Device, keystore.device_id());
    let envelope = identity
        .envelopes()
        .find(|e| e.purpose == Purpose::Irk && e.wrapped_by == device_ref)
        .ok_or_else(|| Error::NotFound("this device's wrap of the root key".into()))?;
    let unlock = keystore.unlock()?;
    let irk: Irk = ecosystem_identity::keys::unwrap_key(
        envelope,
        &unlock,
        &ecosystem_identity::keys::aad(profile.profile_id, None, envelope.key_id, Purpose::Irk),
    )?;
    let isk = ecosystem_identity::signing_key(&irk)?;
    if isk.verifying_key().to_bytes() != profile.isk_pub {
        return Err(Error::Invalid(
            "the root key this device holds doesn't derive the profile's signing key".into(),
        ));
    }
    Ok((irk, envelope.key_id))
}

/// Checks a device record another device proposes for itself, as the
/// crate's handshake does (`check_proposed_device`), plus Lockdown's rule
/// that a new device's receiving key is hybrid (pq-key-wrapping, D253): the
/// root key is about to be wrapped to it.
pub fn check_proposed_device(device: &Device, enroller: Id) -> Result<()> {
    let bad = |why: &str| Err(Error::Invalid(why.to_string()));
    if device.v != RECORD_V {
        return bad("the new device's record is a version this app doesn't write");
    }
    if !device.sig.is_empty() {
        return bad("the new device's record must come unsigned");
    }
    if device.revoked_at.is_some() {
        return bad("the new device's record is already revoked");
    }
    if device.enrolled_by != (EnrolledBy::Device { id: enroller }) {
        return bad("the new device's record must name this device as the one enrolling it");
    }
    if device.name.trim().is_empty() || device.name.chars().count() > 120 {
        return bad("the new device needs a name of 1 to 120 characters");
    }
    crate::pq::require_hybrid(&device.kem_pub, "the new device")?;
    Ok(())
}

/// Adds `proposed` (checked with [`check_proposed_device`]) to the identity
/// store, signed by the Identity Signing Key, enrolled now. Refused when
/// another app manages the identity or the device is already listed.
/// Returns the signed record and the new identity head.
pub fn add_linked_device(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    proposed: &Device,
) -> Result<(Device, Head)> {
    check_manager(identity)?;
    check_proposed_device(proposed, identity.device_id())?;
    let mut record = None;
    let head = write_identity_store(identity, keystore, |contents, isk| {
        if contents.devices.contains_key(&proposed.device_id) {
            return Err(Error::Invalid("this device is already enrolled".into()));
        }
        let mut device = proposed.clone();
        device.enrolled_at = IdTimestamp::now();
        device.sign(isk.inner())?;
        contents.devices.insert(device.device_id, device.clone());
        record = Some(device);
        Ok(true)
    })?
    .expect("adding a device always writes");
    Ok((record.expect("the record was set"), head))
}

/// On a new device with an empty root: the identity from the root key an
/// approval handed over and the records that came with it. Every record is
/// checked under the signing key the root key derives (which must be the
/// profile's), this device's own record must already be in `backup`,
/// unrevoked and with this keystore's keys, and this device's wrap of the
/// root key is written under its keystore, as the crate's recovery does.
pub fn restore_from_root_key(
    root: &Path,
    app: &str,
    keystore: &dyn DeviceKeystore,
    irk: Irk,
    irk_id: Id,
    backup: &Backup,
) -> Result<Identity> {
    let store = ecosystem_identity::Store::at(root);
    if !store.is_empty()? {
        return Err(Error::Invalid(format!(
            "{} isn't empty; joining needs an empty identity folder",
            root.display()
        )));
    }
    if backup.format != ecosystem_identity::FORMAT {
        return Err(Error::Invalid(format!(
            "the records came in a format this app doesn't read ({})",
            backup.format
        )));
    }
    let isk = ecosystem_identity::signing_key(&irk)?;
    let verifying = isk.verifying_key();
    if verifying.to_bytes() != backup.profile.isk_pub {
        return Err(Error::Invalid(
            "the root key handed over doesn't belong to the identity whose records came with it"
                .into(),
        ));
    }
    backup.profile.verify(&verifying)?;
    backup.profile.kdf.check_floor()?;
    let mut contents = Contents {
        profile: Some(backup.profile.clone()),
        ..Default::default()
    };
    for p in &backup.personas {
        p.verify(&verifying)?;
        contents.personas.insert(p.persona_id, p.clone());
    }
    for d in &backup.devices {
        d.verify(&verifying)?;
        contents.devices.insert(d.device_id, d.clone());
    }
    for e in &backup.envelopes {
        e.verify(&verifying)?;
        if e.wrapped_by.kind != KeyRefKind::Device {
            contents.put_envelope(e.clone());
        }
    }
    for c in &backup.consents {
        c.verify(&verifying)?;
        contents.consents.insert(c.file_name(), c.clone());
    }
    match contents.devices.get(&keystore.device_id()) {
        None => {
            return Err(Error::Invalid(
                "the device that approved this one didn't record it".into(),
            ));
        }
        Some(d) if d.revoked_at.is_some() => {
            return Err(Error::Invalid(
                "this device was revoked from the identity".into(),
            ));
        }
        Some(d) if d.device_pub != keystore.device_pub() || d.kem_pub != keystore.kem_pub() => {
            return Err(Error::Invalid(
                "the device record that came back isn't this device's keys".into(),
            ));
        }
        Some(_) => {}
    }
    let profile_id = backup.profile.profile_id;
    let unlock = keystore.unlock()?;
    let mut wrap = ecosystem_identity::keys::wrap(
        irk.expose(),
        &unlock,
        KeyRef::new(KeyRefKind::Device, keystore.device_id()),
        &ecosystem_identity::keys::aad(profile_id, None, irk_id, Purpose::Irk),
    )?;
    wrap.sign(&isk)?;
    contents.put_envelope(wrap);
    store.init()?;
    let (_, head) = store.write(&contents, &isk, keystore.device_id())?;
    drop(irk);
    match Identity::attach(root, app, keystore, Some(head))? {
        Attach::Attached(identity) => Ok(*identity),
        Attach::Nothing => Err(Error::Invalid("the identity store came out empty".into())),
        Attach::NotEnrolled { .. } => Err(Error::Invalid(
            "the identity store doesn't open on this device".into(),
        )),
    }
}

/// Raises the profile's signed Argon2id parameters (the kit's stretching,
/// 04-security §4) to `params`, which must meet or exceed the current set in
/// every parameter: they only ever rise (D26, D32). The kit's wrap was made
/// under the old set, so the caller re-wraps it ([`rewrap_kit`]) with the
/// recovery code in hand, at once. `None` when nothing changed.
pub fn raise_profile_kdf(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    params: ecosystem_identity::Argon2Params,
) -> Result<Option<Head>> {
    check_manager(identity)?;
    let current = identity.profile().kdf;
    if !params.meets(&current) {
        return Err(Error::Invalid(format!(
            "the kit's stretching only rises; {params} is below {current}"
        )));
    }
    if params == current {
        return Ok(None);
    }
    params.check_floor()?;
    write_identity_store(identity, keystore, |contents, isk| {
        let profile = contents
            .profile
            .as_mut()
            .ok_or_else(|| Error::NotFound("the profile".into()))?;
        profile.kdf = params;
        profile.sign(isk.inner())?;
        Ok(true)
    })
}

/// Wraps the root key to `kit` again under the profile's current
/// stretching (after [`raise_profile_kdf`]), replacing the kit's wrap.
pub fn rewrap_kit(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    kit: &ecosystem_identity::RecoveryKit,
) -> Result<Head> {
    check_manager(identity)?;
    let (irk, irk_id) = root_key(identity, keystore)?;
    let profile = identity.profile().clone();
    if kit.profile_id() != profile.profile_id {
        return Err(Error::Invalid("that kit is for another identity".into()));
    }
    let kit_key = kit.kit_key(&profile.kdf)?;
    let mut envelope = ecosystem_identity::keys::wrap(
        irk.expose(),
        &kit_key,
        KeyRef::new(KeyRefKind::Kit, profile.profile_id),
        &ecosystem_identity::keys::aad(profile.profile_id, None, irk_id, Purpose::Irk),
    )?;
    drop(irk);
    let head = write_identity_store(identity, keystore, |contents, isk| {
        envelope.sign(isk.inner())?;
        contents.put_envelope(envelope.clone());
        Ok(true)
    })?
    .expect("a re-wrap always writes");
    Ok(head)
}

/// One of Lockdown's fields on the profile, as last signed.
pub fn profile_extra(identity: &Identity, key: &str) -> Option<Value> {
    identity.profile().extra.get(key).cloned()
}

/// Sets (`Some`) or removes (`None`) Lockdown's fields on the profile, signed
/// by the Identity Signing Key. Every key must start with `lockdown.`.
/// Returns the new identity head, or `None` when nothing changed.
pub fn set_profile_extra(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    entries: &[(&str, Option<Value>)],
) -> Result<Option<Head>> {
    check_manager(identity)?;
    if let Some((k, _)) = entries.iter().find(|(k, _)| !k.starts_with("lockdown.")) {
        return Err(Error::Invalid(format!(
            "'{k}' isn't one of Lockdown's profile fields"
        )));
    }
    write_identity_store(identity, keystore, |contents, isk| {
        let profile = contents
            .profile
            .as_mut()
            .ok_or_else(|| Error::NotFound("the profile".into()))?;
        let mut changed = false;
        for (k, v) in entries {
            let before = profile.extra.get(*k).cloned();
            if before.as_ref() == v.as_ref() {
                continue;
            }
            match v {
                Some(v) => {
                    profile.extra.insert((*k).to_string(), v.clone());
                }
                None => {
                    profile.extra.remove(*k);
                }
            }
            changed = true;
        }
        if changed {
            profile.sign(isk.inner())?;
        }
        Ok(changed)
    })
}

/// An integer field of the profile's extras, when it is one.
pub fn profile_extra_int(identity: &Identity, key: &str) -> Option<i64> {
    match profile_extra(identity, key)? {
        Value::Integer(i) => i64::try_from(i).ok(),
        _ => None,
    }
}

/// Sets an integer field on the profile (`lockdown.drilled_at`).
pub fn set_profile_int(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    key: &str,
    value: i64,
) -> Result<Option<Head>> {
    set_profile_extra(
        identity,
        keystore,
        &[(key, Some(Value::Integer(value.into())))],
    )
}

/// Argon2id parameters kept on the profile (`lockdown.account_ksf`), as a
/// map of the three numbers.
pub fn profile_params(identity: &Identity, key: &str) -> Option<ecosystem_identity::Argon2Params> {
    let value = profile_extra(identity, key)?;
    let Value::Map(map) = value else {
        return None;
    };
    let get = |name: &str| {
        map.iter().find_map(|(k, v)| match (k, v) {
            (Value::Text(k), Value::Integer(i)) if k == name => u32::try_from(*i).ok(),
            _ => None,
        })
    };
    Some(ecosystem_identity::Argon2Params {
        m_cost_kib: get("m_cost_kib")?,
        t_cost: get("t_cost")?,
        p_cost: get("p_cost")?,
    })
}

/// Sets Argon2id parameters on the profile (see [`profile_params`]).
pub fn set_profile_params(
    identity: &mut Identity,
    keystore: &dyn DeviceKeystore,
    key: &str,
    params: &ecosystem_identity::Argon2Params,
) -> Result<Option<Head>> {
    let value = Value::Map(vec![
        (
            Value::Text("m_cost_kib".into()),
            Value::Integer(params.m_cost_kib.into()),
        ),
        (
            Value::Text("p_cost".into()),
            Value::Integer(params.p_cost.into()),
        ),
        (
            Value::Text("t_cost".into()),
            Value::Integer(params.t_cost.into()),
        ),
    ]);
    set_profile_extra(identity, keystore, &[(key, Some(value))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::FileKeystore;

    fn proposed(keystore: &FileKeystore, enroller: Id) -> Device {
        Device {
            v: RECORD_V,
            device_id: keystore.device_id(),
            name: "Lockdown on new-laptop".into(),
            platform: keystore.platform(),
            attestation: keystore.attestation(),
            device_pub: keystore.device_pub(),
            kem_pub: keystore.kem_pub(),
            enrolled_at: IdTimestamp::now(),
            enrolled_by: EnrolledBy::Device { id: enroller },
            revoked_at: None,
            extra: Default::default(),
            sig: Vec::new(),
        }
    }

    #[test]
    fn a_device_added_from_another_restores_from_the_root_key_and_opens() {
        let dir = tempfile::tempdir().unwrap();
        let ks = FileKeystore::open_or_create(&dir.path().join("old.cbor")).unwrap();
        let mut old = crate::identity::open_or_create_identity(
            &dir.path().join("old-root"),
            "Lockdown on old",
            &ks,
            None,
        )
        .unwrap();
        let new_ks = FileKeystore::open_or_create(&dir.path().join("new.cbor")).unwrap();
        let mut device = proposed(&new_ks, old.device_id());

        // The proposal is checked: a wrong enroller, a signed record, no name.
        let mut wrong = device.clone();
        wrong.enrolled_by = EnrolledBy::Kit;
        assert!(add_linked_device(&mut old, &ks, &wrong).is_err());
        let mut nameless = device.clone();
        nameless.name = " ".into();
        assert!(add_linked_device(&mut old, &ks, &nameless).is_err());

        let (signed, _) = add_linked_device(&mut old, &ks, &device).unwrap();
        assert!(!signed.sig.is_empty());
        assert!(old.devices().any(|d| d.device_id == new_ks.device_id()));
        assert!(
            add_linked_device(&mut old, &ks, &device).is_err(),
            "already enrolled"
        );

        let (irk, irk_id) = root_key(&old, &ks).unwrap();
        let backup = old.export_backup();
        let new_root = dir.path().join("new-root");
        let opened = restore_from_root_key(
            &new_root,
            crate::identity::APP,
            &new_ks,
            irk,
            irk_id,
            &backup,
        )
        .unwrap();
        assert_eq!(opened.profile().profile_id, old.profile().profile_id);
        assert_eq!(opened.device_id(), new_ks.device_id());
        // A second restore into the same root is refused.
        let (irk, irk_id) = root_key(&old, &ks).unwrap();
        assert!(
            restore_from_root_key(
                &new_root,
                crate::identity::APP,
                &new_ks,
                irk,
                irk_id,
                &backup
            )
            .is_err()
        );

        // Records without this device, or another identity's root key, fail.
        let other_ks = FileKeystore::open_or_create(&dir.path().join("other.cbor")).unwrap();
        let (irk, irk_id) = root_key(&old, &ks).unwrap();
        let err = restore_from_root_key(
            &dir.path().join("other-root"),
            crate::identity::APP,
            &other_ks,
            irk,
            irk_id,
            &backup,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("didn't record it"), "{err}");
        let stranger = crate::identity::open_or_create_identity(
            &dir.path().join("stranger-root"),
            "x",
            &other_ks,
            None,
        )
        .unwrap();
        let (irk, irk_id) = root_key(&stranger, &other_ks).unwrap();
        device.name = "y".into();
        let err = restore_from_root_key(
            &dir.path().join("third-root"),
            crate::identity::APP,
            &new_ks,
            irk,
            irk_id,
            &backup,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("doesn't belong"), "{err}");
    }

    #[test]
    fn the_kits_stretching_rises_and_the_kit_still_opens() {
        let dir = tempfile::tempdir().unwrap();
        let ks = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let root = dir.path().join("root");
        let mut identity = crate::identity::open_or_create_identity(&root, "d", &ks, None).unwrap();
        let kit = identity.create_recovery_kit().unwrap();
        let before = identity.profile().kdf;
        let raised = ecosystem_identity::Argon2Params {
            t_cost: before.t_cost + 2,
            ..before
        };
        assert!(
            raise_profile_kdf(&mut identity, &ks, before)
                .unwrap()
                .is_none()
        );
        let lower = ecosystem_identity::Argon2Params {
            t_cost: before.t_cost.saturating_sub(1).max(1),
            m_cost_kib: before.m_cost_kib / 2,
            ..before
        };
        assert!(raise_profile_kdf(&mut identity, &ks, lower).is_err());
        raise_profile_kdf(&mut identity, &ks, raised)
            .unwrap()
            .unwrap();
        rewrap_kit(&mut identity, &ks, &kit).unwrap();
        assert_eq!(identity.profile().kdf, raised);
        // The kit rebuilds the identity under the raised set.
        let backup = identity.export_backup();
        let other = FileKeystore::open_or_create(&dir.path().join("o.cbor")).unwrap();
        ecosystem_identity::Identity::recover_with_kit(
            &dir.path().join("again"),
            crate::identity::APP,
            "again",
            &other,
            &kit,
            &backup,
        )
        .unwrap();
    }

    #[test]
    fn lockdowns_profile_fields_are_signed_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let ks = FileKeystore::open_or_create(&dir.path().join("k.cbor")).unwrap();
        let mut identity =
            crate::identity::open_or_create_identity(&dir.path().join("root"), "d", &ks, None)
                .unwrap();
        assert_eq!(profile_extra_int(&identity, "lockdown.drilled_at"), None);
        let head = set_profile_extra(
            &mut identity,
            &ks,
            &[(
                "lockdown.drilled_at",
                Some(Value::Integer(1_700_000_000.into())),
            )],
        )
        .unwrap();
        assert!(head.is_some());
        assert_eq!(
            profile_extra_int(&identity, "lockdown.drilled_at"),
            Some(1_700_000_000)
        );
        // The same value again writes nothing; another app's key is refused.
        assert!(
            set_profile_extra(
                &mut identity,
                &ks,
                &[(
                    "lockdown.drilled_at",
                    Some(Value::Integer(1_700_000_000.into()))
                )]
            )
            .unwrap()
            .is_none()
        );
        assert!(set_profile_extra(&mut identity, &ks, &[("mail.x", None)]).is_err());
        // It survives a reopen: the crate kept it and its signature holds.
        drop(identity);
        let again = crate::identity::open_identity(&dir.path().join("root"), &ks, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            profile_extra_int(&again, "lockdown.drilled_at"),
            Some(1_700_000_000)
        );
    }
}
