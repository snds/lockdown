//! The duplicate finder (02-model §11 "after import, the duplicate finder
//! runs"): items that look like the same thing twice, across every vault
//! of the persona, and the merge that folds a duplicate Login into the one
//! kept.
//!
//! Three kinds of sameness, all decided here so no secret leaves the core:
//!
//! - **Same login**: Logins sharing a website host and a username (case
//!   ignored). These are the ones "Merge" applies to.
//! - **Same password**: items whose password is the same, compared by a
//!   BLAKE3 hash keyed with a random key made for this one search and
//!   dropped after it, so the hashes can't be kept, compared later or
//!   guessed offline.
//! - **Same code**: one-time codes with the same set-up key, by the same
//!   keyed hash.
//!
//! Archived and trashed items are left out; so are groups already listed
//! as the same login.

use std::collections::{BTreeMap, BTreeSet};

use lockdown_vault::{FieldKind, FieldValue, Id, ItemKind, ItemState, Organization, PlainField};

use super::{Session, parse_id, text_of};
use crate::types::DuplicateGroup;

/// One item as the finder sees it.
struct Seen {
    id: Id,
    kind: ItemKind,
    title: String,
    edited_at: i64,
}

/// Joins the sets that share a member (union by key).
fn groups_of(pairs: BTreeMap<Vec<u8>, BTreeSet<usize>>) -> Vec<BTreeSet<usize>> {
    let mut groups: Vec<BTreeSet<usize>> = Vec::new();
    for members in pairs.into_values().filter(|m| m.len() > 1) {
        let touching: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.is_disjoint(&members))
            .map(|(i, _)| i)
            .collect();
        let mut merged = members;
        for i in touching.into_iter().rev() {
            merged.extend(groups.remove(i));
        }
        groups.push(merged);
    }
    groups
}

fn host_of(url: &str) -> Option<String> {
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    lockdown_matcher::Origin::parse(&url)
        .ok()
        .map(|o| o.host.trim_start_matches("www.").to_string())
}

impl Session {
    /// Every group of items that look like the same thing twice.
    pub fn duplicates(&self) -> Result<Vec<DuplicateGroup>, String> {
        let key =
            zeroize::Zeroizing::new(lockdown_crypto::rand::random_32().map_err(|e| e.to_string())?);
        let mut seen: Vec<Seen> = Vec::new();
        let mut logins: BTreeMap<Vec<u8>, BTreeSet<usize>> = BTreeMap::new();
        let mut passwords: BTreeMap<Vec<u8>, BTreeSet<usize>> = BTreeMap::new();
        let mut codes: BTreeMap<Vec<u8>, BTreeSet<usize>> = BTreeMap::new();
        for vault in self.vaults() {
            for item in vault.items() {
                if item.kind.hidden() || item.state != ItemState::Live {
                    continue;
                }
                let fields = vault.open_item(item.item_id).map_err(|e| e.to_string())?;
                let i = seen.len();
                seen.push(Seen {
                    id: item.item_id,
                    kind: item.kind,
                    title: text_of(&fields, "title").unwrap_or_default(),
                    edited_at: item.latest().map(|v| v.edited_at.0).unwrap_or(0),
                });
                if item.kind == ItemKind::Login {
                    let user = text_of(&fields, "user").unwrap_or_default().to_lowercase();
                    for f in &fields {
                        if let FieldValue::Url { value, .. } = &f.value
                            && let Some(host) = host_of(value)
                        {
                            let k = lockdown_crypto::hash::keyed_hash(
                                &key,
                                format!("{host}\n{user}").as_bytes(),
                            );
                            logins.entry(k.as_bytes().to_vec()).or_default().insert(i);
                        }
                    }
                }
                for f in &fields {
                    match (&f.kind, &f.value) {
                        (FieldKind::Hidden, FieldValue::Text(pw))
                            if f.field_id == "pw" && !pw.is_empty() =>
                        {
                            let k = lockdown_crypto::hash::keyed_hash(&key, pw.as_bytes());
                            passwords
                                .entry(k.as_bytes().to_vec())
                                .or_default()
                                .insert(i);
                        }
                        (FieldKind::Totp, FieldValue::Text(uri)) => {
                            if let Ok(code) = lockdown_codes::parse_uri(uri) {
                                let k =
                                    lockdown_crypto::hash::keyed_hash(&key, code.secret.expose());
                                codes.entry(k.as_bytes().to_vec()).or_default().insert(i);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        drop(key);
        let mut out = Vec::new();
        let mut listed: Vec<BTreeSet<usize>> = Vec::new();
        let push = |reason: &str,
                    words: String,
                    members: &BTreeSet<usize>,
                    mergeable: bool,
                    out: &mut Vec<DuplicateGroup>| {
            let mut ids: Vec<&Seen> = members.iter().map(|i| &seen[*i]).collect();
            ids.sort_by(|a, b| b.edited_at.cmp(&a.edited_at).then(a.title.cmp(&b.title)));
            out.push(DuplicateGroup {
                reason: reason.into(),
                words,
                ids: ids.iter().map(|s| s.id.to_string()).collect(),
                mergeable,
            });
        };
        for g in groups_of(logins) {
            push(
                "same-login",
                format!("{} logins for the same website and username", g.len()),
                &g,
                true,
                &mut out,
            );
            listed.push(g);
        }
        for g in groups_of(passwords) {
            if listed.iter().any(|l| g.is_subset(l)) {
                continue;
            }
            let kinds: BTreeSet<ItemKind> = g.iter().map(|i| seen[*i].kind).collect();
            let what = if kinds.len() == 1 && kinds.contains(&ItemKind::Login) {
                "logins"
            } else {
                "items"
            };
            push(
                "same-password",
                format!("{} {what} with the same password", g.len()),
                &g,
                false,
                &mut out,
            );
        }
        for g in groups_of(codes) {
            push(
                "same-code",
                format!("{} items with the same one-time code set-up", g.len()),
                &g,
                false,
                &mut out,
            );
        }
        // The hashes are keyed afresh each search, so their order means
        // nothing: list the groups by kind, then by their newest item's title.
        let title_of = |id: &str| {
            seen.iter()
                .find(|s| s.id.to_string() == id)
                .map(|s| s.title.to_lowercase())
                .unwrap_or_default()
        };
        let rank = |reason: &str| match reason {
            "same-login" => 0,
            "same-password" => 1,
            _ => 2,
        };
        out.sort_by_cached_key(|g| {
            (
                rank(&g.reason),
                g.ids.first().map(|i| title_of(i)).unwrap_or_default(),
                g.ids.clone(),
            )
        });
        Ok(out)
    }

    /// Folds the Logins `others` into `keep`: websites, codes, passkeys and
    /// fields of their own `keep` lacks are added; notes that differ are
    /// appended; tags join and a favourite stays one; a password `keep`
    /// lacks is taken. Then `others` go to the trash, readable through the
    /// grace period. One new version of `keep`. Returns its number.
    pub fn merge_duplicates(&mut self, keep: &str, others: &[String]) -> Result<u64, String> {
        let keep_id = parse_id(keep)?;
        let other_ids = others
            .iter()
            .map(|o| parse_id(o))
            .collect::<Result<Vec<_>, _>>()?;
        if other_ids.is_empty() || other_ids.contains(&keep_id) {
            return Err("name the copy to keep and at least one other to fold into it".into());
        }
        let vault = self.holding(keep_id)?;
        let item = vault.item(keep_id).map_err(|e| e.to_string())?;
        if item.kind != ItemKind::Login || item.state != ItemState::Live {
            return Err("only live Logins merge; trash the others instead".into());
        }
        let mut fields = vault.open_item(keep_id).map_err(|e| e.to_string())?;
        let mut org = Organization::of(&fields);
        for other in &other_ids {
            let ov = self.holding(*other)?;
            let o = ov.item(*other).map_err(|e| e.to_string())?;
            if o.kind != ItemKind::Login || o.state != ItemState::Live {
                return Err("only live Logins merge; trash the others instead".into());
            }
            let theirs = ov.open_item(*other).map_err(|e| e.to_string())?;
            fold(&mut fields, &theirs);
            let their_org = Organization::of(&theirs);
            org.tags.extend(their_org.tags);
            org.favourite |= their_org.favourite;
        }
        org.tags =
            lockdown_vault::organization::normalize_tags(&org.tags).map_err(|e| e.to_string())?;
        org.apply(&mut fields);
        let n = self
            .holding_mut(keep_id)?
            .update_item(keep_id, fields)
            .map_err(|e| e.to_string())?;
        for other in &other_ids {
            self.holding_mut(*other)?
                .trash_item(*other)
                .map_err(|e| e.to_string())?;
        }
        self.save()?;
        Ok(n)
    }
}

/// The highest `<prefix><n>` index among `fields` plus one.
fn next_index(fields: &[PlainField], prefix: &str) -> usize {
    fields
        .iter()
        .filter_map(|f| {
            f.field_id
                .strip_prefix(prefix)?
                .split('.')
                .next()?
                .parse::<usize>()
                .ok()
        })
        .max()
        .map_or(0, |n| n + 1)
}

/// Adds to `into` what `from` holds and it lacks (see `merge_duplicates`).
fn fold(into: &mut Vec<PlainField>, from: &[PlainField]) {
    let has_value = |into: &[PlainField], v: &FieldValue| into.iter().any(|f| f.value == *v);
    // Websites and codes by value, renumbered after the ones already there.
    for f in from
        .iter()
        .filter(|f| f.kind == FieldKind::Url && f.field_id.starts_with("url."))
    {
        if let FieldValue::Url { value, .. } = &f.value
            && !into
                .iter()
                .any(|g| matches!(&g.value, FieldValue::Url { value: v, .. } if v == value))
        {
            let n = next_index(into, "url.");
            into.push(PlainField {
                field_id: format!("url.{n}"),
                ..f.clone()
            });
        }
    }
    for f in from.iter().filter(|f| f.kind == FieldKind::Totp) {
        if !has_value(into, &f.value) {
            let n = next_index(into, "code.");
            into.push(PlainField {
                field_id: format!("code.{n}"),
                ..f.clone()
            });
        }
    }
    // Passkeys move as whole groups (`passkey.<n>.*`), renumbered.
    let mut groups: BTreeMap<String, Vec<&PlainField>> = BTreeMap::new();
    for f in from.iter().filter(|f| f.field_id.starts_with("passkey.")) {
        let n = f.field_id["passkey.".len()..]
            .split('.')
            .next()
            .unwrap_or("")
            .to_string();
        groups.entry(n).or_default().push(f);
    }
    for group in groups.values() {
        let already = group
            .iter()
            .filter(|f| f.field_id.ends_with(".credential_id"))
            .all(|f| has_value(into, &f.value));
        if already && group.iter().any(|f| f.field_id.ends_with(".credential_id")) {
            continue;
        }
        let n = next_index(into, "passkey.");
        for f in group {
            let rest = f.field_id["passkey.".len()..]
                .split_once('.')
                .map(|(_, r)| r)
                .unwrap_or("");
            into.push(PlainField {
                field_id: format!("passkey.{n}.{rest}"),
                ..(*f).clone()
            });
        }
    }
    // A password, a username, when the kept one has none.
    for id in ["pw", "user"] {
        if !into.iter().any(|f| f.field_id == id)
            && let Some(f) = from.iter().find(|f| f.field_id == id)
        {
            into.push(f.clone());
        }
    }
    // Notes that differ are kept, one after the other.
    if let Some(theirs) = text_of(from, "notes").filter(|t| !t.trim().is_empty()) {
        match into.iter_mut().find(|f| f.field_id == "notes") {
            Some(ours) => {
                if let FieldValue::Text(t) = &mut ours.value
                    && !t.contains(theirs.trim())
                {
                    t.push_str("\n\n");
                    t.push_str(&theirs);
                }
            }
            None => {
                if let Some(f) = from.iter().find(|f| f.field_id == "notes") {
                    into.push(f.clone());
                }
            }
        }
    }
    // Fields of their own by label and value.
    for f in from.iter().filter(|f| f.field_id.starts_with("custom.")) {
        if !into
            .iter()
            .any(|g| g.label == f.label && g.value == f.value)
        {
            let n = next_index(into, "custom.");
            into.push(PlainField {
                field_id: format!("custom.{n}"),
                ..f.clone()
            });
        }
    }
}
