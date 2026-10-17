//! Lookalike detection (02-model §10: "lookalike hosts get a hard stop").
//!
//! Three checks, run against the registrable label of a host the person
//! has a login for:
//!
//! 1. **Confusables.** Both labels reduce to the same *skeleton* after
//!    mapping lookalike characters (Cyrillic `а` → `a`, `1` → `l`, `rn` → `m`,
//!    …) but are not the same string. Internationalised labels are decoded
//!    from punycode first. The table is data ([`Confusables`]) and ships in
//!    the signed matcher data file.
//! 2. **Typo-squats.** Labels of six or more characters at edit distance one.
//! 3. **Decoys.** The trusted label appears as a whole hyphen-separated part
//!    (`paypal-secure`), or the trusted registrable domain is a leading
//!    sub-domain of the host (`paypal.com.evil.net`).
//!
//! The `_with` functions take the table in force; the plain ones use the
//! built-in table.

pub use crate::confusables::Confusables;

/// The skeleton of a label under the built-in table.
pub fn skeleton(label: &str) -> String {
    Confusables::builtin().skeleton(label)
}

/// Levenshtein distance, for short labels.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Whether `candidate` (a registrable label) imitates `trusted`, under `table`.
pub fn label_imitates_with(table: &Confusables, candidate: &str, trusted: &str) -> bool {
    let (c, t) = (candidate.to_ascii_lowercase(), trusted.to_ascii_lowercase());
    if c == t || t.is_empty() {
        return false;
    }
    if table.skeleton(&c) == table.skeleton(&t) {
        return true;
    }
    if t.chars().count() >= 6 && edit_distance(&c, &t) == 1 {
        return true;
    }
    c.split('-').any(|part| part == t)
}

/// [`label_imitates_with`] under the built-in table.
pub fn label_imitates(candidate: &str, trusted: &str) -> bool {
    label_imitates_with(Confusables::builtin(), candidate, trusted)
}

/// The registrable domain and its left-most label for a host.
fn parts(host: &str) -> Option<(String, String)> {
    let domain = psl::domain(host.as_bytes())?;
    let domain = std::str::from_utf8(domain.as_bytes()).ok()?.to_string();
    let label = domain.split('.').next()?.to_string();
    Some((domain, label))
}

/// Whether `candidate_host` imitates `trusted_host`, under `table`: a label
/// lookalike, or the trusted registrable domain used as a leading sub-domain.
pub fn host_imitates_with(table: &Confusables, candidate_host: &str, trusted_host: &str) -> bool {
    let Some((trusted_domain, trusted_label)) = parts(trusted_host) else {
        return false;
    };
    if candidate_host.starts_with(&format!("{trusted_domain}."))
        || candidate_host.contains(&format!(".{trusted_domain}."))
    {
        return true;
    }
    let Some((candidate_domain, candidate_label)) = parts(candidate_host) else {
        return false;
    };
    if candidate_domain == trusted_domain {
        return false;
    }
    label_imitates_with(table, &candidate_label, &trusted_label)
}

/// [`host_imitates_with`] under the built-in table.
pub fn host_imitates(candidate_host: &str, trusted_host: &str) -> bool {
    host_imitates_with(Confusables::builtin(), candidate_host, trusted_host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeletons_fold_confusables() {
        assert_eq!(skeleton("pаypal"), "paypal"); // Cyrillic а
        assert_eq!(skeleton("paypa1"), "paypal");
        assert_eq!(skeleton("g00gle"), "google");
        assert_eq!(skeleton("arnazon"), "amazon");
        assert_eq!(skeleton("xn--pypal-4ve"), "paypal");
    }

    #[test]
    fn labels_and_hosts() {
        assert!(label_imitates("paypa1", "paypal"));
        assert!(label_imitates("paypai", "paypal"));
        assert!(label_imitates("paypal-secure", "paypal"));
        assert!(!label_imitates("paypal", "paypal"));
        assert!(!label_imitates("example", "paypal"));
        // Short labels: no edit-distance guess (box vs fox are both real).
        assert!(!label_imitates("fox", "box"));
        assert!(host_imitates("paypal.com.evil.net", "www.paypal.com"));
        assert!(host_imitates("login.paypal.com.evil.net", "www.paypal.com"));
        assert!(!host_imitates("shop.paypal.com", "www.paypal.com"));
        assert!(!host_imitates("127.0.0.1", "www.paypal.com"));
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn an_empty_table_still_catches_typos_and_decoys_but_not_homoglyphs() {
        let none = Confusables::none();
        // `paypa1` is at edit distance one from `paypal`, so the typo rule
        // still fires; the homoglyph `pаypal` (Cyrillic а) does not.
        assert!(label_imitates_with(&none, "paypa1", "paypal"));
        assert!(!label_imitates_with(&none, "xn--pypal-4ve", "paypal"));
        assert!(label_imitates_with(&none, "paypal-secure", "paypal"));
    }
}
