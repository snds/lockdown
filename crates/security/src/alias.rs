//! Email aliases through the services people already use
//! (docs/plan/23-bridges.md §5): SimpleLogin, addy.io, Firefox Relay,
//! Fastmail masked email, DuckDuckGo Email Protection and Forward Email.
//!
//! Each service makes a new forwarding address on request, authorised by
//! an API key the person pastes once. The key is a crown-jewel secret in
//! the vault (`lockdown-app` holds it, behind the `Use` gate); here it is
//! only ever a header on one request to that service. Nothing here keeps,
//! logs or repeats it, and no error message carries an answer's body.
//!
//! What a service answers is hostile: the address must look like one
//! address (no spaces, controls, angle brackets or second `@`, at most 254
//! bytes) or the answer is refused.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::http::{Fetch, FetchError, Request};

/// The services Lockdown makes aliases with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasService {
    /// SimpleLogin (self-hostable).
    SimpleLogin,
    /// addy.io, formerly AnonAddy (self-hostable).
    AddyIo,
    /// Firefox Relay.
    FirefoxRelay,
    /// Fastmail's masked email, over JMAP.
    Fastmail,
    /// DuckDuckGo Email Protection.
    DuckDuckGo,
    /// Forward Email (needs one of your domains).
    ForwardEmail,
}

impl AliasService {
    /// Every service, in the order a picker lists them.
    pub const ALL: [AliasService; 6] = [
        Self::SimpleLogin,
        Self::AddyIo,
        Self::FirefoxRelay,
        Self::Fastmail,
        Self::DuckDuckGo,
        Self::ForwardEmail,
    ];

    /// The id a shell sends.
    pub fn id(self) -> &'static str {
        match self {
            Self::SimpleLogin => "simple_login",
            Self::AddyIo => "addy_io",
            Self::FirefoxRelay => "firefox_relay",
            Self::Fastmail => "fastmail",
            Self::DuckDuckGo => "duck_duck_go",
            Self::ForwardEmail => "forward_email",
        }
    }

    /// The service a shell named.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Self::SimpleLogin => "SimpleLogin",
            Self::AddyIo => "addy.io",
            Self::FirefoxRelay => "Firefox Relay",
            Self::Fastmail => "Fastmail",
            Self::DuckDuckGo => "DuckDuckGo",
            Self::ForwardEmail => "Forward Email",
        }
    }

    /// The service's own address for its API.
    pub fn default_base(self) -> &'static str {
        match self {
            Self::SimpleLogin => "https://app.simplelogin.io",
            Self::AddyIo => "https://app.addy.io",
            Self::FirefoxRelay => "https://relay.firefox.com",
            Self::Fastmail => "https://api.fastmail.com",
            Self::DuckDuckGo => "https://quack.duckduckgo.com",
            Self::ForwardEmail => "https://api.forwardemail.net",
        }
    }

    /// Whether a person may point it at their own server.
    pub fn self_hostable(self) -> bool {
        matches!(self, Self::SimpleLogin | Self::AddyIo)
    }

    /// Whether it needs one of the person's domains.
    pub fn needs_domain(self) -> bool {
        matches!(self, Self::ForwardEmail)
    }

    /// Whether a domain is optional (addy.io makes aliases on the
    /// person's own domain when named, else on its shared one).
    pub fn takes_domain(self) -> bool {
        matches!(self, Self::ForwardEmail | Self::AddyIo)
    }

    /// Where the person finds the key, in one sentence.
    pub fn key_hint(self) -> &'static str {
        match self {
            Self::SimpleLogin => "In SimpleLogin: API Keys, then Create API Key.",
            Self::AddyIo => "In addy.io: Settings, API Keys, then Create New API Key.",
            Self::FirefoxRelay => "In Firefox Relay: Settings, then copy the API Key.",
            Self::Fastmail => {
                "In Fastmail: Settings, Privacy & Security, API tokens; give the token the Masked Email scope."
            }
            Self::DuckDuckGo => {
                "In DuckDuckGo Email Protection: sign in on duckduckgo.com/email and copy the token from Autofill settings."
            }
            Self::ForwardEmail => {
                "In Forward Email: My Account, Security, then copy the API token."
            }
        }
    }
}

/// Why no alias was made.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AliasError {
    /// The request never got an answer, or the address wasn't allowed.
    #[error("{0}")]
    Network(#[from] FetchError),
    /// The service refused the key.
    #[error("{0} refused the API key; check it in the service's settings")]
    BadKey(&'static str),
    /// Too many requests.
    #[error("{0} says too many aliases were asked for just now; try again in a minute")]
    RateLimited(&'static str),
    /// Any other refusal.
    #[error("{service} answered with an error ({status})")]
    Refused {
        /// Which service.
        service: &'static str,
        /// The HTTP status.
        status: u16,
    },
    /// The answer had no usable address.
    #[error("{0} answered, but not with an address Lockdown could use")]
    BadAnswer(&'static str),
    /// Something the person set up doesn't fit.
    #[error("{0}")]
    Setup(String),
}

/// One alias request.
pub struct AliasRequest<'a> {
    /// Which service.
    pub service: AliasService,
    /// Its API address, when not the service's own (a self-hosted one,
    /// or a test server).
    pub base: Option<&'a str>,
    /// The domain to make it on (Forward Email; optional for addy.io).
    pub domain: Option<&'a str>,
    /// The site it is for (a host name), which the service records as
    /// its note so the person can tell aliases apart.
    pub site: &'a str,
    /// The API key, as the person pasted it.
    pub key: &'a [u8],
}

/// What a service made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Alias {
    /// The new address.
    pub address: String,
    /// Which service made it.
    pub service: AliasService,
}

/// Whether `text` is one plain email address Lockdown will put into a
/// field: one `@`, a dotted domain, no spaces, controls or markup, at
/// most 254 bytes.
pub fn plain_address(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    text.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && text.chars().all(|c| {
            c.is_ascii_graphic()
                && !matches!(
                    c,
                    '<' | '>' | '"' | '(' | ')' | ',' | ';' | '\\' | '[' | ']'
                )
        })
}

fn site_label(site: &str) -> String {
    site.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .take(253)
        .collect()
}

fn domain_label(domain: &str) -> Result<String, AliasError> {
    let d = domain.trim().to_ascii_lowercase();
    if d.is_empty()
        || d.len() > 253
        || !d.contains('.')
        || !d
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return Err(AliasError::Setup(format!("{domain} isn't a domain name")));
    }
    Ok(d)
}

fn base_of(req: &AliasRequest<'_>) -> Result<String, AliasError> {
    let base = req.base.unwrap_or(req.service.default_base()).trim();
    let base = base.trim_end_matches('/');
    crate::http::allowed_address(base)?;
    Ok(base.to_string())
}

fn key_text(key: &[u8]) -> Result<Zeroizing<String>, AliasError> {
    let text = std::str::from_utf8(key)
        .map_err(|_| AliasError::Setup("the API key has characters that aren't text".into()))?
        .trim();
    if text.is_empty() || text.len() > 512 || !text.chars().all(|c| c.is_ascii_graphic()) {
        return Err(AliasError::Setup(
            "the API key should be one line of letters, digits and symbols".into(),
        ));
    }
    Ok(Zeroizing::new(text.to_string()))
}

fn check_status(service: AliasService, status: u16) -> Result<(), AliasError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(AliasError::BadKey(service.name())),
        429 => Err(AliasError::RateLimited(service.name())),
        status => Err(AliasError::Refused {
            service: service.name(),
            status,
        }),
    }
}

fn json_body(service: AliasService, body: &[u8]) -> Result<Value, AliasError> {
    serde_json::from_slice(body).map_err(|_| AliasError::BadAnswer(service.name()))
}

fn address_from(service: AliasService, value: Option<&Value>) -> Result<Alias, AliasError> {
    let address = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|a| plain_address(a))
        .ok_or(AliasError::BadAnswer(service.name()))?;
    Ok(Alias {
        address: address.to_ascii_lowercase(),
        service,
    })
}

fn send(
    fetch: &dyn Fetch,
    service: AliasService,
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Value, AliasError> {
    let body = serde_json::to_vec(body).map_err(|_| AliasError::BadAnswer(service.name()))?;
    let answer = fetch.send(&Request {
        method,
        url,
        headers,
        body: &body,
    })?;
    check_status(service, answer.status)?;
    json_body(service, &answer.body)
}

/// Makes a new alias with the service named in `req`.
pub fn create_alias(fetch: &dyn Fetch, req: &AliasRequest<'_>) -> Result<Alias, AliasError> {
    let service = req.service;
    let base = base_of(req)?;
    let key = key_text(req.key)?;
    let site = site_label(req.site);
    let note = if site.is_empty() {
        "Made by Lockdown".to_string()
    } else {
        format!("Made by Lockdown for {site}")
    };
    let json_type = ("Content-Type", "application/json");
    let accept = ("Accept", "application/json");
    match service {
        AliasService::SimpleLogin => {
            let url = format!(
                "{base}/api/alias/random/new{}",
                if site.is_empty() {
                    String::new()
                } else {
                    format!("?hostname={site}")
                }
            );
            let v = send(
                fetch,
                service,
                "POST",
                &url,
                &[json_type, accept, ("Authentication", key.as_str())],
                &json!({ "note": note }),
            )?;
            address_from(service, v.get("alias"))
        }
        AliasService::AddyIo => {
            let bearer = Zeroizing::new(format!("Bearer {}", key.as_str()));
            let mut body = json!({ "description": note, "format": "random_characters" });
            if let Some(d) = req.domain.filter(|d| !d.trim().is_empty()) {
                body["domain"] = json!(domain_label(d)?);
            } else {
                body["domain"] = json!("anonaddy.me");
            }
            let v = send(
                fetch,
                service,
                "POST",
                &format!("{base}/api/v1/aliases"),
                &[
                    json_type,
                    accept,
                    ("X-Requested-With", "XMLHttpRequest"),
                    ("Authorization", bearer.as_str()),
                ],
                &body,
            )?;
            address_from(service, v.get("data").and_then(|d| d.get("email")))
        }
        AliasService::FirefoxRelay => {
            let token = Zeroizing::new(format!("Token {}", key.as_str()));
            let v = send(
                fetch,
                service,
                "POST",
                &format!("{base}/api/v1/relayaddresses/"),
                &[json_type, accept, ("Authorization", token.as_str())],
                &json!({ "enabled": true, "description": note, "generated_for": site, "used_on": site }),
            )?;
            address_from(service, v.get("full_address"))
        }
        AliasService::DuckDuckGo => {
            let bearer = Zeroizing::new(format!("Bearer {}", key.as_str()));
            let v = send(
                fetch,
                service,
                "POST",
                &format!("{base}/api/email/addresses"),
                &[json_type, accept, ("Authorization", bearer.as_str())],
                &json!({}),
            )?;
            let local = v
                .get("address")
                .and_then(Value::as_str)
                .filter(|a| !a.contains('@'))
                .map(|a| format!("{a}@duck.com"));
            address_from(service, local.as_ref().map(|a| json!(a)).as_ref())
        }
        AliasService::ForwardEmail => {
            let domain = req.domain.filter(|d| !d.trim().is_empty()).ok_or_else(|| {
                AliasError::Setup("Forward Email needs one of your domains".into())
            })?;
            let domain = domain_label(domain)?;
            let basic = Zeroizing::new(format!(
                "Basic {}",
                data_encoding::BASE64.encode(format!("{}:", key.as_str()).as_bytes())
            ));
            let v = send(
                fetch,
                service,
                "POST",
                &format!("{base}/v1/domains/{domain}/aliases"),
                &[json_type, accept, ("Authorization", basic.as_str())],
                &json!({ "labels": site, "description": note, "is_enabled": true }),
            )?;
            let name = v.get("name").and_then(Value::as_str);
            let on = v
                .get("domain")
                .and_then(|d| d.get("name"))
                .and_then(Value::as_str);
            match (name, on) {
                (Some(n), Some(d)) if d.eq_ignore_ascii_case(&domain) => {
                    address_from(service, Some(&json!(format!("{n}@{d}"))))
                }
                _ => Err(AliasError::BadAnswer(service.name())),
            }
        }
        AliasService::Fastmail => {
            let bearer = Zeroizing::new(format!("Bearer {}", key.as_str()));
            let auth = [accept, ("Authorization", bearer.as_str())];
            let session = fetch.get(&format!("{base}/jmap/session"), &auth)?;
            check_status(service, session.status)?;
            let session = json_body(service, &session.body)?;
            const MASKED: &str = "https://www.fastmail.com/dev/maskedemail";
            let account = session
                .get("primaryAccounts")
                .and_then(|p| p.get(MASKED))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AliasError::Setup(
                        "this Fastmail token can't make masked email; give it the Masked Email scope"
                            .into(),
                    )
                })?
                .to_string();
            // The API address comes from the session, which is hostile:
            // it must be on the same host as the session itself.
            let api = session
                .get("apiUrl")
                .and_then(Value::as_str)
                .filter(|u| same_host(u, &base))
                .ok_or(AliasError::BadAnswer(service.name()))?
                .to_string();
            let for_domain = if site.is_empty() {
                String::new()
            } else {
                format!("https://{site}")
            };
            let v = send(
                fetch,
                service,
                "POST",
                &api,
                &[json_type, accept, ("Authorization", bearer.as_str())],
                &json!({
                    "using": ["urn:ietf:params:jmap:core", MASKED],
                    "methodCalls": [[
                        "MaskedEmail/set",
                        { "accountId": account, "create": { "lockdown": {
                            "state": "enabled", "description": note, "forDomain": for_domain
                        } } },
                        "0"
                    ]]
                }),
            )?;
            let created = v
                .get("methodResponses")
                .and_then(|r| r.get(0))
                .and_then(|r| r.get(1))
                .and_then(|r| r.get("created"))
                .and_then(|c| c.get("lockdown"))
                .and_then(|c| c.get("email"));
            address_from(service, created)
        }
    }
}

/// Whether `url` is on the same scheme, host and port as `base`.
fn same_host(url: &str, base: &str) -> bool {
    match (url::Url::parse(url), url::Url::parse(base)) {
        (Ok(a), Ok(b)) => {
            a.scheme() == b.scheme()
                && a.host_str() == b.host_str()
                && a.port_or_known_default() == b.port_or_known_default()
        }
        _ => false,
    }
}

/// The alias services' known shared domains, so a login whose user name
/// is already an alias can be recognised (the monitored set, §9).
pub const ALIAS_DOMAINS: &[(&str, AliasService)] = &[
    ("simplelogin.com", AliasService::SimpleLogin),
    ("simplelogin.fr", AliasService::SimpleLogin),
    ("aleeas.com", AliasService::SimpleLogin),
    ("slmail.me", AliasService::SimpleLogin),
    ("silomails.com", AliasService::SimpleLogin),
    ("anonaddy.me", AliasService::AddyIo),
    ("anonaddy.com", AliasService::AddyIo),
    ("addy.io", AliasService::AddyIo),
    ("mozmail.com", AliasService::FirefoxRelay),
    ("duck.com", AliasService::DuckDuckGo),
];

/// The service whose shared domain `address` is on, if any.
pub fn service_of_address(address: &str) -> Option<AliasService> {
    let (_, domain) = address.rsplit_once('@')?;
    let domain = domain.to_ascii_lowercase();
    ALIAS_DOMAINS
        .iter()
        .find(|(d, _)| domain == *d || domain.ends_with(&format!(".{d}")))
        .map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_every_service_says_where_its_key_is() {
        for s in AliasService::ALL {
            assert_eq!(AliasService::from_id(s.id()), Some(s));
            assert!(s.default_base().starts_with("https://"));
            assert!(!s.key_hint().is_empty());
        }
        assert_eq!(AliasService::from_id("nope"), None);
    }

    #[test]
    fn only_one_plain_address_is_accepted() {
        for good in ["a1b2@simplelogin.com", "x.y+z@anonaddy.me", "abc@duck.com"] {
            assert!(plain_address(good), "{good}");
        }
        for bad in [
            "",
            "no-at-sign",
            "@duck.com",
            "a@b",
            "a@b@c.com",
            "a b@duck.com",
            "<script>@x.com",
            "a@duck.com\nBcc: x@y.com",
            "a@.com",
            "a@com.",
            &format!("{}@x.com", "a".repeat(65)),
        ] {
            assert!(!plain_address(bad), "{bad:?}");
        }
        assert_eq!(
            service_of_address("Q@Mozmail.com"),
            Some(AliasService::FirefoxRelay)
        );
        assert_eq!(
            service_of_address("q@sub.simplelogin.com"),
            Some(AliasService::SimpleLogin)
        );
        assert_eq!(service_of_address("q@example.com"), None);
    }

    #[test]
    fn a_key_that_isnt_one_line_of_text_is_refused_before_any_call() {
        struct Never;
        impl Fetch for Never {
            fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<crate::Fetched, FetchError> {
                panic!("no call expected")
            }
            fn send(&self, _: &Request<'_>) -> Result<crate::Fetched, FetchError> {
                panic!("no call expected")
            }
        }
        for key in [&b""[..], b"two\nlines", b"\xff\xfe", b"has space"] {
            let r = create_alias(
                &Never,
                &AliasRequest {
                    service: AliasService::DuckDuckGo,
                    base: None,
                    domain: None,
                    site: "example.com",
                    key,
                },
            );
            assert!(matches!(r, Err(AliasError::Setup(_))), "{key:?}");
        }
        let r = create_alias(
            &Never,
            &AliasRequest {
                service: AliasService::ForwardEmail,
                base: None,
                domain: None,
                site: "example.com",
                key: b"k",
            },
        );
        assert!(matches!(r, Err(AliasError::Setup(_))));
        let r = create_alias(
            &Never,
            &AliasRequest {
                service: AliasService::SimpleLogin,
                base: Some("http://alias.example"),
                domain: None,
                site: "example.com",
                key: b"k",
            },
        );
        assert!(matches!(
            r,
            Err(AliasError::Network(FetchError::BadAddress(_)))
        ));
    }
}
