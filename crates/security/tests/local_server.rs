//! The real HTTP client against a local fixture server (the sandbox and CI
//! may not reach the internet): what goes over the wire for a breach check
//! and a change-password probe, read back from the server's side.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lockdown_security::breach::{self, Candidate, HttpRange};
use lockdown_security::change_password::{self, LinkKind};
use lockdown_security::http::HttpFetch;

/// What the fixture server saw: each request's line and headers.
type Seen = Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// Serves `route` on a free local port until the test ends.
fn serve(route: fn(&str) -> (u16, String)) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let log = seen.clone();
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
                headers.push(h.trim().to_string());
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            log.lock().unwrap().push((line.trim().to_string(), headers));
            let (status, body) = route(&path);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (format!("http://{addr}"), seen)
}

// SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8.
fn ranges(path: &str) -> (u16, String) {
    match path {
        "/range/5BAA6" => (
            200,
            "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:10437277\r\n".into(),
        ),
        p if p.starts_with("/range/") => (200, "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n".into()),
        _ => (404, String::new()),
    }
}

#[test]
fn a_breach_check_sends_five_characters_and_asks_for_padding() {
    let (base, seen) = serve(ranges);
    let fetch = HttpFetch::new(Duration::from_secs(5)).unwrap();
    let source = HttpRange::new(&format!("{base}/range"), Box::new(fetch)).unwrap();
    let out = breach::check(
        &source,
        &[
            Candidate::new("weak", b"password"),
            Candidate::new("strong", b"k7#Qm2^vX9@pL4&wR8!z"),
        ],
    );
    assert_eq!(out.found, vec![("weak".to_string(), 10_437_277)]);
    assert_eq!(out.clear, vec!["strong".to_string()]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for (line, headers) in seen.iter() {
        let path = line.split_whitespace().nth(1).unwrap();
        let prefix = path.strip_prefix("/range/").unwrap();
        assert_eq!(prefix.len(), 5, "{line}");
        assert!(line.starts_with("GET "), "{line}");
        assert!(
            headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case("add-padding: true")),
            "{headers:?}"
        );
        // Nothing else about the password: no body, no cookie, no other header naming it.
        let all = headers.join("\n").to_ascii_uppercase();
        assert!(!all.contains("1E4C9B93F3F0682250B6CF8331B7EE68FD8"));
        assert!(!all.contains("COOKIE"));
    }
}

#[test]
fn an_error_status_leaves_the_check_unfinished_in_plain_words() {
    let (base, _) = serve(|_| (503, "busy".into()));
    let fetch = HttpFetch::new(Duration::from_secs(5)).unwrap();
    let source = HttpRange::new(&format!("{base}/range/"), Box::new(fetch)).unwrap();
    let out = breach::check(&source, &[Candidate::new("a", b"password")]);
    assert_eq!(out.unchecked, vec!["a".to_string()]);
    let words = out.error.unwrap().to_string();
    assert!(
        words.contains("503") && words.contains("127.0.0.1"),
        "{words}"
    );
    assert!(!words.contains("5BAA6"), "{words}");
}

#[test]
fn the_change_password_probe_reads_the_sites_answers_and_follows_nothing() {
    let (base, seen) = serve(|path| match path {
        "/.well-known/change-password" => (302, String::new()),
        _ => (404, String::new()),
    });
    let fetch = HttpFetch::new(Duration::from_secs(5)).unwrap();
    let link = change_password::resolve(&format!("{base}/account/login"), Some(&fetch)).unwrap();
    assert_eq!(link.kind, LinkKind::WellKnown);
    assert_eq!(link.url, format!("{base}/.well-known/change-password"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(seen.iter().all(|(line, _)| line.contains("/.well-known/")));

    let (base, _) = serve(|_| (200, "everything is fine".into()));
    let link = change_password::resolve(&base, Some(&fetch)).unwrap();
    assert_eq!(link.kind, LinkKind::SiteRoot);
    assert_eq!(link.url, format!("{base}/"));
}
