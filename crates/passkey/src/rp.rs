//! Who is asking, and for which relying party (WebAuthn Level 3 §5.1.3
//! step 8, §5.1.4 step 8, §5.11 related origins). The caller's address
//! comes from the browser's own record of the tab, never from the page;
//! everything the page names (the RP ID, the related-origins document it
//! points at) is checked against it here.
//!
//! - The caller must be a secure context: `https`, or this machine
//!   (`localhost` and `*.localhost`). An IP address is not a valid domain,
//!   so it has no passkeys.
//! - The RP ID is the caller's host when the page names none; otherwise it
//!   must equal the host or be a registrable-domain suffix of it (HTML's
//!   "is a registrable domain suffix of or is equal to"), never a public
//!   suffix.
//! - Failing that, the caller may be one of the RP's related origins: the
//!   document at `https://<rp id>/.well-known/webauthn`, fetched by the
//!   extension, capped at five labels by the matcher's rule.

use lockdown_matcher::Origin;
use lockdown_matcher::related_origins::{RelatedOriginsDoc, effective_origins};

use crate::{Refusal, RefusalName};

/// The page asking, as the browser reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    /// `https://example.com`, `http://localhost:8080`: the serialised origin
    /// that goes into the client data.
    pub origin: String,
    /// The host, lowercase ASCII.
    pub host: String,
}

fn security(message: impl Into<String>) -> Refusal {
    Refusal::new(RefusalName::Security, message)
}

/// Whether a host names this machine.
pub fn is_local(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

/// The serialised origin of a parsed address: scheme, host, and the port
/// only when it isn't the scheme's default.
pub fn web_origin(o: &Origin) -> String {
    match o.port {
        Some(p) => format!("{}://{}:{p}", o.scheme, o.host),
        None => format!("{}://{}", o.scheme, o.host),
    }
}

impl Caller {
    /// The caller at `page` (a full address), refused unless it is a secure
    /// context with a domain name.
    pub fn of(page: &str) -> Result<Self, Refusal> {
        let o = Origin::parse(page)
            .map_err(|_| security("this page has no web address Lockdown can check"))?;
        if o.host.parse::<std::net::IpAddr>().is_ok() || o.host.starts_with('[') {
            return Err(security(format!(
                "{} is an IP address; passkeys belong to named sites",
                o.host
            )));
        }
        let secure = o.scheme == "https" || (o.scheme == "http" && is_local(&o.host));
        if !secure {
            return Err(security(format!(
                "{} isn't a secure (https) page; passkeys work only on secure pages",
                o.host
            )));
        }
        Ok(Self {
            origin: web_origin(&o),
            host: o.host,
        })
    }
}

/// An RP ID as the page wrote it, normalised to lowercase ASCII, or refused
/// when it isn't a bare domain name.
pub fn normalise_rp_id(text: &str) -> Result<String, Refusal> {
    let t = text.trim().trim_end_matches('.');
    if t.is_empty() || t.contains(['/', ':', '@', '?', '#', ' ']) {
        return Err(security(format!("'{text}' isn't a site name")));
    }
    let o = Origin::parse(&format!("https://{t}/"))
        .map_err(|_| security(format!("'{text}' isn't a site name")))?;
    Ok(o.host)
}

fn public_suffix(host: &str) -> Option<String> {
    psl::suffix(host.as_bytes())
        .and_then(|s| std::str::from_utf8(s.as_bytes()).ok().map(str::to_string))
}

/// HTML's "is a registrable domain suffix of or is equal to": `rp` equals
/// `host`, or `host` ends with `.rp` and `rp` is not a public suffix (nor
/// a part of `host`'s public suffix).
pub fn is_registrable_suffix(rp: &str, host: &str) -> bool {
    if rp.is_empty() {
        return false;
    }
    if rp == host {
        return true;
    }
    if !host.ends_with(&format!(".{rp}")) {
        return false;
    }
    if public_suffix(rp).as_deref() == Some(rp) {
        return false;
    }
    if let Some(suffix) = public_suffix(host)
        && (suffix == rp || suffix.ends_with(&format!(".{rp}")))
    {
        return false;
    }
    true
}

/// The RP ID a request runs under: the page's own host when it names none,
/// else the one it names if the caller may use it, directly or as a related
/// origin from `related` (the document the extension fetched from that
/// RP ID's `/.well-known/webauthn`).
pub fn rp_id_for(
    caller: &Caller,
    requested: Option<&str>,
    related: Option<&RelatedOriginsDoc>,
) -> Result<String, Refusal> {
    let Some(requested) = requested.filter(|r| !r.is_empty()) else {
        return Ok(caller.host.clone());
    };
    let rp = normalise_rp_id(requested)?;
    if is_registrable_suffix(&rp, &caller.host) {
        return Ok(rp);
    }
    if let Some(doc) = related
        && effective_origins(doc).iter().any(|o| *o == caller.origin)
    {
        return Ok(rp);
    }
    Err(security(format!(
        "{} can't use passkeys for {rp}",
        caller.host
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(page: &str) -> Caller {
        Caller::of(page).unwrap()
    }

    #[test]
    fn only_secure_named_pages_may_ask() {
        assert_eq!(
            caller("https://www.example.com/login?x=1").origin,
            "https://www.example.com"
        );
        assert_eq!(
            caller("https://example.com:8443/").origin,
            "https://example.com:8443"
        );
        assert_eq!(
            caller("http://localhost:3000/a").origin,
            "http://localhost:3000"
        );
        assert_eq!(caller("http://shop.localhost/").host, "shop.localhost");
        for bad in [
            "http://example.com/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "http://127.0.0.1:8080/",
            "file:///tmp/x.html",
            "about:blank",
        ] {
            assert_eq!(
                Caller::of(bad).unwrap_err().name,
                RefusalName::Security,
                "{bad}"
            );
        }
    }

    #[test]
    fn the_rp_id_is_the_host_or_a_registrable_suffix_of_it() {
        let c = caller("https://login.example.co.uk/");
        assert_eq!(rp_id_for(&c, None, None).unwrap(), "login.example.co.uk");
        assert_eq!(
            rp_id_for(&c, Some("example.co.uk"), None).unwrap(),
            "example.co.uk"
        );
        assert_eq!(
            rp_id_for(&c, Some("EXAMPLE.co.uk."), None).unwrap(),
            "example.co.uk"
        );
        // A public suffix is never an RP ID.
        assert!(rp_id_for(&c, Some("co.uk"), None).is_err());
        assert!(rp_id_for(&c, Some("uk"), None).is_err());
        // Another site, a sibling, a deeper name.
        assert!(rp_id_for(&c, Some("example.com"), None).is_err());
        assert!(rp_id_for(&c, Some("www.example.co.uk"), None).is_err());
        assert!(rp_id_for(&c, Some("x.login.example.co.uk"), None).is_err());
        // Not a bare name.
        assert!(rp_id_for(&c, Some("https://example.co.uk"), None).is_err());
        assert!(rp_id_for(&c, Some("example.co.uk:443"), None).is_err());
        // A host under a private public suffix can't claim the suffix's owner.
        let gh = caller("https://me.github.io/");
        assert!(rp_id_for(&gh, Some("github.io"), None).is_err());
        assert_eq!(
            rp_id_for(&gh, Some("me.github.io"), None).unwrap(),
            "me.github.io"
        );
        // This machine.
        let local = caller("http://localhost:8080/");
        assert_eq!(
            rp_id_for(&local, Some("localhost"), None).unwrap(),
            "localhost"
        );
    }

    #[test]
    fn a_related_origin_may_use_the_rp_id_within_the_five_label_cap() {
        let doc: RelatedOriginsDoc = serde_json::from_str(
            r#"{"origins": ["https://example.de", "https://brand-a.com", "https://brand-b.com",
                "https://brand-c.com", "https://brand-d.com", "https://brand-e.com"]}"#,
        )
        .unwrap();
        let de = caller("https://example.de/");
        assert_eq!(
            rp_id_for(&de, Some("example.com"), Some(&doc)).unwrap(),
            "example.com"
        );
        assert!(
            rp_id_for(
                &caller("https://brand-d.com/"),
                Some("example.com"),
                Some(&doc)
            )
            .is_ok()
        );
        // The sixth label is past the cap; an origin not listed is refused.
        assert!(
            rp_id_for(
                &caller("https://brand-e.com/"),
                Some("example.com"),
                Some(&doc)
            )
            .is_err()
        );
        assert!(
            rp_id_for(
                &caller("https://www.example.de/"),
                Some("example.com"),
                Some(&doc)
            )
            .is_err()
        );
        assert!(rp_id_for(&de, Some("example.com"), None).is_err());
    }
}
