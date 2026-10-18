//! TV and console activation (RFC 8628, brief §2.7): what a scanned or
//! pasted sign-in code is, and whether its page is one Lockdown opens.
//!
//! **The allow-list is signed data.** The activation pages ship as one file
//! in the matcher's signed envelope (D66, D260's pattern): format
//! `lockdown-activation-pages`, its own signing prefix, the matcher's
//! publisher keys (D63), a version that only goes up. Nothing in the
//! payload is read before the signature holds.
//!
//! **A code is read as hostile.** [`classify`] takes the text a QR code or
//! the clipboard gave and decides what it is without trusting any of it:
//! an `https` address whose host is exactly a listed page's host and whose
//! path is that page's path is a device code; the code is taken from the
//! query parameter or the path segment the page's entry names, and must be
//! four to sixteen letters and digits (dashes and spaces between groups
//! allowed). Anything else (`http`, another host, a look-alike, a user name
//! in the address, a port, a fragment, a code of the wrong shape) is not a
//! device code. The address Lockdown opens is **rebuilt** from the entry
//! and the code, never the scanned text, so extra parameters or a path
//! trick never reach the browser.
//!
//! **The other two flows.** A `FIDO:/` payload is a passkey cross-device
//! sign-in (the hybrid QR); an `https://s.team/q/…` address is Steam's
//! sign-in QR. Both go through the same proximity gate as a device code
//! ([`crate::ApprovalService::evaluate`]); neither can be finished on the
//! desktop today (the hybrid flow needs the phone's Bluetooth link to the
//! computer showing the QR, and Steam approval is parked under D8), and the
//! shell says so after the gate, never before it.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ciborium::Value;
use lockdown_matcher::signed::{
    DataError, TrustedKeys, decode_exact, sign_envelope, to_canonical_cbor, verify_envelope,
};
use serde::{Deserialize, Serialize};

/// The envelope's `format`.
pub const FORMAT: &str = "lockdown-activation-pages";
/// Domain separation for this file's signatures.
pub const SIGNING_PREFIX: &[u8] = b"lockdown-activation-pages/v1\0";
/// The built-in signed file.
pub const BUILTIN_SIGNED: &[u8] = include_bytes!("../data/activation-pages.signed.cbor");

/// The longest text read as a code: a QR code holds at most a few
/// kilobytes, and no activation address is longer than this.
pub const MAX_TEXT: usize = 2048;

/// One activation page: a service's own page where a TV's code is
/// entered, exactly as Lockdown opens it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationPage {
    /// The service, as people know it ("YouTube").
    pub service: String,
    /// The page's hosts, exactly (`www.youtube.com`, `youtube.com`); the
    /// first is the one Lockdown opens.
    pub hosts: Vec<String>,
    /// The page's path, exactly (`/activate`), compared without a final `/`.
    pub path: String,
    /// The query parameters that may carry the code in a scanned address
    /// (`user_code`, `otc`), in the order tried.
    #[serde(default)]
    pub params: Vec<String>,
    /// The parameter Lockdown puts the code in when it opens the page;
    /// `None` when the page takes no code in its address, so the person
    /// types it, shown large in Lockdown.
    #[serde(default)]
    pub opens_with: Option<String>,
    /// Whether the code may also be the path segment after `path`
    /// (`/activate/ABCD-EFGH`).
    #[serde(default)]
    pub path_code: bool,
}

/// What the publisher signs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivationPayload {
    /// The monotonic data version.
    pub activation_pages: u64,
    /// A date label for people.
    #[serde(default)]
    pub published: String,
    /// The pages.
    #[serde(default)]
    pub pages: Vec<ActivationPage>,
    /// Fields this build does not know, carried through.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ActivationPayload {
    /// What the signature can't check: every host is a plain lower-case
    /// name with a dot, every path starts with `/`, a parameter Lockdown
    /// opens with is one it reads. A file failing this is refused whole.
    fn check(&self) -> Result<(), DataError> {
        let bad = |why: String| Err(DataError::BadPayload(why));
        for p in &self.pages {
            if p.service.trim().is_empty() || p.hosts.is_empty() {
                return bad("an activation page has no service or host".into());
            }
            for h in &p.hosts {
                let plain = h.contains('.')
                    && h.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'
                    });
                if !plain {
                    return bad(format!("{h} isn't a plain host name"));
                }
            }
            if !p.path.starts_with('/') {
                return bad(format!("{}'s path doesn't start with /", p.service));
            }
            if let Some(o) = &p.opens_with
                && !p.params.contains(o)
            {
                return bad(format!(
                    "{} opens with a parameter it doesn't read",
                    p.service
                ));
            }
        }
        Ok(())
    }
}

/// Signs a payload into envelope bytes (the `sign-activation-pages`
/// example and tests; release files are signed by the release service, D63).
pub fn sign(
    payload: &ActivationPayload,
    key: &ed25519_dalek::SigningKey,
    key_id: &str,
) -> Result<Vec<u8>, DataError> {
    sign_envelope(
        FORMAT,
        SIGNING_PREFIX,
        &to_canonical_cbor(payload)?,
        key,
        key_id,
    )
}

/// Verifies envelope bytes; nothing in the payload is read before the
/// signature holds.
pub fn verify(bytes: &[u8], keys: &TrustedKeys) -> Result<(String, ActivationPayload), DataError> {
    let (key_id, payload) = verify_envelope(bytes, FORMAT, SIGNING_PREFIX, keys)?;
    let payload: ActivationPayload = decode_exact(&payload)?;
    payload.check()?;
    Ok((key_id, payload))
}

/// The pages in force.
#[derive(Clone, Debug)]
pub struct ActivationPages {
    /// `activation_pages` of the file.
    pub version: u64,
    /// The key that signed it.
    pub key_id: String,
    pages: Vec<ActivationPage>,
}

impl ActivationPages {
    /// Verifies and loads a file whose version is at least `floor`.
    pub fn load(bytes: &[u8], keys: &TrustedKeys, floor: u64) -> Result<Self, DataError> {
        let (key_id, payload) = verify(bytes, keys)?;
        if payload.activation_pages < floor {
            return Err(DataError::Downgrade {
                offered: payload.activation_pages,
                newest: floor,
            });
        }
        Ok(Self {
            version: payload.activation_pages,
            key_id,
            pages: payload.pages,
        })
    }

    /// The built-in file under the build's keys, once per process.
    pub fn builtin() -> &'static Self {
        static PAGES: OnceLock<ActivationPages> = OnceLock::new();
        PAGES.get_or_init(|| {
            Self::load(BUILTIN_SIGNED, &TrustedKeys::builtin(), 0).unwrap_or(Self {
                version: 0,
                key_id: String::new(),
                pages: Vec::new(),
            })
        })
    }

    /// Every page, for a settings list.
    pub fn pages(&self) -> &[ActivationPage] {
        &self.pages
    }

    /// The page whose host and path are exactly these.
    fn page_for(&self, host: &str, path: &str) -> Option<(&ActivationPage, Option<String>)> {
        let path = path.trim_end_matches('/');
        self.pages.iter().find_map(|p| {
            if !p.hosts.iter().any(|h| h == host) {
                return None;
            }
            let want = p.path.trim_end_matches('/');
            if path.eq_ignore_ascii_case(want) {
                return Some((p, None));
            }
            let tail = path.strip_prefix(want)?.strip_prefix('/')?;
            (p.path_code && !tail.contains('/')).then(|| (p, Some(tail.to_string())))
        })
    }
}

/// A device code read from a scan or a paste, ready to open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceCode {
    /// The service ("YouTube").
    pub service: String,
    /// The page's host as Lockdown opens it: the full domain shown before
    /// any approval (brief §2.7).
    pub host: String,
    /// The code, upper case, as the TV shows it.
    pub code: String,
    /// The address Lockdown opens, rebuilt from the list and the code.
    pub url: String,
    /// Whether that address carries the code; when not, the person types
    /// it into the page.
    pub code_in_url: bool,
}

/// What a scanned or pasted text is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scanned {
    /// A TV or console's activation address on a listed page.
    DeviceCode(DeviceCode),
    /// A passkey cross-device sign-in (`FIDO:/…`).
    PasskeyQr,
    /// Steam's sign-in QR (`https://s.team/q/…`).
    SteamQr,
    /// An address on a page that isn't on the list, named by its host so
    /// the person sees what it was.
    Unlisted {
        /// The host, as written; empty when there was none.
        host: String,
    },
    /// Nothing Lockdown approves.
    Other,
}

impl Scanned {
    /// The approval kind this asks for, when it asks for one.
    pub fn kind(&self) -> Option<crate::ApprovalKind> {
        match self {
            Self::DeviceCode(_) => Some(crate::ApprovalKind::DeviceCode),
            Self::PasskeyQr => Some(crate::ApprovalKind::PasskeyQr),
            Self::SteamQr => Some(crate::ApprovalKind::BridgeQr),
            Self::Unlisted { .. } | Self::Other => None,
        }
    }
}

/// Reads `text` (a QR code's content or what was pasted) as hostile and
/// says what it is, against `pages`.
pub fn classify(text: &str, pages: &ActivationPages) -> Scanned {
    if text.len() > MAX_TEXT {
        return Scanned::Other;
    }
    let text = text.trim();
    if text.len() >= 6 && text[..6].eq_ignore_ascii_case("FIDO:/") {
        return Scanned::PasskeyQr;
    }
    let Ok(url) = url::Url::parse(text) else {
        return Scanned::Other;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if url.scheme() == "https" && host == "s.team" && url.path().starts_with("/q/") {
        return Scanned::SteamQr;
    }
    let plain = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && matches!(url.host(), Some(url::Host::Domain(_)));
    if !plain {
        return if url.scheme() == "https" || url.scheme() == "http" {
            Scanned::Unlisted { host }
        } else {
            Scanned::Other
        };
    }
    let Some((page, from_path)) = pages.page_for(&host, url.path()) else {
        return Scanned::Unlisted { host };
    };
    let from_query = page.params.iter().find_map(|p| {
        url.query_pairs()
            .find(|(k, _)| k == p.as_str())
            .map(|(_, v)| v.into_owned())
    });
    let Some(code) = from_query.or(from_path).and_then(|c| normal_code(&c)) else {
        return Scanned::Unlisted { host };
    };
    let open_host = page.hosts[0].clone();
    let (url, code_in_url) = match &page.opens_with {
        Some(param) => {
            let mut u = url::Url::parse(&format!("https://{open_host}{}", page.path))
                .unwrap_or_else(|_| url.clone());
            u.query_pairs_mut().append_pair(param, &code);
            (u.to_string(), true)
        }
        None => (format!("https://{open_host}{}", page.path), false),
    };
    Scanned::DeviceCode(DeviceCode {
        service: page.service.clone(),
        host: open_host,
        code,
        url,
        code_in_url,
    })
}

/// A code as a TV shows it: four to sixteen ASCII letters and digits, in
/// groups that dashes or spaces may separate, upper-cased; anything else
/// is not a code.
pub fn normal_code(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 40 {
        return None;
    }
    let mut out = String::new();
    let mut letters = 0;
    let mut last_sep = true;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
            letters += 1;
            last_sep = false;
        } else if (c == '-' || c == ' ') && !last_sep {
            out.push('-');
            last_sep = true;
        } else {
            return None;
        }
    }
    if last_sep || !(4..=16).contains(&letters) {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lockdown_matcher::signed::DEV_KEY_ID;

    fn dev_key() -> ed25519_dalek::SigningKey {
        let hex = include_str!("../../matcher/data/dev/publisher.key");
        let seed: [u8; 32] = data_encoding::HEXLOWER
            .decode(hex.trim().as_bytes())
            .unwrap()
            .try_into()
            .unwrap();
        ed25519_dalek::SigningKey::from_bytes(&seed)
    }

    fn pages() -> &'static ActivationPages {
        ActivationPages::builtin()
    }

    fn device(text: &str) -> DeviceCode {
        match classify(text, pages()) {
            Scanned::DeviceCode(d) => d,
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn the_built_in_list_is_signed_current_and_names_the_brief_s_pages() {
        let (key_id, built) = verify(BUILTIN_SIGNED, &TrustedKeys::builtin()).unwrap();
        assert_eq!(key_id, DEV_KEY_ID);
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/activation-pages.json"),
        )
        .unwrap();
        let source: ActivationPayload = serde_json::from_str(&source).unwrap();
        assert_eq!(
            source, built,
            "activation-pages.signed.cbor is stale; run the sign-activation-pages example"
        );
        let services: Vec<&str> = pages().pages().iter().map(|p| p.service.as_str()).collect();
        for s in [
            "Microsoft",
            "YouTube",
            "Google",
            "Epic Games",
            "Discord",
            "Netflix",
            "Disney+",
            "Amazon",
        ] {
            assert!(services.contains(&s), "{s} missing from {services:?}");
        }
    }

    #[test]
    fn a_listed_page_with_its_code_is_a_device_code_and_is_rebuilt() {
        let d = device("https://www.youtube.com/activate?user_code=abcd-efgh&utm=evil");
        assert_eq!(d.service, "YouTube");
        assert_eq!(d.host, "www.youtube.com");
        assert_eq!(d.code, "ABCD-EFGH");
        // Rebuilt: the extra parameter never reaches the browser.
        assert_eq!(
            d.url,
            "https://www.youtube.com/activate?user_code=ABCD-EFGH"
        );
        assert!(d.code_in_url);
        let d = device("https://microsoft.com/link?otc=ABCDEFGH");
        assert_eq!(d.host, "www.microsoft.com");
        // A page that takes no code in its address: the person types it.
        let d = device("https://www.netflix.com/tv8/1234-5678");
        assert_eq!(d.code, "1234-5678");
        assert_eq!(d.url, "https://www.netflix.com/tv8");
        assert!(!d.code_in_url);
    }

    #[test]
    fn anything_else_is_not_a_device_code() {
        for text in [
            // Not https, a look-alike, a sub-domain, a user name, a port, a fragment.
            "http://www.youtube.com/activate?user_code=ABCD-EFGH",
            "https://www.youtube.com.evil.example/activate?user_code=ABCD-EFGH",
            "https://evil.www.youtube.com/activate?user_code=ABCD-EFGH",
            "https://me@www.youtube.com/activate?user_code=ABCD-EFGH",
            "https://www.youtube.com:8443/activate?user_code=ABCD-EFGH",
            "https://www.youtube.com/activate?user_code=ABCD-EFGH#x",
            // Another path, no code, a code of the wrong shape.
            "https://www.youtube.com/watch?user_code=ABCD-EFGH",
            "https://www.youtube.com/activate",
            "https://www.youtube.com/activate?user_code=AB",
            "https://www.youtube.com/activate?user_code=%3Cscript%3E",
            "https://www.youtube.com/activate?user_code=ABCD--EFGH",
        ] {
            assert!(
                matches!(classify(text, pages()), Scanned::Unlisted { .. }),
                "{text}"
            );
        }
        assert_eq!(classify("hello", pages()), Scanned::Other);
        assert_eq!(classify("javascript:alert(1)", pages()), Scanned::Other);
        assert_eq!(classify(&"a".repeat(MAX_TEXT + 1), pages()), Scanned::Other);
        assert_eq!(
            classify("FIDO:/1234567890123456", pages()),
            Scanned::PasskeyQr
        );
        assert_eq!(
            classify("https://s.team/q/1/2345678901234567890", pages()),
            Scanned::SteamQr
        );
        assert_eq!(
            classify("https://example.com/activate?user_code=ABCD", pages()),
            Scanned::Unlisted {
                host: "example.com".into()
            }
        );
    }

    #[test]
    fn codes_are_letters_and_digits_in_groups() {
        assert_eq!(normal_code("abcd efgh").as_deref(), Some("ABCD-EFGH"));
        assert_eq!(normal_code("WDJB-MJHT").as_deref(), Some("WDJB-MJHT"));
        for bad in [
            "",
            "ab",
            "abc-",
            "-abcd",
            "ab cd ef gh ij kl mn op q",
            "ABCD_EFGH",
            "ÄBCD",
        ] {
            assert_eq!(normal_code(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_changed_unknown_downgraded_or_malformed_file_is_refused() {
        let payload = |v| ActivationPayload {
            activation_pages: v,
            published: "test".into(),
            pages: vec![ActivationPage {
                service: "Test".into(),
                hosts: vec!["tv.example".into()],
                path: "/go".into(),
                params: vec!["code".into()],
                opens_with: Some("code".into()),
                path_code: false,
            }],
            extra: BTreeMap::new(),
        };
        let keys = TrustedKeys::dev();
        let bytes = sign(&payload(5), &dev_key(), DEV_KEY_ID).unwrap();
        let loaded = ActivationPages::load(&bytes, &keys, 0).unwrap();
        assert_eq!(loaded.version, 5);
        assert!(matches!(
            classify("https://tv.example/go?code=ABCD1234", &loaded),
            Scanned::DeviceCode(_)
        ));
        assert!(matches!(
            ActivationPages::load(&bytes, &keys, 6),
            Err(DataError::Downgrade { .. })
        ));
        let mut flipped = bytes.clone();
        let n = flipped.len() - 70;
        flipped[n] ^= 1;
        assert!(ActivationPages::load(&flipped, &keys, 0).is_err());
        assert!(ActivationPages::load(&bytes, &TrustedKeys::none(), 0).is_err());
        // Signed by the matcher's own prefix: another file's signature never counts.
        let as_matcher = sign_envelope(
            FORMAT,
            b"lockdown-matcher-data/v1\0",
            &to_canonical_cbor(&payload(5)).unwrap(),
            &dev_key(),
            DEV_KEY_ID,
        )
        .unwrap();
        assert!(ActivationPages::load(&as_matcher, &keys, 0).is_err());
        // A host that isn't plain, or an open parameter it doesn't read.
        let mut odd = payload(1);
        odd.pages[0].hosts = vec!["TV.example".into()];
        assert!(verify(&sign(&odd, &dev_key(), DEV_KEY_ID).unwrap(), &keys).is_err());
        let mut odd = payload(1);
        odd.pages[0].opens_with = Some("other".into());
        assert!(verify(&sign(&odd, &dev_key(), DEV_KEY_ID).unwrap(), &keys).is_err());
    }
}
