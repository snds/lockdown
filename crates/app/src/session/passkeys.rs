//! Passkeys as fields of an item (02-model §4): each passkey is the group
//! `passkey.<n>.*` on a Login (or a standalone Passkey item, as an import
//! makes one), and syncs, versions and exports like every other field.
//!
//! | part | kind | what |
//! |---|---|---|
//! | `rp_id`, `user_name`, `user_display_name` | text | the relying party and the account, as the site named them |
//! | `credential_id`, `user_handle`, `key` | passkey (bytes) | the credential id, the site's user handle, the PKCS#8 private key |
//! | `aaguid` | text | the provider that made it (Lockdown's for one made here; absent on an import) |
//! | `created` | text | seconds since the epoch, when made here |
//! | `backup_eligible`, `backed_up` | boolean | the BE and BS flags it signs with; absent means both (it syncs) |
//! | `hidden` | text | `unknown <secs>` or `not-accepted <secs>`: hidden by the site's Signal, undone by removing it |
//!
//! The key part never leaves this crate: [`Session::passkey_key`] is
//! crate-internal and the app signs with it in place; `reveal` refuses
//! every passkey field.

use lockdown_vault::{FieldKind, FieldValue, Id, ItemClass, ItemKind, ItemState, PlainField};
use zeroize::Zeroizing;

use super::{Session, now_secs, parse_id, text_of};

/// One passkey as stored, without its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// Its group number on the item.
    pub n: usize,
    pub rp_id: String,
    pub user_name: String,
    pub user_display_name: String,
    pub credential_id: Vec<u8>,
    pub user_handle: Vec<u8>,
    pub aaguid: Option<String>,
    pub created: Option<i64>,
    pub backup_eligible: bool,
    pub backed_up: bool,
    pub hidden: Option<Hidden>,
}

/// Hidden by a Signal from the site (never deleted by one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hidden {
    /// `unknown` (signalUnknownCredential) or `not-accepted`
    /// (signalAllAcceptedCredentials).
    pub reason: String,
    /// Seconds since the epoch.
    pub at: i64,
}

/// A passkey to put on an item.
pub struct NewPasskey {
    pub rp_id: String,
    pub user_name: String,
    pub user_display_name: String,
    pub credential_id: Vec<u8>,
    pub user_handle: Vec<u8>,
    pub pkcs8: Zeroizing<Vec<u8>>,
    pub aaguid: String,
    pub created: i64,
    pub backup_eligible: bool,
    pub backed_up: bool,
}

/// One passkey found in the vault, with the item that holds it.
#[derive(Clone, Debug)]
pub struct Found {
    pub item_id: Id,
    pub title: String,
    pub crown_jewel: bool,
    pub passkey: Stored,
}

fn part(field_id: &str) -> Option<(usize, &str)> {
    let rest = field_id.strip_prefix("passkey.")?;
    let (n, part) = rest.split_once('.')?;
    Some((n.parse().ok()?, part))
}

fn bytes_of(fields: &[PlainField], id: &str) -> Option<Vec<u8>> {
    fields
        .iter()
        .find(|f| f.field_id == id)
        .and_then(|f| match &f.value {
            FieldValue::Bytes(b) => Some(b.clone()),
            _ => None,
        })
}

fn bool_of(fields: &[PlainField], id: &str) -> Option<bool> {
    fields
        .iter()
        .find(|f| f.field_id == id)
        .and_then(|f| match &f.value {
            FieldValue::Boolean(b) => Some(*b),
            FieldValue::Text(t) => Some(t == "true"),
            _ => None,
        })
}

/// Every passkey group on an item that has a key and a credential id, in
/// group order.
pub fn groups(fields: &[PlainField]) -> Vec<Stored> {
    let mut ns: Vec<usize> = fields
        .iter()
        .filter_map(|f| part(&f.field_id))
        .filter(|(_, p)| *p == "key")
        .map(|(n, _)| n)
        .collect();
    ns.sort_unstable();
    ns.dedup();
    ns.into_iter()
        .filter_map(|n| {
            let p = |name: &str| format!("passkey.{n}.{name}");
            let credential_id = bytes_of(fields, &p("credential_id"))?;
            Some(Stored {
                n,
                rp_id: text_of(fields, &p("rp_id"))
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                user_name: text_of(fields, &p("user_name")).unwrap_or_default(),
                user_display_name: text_of(fields, &p("user_display_name")).unwrap_or_default(),
                credential_id,
                user_handle: bytes_of(fields, &p("user_handle")).unwrap_or_default(),
                aaguid: text_of(fields, &p("aaguid")).filter(|a| !a.is_empty()),
                created: text_of(fields, &p("created")).and_then(|t| t.parse().ok()),
                backup_eligible: bool_of(fields, &p("backup_eligible")).unwrap_or(true),
                backed_up: bool_of(fields, &p("backed_up")).unwrap_or(true),
                hidden: text_of(fields, &p("hidden")).and_then(|t| {
                    let (reason, at) = t.split_once(' ')?;
                    Some(Hidden {
                        reason: reason.to_string(),
                        at: at.parse().ok()?,
                    })
                }),
            })
        })
        .collect()
}

/// The number the next group takes.
fn next_n(fields: &[PlainField]) -> usize {
    fields
        .iter()
        .filter_map(|f| part(&f.field_id).map(|(n, _)| n))
        .max()
        .map_or(0, |n| n + 1)
}

/// Appends a passkey's group; returns its number.
pub fn append(fields: &mut Vec<PlainField>, p: &NewPasskey) -> usize {
    let n = next_n(fields);
    let id = |name: &str| format!("passkey.{n}.{name}");
    fields.push(PlainField::text(&id("rp_id"), "relying party", &p.rp_id));
    fields.push(PlainField::text(
        &id("user_name"),
        "user name",
        &p.user_name,
    ));
    fields.push(PlainField::text(
        &id("user_display_name"),
        "display name",
        &p.user_display_name,
    ));
    for (name, label, bytes) in [
        ("credential_id", "credential id", p.credential_id.clone()),
        ("user_handle", "user handle", p.user_handle.clone()),
        ("key", "private key", p.pkcs8.to_vec()),
    ] {
        fields.push(PlainField::new(
            id(name),
            FieldKind::Passkey,
            label,
            FieldValue::Bytes(bytes),
        ));
    }
    fields.push(PlainField::text(&id("aaguid"), "provider id", &p.aaguid));
    fields.push(PlainField::text(
        &id("created"),
        "created",
        &p.created.to_string(),
    ));
    for (name, label, v) in [
        ("backup_eligible", "can sync", p.backup_eligible),
        ("backed_up", "synced", p.backed_up),
    ] {
        fields.push(PlainField::new(
            id(name),
            FieldKind::Boolean,
            label,
            FieldValue::Boolean(v),
        ));
    }
    n
}

/// Removes group `n` and renumbers the groups after it, so the groups stay
/// `0..k` (an export walks them in order).
pub fn remove(fields: &mut Vec<PlainField>, n: usize) {
    fields.retain(|f| part(&f.field_id).is_none_or(|(m, _)| m != n));
    for f in fields.iter_mut() {
        if let Some((m, rest)) = part(&f.field_id).map(|(m, r)| (m, r.to_string()))
            && m > n
        {
            f.field_id = format!("passkey.{}.{rest}", m - 1);
        }
    }
}

/// Sets (or, with `None`, removes) one text part of group `n`.
pub fn set_text(
    fields: &mut Vec<PlainField>,
    n: usize,
    name: &str,
    label: &str,
    value: Option<&str>,
) {
    let id = format!("passkey.{n}.{name}");
    match value {
        None => fields.retain(|f| f.field_id != id),
        Some(v) => match fields.iter_mut().find(|f| f.field_id == id) {
            Some(f) => f.value = FieldValue::Text(v.to_string()),
            None => fields.push(PlainField::text(&id, label, v)),
        },
    }
}

impl Session {
    /// Every passkey for `rp_id` on a live Login or Passkey item, in every
    /// open vault, ordered by title, user name and credential id so the
    /// answer never depends on storage order.
    pub(crate) fn passkeys_for(&self, rp_id: &str) -> Result<Vec<Found>, String> {
        let rp = rp_id.to_ascii_lowercase();
        let mut out = Vec::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if !matches!(item.kind, ItemKind::Login | ItemKind::Passkey)
                    || item.state != ItemState::Live
                {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                let title = text_of(&fields, "title").unwrap_or_default();
                for passkey in groups(&fields).into_iter().filter(|p| p.rp_id == rp) {
                    out.push(Found {
                        item_id: item.item_id,
                        title: title.clone(),
                        crown_jewel: item.class == ItemClass::CrownJewel,
                        passkey,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            a.title
                .cmp(&b.title)
                .then(a.passkey.user_name.cmp(&b.passkey.user_name))
                .then(a.passkey.credential_id.cmp(&b.passkey.credential_id))
        });
        Ok(out)
    }

    /// The passkeys on one item, in group order.
    pub(crate) fn passkeys_on(&self, item_id: Id) -> Result<(ItemKind, Vec<PlainField>), String> {
        let vault = self.holding(item_id)?;
        let item = vault.item(item_id).map_err(|e| e.to_string())?;
        Ok((
            item.kind,
            vault.open_item(item_id).map_err(|e| e.to_string())?,
        ))
    }

    /// The PKCS#8 key of the passkey `credential_id` on `item_id`, wiped on
    /// drop. Crate-internal: the key is signed with here, never handed out.
    pub(crate) fn passkey_key(
        &self,
        item_id: Id,
        credential_id: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, String> {
        let (_, fields) = self.passkeys_on(item_id)?;
        let p = groups(&fields)
            .into_iter()
            .find(|p| p.credential_id == credential_id)
            .ok_or("that passkey isn't on this item")?;
        bytes_of(&fields, &format!("passkey.{}.key", p.n))
            .map(Zeroizing::new)
            .ok_or_else(|| "that passkey has no key".to_string())
    }

    /// Changes the fields of `item_id` through `change`, which says whether
    /// it changed anything; saves a new version only then. Returns the
    /// version number, or `None` when nothing changed.
    pub(crate) fn passkey_edit(
        &mut self,
        item_id: Id,
        change: impl FnOnce(&mut Vec<PlainField>) -> Result<bool, String>,
    ) -> Result<Option<u64>, String> {
        let (_, mut fields) = self.passkeys_on(item_id)?;
        let item = self
            .holding(item_id)?
            .item(item_id)
            .map_err(|e| e.to_string())?;
        if item.state != ItemState::Live {
            return Err("this item is in the trash or archived".into());
        }
        if !change(&mut fields)? {
            return Ok(None);
        }
        let n = self
            .holding_mut(item_id)?
            .update_item(item_id, fields)
            .map_err(|e| e.to_string())?;
        self.save()?;
        Ok(Some(n))
    }

    /// Puts a new passkey on the Login `target`, or on a new Login for
    /// `site` titled `title` when there is none. Returns the item.
    pub(crate) fn passkey_add(
        &mut self,
        target: Option<Id>,
        site: &str,
        title: &str,
        p: &NewPasskey,
    ) -> Result<Id, String> {
        let item_id = match target {
            Some(id) => id,
            None => self.add_item(
                "login",
                Some(title),
                false,
                vec![
                    lockdown_vault::FieldChange::text("url.0", site),
                    lockdown_vault::FieldChange::text("user", &p.user_name),
                    lockdown_vault::FieldChange::text("sign_in_method", "Passkey"),
                ],
            )?,
        };
        self.passkey_edit(item_id, |fields| {
            append(fields, p);
            Ok(true)
        })?;
        Ok(item_id)
    }

    /// Notes a passkey sign-in: a use of the item (D151) and the passkey's
    /// own last use, both on this device only.
    pub(crate) fn note_passkey_use(
        &mut self,
        item_id: Id,
        credential_id: &[u8],
    ) -> Result<(), String> {
        let key = &self.opened.keys.app_data_key;
        self.recency.note(item_id, now_secs(), key)?;
        self.recency
            .note_passkey(item_id, credential_id, now_secs(), key)
    }

    /// When this device last signed in with a passkey.
    pub(crate) fn passkey_last_use(&self, item_id: Id, credential_id: &[u8]) -> Option<i64> {
        self.recency.passkey(item_id, credential_id)
    }

    /// The id of an item named by text, for the app's passkey calls.
    pub(crate) fn passkey_item(&self, id: &str) -> Result<Id, String> {
        let item_id = parse_id(id)?;
        self.holding(item_id)?;
        Ok(item_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(rp: &str, cred: u8) -> NewPasskey {
        NewPasskey {
            rp_id: rp.into(),
            user_name: "me".into(),
            user_display_name: "Me".into(),
            credential_id: vec![cred; 16],
            user_handle: vec![9; 8],
            pkcs8: Zeroizing::new(vec![1, 2, 3]),
            aaguid: "07017dfd-2717-4c97-92c2-b3d6513f46ce".into(),
            created: 100,
            backup_eligible: true,
            backed_up: true,
        }
    }

    #[test]
    fn groups_append_read_back_and_renumber_on_removal() {
        let mut fields = vec![PlainField::text("user", "username", "me")];
        assert_eq!(append(&mut fields, &new("a.example", 1)), 0);
        assert_eq!(append(&mut fields, &new("b.example", 2)), 1);
        assert_eq!(append(&mut fields, &new("c.example", 3)), 2);
        let g = groups(&fields);
        assert_eq!(g.len(), 3);
        assert_eq!(g[1].rp_id, "b.example");
        assert_eq!(g[1].created, Some(100));
        assert!(g[1].backup_eligible && g[1].backed_up && g[1].hidden.is_none());
        set_text(
            &mut fields,
            1,
            "hidden",
            "hidden by the site",
            Some("unknown 500"),
        );
        assert_eq!(
            groups(&fields)[1].hidden,
            Some(Hidden {
                reason: "unknown".into(),
                at: 500
            })
        );
        remove(&mut fields, 0);
        let g = groups(&fields);
        assert_eq!(
            g.iter()
                .map(|p| (p.n, p.rp_id.as_str()))
                .collect::<Vec<_>>(),
            [(0, "b.example"), (1, "c.example")]
        );
        assert!(g[0].hidden.is_some(), "a group's parts move together");
        assert!(fields.iter().any(|f| f.field_id == "user"));
    }

    #[test]
    fn an_import_without_the_new_parts_reads_as_synced_with_no_provider() {
        let mut fields = Vec::new();
        for (part, value) in [("rp_id", "Example.COM"), ("user_name", "me")] {
            fields.push(PlainField::text(&format!("passkey.0.{part}"), part, value));
        }
        for part in ["credential_id", "user_handle", "key"] {
            fields.push(PlainField::new(
                format!("passkey.0.{part}"),
                FieldKind::Passkey,
                part,
                FieldValue::Bytes(vec![7]),
            ));
        }
        let g = groups(&fields);
        assert_eq!(g[0].rp_id, "example.com");
        assert_eq!(g[0].aaguid, None);
        assert!(g[0].backup_eligible && g[0].backed_up);
    }
}
