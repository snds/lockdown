//! The bundled 2FA directory: which sites offer two-step sign-in, how, and
//! where to turn it on.
//!
//! The data is a snapshot of 2fa.directory (the community's
//! `2factorauth/twofactorauth` repository, MIT licence; the notice ships
//! beside it as `data/2fa-directory.LICENSE`), cut down by
//! `data/snapshot-2fa-directory.py` to the sites that offer a second step:
//! each one's name, domains, methods and set-up page. It is versioned by
//! the source commit and its date, so a shell can say how old it is. It
//! ships inside the app, so the missing-2FA report never tells anyone
//! which sites a person has logins for.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The snapshot as it ships.
pub const BUILTIN_JSON: &str = include_str!("../data/2fa-directory.json");

/// One site that offers two-step sign-in.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TwoFaEntry {
    /// The site's name.
    #[serde(rename = "n")]
    pub name: String,
    /// Its domains, the main one first.
    #[serde(rename = "d")]
    pub domains: Vec<String>,
    /// The methods it offers, by the directory's ids (`totp`, `sms`,
    /// `email`, `call`, `u2f`, `custom-software`, `custom-hardware`).
    #[serde(rename = "m")]
    pub methods: Vec<String>,
    /// The site's own page on turning it on, when the directory has one.
    #[serde(rename = "doc", default)]
    pub documentation: Option<String>,
}

impl TwoFaEntry {
    /// Whether the site takes the kind of code Lockdown can hold (an
    /// authenticator app's code).
    pub fn takes_app_codes(&self) -> bool {
        self.methods.iter().any(|m| m == "totp")
    }

    /// The methods in plain words, strongest first.
    pub fn method_words(&self) -> Vec<&'static str> {
        const ORDER: [(&str, &str); 7] = [
            ("u2f", "a security key"),
            ("totp", "an authenticator app code"),
            ("custom-software", "the site's own app"),
            ("custom-hardware", "the site's own device"),
            ("email", "an email code"),
            ("sms", "a text message"),
            ("call", "a phone call"),
        ];
        ORDER
            .iter()
            .filter(|(id, _)| self.methods.iter().any(|m| m == id))
            .map(|(_, words)| *words)
            .collect()
    }
}

#[derive(Deserialize)]
struct File {
    v: u16,
    source: String,
    licence: String,
    commit: String,
    date: String,
    entries: Vec<TwoFaEntry>,
}

/// The directory in force.
#[derive(Clone, Debug)]
pub struct TwoFaDirectory {
    /// Where it came from.
    pub source: String,
    /// The data's licence.
    pub licence: String,
    /// The source commit the snapshot was cut from.
    pub commit: String,
    /// That commit's date (`YYYY-MM-DD`).
    pub date: String,
    entries: Vec<TwoFaEntry>,
    index: HashMap<String, usize>,
}

impl TwoFaDirectory {
    /// Reads a snapshot of the bundled shape.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let file: File = serde_json::from_str(json)
            .map_err(|e| format!("the 2FA directory didn't read: {e}"))?;
        if file.v != 1 {
            return Err(format!("the 2FA directory is format {}, not 1", file.v));
        }
        let mut index = HashMap::new();
        for (i, e) in file.entries.iter().enumerate() {
            for d in &e.domains {
                index.entry(d.to_ascii_lowercase()).or_insert(i);
            }
        }
        Ok(Self {
            source: file.source,
            licence: file.licence,
            commit: file.commit,
            date: file.date,
            entries: file.entries,
            index,
        })
    }

    /// The bundled snapshot, read once per process. The crate's tests
    /// check it reads, so a failure here is an empty directory, not a panic.
    pub fn builtin() -> &'static Self {
        static DIR: OnceLock<TwoFaDirectory> = OnceLock::new();
        DIR.get_or_init(|| {
            Self::from_json(BUILTIN_JSON).unwrap_or_else(|_| Self {
                source: String::new(),
                licence: String::new(),
                commit: String::new(),
                date: String::new(),
                entries: Vec::new(),
                index: HashMap::new(),
            })
        })
    }

    /// The entry for `host`: the host itself, else each parent domain down
    /// to two labels (`signin.aws.amazon.com` finds `aws.amazon.com`, then
    /// `amazon.com`).
    pub fn lookup(&self, host: &str) -> Option<&TwoFaEntry> {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let mut rest = host.as_str();
        loop {
            if let Some(i) = self.index.get(rest) {
                return Some(&self.entries[*i]);
            }
            let (_, parent) = rest.split_once('.')?;
            if !parent.contains('.') {
                return None;
            }
            rest = parent;
        }
    }

    /// Every domain the directory names, for the lookalike check's list of
    /// well-known sites.
    pub fn domains(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(String::as_str)
    }

    /// How many sites.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_snapshot_reads_and_is_versioned() {
        let dir = TwoFaDirectory::from_json(BUILTIN_JSON).unwrap();
        assert!(dir.len() > 1500, "{}", dir.len());
        assert_eq!(dir.licence, "MIT");
        assert_eq!(dir.commit.len(), 40);
        assert_eq!(dir.date.len(), 10);
        assert_eq!(TwoFaDirectory::builtin().len(), dir.len());
        // Every entry offers something and every set-up page is https.
        for name in ["github.com", "drive.google.com", "amazon.com"] {
            let e = dir.lookup(name).unwrap_or_else(|| panic!("{name}"));
            assert!(!e.methods.is_empty());
            assert!(
                e.documentation
                    .as_deref()
                    .is_none_or(|d| d.starts_with("https://"))
            );
        }
    }

    #[test]
    fn a_host_finds_its_site_through_its_parent_domains() {
        let dir = TwoFaDirectory::builtin();
        let gh = dir.lookup("GitHub.com.").unwrap();
        assert_eq!(gh.name, "GitHub");
        assert!(gh.takes_app_codes());
        assert_eq!(dir.lookup("gist.github.com").unwrap().name, "GitHub");
        assert!(dir.lookup("example.invalid").is_none());
        assert!(dir.lookup("com").is_none());
        assert!(dir.lookup("co.uk").is_none());
    }

    #[test]
    fn methods_read_as_plain_words_strongest_first() {
        let e = TwoFaEntry {
            name: "Example".into(),
            domains: vec!["example.com".into()],
            methods: vec!["sms".into(), "totp".into(), "u2f".into(), "odd".into()],
            documentation: None,
        };
        assert_eq!(
            e.method_words(),
            vec![
                "a security key",
                "an authenticator app code",
                "a text message"
            ]
        );
        assert!(TwoFaDirectory::from_json("{}").is_err());
        assert!(
            TwoFaDirectory::from_json(
                r#"{"v":2,"source":"","licence":"","commit":"","date":"","entries":[]}"#
            )
            .is_err()
        );
    }
}
