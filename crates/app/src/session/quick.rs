//! The quick-access index over this session's items (`lockdown_vault::search`):
//! built from the same masked summaries the list shows, kept until anything
//! it was built from changes, dropped with the session on lock.
//!
//! What a doc holds: the title, the website's host, the username, the
//! template's subtitle, the kind's names, the tags and the folder's path,
//! all plain words the list already shows. A crown jewel's summary has no
//! username, website or subtitle (D113), and its doc leaves out tags and the
//! folder too, so it is found by title and kind alone (02-model §3). No
//! secret field is read here: the summary carries none.
//!
//! When it is rebuilt: the stamp is a BLAKE3 over every item of every open
//! vault (id, revision, how many versions, state, class) and this device's
//! record of uses, which takes no decryption; a save, a sync, a move, a
//! folder renamed (the vault's hidden info item changes) or a use moves it.

use std::collections::BTreeMap;

use lockdown_vault::ItemState;
use lockdown_vault::search::{Doc, Field, Index};

use super::{Session, now_secs};
use crate::ItemSummary;

/// The index and the stamp it was built at.
pub(crate) struct Cache {
    stamp: lockdown_crypto::Hash32,
    index: Index<ItemSummary>,
}

/// One result: the item's summary and whether every word matched exactly.
#[derive(Clone, Debug, serde::Serialize)]
pub struct QuickHit {
    #[serde(flatten)]
    pub item: ItemSummary,
    /// `false` when a typo was forgiven somewhere in the match.
    pub exact: bool,
}

/// The most results a search answers.
pub const MAX_RESULTS: usize = 200;

impl Session {
    /// What the index was built from, without opening anything.
    fn search_stamp(&self) -> lockdown_crypto::Hash32 {
        let mut h = lockdown_crypto::hash::Hasher::new();
        for vault in self.vaults() {
            h.update(vault.context().vault_id.as_bytes());
            for item in vault.items() {
                h.update(item.item_id.as_bytes());
                h.update(&item.rev.to_le_bytes());
                h.update(&(item.versions.len() as u64).to_le_bytes());
                h.update(&[item_state_byte(item.state), item.class as u8]);
                if let Some(u) = self.recency.get(item.item_id) {
                    h.update(&u.last.to_le_bytes());
                    h.update(&u.count.to_le_bytes());
                }
            }
        }
        h.finalize()
    }

    /// The live items matching `query`, best first (at most `limit`, up to
    /// [`MAX_RESULTS`]); an empty query lists them in the palette's order.
    pub fn quick_search(&mut self, query: &str, limit: usize) -> Result<Vec<QuickHit>, String> {
        let stamp = self.search_stamp();
        if self.quick.as_ref().is_none_or(|c| c.stamp != stamp) {
            let index = build(self.list()?, &self.folder_paths()?, now_secs());
            self.quick = Some(Cache { stamp, index });
        }
        let Some(cache) = self.quick.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(cache
            .index
            .search(query, limit.clamp(1, MAX_RESULTS))
            .into_iter()
            .map(|hit| QuickHit {
                item: hit.key.clone(),
                exact: hit.exact,
            })
            .collect())
    }

    /// Every folder's path by id, across the open vaults.
    fn folder_paths(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self
            .vault_views()?
            .into_iter()
            .flat_map(|v| v.folders.into_iter().map(|f| (f.id, f.path)))
            .collect())
    }
}

fn item_state_byte(state: ItemState) -> u8 {
    match state {
        ItemState::Live => 1,
        ItemState::Archived => 2,
        ItemState::Trashed => 3,
        ItemState::Tombstone => 4,
    }
}

/// The index over the live rows: favourites first, then this device's uses
/// (`recency::score`), then A–Z by title, the order every shell's palette
/// already used (D151).
pub(crate) fn build(
    rows: Vec<ItemSummary>,
    folders: &BTreeMap<String, String>,
    now: i64,
) -> Index<ItemSummary> {
    let names: BTreeMap<String, String> = super::items::templates()
        .into_iter()
        .map(|t| (t.id, t.name))
        .collect();
    let mut live: Vec<ItemSummary> = rows.into_iter().filter(|r| r.state == "live").collect();
    live.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Index::build(live.into_iter().map(|row| {
        let uses = crate::recency::score(
            row.last_used.map(|last| crate::recency::Use {
                last,
                count: row.uses,
            }),
            now,
        );
        // Favourites above any use; uses kept to a thousandth.
        let boost = (u64::from(row.favourite) << 40) + (uses * 1000.0).min(1e12) as u64;
        let host = lockdown_matcher::Origin::parse(&row.site)
            .map(|o| o.host)
            .unwrap_or_else(|_| row.site.clone());
        let kind_name = names
            .get(&row.template)
            .or_else(|| names.get(&row.kind))
            .cloned()
            .unwrap_or_default();
        let mut doc = Doc::new(row.clone(), boost)
            .with(Field::Title, &row.title)
            .with(Field::Host, &host)
            .with(Field::Username, &row.username)
            .with(Field::Subtitle, &row.subtitle)
            .with(Field::Kind, &row.kind)
            .with(Field::Kind, &kind_name);
        if !row.crown_jewel {
            for tag in &row.tags {
                doc = doc.with(Field::Tag, tag);
            }
            if let Some(path) = row.folder.as_ref().and_then(|f| folders.get(f)) {
                doc = doc.with(Field::Folder, path);
            }
        }
        doc
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, title: &str) -> ItemSummary {
        ItemSummary {
            id: id.into(),
            kind: "login".into(),
            state: "live".into(),
            title: title.into(),
            site: String::new(),
            username: String::new(),
            edited_at: 0,
            crown_jewel: false,
            methods: Vec::new(),
            passkeys: 0,
            codes: 0,
            expires_at: None,
            conflict_copy: false,
            warnings: Vec::new(),
            views: Vec::new(),
            subtitle: String::new(),
            copy: None,
            vault_id: "v".into(),
            folder: None,
            tags: Vec::new(),
            favourite: false,
            last_used: None,
            uses: 0,
            expires_in_days: None,
            attachments: 0,
            access: None,
            template: "login".into(),
        }
    }

    fn ids(index: &Index<ItemSummary>, q: &str) -> Vec<String> {
        index
            .search(q, 50)
            .into_iter()
            .map(|h| h.key.id.clone())
            .collect()
    }

    #[test]
    fn the_index_ranks_like_the_palette_and_skips_what_search_leaves_out() {
        let now = 1_800_000_000;
        let mut used = row("used", "Bank B");
        used.last_used = Some(now - 86_400);
        used.uses = 4;
        let mut fav = row("fav", "Bank C");
        fav.favourite = true;
        let mut trashed = row("trashed", "Bank D");
        trashed.state = "trashed".into();
        let mut archived = row("archived", "Bank E");
        archived.state = "archived".into();
        let mut jewel = row("jewel", "Bank vault");
        jewel.crown_jewel = true;
        jewel.tags = vec!["secret-tag".into()];
        jewel.folder = Some("f1".into());
        let mut filed = row("filed", "Router");
        filed.folder = Some("f1".into());
        filed.tags = vec!["house/network".into()];
        filed.site = "https://www.netgear.com/login".into();
        let folders = BTreeMap::from([("f1".to_string(), "Home / Kit".to_string())]);
        let index = build(
            vec![
                row("plain", "Bank A"),
                used,
                fav,
                trashed,
                archived,
                jewel,
                filed,
            ],
            &folders,
            now,
        );
        assert_eq!(ids(&index, "bank"), ["fav", "used", "plain", "jewel"]);
        // Trashed and archived items leave search (D149).
        assert_eq!(index.len(), 5);
        // A crown jewel is found by title and kind, never its tags or folder.
        assert!(ids(&index, "secret").is_empty());
        assert_eq!(ids(&index, "kit"), ["filed"]);
        assert_eq!(ids(&index, "network"), ["filed"]);
        assert_eq!(ids(&index, "netgear.com"), ["filed"]);
        // The kind's plain name: "Login".
        assert_eq!(ids(&index, "routr login"), ["filed"]);
    }
}
