//! Settings' memory report (`memory_report`, doc 26 §7) counts Lockdown's
//! secrets and the identity crate's keys on one page table
//! (docs/plan/52-identity-adopt.md): a crate key made on its own moves it,
//! and so does opening the vault, which holds keys of both.

use ecosystem_identity::memlock;
use ecosystem_identity::{AppDataKey, SymmetricKey};
use lockdown_app::{App, LockPolicy, Paths};

#[test]
fn the_settings_report_counts_the_identity_crates_keys_too() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::resolve(Some(dir.path().join("snds/identity")), None).unwrap();
    let app = App::start(paths, None, LockPolicy::default());
    let r0 = app.memory_report();
    assert!(r0.supported);

    // A key of the identity crate's, made by nothing of Lockdown's.
    let key = AppDataKey::from_bytes([7; 32]);
    let r1 = app.memory_report();
    if key.is_locked() {
        assert!(r1.pages_locked >= 1);
        assert!(r1.pages_locked_ever >= r0.pages_locked_ever);
    } else {
        assert!(r1.refused > r0.refused, "a refusal shows in Settings");
    }
    assert_eq!(key.expose(), &[7; 32]);

    // Opening holds the crate's keys (the root key, the App Data Key, the
    // Identity Signing Key) and Lockdown's (the vault keys).
    app.open_or_create_plain().unwrap();
    let r2 = app.memory_report();
    assert!(r2.pages_locked_ever > r1.pages_locked_ever || r2.refused > r1.refused);

    // On Windows Lockdown makes room in its working set before each lock
    // (doc 59), so after opening, and after a deep secret-stack call (its
    // 1 MiB block), Settings shows no refusal.
    #[cfg(windows)]
    {
        lockdown_crypto::secret_stack::run_deep(|| ());
        assert_eq!(app.memory_report().refused, 0, "Settings shows no refusal");
    }

    // The numbers are the crate's table; Lockdown adds only the dump flag.
    let shared = memlock::report();
    assert_eq!(shared.page_size, r2.page_size);
    assert!(shared.pages_locked_ever >= r2.pages_locked_ever);
    assert!(shared.refused >= r2.refused);
}
