//! The recovery kit as a file through `App` (docs/plan/38-mac-fixes.md §4):
//! the page `kit_create` really returns lays out, the file is written to the
//! place given and nowhere else, owner-only, and nothing is laid out while
//! the vault is locked.

use lockdown_app::{App, ErrorCode, LockPolicy, Paths, kit_pdf};

fn fresh() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create(true).unwrap();
    (dir, app)
}

/// The typed form of a labelled value on the page: no dashes.
fn typed(page: &str, label: &str) -> String {
    page.lines()
        .find_map(|l| l.strip_prefix(label))
        .and_then(|l| l.strip_prefix(':'))
        .unwrap()
        .trim()
        .replace('-', "")
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

#[test]
fn the_page_kit_create_returns_saves_as_a_pdf_in_the_place_given_only() {
    let (dir, app) = fresh();
    let made = app.kit_create(false, None).unwrap();
    let page = String::from_utf8(made.kit.expose().to_vec()).unwrap();

    let pdf = app.kit_pdf(&made.notes, &page).unwrap();
    assert!(pdf.starts_with(b"%PDF-1."));
    // Each value once, as its groups of four (each group its own run).
    for label in ["Secret Key", "Recovery code"] {
        let value = typed(&page, label);
        let first = format!("({})", &value[..4]);
        assert_eq!(
            count(&pdf, first.as_bytes()),
            1,
            "{label}'s first group once"
        );
    }

    let out = dir.path().join("elsewhere");
    std::fs::create_dir(&out).unwrap();
    let path = out.join(kit_pdf::FILE_NAME);
    app.kit_pdf_save(&made.notes, &page, &path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), *pdf);
    // Written there and only there: the folder holds that one file, and
    // the app's own data holds nothing new with the kit in it.
    let names: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        names,
        [std::ffi::OsString::from("Lockdown Recovery Kit.pdf")]
    );
    let sk = typed(&page, "Secret Key");
    for entry in walk(&dir.path().join("snds")) {
        let bytes = std::fs::read(&entry).unwrap();
        assert_eq!(
            count(&bytes, &sk.as_bytes()[..12]),
            0,
            "{}",
            entry.display()
        );
        assert_eq!(count(&bytes, b"%PDF-"), 0, "{}", entry.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // Not a kit page: refused, and nothing written.
    let other = out.join("other.pdf");
    let refused = app
        .kit_pdf_save(&[], "a letter to the bank", &other)
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Refused);
    assert!(!other.exists());

    // Locked: nothing is laid out until the vault is open again.
    app.lock().unwrap();
    assert_eq!(
        app.kit_pdf(&made.notes, &page).unwrap_err().code,
        ErrorCode::Locked
    );
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
