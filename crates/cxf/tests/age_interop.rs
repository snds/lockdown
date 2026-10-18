//! Lockdown's age code against the reference implementation (rage, through
//! `pyrage`; `fixtures/src/make-age-fixtures.py`): files rage wrote, plain
//! and armored, open here; a file written here is kept as
//! `age-from-lockdown.age`, which the script opens with rage.

use std::path::PathBuf;

use lockdown_cxf::age_file::{self, Identity};
use lockdown_cxf::{CxfError, Limits};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn sample() -> Vec<u8> {
    let line: &[u8] = b"Lockdown age interop sample; every value made up.\n";
    line.repeat(70_000 / line.len() + 1)
}

fn key() -> String {
    std::fs::read_to_string(fixture("age-reference.key")).unwrap()
}

#[test]
fn files_the_reference_implementation_wrote_open_here() {
    for name in ["age-reference.age", "age-reference.armored.age"] {
        let bytes = std::fs::read(fixture(name)).unwrap();
        assert!(age_file::is_age(&bytes), "{name}");
        let plain = age_file::open_with(&bytes, key().as_bytes(), &Limits::default()).unwrap();
        assert_eq!(plain.as_slice(), sample().as_slice(), "{name}");
        let other = Identity::generate().unwrap();
        assert!(matches!(
            age_file::open_with(&bytes, other.encode().as_bytes(), &Limits::default()),
            Err(CxfError::WrongPassword)
        ));
    }
    // The key's public half is the recipient the script printed.
    let id = Identity::parse(key().trim()).unwrap();
    assert_eq!(
        id.recipient().encode(),
        "age17y3knya604c0flqdkveetmyh4hr48shvmczv3zqvkg3khckw4f9smch4ry"
    );
}

#[test]
fn a_file_written_here_is_kept_for_the_reference_to_open() {
    let id = Identity::parse(key().trim()).unwrap();
    let path = fixture("age-from-lockdown.age");
    if std::env::var_os("LOCKDOWN_WRITE_AGE").is_some() {
        let sealed = age_file::seal_to(&sample(), &[id.recipient().encode()]).unwrap();
        std::fs::write(&path, sealed).unwrap();
    }
    let bytes = std::fs::read(&path).unwrap();
    let plain = age_file::open_with(&bytes, key().as_bytes(), &Limits::default()).unwrap();
    assert_eq!(plain.as_slice(), sample().as_slice());
}
