//! The confusables table as data (02-model §10; 09-extension-boundary §2).
//!
//! A *skeleton* is what a label looks like once every character that
//! renders like a Latin letter is replaced by that letter (`а` → `a`,
//! `1` → `l`) and a few two-character sequences are folded (`rn` → `m`).
//! Two labels with one skeleton and different text are lookalikes.
//!
//! The table ships inside the signed matcher data file, so it can grow
//! toward the full Unicode confusables list without a new build. The
//! shape here is deliberately small: a map of single characters to their
//! Latin lookalike, and an ordered list of sequence folds.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::MatchError;

/// The table as it travels in the data file.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ConfusablesTable {
    /// One character (as a one-character string) to its Latin lookalike.
    #[serde(default)]
    pub map: BTreeMap<String, String>,
    /// Sequence folds applied in order after the map: `["rn", "m"]`.
    #[serde(default)]
    pub folds: Vec<(String, String)>,
}

/// The table compiled for lookups.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Confusables {
    map: BTreeMap<char, String>,
    folds: Vec<(String, String)>,
}

impl Confusables {
    /// An empty table: skeletons are plain lowercase.
    pub fn none() -> Self {
        Self::default()
    }

    /// Compiles a table. Every map key must be exactly one character.
    pub fn from_table(table: &ConfusablesTable) -> Result<Self, MatchError> {
        let mut map = BTreeMap::new();
        for (k, v) in &table.map {
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return Err(MatchError::BadData(format!(
                    "confusables key '{k}' is not a single character"
                )));
            };
            map.insert(c, v.clone());
        }
        Ok(Self {
            map,
            folds: table.folds.clone(),
        })
    }

    /// The table from the built-in signed data. Falls back to an empty
    /// table if the built-in file fails to verify; the crate's tests check
    /// that it does not.
    pub fn builtin() -> &'static Confusables {
        static BUILTIN: OnceLock<Confusables> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            crate::signed::MatcherData::builtin()
                .map(|d| d.confusables)
                .unwrap_or_default()
        })
    }

    /// Back to the data-file shape.
    pub fn to_table(&self) -> ConfusablesTable {
        ConfusablesTable {
            map: self
                .map
                .iter()
                .map(|(c, v)| (c.to_string(), v.clone()))
                .collect(),
            folds: self.folds.clone(),
        }
    }

    /// How many characters are mapped.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether nothing is mapped.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The skeleton of a label: lowercase, punycode decoded, confusables
    /// mapped, sequences folded.
    pub fn skeleton(&self, label: &str) -> String {
        let decoded = if label.starts_with("xn--") {
            idna::domain_to_unicode(label).0
        } else {
            label.to_string()
        };
        let mut out = String::with_capacity(decoded.len());
        for c in decoded.chars().flat_map(char::to_lowercase) {
            match self.map.get(&c) {
                Some(s) => out.push_str(s),
                None => out.push(c),
            }
        }
        for (from, to) in &self.folds {
            out = out.replace(from.as_str(), to.as_str());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_key_is_refused_and_the_table_round_trips() {
        let mut t = ConfusablesTable::default();
        t.map.insert("ab".into(), "a".into());
        assert!(matches!(
            Confusables::from_table(&t),
            Err(MatchError::BadData(_))
        ));
        let built = Confusables::builtin();
        assert!(built.len() >= 30, "{} entries", built.len());
        let again = Confusables::from_table(&built.to_table()).unwrap();
        assert_eq!(&again, built);
        assert_eq!(Confusables::none().skeleton("Paypa1"), "paypa1");
        assert_eq!(built.skeleton("Paypa1"), "paypal");
    }
}
