//! The security center through `App`, end to end on a real vault: the
//! report's findings and their words, what crosses to a shell (never a
//! password, a hash or a reuse key), the breach check against a local
//! range server (five characters per request, verdicts cached without a
//! hash, re-checked only when a password changes), dismiss and snooze as a
//! setting on the item that survives a lock and never shows in `show`, the
//! change-password link, and the account's new-device alert against the
//! in-process server.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use lockdown_app::{App, FieldChange, LockPolicy, Paths, SecurityReport};
use lockdown_server::{AppState, Config, run_in_background};

/// SHA-1("password") = 5BAA6 1E4C9B93F3F0682250B6CF8331B7EE68FD8.
const WEAK: &[u8] = b"password";
const STRONG: &[u8] = b"k7#Qm2^vX9@pL4&wR8!z";
const ALSO_STRONG: &[u8] = b"Vq8$wN3!rT6&yP1^zL5@";

fn fresh() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    (dir, app)
}

type Seen = Arc<Mutex<Vec<String>>>;

/// A local HTTP server answering `route(path)`; it records request lines.
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
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            log.lock().unwrap().push(path.clone());
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

fn kinds(report: &SecurityReport, item: &str) -> Vec<String> {
    let mut k: Vec<String> = report
        .findings
        .iter()
        .filter(|f| f.item_id.as_deref() == Some(item))
        .map(|f| f.kind.clone())
        .collect();
    k.sort();
    k
}

/// Nothing a shell gets carries a password or the hash of one.
fn assert_clean(report: &SecurityReport) {
    let json = serde_json::to_string(report).unwrap();
    for needle in [
        ":\"password\"",
        "k7#Qm2",
        "Vq8$wN3",
        "5BAA6",
        "1E4C9B93F3F0682250B6CF8331B7EE68FD8",
    ] {
        assert!(!json.contains(needle), "{needle} in {json}");
    }
}

#[test]
fn the_report_finds_weak_reused_missing_two_step_and_lookalikes_without_showing_a_password() {
    let (_dir, app) = fresh();
    let github = app
        .add_login(
            "https://github.com/login",
            "sean",
            WEAK.to_vec(),
            Some("GitHub"),
        )
        .unwrap();
    let mail = app
        .add_login(
            "https://mail.example.com",
            "sean",
            WEAK.to_vec(),
            Some("Mail"),
        )
        .unwrap();
    let bank = app
        .add_login(
            "https://bank.example",
            "sean",
            STRONG.to_vec(),
            Some("Bank"),
        )
        .unwrap();
    let fake = app
        .add_login(
            "https://paypa1.com/signin",
            "sean",
            ALSO_STRONG.to_vec(),
            Some("Pay"),
        )
        .unwrap();
    let report = app.security_report().unwrap();
    assert_clean(&report);
    assert_eq!(report.passwords, 4);
    assert_eq!(
        kinds(&report, &github),
        vec!["missing_2fa", "reused", "weak"]
    );
    assert_eq!(kinds(&report, &mail), vec!["reused", "weak"]);
    assert!(
        kinds(&report, &bank).is_empty(),
        "{:?}",
        kinds(&report, &bank)
    );
    assert_eq!(kinds(&report, &fake), vec!["lookalike"]);

    let reused = report
        .findings
        .iter()
        .find(|f| f.kind == "reused" && f.item_id.as_deref() == Some(github.as_str()))
        .unwrap();
    assert_eq!(reused.related.len(), 1);
    assert_eq!(reused.related[0].id, mail);
    assert!(
        reused.detail.contains("\u{201c}Mail\u{201d}"),
        "{}",
        reused.detail
    );
    assert_eq!(reused.key, "reused:pw");
    assert!(reused.change_password);
    assert_eq!(reused.playbook, "reused-password");

    let two_step = report
        .findings
        .iter()
        .find(|f| f.kind == "missing_2fa")
        .unwrap();
    assert!(
        two_step
            .setup_url
            .as_deref()
            .unwrap()
            .starts_with("https://")
    );
    assert!(
        two_step
            .detail
            .starts_with("GitHub lets you add a second step"),
        "{}",
        two_step.detail
    );
    assert_eq!(two_step.severity, "suggested");

    let look = report
        .findings
        .iter()
        .find(|f| f.kind == "lookalike")
        .unwrap();
    assert!(look.detail.contains("paypal.com"), "{}", look.detail);
    assert_eq!(look.severity, "important");

    // Most severe first, and the counts add up.
    let ranks: Vec<u8> = report
        .findings
        .iter()
        .map(|f| match f.severity.as_str() {
            "urgent" => 0,
            "important" => 1,
            _ => 2,
        })
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
    let c = report.counts;
    assert_eq!(
        (c.urgent + c.important + c.suggested) as usize,
        report.findings.len()
    );
    // Nothing checked against breaches yet, and no account to ask.
    assert_eq!(report.breach.checked, 0);
    assert_eq!(report.breach.unchecked, 4);
    assert!(!report.alerts.account);
    assert!(report.directory.sites > 1500);
    assert_eq!(report.directory.licence, "MIT");
}

#[test]
fn a_code_or_a_passkey_answers_the_two_step_finding_and_a_crown_jewel_shows_no_website() {
    let (_dir, app) = fresh();
    let id = app
        .add_login(
            "https://github.com",
            "sean",
            STRONG.to_vec(),
            Some("GitHub"),
        )
        .unwrap();
    assert_eq!(
        kinds(&app.security_report().unwrap(), &id),
        vec!["missing_2fa"]
    );
    app.add_code(
        "otpauth://totp/GitHub:sean?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
        None,
        Some(&id),
    )
    .unwrap();
    assert!(kinds(&app.security_report().unwrap(), &id).is_empty());

    let jewel = app
        .add_item(
            "login",
            Some("Recovery mailbox"),
            true,
            vec![
                FieldChange::text("url.0", "https://paypa1.com"),
                FieldChange::text("user", "sean"),
                FieldChange::secret("pw", WEAK),
            ],
        )
        .unwrap();
    let report = app.security_report().unwrap();
    let mine: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.item_id.as_deref() == Some(jewel.as_str()))
        .collect();
    assert!(!mine.is_empty());
    for f in mine {
        assert!(f.site.is_none(), "{f:?}");
        assert!(!f.detail.contains("paypa1"), "{}", f.detail);
    }
}

#[test]
fn the_breach_check_sends_prefixes_caches_verdicts_and_rechecks_only_what_changed() {
    let (dir, app) = fresh();
    let (base, seen) = serve(ranges);
    let mut settings = app.settings().unwrap();
    settings.breach_endpoint = Some(format!("{base}/range/"));
    app.set_settings(settings).unwrap();
    let github = app
        .add_login("https://github.com", "sean", WEAK.to_vec(), Some("GitHub"))
        .unwrap();
    let bank = app
        .add_login(
            "https://bank.example",
            "sean",
            STRONG.to_vec(),
            Some("Bank"),
        )
        .unwrap();

    let report = app.security_check().unwrap();
    assert_clean(&report);
    let breached: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == "breached")
        .collect();
    assert_eq!(breached.len(), 1);
    assert_eq!(breached[0].item_id.as_deref(), Some(github.as_str()));
    assert_eq!(breached[0].severity, "urgent");
    assert!(
        breached[0].detail.contains("10,437,277 times"),
        "{}",
        breached[0].detail
    );
    assert_eq!((report.breach.checked, report.breach.unchecked), (2, 0));
    assert!(report.breach.checked_at.is_some());
    assert!(
        report.breach.source.starts_with("127.0.0.1"),
        "{}",
        report.breach.source
    );
    {
        let paths = seen.lock().unwrap();
        assert_eq!(paths.len(), 2, "{paths:?}");
        assert!(
            paths
                .iter()
                .all(|p| p.strip_prefix("/range/").is_some_and(|x| x.len() == 5))
        );
    }
    // The cache holds verdicts, never a hash or a password.
    let cache = std::fs::read(dir.path().join("snds/lockdown/security.cbor"))
        .or_else(|_| {
            let found = walk(dir.path(), "security.cbor").expect("the cache file");
            std::fs::read(found)
        })
        .unwrap();
    let text = String::from_utf8_lossy(&cache).to_uppercase();
    assert!(!text.contains("1E4C9B93F3F0682250B6CF8331B7EE68FD8") && !text.contains("5BAA6"));
    assert!(!String::from_utf8_lossy(&cache).contains("password"));

    // Nothing changed: nothing is asked again; a report without the network still knows.
    app.security_check().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(
        app.security_report()
            .unwrap()
            .findings
            .iter()
            .filter(|f| f.kind == "breached")
            .count(),
        1
    );
    // A changed password is a new version, checked again on its own; the fix clears the finding.
    app.edit_item(
        &github,
        None,
        vec![
            FieldChange::keep("user"),
            FieldChange::secret("pw", ALSO_STRONG),
            FieldChange::keep("url.0"),
        ],
        Default::default(),
    )
    .unwrap();
    let report = app.security_check().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 3);
    assert!(report.findings.iter().all(|f| f.kind != "breached"));
    assert!(kinds(&report, &bank).is_empty());

    // Turned off, nothing is asked at all.
    let mut settings = app.settings().unwrap();
    settings.breach_check = false;
    app.set_settings(settings).unwrap();
    app.edit_item(
        &bank,
        None,
        vec![
            FieldChange::keep("user"),
            FieldChange::secret("pw", WEAK),
            FieldChange::keep("url.0"),
        ],
        Default::default(),
    )
    .unwrap();
    let report = app.security_check().unwrap();
    assert!(!report.breach.enabled);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

fn walk(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = walk(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

#[test]
fn a_failed_range_says_so_and_leaves_the_password_unchecked() {
    let (_dir, app) = fresh();
    let (base, _) = serve(|_| (503, "busy".into()));
    let mut settings = app.settings().unwrap();
    settings.breach_endpoint = Some(format!("{base}/range"));
    app.set_settings(settings).unwrap();
    app.add_login("https://github.com", "sean", WEAK.to_vec(), None)
        .unwrap();
    let report = app.security_check().unwrap();
    assert_eq!(report.breach.unchecked, 1);
    let message = report.breach.message.unwrap();
    assert!(message.contains("503"), "{message}");
    // A plain-http address that isn't this machine is refused before any call.
    let mut settings = app.settings().unwrap();
    settings.breach_endpoint = Some("http://ranges.example/range/".into());
    app.set_settings(settings).unwrap();
    let message = app.security_check().unwrap().breach.message.unwrap();
    assert!(message.contains("https"), "{message}");
}

#[test]
fn dismiss_and_snooze_are_a_setting_on_the_item_kept_through_a_lock_and_never_shown() {
    let (_dir, app) = fresh();
    let id = app
        .add_login("https://github.com", "sean", WEAK.to_vec(), Some("GitHub"))
        .unwrap();
    let before = app.show(&id, false).unwrap().version;
    let report = app.security_dismiss(&id, "weak:pw", None).unwrap();
    assert!(report.findings.iter().all(|f| f.key != "weak:pw"));
    let dismissed = report
        .dismissed
        .iter()
        .find(|f| f.key == "weak:pw")
        .unwrap();
    assert_eq!(dismissed.dismissed.unwrap().until, None);
    // A new version of the item, and the field is never shown.
    let detail = app.show(&id, false).unwrap();
    assert_eq!(detail.version, before + 1);
    assert!(
        detail
            .fields
            .iter()
            .all(|f| !f.field_id.starts_with("lockdown."))
    );
    // An editor that names only what it was shown still saves, and keeps the dismissal.
    app.edit_item(
        &id,
        Some("GitHub (work)"),
        vec![
            FieldChange::keep("user"),
            FieldChange::keep("pw"),
            FieldChange::keep("url.0"),
        ],
        Default::default(),
    )
    .unwrap();
    // It is in the vault: a lock and an unlock keep it.
    app.lock().unwrap();
    app.unlock().unwrap();
    let report = app.security_report().unwrap();
    assert!(report.dismissed.iter().any(|f| f.key == "weak:pw"));
    // A snooze must end in the future; one that does hides the finding until then.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        app.security_dismiss(&id, "missing_2fa", Some(now - 1))
            .is_err()
    );
    let report = app
        .security_dismiss(&id, "missing_2fa", Some(now + 7 * 86_400))
        .unwrap();
    let snoozed = report
        .dismissed
        .iter()
        .find(|f| f.key == "missing_2fa")
        .unwrap();
    assert_eq!(snoozed.dismissed.unwrap().until, Some(now + 7 * 86_400));
    // Restore brings both back.
    app.security_restore(&id, "weak:pw").unwrap();
    let report = app.security_restore(&id, "missing_2fa").unwrap();
    assert!(report.dismissed.is_empty());
    assert_eq!(kinds(&report, &id), vec!["missing_2fa", "weak"]);
    // Bad keys and trashed items are refused.
    assert!(app.security_dismiss(&id, "", None).is_err());
    app.trash(&id).unwrap();
    assert!(app.security_dismiss(&id, "weak:pw", None).is_err());
}

#[test]
fn change_password_opens_the_well_known_page_when_the_site_has_one() {
    let (_dir, app) = fresh();
    let (base, seen) = serve(|path| match path {
        "/.well-known/change-password" => (303, String::new()),
        _ => (404, String::new()),
    });
    let id = app
        .add_login(
            &format!("{base}/login"),
            "sean",
            WEAK.to_vec(),
            Some("Local"),
        )
        .unwrap();
    let link = app.security_change_link(&id).unwrap();
    assert_eq!(link.url, format!("{base}/.well-known/change-password"));
    assert!(link.checked);
    assert_eq!(seen.lock().unwrap().len(), 2);
    // With the check off, the page is offered unchecked and nothing is asked.
    let mut settings = app.settings().unwrap();
    settings.change_password_check = false;
    app.set_settings(settings).unwrap();
    let link = app.security_change_link(&id).unwrap();
    assert!(!link.checked);
    assert_eq!(seen.lock().unwrap().len(), 2);
    // An item with no website has no link.
    let note = app
        .add_item(
            "note",
            Some("n"),
            false,
            vec![FieldChange::text("body", "hi")],
        )
        .unwrap();
    assert!(app.security_change_link(&note).is_err());
    assert_eq!(app.security_playbooks().len(), 7);
}

#[test]
fn a_new_device_on_the_account_is_a_finding_on_the_others_until_dismissed() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::in_memory(Config::default()).unwrap());
    let running = run_in_background(state).unwrap();
    let url = running.url();
    let device = |name: &str| {
        let paths =
            Paths::resolve(Some(dir.path().join(name).join("snds/identity")), None).unwrap();
        App::start(paths, None, LockPolicy::default())
    };
    let a = device("a");
    a.open_or_create(true).unwrap();
    let registered = a
        .account_register(&url, "me@example.com", "correct horse battery staple", None)
        .unwrap();
    let page = String::from_utf8(registered.kit.as_ref().unwrap().expose().to_vec()).unwrap();
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .unwrap()
        .trim()
        .to_string();
    a.sync().unwrap();
    let report = a.security_check().unwrap();
    assert!(report.alerts.account);
    assert!(
        report.alerts.message.is_none(),
        "{:?}",
        report.alerts.message
    );
    assert!(report.findings.iter().all(|f| f.kind != "new_sign_in"));

    let b = device("b");
    b.account_login(
        &url,
        "me@example.com",
        "correct horse battery staple",
        Some(&secret_key),
    )
    .unwrap();
    b.open_or_create(true).unwrap();
    a.sync().unwrap();
    let report = a.security_check().unwrap();
    let alert = report
        .findings
        .iter()
        .find(|f| f.kind == "new_sign_in")
        .expect("A hears about B");
    assert_eq!(alert.severity, "important");
    assert_eq!(alert.playbook, "new-sign-in");
    assert!(
        alert.detail.contains("signed in to your Lockdown account"),
        "{}",
        alert.detail
    );
    let seq = alert.alert.unwrap();
    // B doesn't hear about itself.
    assert!(
        b.security_check()
            .unwrap()
            .findings
            .iter()
            .all(|f| f.kind != "new_sign_in")
    );
    // Dismissed on A, it is dismissed for the account.
    let report = a.security_dismiss_alert(seq).unwrap();
    assert!(report.findings.iter().all(|f| f.kind != "new_sign_in"));
    assert!(report.dismissed.iter().any(|f| f.alert == Some(seq)));
    assert!(
        a.security_check()
            .unwrap()
            .findings
            .iter()
            .all(|f| f.kind != "new_sign_in")
    );
}
