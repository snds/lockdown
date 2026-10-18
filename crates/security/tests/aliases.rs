//! The six alias services, each against a local stand-in that answers the
//! way the real API does (the tests never call the internet): what goes
//! over the wire, read back from the server's side, and what comes back.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lockdown_security::alias::{AliasError, AliasRequest, AliasService, create_alias};
use lockdown_security::http::HttpFetch;

/// One request as the stand-in saw it.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

type Log = Arc<Mutex<Vec<Seen>>>;
type Route = Box<dyn Fn(&Seen, &str) -> (u16, String) + Send>;

/// Serves `route` on a free local port; `route` gets the request and the
/// server's own base address.
fn serve(route: Route) -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log: Log = Arc::default();
    let seen = log.clone();
    let me = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((n, v)) = h.trim().split_once(':') {
                    headers.push((n.trim().to_string(), v.trim().to_string()));
                }
            }
            let len: usize = headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0; len];
            let _ = reader.read_exact(&mut body);
            let mut words = line.split_whitespace();
            let req = Seen {
                method: words.next().unwrap_or("").into(),
                path: words.next().unwrap_or("/").into(),
                headers,
                body: String::from_utf8_lossy(&body).into(),
            };
            seen.lock().unwrap().push(req.clone());
            let (status, body) = route(&req, &me);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, log)
}

fn make(
    service: AliasService,
    base: &str,
    domain: Option<&str>,
    key: &[u8],
) -> Result<String, AliasError> {
    let fetch = HttpFetch::new(Duration::from_secs(5)).unwrap();
    create_alias(
        &fetch,
        &AliasRequest {
            service,
            base: Some(base),
            domain,
            site: "shop.example",
            key,
        },
    )
    .map(|a| a.address)
}

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap()
}

#[test]
fn simplelogin_takes_the_key_in_its_own_header_and_the_site_as_hostname() {
    let (base, log) = serve(Box::new(|req, _| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/alias/random/new?hostname=shop.example");
        (
            201,
            r#"{"alias":"Shop.Pear123@SimpleLogin.com","id":5}"#.into(),
        )
    }));
    let address = make(AliasService::SimpleLogin, &base, None, b"sl-key-1").unwrap();
    assert_eq!(address, "shop.pear123@simplelogin.com");
    let seen = log.lock().unwrap();
    assert_eq!(seen[0].header("Authentication"), Some("sl-key-1"));
    assert_eq!(seen[0].header("Authorization"), None);
    assert!(
        json(&seen[0].body)["note"]
            .as_str()
            .unwrap()
            .contains("shop.example")
    );
}

#[test]
fn addy_uses_a_bearer_token_and_the_persons_domain_when_named() {
    let (base, log) = serve(Box::new(|req, _| {
        assert_eq!(
            (req.method.as_str(), req.path.as_str()),
            ("POST", "/api/v1/aliases")
        );
        (
            201,
            r#"{"data":{"id":"x","email":"q7w8@mine.example"}}"#.into(),
        )
    }));
    let address = make(
        AliasService::AddyIo,
        &base,
        Some("Mine.Example"),
        b"addy-token",
    )
    .unwrap();
    assert_eq!(address, "q7w8@mine.example");
    let seen = log.lock().unwrap();
    assert_eq!(seen[0].header("Authorization"), Some("Bearer addy-token"));
    assert_eq!(seen[0].header("X-Requested-With"), Some("XMLHttpRequest"));
    let body = json(&seen[0].body);
    assert_eq!(body["domain"], "mine.example");
    assert_eq!(body["format"], "random_characters");
}

#[test]
fn firefox_relay_uses_its_token_scheme() {
    let (base, log) = serve(Box::new(|req, _| {
        assert_eq!(req.path, "/api/v1/relayaddresses/");
        (
            201,
            r#"{"id":1,"full_address":"abc123@mozmail.com"}"#.into(),
        )
    }));
    assert_eq!(
        make(AliasService::FirefoxRelay, &base, None, b"relay-k").unwrap(),
        "abc123@mozmail.com"
    );
    let seen = log.lock().unwrap();
    assert_eq!(seen[0].header("Authorization"), Some("Token relay-k"));
    assert_eq!(json(&seen[0].body)["generated_for"], "shop.example");
}

#[test]
fn duckduckgo_answers_the_local_part_only() {
    let (base, log) = serve(Box::new(|req, _| {
        assert_eq!(req.path, "/api/email/addresses");
        (201, r#"{"address":"fuzzy-lamp-otter"}"#.into())
    }));
    assert_eq!(
        make(AliasService::DuckDuckGo, &base, None, b"ddg").unwrap(),
        "fuzzy-lamp-otter@duck.com"
    );
    assert_eq!(
        log.lock().unwrap()[0].header("Authorization"),
        Some("Bearer ddg")
    );
}

#[test]
fn forward_email_takes_basic_auth_on_the_persons_domain() {
    let (base, log) = serve(Box::new(|req, _| {
        assert_eq!(req.path, "/v1/domains/mine.example/aliases");
        (
            200,
            r#"{"name":"r4nd0m","domain":{"name":"mine.example"},"is_enabled":true}"#.into(),
        )
    }));
    assert_eq!(
        make(
            AliasService::ForwardEmail,
            &base,
            Some("mine.example"),
            b"fe-token"
        )
        .unwrap(),
        "r4nd0m@mine.example"
    );
    // base64("fe-token:")
    assert_eq!(
        log.lock().unwrap()[0].header("Authorization"),
        Some("Basic ZmUtdG9rZW46")
    );
    // An answer for another domain than asked is refused.
    let (base, _) = serve(Box::new(|_, _| {
        (
            200,
            r#"{"name":"r4nd0m","domain":{"name":"evil.example"}}"#.into(),
        )
    }));
    assert!(matches!(
        make(
            AliasService::ForwardEmail,
            &base,
            Some("mine.example"),
            b"fe-token"
        ),
        Err(AliasError::BadAnswer(_))
    ));
}

#[test]
fn fastmail_reads_its_session_then_makes_a_masked_address_on_the_same_host() {
    let (base, log) = serve(Box::new(|req, me| match req.path.as_str() {
        "/jmap/session" => (
            200,
            format!(
                r#"{{"apiUrl":"{me}/jmap/api/","primaryAccounts":{{"https://www.fastmail.com/dev/maskedemail":"u123"}}}}"#
            ),
        ),
        "/jmap/api/" => {
            let body: serde_json::Value = serde_json::from_str(&req.body).unwrap();
            let call = &body["methodCalls"][0];
            assert_eq!(call[0], "MaskedEmail/set");
            assert_eq!(call[1]["accountId"], "u123");
            assert_eq!(
                call[1]["create"]["lockdown"]["forDomain"],
                "https://shop.example"
            );
            (
                200,
                r#"{"methodResponses":[["MaskedEmail/set",{"created":{"lockdown":{"email":"blue.fox1@fastmail.com"}}},"0"]]}"#.into(),
            )
        }
        _ => (404, "{}".into()),
    }));
    assert_eq!(
        make(AliasService::Fastmail, &base, None, b"fm-token").unwrap(),
        "blue.fox1@fastmail.com"
    );
    let seen = log.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|s| s.header("Authorization") == Some("Bearer fm-token"))
    );
    drop(seen);
    // A session that points the key at another host is refused, and the
    // key goes nowhere else.
    let (base, log) = serve(Box::new(|_, _| {
        (
            200,
            r#"{"apiUrl":"https://collector.example/jmap/api/","primaryAccounts":{"https://www.fastmail.com/dev/maskedemail":"u1"}}"#.into(),
        )
    }));
    assert!(matches!(
        make(AliasService::Fastmail, &base, None, b"fm"),
        Err(AliasError::BadAnswer(_))
    ));
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[test]
fn refusals_and_hostile_answers_are_plain_errors_that_never_echo_the_body() {
    let (base, _) = serve(Box::new(|_, _| {
        (401, r#"{"error":"bad key sl-secret"}"#.into())
    }));
    let e = make(AliasService::SimpleLogin, &base, None, b"sl-secret").unwrap_err();
    assert!(matches!(e, AliasError::BadKey(_)));
    assert!(!e.to_string().contains("sl-secret"));
    let (base, _) = serve(Box::new(|_, _| (429, "{}".into())));
    assert!(matches!(
        make(AliasService::SimpleLogin, &base, None, b"k"),
        Err(AliasError::RateLimited(_))
    ));
    let (base, _) = serve(Box::new(|_, _| (500, "{}".into())));
    assert!(matches!(
        make(AliasService::SimpleLogin, &base, None, b"k"),
        Err(AliasError::Refused { status: 500, .. })
    ));
    for answer in [
        r#"{"alias":"two@x.com, three@y.com"}"#,
        r#"{"alias":"<img src=x>@x.com"}"#,
        r#"{"alias":"not-an-address"}"#,
        r#"{"alias":42}"#,
        "not json",
    ] {
        let answer = answer.to_string();
        let (base, _) = serve(Box::new(move |_, _| (201, answer.clone())));
        assert!(matches!(
            make(AliasService::SimpleLogin, &base, None, b"k"),
            Err(AliasError::BadAnswer(_))
        ),);
    }
}
