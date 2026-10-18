//! "Change password": the page where a site lets you change it.
//!
//! The W3C's well-known URL for changing passwords is
//! `https://<host>/.well-known/change-password`; a site that supports it
//! answers there (usually with a redirect to its own page). Before offering
//! it, Lockdown checks the site really supports it, the way the
//! specification suggests: a site that answers 200 for a well-known path
//! that cannot exist answers 200 for everything, so its answer proves
//! nothing. When the check fails, can't be made, or the person turned it
//! off, the fallback is the site's front page.
//!
//! The check is two GETs to the site itself, with no cookies and no
//! account data: the same request any browser makes on the way in. It is
//! made only when the person asks to change a password.

use lockdown_matcher::Origin;

use crate::http::Fetch;

/// The path the specification says no site serves, to see whether a site
/// answers 200 for anything.
pub const SHOULD_NOT_EXIST: &str =
    "/.well-known/resource-that-should-not-exist-whose-status-code-should-not-be-200";

/// The change-password path.
pub const CHANGE_PASSWORD: &str = "/.well-known/change-password";

/// Which page the link is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// The site's change-password page, through the well-known address.
    WellKnown,
    /// The site's front page: it has no well-known page, or it couldn't be checked.
    SiteRoot,
}

/// A link to offer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ChangeLink {
    /// The address to open.
    pub url: String,
    /// Which page it is.
    pub kind: LinkKind,
    /// Whether the site was asked (false when the check is off or failed).
    pub checked: bool,
}

/// `scheme://host[:port]` of a saved web address, for http and https only.
fn base_of(site: &str) -> Option<String> {
    let origin = Origin::parse(site).ok()?;
    if origin.scheme != "https" && origin.scheme != "http" {
        return None;
    }
    Some(match origin.port {
        Some(p) => format!("{}://{}:{p}", origin.scheme, origin.host),
        None => format!("{}://{}", origin.scheme, origin.host),
    })
}

/// The site's front page.
pub fn site_root(site: &str) -> Option<String> {
    base_of(site).map(|b| format!("{b}/"))
}

/// The site's well-known change-password address.
pub fn well_known(site: &str) -> Option<String> {
    base_of(site).map(|b| format!("{b}{CHANGE_PASSWORD}"))
}

/// The link for `site`. With `fetch` the site is checked first; without
/// one (the check is off) the well-known address is offered unchecked,
/// since a browser shows the site's own "not found" page at worst.
pub fn resolve(site: &str, fetch: Option<&dyn Fetch>) -> Option<ChangeLink> {
    let base = base_of(site)?;
    let root = format!("{base}/");
    let known = format!("{base}{CHANGE_PASSWORD}");
    let Some(fetch) = fetch else {
        return Some(ChangeLink {
            url: known,
            kind: LinkKind::WellKnown,
            checked: false,
        });
    };
    let fallback = |checked| ChangeLink {
        url: root.clone(),
        kind: LinkKind::SiteRoot,
        checked,
    };
    match fetch.get(&format!("{base}{SHOULD_NOT_EXIST}"), &[]) {
        Ok(r) if (200..300).contains(&r.status) => return Some(fallback(true)),
        Ok(_) => {}
        Err(_) => return Some(fallback(false)),
    }
    Some(match fetch.get(&known, &[]) {
        Ok(r) if (200..400).contains(&r.status) => ChangeLink {
            url: known,
            kind: LinkKind::WellKnown,
            checked: true,
        },
        Ok(_) => fallback(true),
        Err(_) => fallback(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{FetchError, Fetched};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct Sites {
        answers: HashMap<String, u16>,
        asked: Mutex<Vec<String>>,
    }

    impl Fetch for Sites {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Fetched, FetchError> {
            assert!(headers.is_empty(), "the probe sends no headers of its own");
            self.asked.lock().unwrap().push(url.to_string());
            match self.answers.get(url) {
                Some(status) => Ok(Fetched {
                    status: *status,
                    body: Vec::new(),
                }),
                None => Err(FetchError::Network {
                    host: "x".into(),
                    why: "down".into(),
                }),
            }
        }
    }

    fn sites(answers: &[(&str, u16)]) -> Sites {
        Sites {
            answers: answers.iter().map(|(u, s)| (u.to_string(), *s)).collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn a_site_that_supports_it_gets_its_well_known_page() {
        let s = sites(&[
            (
                "https://example.com/.well-known/resource-that-should-not-exist-whose-status-code-should-not-be-200",
                404,
            ),
            ("https://example.com/.well-known/change-password", 302),
        ]);
        let link = resolve("https://example.com/login?next=/", Some(&s)).unwrap();
        assert_eq!(link.url, "https://example.com/.well-known/change-password");
        assert_eq!(link.kind, LinkKind::WellKnown);
        assert!(link.checked);
        // Only the site's own two addresses were asked, nothing of the saved path.
        let asked = s.asked.lock().unwrap();
        assert_eq!(asked.len(), 2);
        assert!(
            asked
                .iter()
                .all(|u| u.starts_with("https://example.com/.well-known/"))
        );
    }

    #[test]
    fn a_site_without_it_or_that_answers_everything_falls_back_to_its_front_page() {
        let none = sites(&[
            (
                "https://a.example/.well-known/resource-that-should-not-exist-whose-status-code-should-not-be-200",
                404,
            ),
            ("https://a.example/.well-known/change-password", 404),
        ]);
        let link = resolve("a.example", Some(&none)).unwrap();
        assert_eq!(
            (link.url.as_str(), link.kind),
            ("https://a.example/", LinkKind::SiteRoot)
        );
        let liar = sites(&[(
            "https://b.example:8443/.well-known/resource-that-should-not-exist-whose-status-code-should-not-be-200",
            200,
        )]);
        let link = resolve("https://b.example:8443/x", Some(&liar)).unwrap();
        assert_eq!(link.url, "https://b.example:8443/");
        assert_eq!(liar.asked.lock().unwrap().len(), 1);
        let down = sites(&[]);
        let link = resolve("https://c.example", Some(&down)).unwrap();
        assert_eq!((link.kind, link.checked), (LinkKind::SiteRoot, false));
    }

    #[test]
    fn without_the_check_the_well_known_page_is_offered_unchecked() {
        let link = resolve("http://localhost:3000/app", None).unwrap();
        assert_eq!(
            link.url,
            "http://localhost:3000/.well-known/change-password"
        );
        assert!(!link.checked);
        assert!(resolve("mailto:me@example.com", None).is_none());
        assert!(resolve("ftp://example.com", None).is_none());
        assert_eq!(
            site_root("example.com").as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(
            well_known("https://Example.COM/x").as_deref(),
            Some("https://example.com/.well-known/change-password")
        );
    }
}
