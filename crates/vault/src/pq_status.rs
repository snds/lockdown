//! Where the post-quantum signature rollout stands for an identity, as one
//! typed answer every shell shows: the desktop's and the web vault's
//! Security screen and `lockdown security` (D346;
//! docs/plan/30-pq-status-line.md).
//!
//! The rollout has two steps ([`crate::pq`]): the identity's ML-DSA-65 key
//! is published once every device not revoked marks level 1
//! ([`MANIFEST_LEVEL`]), and vault heads carry the second signature once
//! every one marks level 2 ([`HEAD_LEVEL`]). [`rollout`] reads both from
//! the identity's records and answers one of three states:
//!
//! - **On**: the key is published and every device reads post-quantum
//!   heads, so new heads carry both signatures.
//! - **Waiting**: some devices don't say so yet. They are named, each in
//!   one list only: below level 1, or at level 1 but below level 2.
//! - **Unknown**: this device can't tell. No account, the server not
//!   reached since unlock, a server that holds no signed record set, or
//!   records that should have switched it on and didn't. Never "on".
//!
//! What leaves here about a device is its id and the name the Devices
//! screen shows; nothing else.

use ecosystem_identity::Identity;
use ecosystem_identity::records::Id;

use crate::Result;
use crate::pq::{HEAD_LEVEL, MANIFEST_LEVEL, published_key, supports_level};

/// What the account's server said about the identity's records, as the
/// caller knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reported {
    /// This device has no account: no server holds its records.
    NoAccount,
    /// The server hasn't confirmed the records since this device unlocked,
    /// or the last try failed (offline, a run-out session).
    Unreached,
    /// The server answered but holds no signed record set for the account
    /// (an older server, or one that lost it).
    Silent,
    /// The server confirmed the records since this device unlocked.
    Confirmed,
}

/// Why this device can't tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unknown {
    /// No account on this device.
    NoAccount,
    /// The server hasn't confirmed the records since unlock.
    Unreached,
    /// The server holds no signed record set for the account.
    Silent,
    /// Every device is ready, but the key isn't published yet: the next
    /// sync publishes it.
    NotPublished,
    /// Another app manages the identity's devices, so Lockdown never
    /// publishes the key.
    ManagedElsewhere,
}

/// Where the rollout stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RolloutState {
    /// Heads carry both signatures.
    On,
    /// Some devices hold it back (named in [`Rollout`]).
    Waiting,
    /// This device can't tell.
    Unknown(Unknown),
}

/// One device holding the rollout back: its id and the name the Devices
/// screen shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolloutDevice {
    pub id: Id,
    pub name: String,
}

/// The rollout's state and who holds it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rollout {
    pub state: RolloutState,
    /// Devices not revoked that don't mark level 1: they hold back the
    /// identity's key and so everything after it. Empty unless waiting.
    pub without_level_1: Vec<RolloutDevice>,
    /// Devices not revoked that mark level 1 but not level 2: they hold
    /// back the heads' signature only. Never in `without_level_1` too.
    /// Empty unless waiting.
    pub without_level_2: Vec<RolloutDevice>,
}

/// Where the rollout stands for `identity`, given what its server said
/// (`reported`). Devices come in the Devices screen's order: oldest
/// enrolment first, then by id.
pub fn rollout(identity: &Identity, reported: Reported) -> Result<Rollout> {
    let unknown = |why| {
        Ok(Rollout {
            state: RolloutState::Unknown(why),
            without_level_1: Vec::new(),
            without_level_2: Vec::new(),
        })
    };
    match reported {
        Reported::NoAccount => return unknown(Unknown::NoAccount),
        Reported::Unreached => return unknown(Unknown::Unreached),
        Reported::Silent => return unknown(Unknown::Silent),
        Reported::Confirmed => {}
    }
    let published = published_key(identity.profile())?.is_some();
    if !published && crate::identity::check_manager_public(identity).is_err() {
        return unknown(Unknown::ManagedElsewhere);
    }
    let mut devices: Vec<_> = identity
        .devices()
        .filter(|d| d.revoked_at.is_none())
        .collect();
    devices.sort_by_key(|d| (d.enrolled_at, d.device_id));
    let mut without_level_1 = Vec::new();
    let mut without_level_2 = Vec::new();
    for d in devices {
        let entry = || RolloutDevice {
            id: d.device_id,
            name: d.name.clone(),
        };
        if !supports_level(d, MANIFEST_LEVEL) {
            without_level_1.push(entry());
        } else if !supports_level(d, HEAD_LEVEL) {
            without_level_2.push(entry());
        }
    }
    if !without_level_1.is_empty() || !without_level_2.is_empty() {
        return Ok(Rollout {
            state: RolloutState::Waiting,
            without_level_1,
            without_level_2,
        });
    }
    if !published {
        return unknown(Unknown::NotPublished);
    }
    Ok(Rollout {
        state: RolloutState::On,
        without_level_1,
        without_level_2,
    })
}

impl Rollout {
    /// Every device holding it back: level 1's first, then level 2's.
    pub fn waiting_for(&self) -> impl Iterator<Item = &RolloutDevice> {
        self.without_level_1
            .iter()
            .chain(self.without_level_2.iter())
    }

    /// The line itself, in words for someone who has never heard
    /// "post-quantum".
    pub fn headline(&self) -> String {
        match self.state {
            RolloutState::On => "Quantum-safe signatures: on".into(),
            RolloutState::Waiting => {
                let names: Vec<&str> = self.waiting_for().map(|d| d.name.as_str()).collect();
                format!(
                    "Quantum-safe signatures: waiting for {}",
                    join_names(&names)
                )
            }
            RolloutState::Unknown(_) => "Quantum-safe signatures: can't check right now".into(),
        }
    }

    /// One short sentence under the line: what it means, or what to do.
    pub fn explanation(&self) -> &'static str {
        let one = self.waiting_for().nth(1).is_none();
        match self.state {
            RolloutState::On => {
                "Every change your devices make to your vaults is signed in a way that even a future quantum computer couldn't fake."
            }
            RolloutState::Waiting if one => {
                "This device needs the latest Lockdown. Once it's updated and opened, the stronger signatures switch on by themselves."
            }
            RolloutState::Waiting => {
                "These devices need the latest Lockdown. Once each is updated and opened, the stronger signatures switch on by themselves."
            }
            RolloutState::Unknown(Unknown::NoAccount) => {
                "They switch on through your account, and this device isn't signed in to one."
            }
            RolloutState::Unknown(Unknown::Unreached) => {
                "Lockdown hasn't reached your server since you unlocked. It checks again at the next sync."
            }
            RolloutState::Unknown(Unknown::Silent) => {
                "Your server didn't report your account's devices. It may be an older version."
            }
            RolloutState::Unknown(Unknown::NotPublished) => {
                "All your devices are ready. Lockdown switches them on at the next sync."
            }
            RolloutState::Unknown(Unknown::ManagedElsewhere) => {
                "Another app looks after your account's devices, so Lockdown can't switch them on."
            }
        }
    }
}

/// "A", "A and B", "A, B and C".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pq::{SUPPORT_FIELD, mark_this_device, publish_key};
    use ecosystem_identity::{FileKeystore, Record as _};

    /// An identity whose own device marks level 2, plus devices named in
    /// `others` marking the level given (`None`: no mark, an old build).
    fn identity_with(
        others: &[(&str, Option<u64>)],
    ) -> (tempfile::TempDir, Identity, FileKeystore, Vec<Id>) {
        let dir = tempfile::tempdir().unwrap();
        let keystore = FileKeystore::open_or_create(&dir.path().join("a.cbor")).unwrap();
        let mut identity =
            Identity::create(&dir.path().join("a"), "lockdown", "a", &keystore).unwrap();
        mark_this_device(&mut identity, &keystore).unwrap();
        let mut ids = Vec::new();
        for (i, (name, level)) in others.iter().enumerate() {
            let id = Id::new();
            ids.push(id);
            crate::identity::write_identity_store(&mut identity, &keystore, |contents, isk| {
                let mut d = contents.devices.values().next().unwrap().clone();
                d.device_id = id;
                d.name = (*name).into();
                // Enrolled one after another, so the order is the screen's.
                d.enrolled_at =
                    ecosystem_identity::records::Timestamp(d.enrolled_at.0 + 1 + i as i64);
                match level {
                    Some(n) => {
                        d.extra
                            .insert(SUPPORT_FIELD.into(), ciborium::Value::Integer((*n).into()));
                    }
                    None => {
                        d.extra.remove(SUPPORT_FIELD);
                    }
                }
                d.sign(isk.inner())?;
                contents.devices.insert(id, d);
                Ok(true)
            })
            .unwrap();
        }
        (dir, identity, keystore, ids)
    }

    fn names(list: &[RolloutDevice]) -> Vec<&str> {
        list.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn on_once_the_key_is_published_and_every_device_reads_heads() {
        let (_dir, mut identity, keystore, _) = identity_with(&[("Work laptop", Some(2))]);
        publish_key(&mut identity, &keystore).unwrap();
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::On);
        assert!(r.without_level_1.is_empty() && r.without_level_2.is_empty());
        assert_eq!(r.headline(), "Quantum-safe signatures: on");
        assert!(r.explanation().contains("quantum computer couldn't fake"));
    }

    #[test]
    fn waiting_on_level_1_names_the_device_that_holds_the_key_back() {
        let (_dir, identity, _keystore, ids) =
            identity_with(&[("Lockdown on the old laptop", None)]);
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::Waiting);
        assert_eq!(names(&r.without_level_1), ["Lockdown on the old laptop"]);
        assert_eq!(r.without_level_1[0].id, ids[0]);
        assert!(r.without_level_2.is_empty());
        assert_eq!(
            r.headline(),
            "Quantum-safe signatures: waiting for Lockdown on the old laptop"
        );
        assert!(
            r.explanation()
                .starts_with("This device needs the latest Lockdown")
        );
    }

    #[test]
    fn waiting_on_level_2_names_the_device_that_holds_heads_back() {
        let (_dir, mut identity, keystore, ids) = identity_with(&[("Kitchen tablet", Some(1))]);
        publish_key(&mut identity, &keystore).unwrap();
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::Waiting);
        assert!(r.without_level_1.is_empty());
        assert_eq!(names(&r.without_level_2), ["Kitchen tablet"]);
        assert_eq!(r.without_level_2[0].id, ids[0]);
        assert_eq!(
            r.headline(),
            "Quantum-safe signatures: waiting for Kitchen tablet"
        );
    }

    #[test]
    fn both_levels_waiting_keep_their_lists_apart_and_skip_revoked_devices() {
        let (_dir, mut identity, keystore, ids) = identity_with(&[
            ("Old phone", None),
            ("Kitchen tablet", Some(1)),
            ("Work laptop", Some(2)),
            ("Gone laptop", None),
            ("Another old one", Some(0)),
        ]);
        crate::identity::revoke_device(&mut identity, &keystore, ids[3]).unwrap();
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::Waiting);
        // Enrolment order, as the Devices screen lists them; each device in
        // one list only; the revoked one in neither.
        assert_eq!(names(&r.without_level_1), ["Old phone", "Another old one"]);
        assert_eq!(names(&r.without_level_2), ["Kitchen tablet"]);
        assert_eq!(
            r.headline(),
            "Quantum-safe signatures: waiting for Old phone, Another old one and Kitchen tablet"
        );
        assert!(
            r.explanation()
                .starts_with("These devices need the latest Lockdown")
        );
    }

    #[test]
    fn unknown_is_never_on_and_names_no_device() {
        // Every device ready and the key published: still unknown while
        // the server hasn't confirmed it.
        let (_dir, mut identity, keystore, _) = identity_with(&[("Work laptop", Some(2))]);
        publish_key(&mut identity, &keystore).unwrap();
        for (reported, why) in [
            (Reported::NoAccount, Unknown::NoAccount),
            (Reported::Unreached, Unknown::Unreached),
            (Reported::Silent, Unknown::Silent),
        ] {
            let r = rollout(&identity, reported).unwrap();
            assert_eq!(r.state, RolloutState::Unknown(why));
            assert!(r.waiting_for().next().is_none());
            assert_eq!(
                r.headline(),
                "Quantum-safe signatures: can't check right now"
            );
        }
        // A device waiting, but no word from the server: unknown, unnamed.
        let (_dir, identity, _keystore, _) = identity_with(&[("Old phone", None)]);
        let r = rollout(&identity, Reported::Unreached).unwrap();
        assert_eq!(r.state, RolloutState::Unknown(Unknown::Unreached));
        assert!(r.without_level_1.is_empty());
        // Every device ready and confirmed, but the key not published yet.
        let (_dir, identity, _keystore, _) = identity_with(&[("Work laptop", Some(2))]);
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::Unknown(Unknown::NotPublished));
        assert!(r.explanation().contains("next sync"));
    }

    #[test]
    fn an_identity_another_app_manages_is_unknown_not_waiting() {
        let (_dir, mut identity, _keystore, _) = identity_with(&[("Old phone", None)]);
        identity.adopt("notes").unwrap();
        let r = rollout(&identity, Reported::Confirmed).unwrap();
        assert_eq!(r.state, RolloutState::Unknown(Unknown::ManagedElsewhere));
        assert!(r.waiting_for().next().is_none());
    }

    #[test]
    fn names_join_as_a_sentence_does() {
        assert_eq!(join_names(&["A"]), "A");
        assert_eq!(join_names(&["A", "B"]), "A and B");
        assert_eq!(join_names(&["A", "B", "C"]), "A, B and C");
    }
}
