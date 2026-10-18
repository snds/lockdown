//! The quick-access search index (02-model §9 "built on the device from
//! decrypted fields, never synced"; 02-model §10 "favourites and recency
//! rank suggestions"): one ranked, typo-tolerant index that every shell
//! asks the same way, so the desktop's palette, the web vault's, the
//! browser extension's popup and the command line find the same items in
//! the same order.
//!
//! **What goes in is the caller's choice and never a secret.** A [`Doc`]
//! holds the plain words a summary already shows (title, website host,
//! username, the template's subtitle, the kind, tags, the folder's path);
//! a crown jewel's doc holds its title and kind only (02-model §3 "never
//! in search previews"). Nothing here reads an item, so nothing here can
//! index a password, a code's set-up or a note's body by mistake.
//!
//! **How a word matches.** The text is folded to lower case and cut into
//! words at anything that is not a letter or a digit; a website's whole
//! host is kept as one more word. Each word of the query must match some
//! word of the doc (any order), and the doc's score is the sum of each
//! query word's best match times the field's weight:
//!
//! | Match | Score | Example (`github`) |
//! |---|---|---|
//! | the whole word | 100 | `github` |
//! | the start of a word | 60 to 90, more the more of it is typed | `git` |
//! | inside a word, two letters or more | 40 | `thu` |
//! | one letter off (four to seven letters typed; two off from eight) | 15 to 25 | `gtihub`, `githb` |
//!
//! A typo match ranks below every exact one, so the first result, which
//! Ctrl+Enter copies, is a typo only when nothing matches exactly (D118's
//! worry). Words of three letters or fewer are never matched loosely: the
//! letters `gml` find words that contain `gml`, nothing spread across a
//! title. Ties go to the caller's own boost (favourite, then recent use),
//! then to the order the docs were given in (A–Z in every shell).
//!
//! Every structure is a `BTreeMap` or a `Vec`, so the same docs and the
//! same query give the same answer in every build.

use std::collections::BTreeMap;

/// Which part of an item a word came from; it weighs the match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    /// The item's title.
    Title,
    /// A website's host (`github.com`), whole and in parts.
    Host,
    /// The username or email the summary shows.
    Username,
    /// The template's plain field under the title (a network's name, a card's holder).
    Subtitle,
    /// The kind's names (`login`, `Wi-Fi network`).
    Kind,
    /// A tag (`work/clients`).
    Tag,
    /// The folder's path.
    Folder,
}

impl Field {
    fn weight(self) -> u32 {
        match self {
            Self::Title | Self::Host => 3,
            Self::Username | Self::Tag => 2,
            Self::Subtitle | Self::Kind | Self::Folder => 1,
        }
    }
}

/// One item as the index sees it: a key the caller knows it by, the plain
/// words it is found by, and a boost that breaks ties (higher first).
#[derive(Clone, Debug)]
pub struct Doc<K> {
    pub key: K,
    pub terms: Vec<(Field, String)>,
    pub boost: u64,
}

impl<K> Doc<K> {
    /// A doc with no words yet.
    pub fn new(key: K, boost: u64) -> Self {
        Self {
            key,
            terms: Vec::new(),
            boost,
        }
    }

    /// Adds the words of `text` under `field`; empty text adds nothing.
    pub fn with(mut self, field: Field, text: &str) -> Self {
        if !text.trim().is_empty() {
            self.terms.push((field, text.to_string()));
        }
        self
    }
}

/// A result: which doc, its score, and whether every word matched exactly
/// (no typo was forgiven).
#[derive(Debug, PartialEq, Eq)]
pub struct Hit<'a, K> {
    pub key: &'a K,
    pub score: u32,
    pub exact: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Posting {
    doc: u32,
    field: Field,
}

/// The index: every distinct word once, with the docs and fields it came
/// from, and the docs in the order given.
#[derive(Debug)]
pub struct Index<K> {
    docs: Vec<(K, u64)>,
    words: BTreeMap<String, Vec<Posting>>,
}

/// The longest query this answers; the rest is ignored.
pub const MAX_QUERY_CHARS: usize = 256;
/// The most words of a query that count.
pub const MAX_QUERY_WORDS: usize = 8;
/// The longest a word may be and still be matched loosely.
const MAX_FUZZY_CHARS: usize = 32;

/// Folds text to lower case and cuts it into words at anything that is not
/// a letter or a digit.
pub fn words(text: &str) -> Vec<String> {
    let lower: String = text.chars().flat_map(char::to_lowercase).collect();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

impl<K> Index<K> {
    /// Builds the index from `docs`, kept in the order given.
    pub fn build(docs: impl IntoIterator<Item = Doc<K>>) -> Self {
        let mut out = Self {
            docs: Vec::new(),
            words: BTreeMap::new(),
        };
        for doc in docs {
            let n = out.docs.len() as u32;
            for (field, text) in &doc.terms {
                let mut seen = words(text);
                if *field == Field::Host {
                    let host = text.trim().to_lowercase();
                    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
                    if host.contains('.') {
                        seen.push(host);
                    }
                    seen.retain(|w| w != "www");
                }
                for w in seen {
                    let postings = out.words.entry(w).or_default();
                    let p = Posting {
                        doc: n,
                        field: *field,
                    };
                    if !postings.contains(&p) {
                        postings.push(p);
                    }
                }
            }
            out.docs.push((doc.key, doc.boost));
        }
        out
    }

    /// How many docs the index holds.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// The docs matching every word of `query`, best first, at most
    /// `limit`. An empty query lists every doc by boost, then in order.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'_, K>> {
        let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
        let mut asked = words(&query);
        asked.dedup();
        asked.truncate(MAX_QUERY_WORDS);
        // (score, exact) per doc, for docs every word so far matched.
        let mut found: Option<BTreeMap<u32, (u32, bool)>> = None;
        for q in &asked {
            let best = self.best_for(q);
            found = Some(match found {
                None => best,
                Some(prev) => prev
                    .into_iter()
                    .filter_map(|(doc, (score, exact))| {
                        best.get(&doc).map(|(s, e)| (doc, (score + s, exact && *e)))
                    })
                    .collect(),
            });
        }
        let mut hits: Vec<(u32, u32, bool)> = match found {
            None => (0..self.docs.len() as u32).map(|d| (d, 0, true)).collect(),
            Some(m) => m.into_iter().map(|(d, (s, e))| (d, s, e)).collect(),
        };
        // Exact before forgiven, then the score, the boost, the order given.
        hits.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then(b.1.cmp(&a.1))
                .then(self.docs[b.0 as usize].1.cmp(&self.docs[a.0 as usize].1))
                .then(a.0.cmp(&b.0))
        });
        hits.into_iter()
            .take(limit)
            .map(|(d, score, exact)| Hit {
                key: &self.docs[d as usize].0,
                score,
                exact,
            })
            .collect()
    }

    /// For one query word: each doc it matches, with the best score over
    /// that doc's words and whether that best was exact.
    fn best_for(&self, q: &str) -> BTreeMap<u32, (u32, bool)> {
        let mut best: BTreeMap<u32, (u32, bool)> = BTreeMap::new();
        let mut note = |postings: &[Posting], score: u32, exact: bool| {
            for p in postings {
                let s = score * p.field.weight();
                let e = best.entry(p.doc).or_insert((0, false));
                // An exact match beats a forgiven one whatever the scores.
                if (exact, s) > (e.1, e.0) {
                    *e = (s, exact);
                }
            }
        };
        let qn = q.chars().count();
        for (word, postings) in &self.words {
            if let Some(score) = exact_score(q, qn, word) {
                note(postings, score, true);
            } else if let Some(score) = typo_score(q, qn, word) {
                note(postings, score, false);
            }
        }
        best
    }
}

/// The whole word, its start, or inside it.
fn exact_score(q: &str, qn: usize, word: &str) -> Option<u32> {
    if word == q {
        return Some(100);
    }
    if word.starts_with(q) {
        let wn = word.chars().count().max(1) as u32;
        return Some(60 + 30 * qn as u32 / wn);
    }
    if qn >= 2 && word.contains(q) {
        return Some(40);
    }
    None
}

/// One letter off for four to seven letters typed, two from eight: against
/// the whole word, or against its start (the person is still typing).
fn typo_score(q: &str, qn: usize, word: &str) -> Option<u32> {
    let allowed = match qn {
        0..=3 => return None,
        4..=7 => 1,
        _ => 2,
    };
    if qn > MAX_FUZZY_CHARS {
        return None;
    }
    let w: Vec<char> = word.chars().take(MAX_FUZZY_CHARS + 2).collect();
    let q: Vec<char> = q.chars().collect();
    if w.len().abs_diff(q.len()) <= allowed {
        let d = distance(&q, &w);
        if d <= allowed {
            return Some(25 - 5 * d as u32);
        }
    }
    // The start of a longer word, give or take the letters forgiven.
    let shortest = qn.saturating_sub(allowed).max(1);
    let longest = (qn + allowed).min(w.len().saturating_sub(1));
    (shortest..=longest)
        .map(|n| distance(&q, &w[..n]))
        .min()
        .filter(|d| *d <= allowed)
        .map(|d| 20 - 5 * d as u32)
}

/// Optimal string alignment distance: insertions, deletions, changes and
/// two neighbours swapped, each one step.
fn distance(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index<&'static str> {
        Index::build([
            Doc::new("github", 0)
                .with(Field::Title, "GitHub")
                .with(Field::Host, "github.com")
                .with(Field::Username, "sean@example.com")
                .with(Field::Kind, "login"),
            Doc::new("mail", 0)
                .with(Field::Title, "Mail")
                .with(Field::Host, "www.gmail.com")
                .with(Field::Username, "sean@example.com")
                .with(Field::Kind, "login"),
            Doc::new("wifi", 0)
                .with(Field::Title, "Home network")
                .with(Field::Subtitle, "Thunderbird-5G")
                .with(Field::Kind, "Wi-Fi network")
                .with(Field::Tag, "house"),
            Doc::new("wallet", 3)
                .with(Field::Title, "Cold wallet")
                .with(Field::Kind, "Crypto wallet"),
        ])
    }

    fn keys(hits: &[Hit<'_, &'static str>]) -> Vec<&'static str> {
        hits.iter().map(|h| *h.key).collect()
    }

    #[test]
    fn every_word_must_match_in_any_order() {
        let ix = index();
        assert_eq!(keys(&ix.search("example sean git", 10)), ["github"]);
        assert_eq!(keys(&ix.search("git sean", 10)), ["github"]);
        assert!(ix.search("github thunderbird", 10).is_empty());
    }

    #[test]
    fn short_words_never_match_loosely() {
        let ix = index();
        // Not g…m…l across GitHub's words (D118): only words containing "gml".
        assert!(ix.search("gml", 10).is_empty());
        assert_eq!(keys(&ix.search("gma", 10)), ["mail"]);
    }

    #[test]
    fn typos_are_forgiven_but_rank_below_exact_matches() {
        let ix = index();
        let hits = ix.search("gtihub", 10);
        assert_eq!(keys(&hits), ["github"]);
        assert!(!hits[0].exact);
        assert_eq!(keys(&ix.search("thnderbird", 10)), ["wifi"]);
        assert_eq!(keys(&ix.search("thunderbrid", 10)), ["wifi"]);
        // Still typing, with a slip: the start of the word.
        assert_eq!(keys(&ix.search("thnder", 10)), ["wifi"]);
        // "mail" is exact for Mail and one letter off "gmail"'s nothing; "mial"
        // is one swap from "mail".
        let hits = ix.search("mial", 10);
        assert_eq!(keys(&hits), ["mail"]);
        // An exact hit on one item beats a typo hit on another.
        let ix = Index::build([
            Doc::new("typo", 9).with(Field::Title, "Bank"),
            Doc::new("exact", 0).with(Field::Title, "Banks"),
        ]);
        let hits = ix.search("banks", 10);
        assert_eq!(keys(&hits), ["exact", "typo"]);
        assert!(hits[0].exact && !hits[1].exact);
    }

    #[test]
    fn hosts_match_whole_and_in_parts_without_www() {
        let ix = index();
        assert_eq!(keys(&ix.search("gmail.com", 10)), ["mail"]);
        assert_eq!(keys(&ix.search("github.com", 10)), ["github"]);
        assert!(ix.search("www", 10).is_empty());
    }

    #[test]
    fn ranking_is_field_weight_then_boost_then_order() {
        let ix = Index::build([
            Doc::new("tagged", 0)
                .with(Field::Title, "Router")
                .with(Field::Tag, "bank"),
            Doc::new("titled", 0).with(Field::Title, "Bank"),
            Doc::new("favourite", 5).with(Field::Subtitle, "bank"),
            Doc::new("also", 0).with(Field::Title, "Bank"),
        ]);
        assert_eq!(
            keys(&ix.search("bank", 10)),
            ["titled", "also", "tagged", "favourite"]
        );
        // No query: boost first, then the order given.
        assert_eq!(
            keys(&ix.search("", 10)),
            ["favourite", "tagged", "titled", "also"]
        );
        assert_eq!(ix.search("", 2).len(), 2);
    }

    #[test]
    fn what_is_not_given_is_not_found() {
        // The index holds only the words a summary shows; a crown jewel's
        // doc has its title and kind, so its username never finds it.
        let ix = Index::build([Doc::new("jewel", 0)
            .with(Field::Title, "Cold wallet")
            .with(Field::Kind, "Crypto wallet")]);
        assert!(ix.search("hunter2", 10).is_empty());
        assert_eq!(ix.search("wallet", 10).len(), 1);
    }

    #[test]
    fn letters_fold_and_the_query_is_bounded() {
        let ix = Index::build([Doc::new("cafe", 0).with(Field::Title, "CAFÉ Ünïcode")]);
        assert_eq!(ix.search("café", 10).len(), 1);
        assert_eq!(ix.search("ÜNÏ", 10).len(), 1);
        let long = "a ".repeat(10_000);
        assert!(ix.search(&long, 10).is_empty());
        assert_eq!(distance(&['a', 'b'], &['b', 'a']), 1);
        assert_eq!(distance(&[], &['x']), 1);
    }
}
