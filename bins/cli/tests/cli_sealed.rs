//! Every vault `lockdown init` makes is sealed under a master password
//! (docs/plan/43-cli-sealed.md): from a script (the first stdin line or the
//! variable), at a terminal (asked, with echo off), and refused with
//! nothing written when neither can give the name and the password. The
//! vault is the app's: the desktop core opens it with the same name and
//! password and not without. An install made before this keeps opening and
//! takes a master password with `password set`. `account register` on a
//! sealed vault moves it to a server.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use lockdown_server::{AppState, Config, run_in_background};

const MASTER: &str = "correct horse battery staple";

struct Run {
    ok: bool,
    out: String,
    err: String,
}

fn command(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lockdown"));
    cmd.arg("--root")
        .arg(root)
        .args(args)
        .env_remove("LOCKDOWN_PASSWORD")
        .env_remove("LOCKDOWN_MASTER_PASSWORD")
        .env_remove("LOCKDOWN_SECRET_KEY")
        .env_remove("LOCKDOWN_RECOVERY_CODE")
        .env("HOSTNAME", "test-laptop")
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn finish(child: std::process::Child) -> Run {
    let out = child.wait_with_output().unwrap();
    Run {
        ok: out.status.success(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn lockdown(root: &Path, args: &[&str], stdin: Option<&str>, env: &[(&str, &str)]) -> Run {
    let mut cmd = command(root, args, env);
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().expect("the lockdown binary runs");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    finish(child)
}

fn kit_of(out: &str) -> (String, String) {
    let value = |label: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap_or_else(|| panic!("no {label} in {out}"))
            .trim()
            .to_string()
    };
    (value("Secret Key:"), value("Recovery code:"))
}

/// Every file and folder under `dir`, for "nothing was written".
fn everything_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p.clone());
            }
            found.push(p);
        }
    }
    found.sort();
    found
}

fn data_of(root: &Path) -> PathBuf {
    root.parent().unwrap().join("lockdown")
}

fn core(root: &Path) -> lockdown_app::App {
    lockdown_app::App::start(
        lockdown_app::Paths {
            root: root.to_path_buf(),
            data: data_of(root),
        },
        None,
        lockdown_app::LockPolicy::default(),
    )
}

#[test]
fn init_seals_from_the_first_stdin_line_and_never_prints_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let run = lockdown(
        &root,
        &["--master-password-stdin", "init", "--name", "Sean"],
        Some(&format!("{MASTER}\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    assert!(
        run.out
            .starts_with("vault created for Sean; it opens only with your master password"),
        "{}",
        run.out
    );
    let (sk, rc) = kit_of(&run.out);
    assert!(sk.starts_with("A1-"), "{sk}");
    assert!(!rc.is_empty());
    assert!(
        run.out
            .contains("Your recovery kit is printed above, shown once."),
        "{}",
        run.out
    );
    assert!(!run.out.contains(MASTER) && !run.err.contains(MASTER));
    let data = data_of(&root);
    assert!(data.join("device-key.sealed.cbor").exists());
    assert!(!data.join("device-key.cbor").exists());

    // Locked without it, open with it.
    let run = lockdown(&root, &["list"], None, &[]);
    assert!(!run.ok);
    assert!(
        run.err.contains("locked with a master password"),
        "{}",
        run.err
    );
    let run = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(run.ok, "{}", run.err);
    assert!(!run.err.contains("no master password yet"), "{}", run.err);

    // Again: it opens what is there, with the password, and makes nothing.
    let run = lockdown(
        &root,
        &["init"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(run.ok, "{}", run.err);
    assert!(
        run.out.starts_with("identity attached, vault opened"),
        "{}",
        run.out
    );
    let run = lockdown(&root, &["init"], None, &[]);
    assert!(!run.ok);
    assert!(
        run.err.contains("locked with a master password"),
        "{}",
        run.err
    );
    let run = lockdown(
        &root,
        &["--master-password-stdin", "init", "--name", "Sean"],
        Some(&format!("{MASTER}\n")),
        &[],
    );
    assert!(!run.ok);
    assert!(run.err.contains("already set up"), "{}", run.err);
}

#[test]
fn init_seals_from_the_variable_and_saves_the_kit_where_asked() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let kit = dir.path().join("kit.txt");
    let run = lockdown(
        &root,
        &["init", "--name", "Sean", "--save", kit.to_str().unwrap()],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(run.ok, "{}", run.err);
    assert!(
        run.out
            .contains(&format!("recovery kit saved to {}", kit.display())),
        "{}",
        run.out
    );
    assert!(!run.out.contains("Secret Key:"), "{}", run.out);
    let page = std::fs::read_to_string(&kit).unwrap();
    let _ = kit_of(&page);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&kit).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let run = lockdown(
        &root,
        &["status"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(run.ok, "{}", run.err);
}

#[test]
fn without_a_terminal_and_without_a_name_and_password_init_refuses_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let want = "lockdown init needs a name and a master password. Give them with --name NAME and --master-password-stdin (or LOCKDOWN_MASTER_PASSWORD), or run lockdown init in a terminal to be asked.";
    for (args, env) in [
        (vec!["init"], vec![]),
        (vec!["init", "--name", "Sean"], vec![]),
        (vec!["init"], vec![("LOCKDOWN_MASTER_PASSWORD", MASTER)]),
    ] {
        let run = lockdown(&root, &args, None, &env);
        assert!(!run.ok, "{args:?}");
        assert!(run.err.contains(want), "{args:?}: {}", run.err);
        assert!(everything_in(dir.path()).is_empty(), "{args:?}");
    }
}

#[test]
fn a_weak_master_password_is_refused_by_the_apps_rule_before_anything_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    for weak in ["sean1234", "password123"] {
        let run = lockdown(
            &root,
            &["--master-password-stdin", "init", "--name", "Sean"],
            Some(&format!("{weak}\n")),
            &[],
        );
        assert!(!run.ok, "{weak}");
        assert!(
            run.err.contains("This password protects everything else, so it needs to be at least fair. Add another word or two."),
            "{}",
            run.err
        );
        assert!(everything_in(dir.path()).is_empty(), "{weak}");
    }
    // The same rule the core applies to the app's first run.
    assert!(lockdown_app::check_master_password("sean1234", Some("Sean")).is_err());
    assert!(lockdown_app::check_master_password(MASTER, Some("Sean")).is_ok());
}

#[test]
fn the_vault_init_makes_opens_in_the_desktop_core_with_the_same_name_and_password() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let run = lockdown(
        &root,
        &["init", "--name", "Sean"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(run.ok, "{}", run.err);
    let run = lockdown(
        &root,
        &[
            "--master-password-stdin",
            "add",
            "login",
            "--site",
            "https://example.com",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some(&format!("{MASTER}\nitem-password\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);

    // The shared account model: the name the lock screen shows, and the
    // one master password, in the app's own core.
    assert_eq!(
        lockdown_app::onboarding::Onboarding::load(&data_of(&root))
            .name
            .as_deref(),
        Some("Sean")
    );
    let app = core(&root);
    assert!(app.unlock().is_err(), "opened without the master password");
    assert!(app.unlock_with_password("not the one at all").is_err());
    let status = app.unlock_with_password(MASTER).unwrap();
    assert_eq!(status.live_items, 1);
    app.lock().unwrap();
}

/// A pseudo-terminal for the child's standard input: what a person types.
#[cfg(unix)]
struct Terminal {
    main: std::fs::File,
    side: std::fs::File,
}

#[cfg(unix)]
impl Terminal {
    fn open() -> Self {
        use std::os::fd::FromRawFd;
        // SAFETY: plain libc calls on a descriptor this function owns; the
        // name `ptsname` returns is copied before anything else runs.
        unsafe {
            let fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(fd >= 0, "no pseudo-terminal");
            assert_eq!(libc::grantpt(fd), 0);
            assert_eq!(libc::unlockpt(fd), 0);
            let name = std::ffi::CStr::from_ptr(libc::ptsname(fd))
                .to_string_lossy()
                .into_owned();
            let main = std::fs::File::from_raw_fd(fd);
            let side = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(name)
                .unwrap();
            Self { main, side }
        }
    }

    /// Runs `lockdown` with this terminal as its stdin, typing `typed`.
    fn run(mut self, root: &Path, args: &[&str], typed: &str, env: &[(&str, &str)]) -> Run {
        let mut cmd = command(root, args, env);
        cmd.stdin(Stdio::from(self.side.try_clone().unwrap()));
        let child = cmd.spawn().expect("the lockdown binary runs");
        self.main.write_all(typed.as_bytes()).unwrap();
        let run = finish(child);
        drop(self.side);
        run
    }
}

#[cfg(unix)]
#[test]
fn at_a_terminal_init_asks_for_the_name_and_the_password_twice() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    let env = [("USER", "sam"), ("USERNAME", ""), ("LOGNAME", "")];

    // Mismatched: refused, nothing written.
    let run = Terminal::open().run(
        &root,
        &["init"],
        &format!("Sean\n{MASTER}\norbit lantern pickle meadow\n"),
        &env,
    );
    assert!(!run.ok);
    assert!(
        run.err
            .contains("Your master password opens your vault, and Lockdown can't see it or reset it. If you forget it, only your recovery kit gets you back in"),
        "{}",
        run.err
    );
    assert!(
        run.err
            .contains("Your name, shown when Lockdown is locked [sam]: "),
        "{}",
        run.err
    );
    assert!(
        run.err.contains("Choose a master password: "),
        "{}",
        run.err
    );
    assert!(run.err.contains("Type it again: "), "{}", run.err);
    assert!(
        run.err.contains("The two passwords don't match yet."),
        "{}",
        run.err
    );
    assert!(everything_in(dir.path()).is_empty());

    // Weak: refused before the second ask.
    let run = Terminal::open().run(&root, &["init"], "Sean\nsean1234\n", &env);
    assert!(!run.ok);
    assert!(run.err.contains("at least fair"), "{}", run.err);
    assert!(!run.err.contains("Type it again"), "{}", run.err);
    assert!(everything_in(dir.path()).is_empty());

    // The offered name taken with Enter; the password typed twice.
    let run = Terminal::open().run(&root, &["init"], &format!("\n{MASTER}\n{MASTER}\n"), &env);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.starts_with("vault created for sam"), "{}", run.out);
    let _ = kit_of(&run.out);
    assert!(!run.out.contains(MASTER) && !run.err.contains(MASTER));

    // Later commands at a terminal ask for it by name.
    let run = Terminal::open().run(&root, &["list"], &format!("{MASTER}\n"), &[]);
    assert!(run.ok, "{}", run.err);
    assert!(run.err.contains("Master password for sam: "), "{}", run.err);
}

#[test]
fn an_install_made_without_a_master_password_still_opens_and_then_seals() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("snds/identity");
    lockdown_devtools::legacy::make(&root, None).unwrap();
    let data = data_of(&root);
    assert!(data.join("device-key.cbor").exists());

    // It opens, and every command says it has no master password.
    let run = lockdown(
        &root,
        &[
            "add",
            "login",
            "--site",
            "https://a.example",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("pw\n"),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    let notice = "This vault has no master password yet. Run lockdown password set --name NAME";
    assert!(run.err.contains(notice), "{}", run.err);
    let before = lockdown(&root, &["list"], None, &[]);
    assert!(before.ok, "{}", before.err);
    assert!(before.err.contains(notice), "{}", before.err);
    assert!(before.out.contains("a.example"), "{}", before.out);
    let run = lockdown(&root, &["init"], None, &[]);
    assert!(run.ok, "{}", run.err);
    assert!(run.err.contains(notice), "{}", run.err);

    // `password set --name` seals it, with a new kit.
    let run = lockdown(
        &root,
        &[
            "password",
            "set",
            "--name",
            "Sean",
            "--new-kit",
            "--password-stdin",
        ],
        Some(&format!("{MASTER}\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    let _ = kit_of(&run.out);
    assert!(data.join("device-key.sealed.cbor").exists());
    assert!(!data.join("device-key.cbor").exists());
    let run = lockdown(&root, &["list"], None, &[]);
    assert!(!run.ok);
    let after = lockdown(
        &root,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", MASTER)],
    );
    assert!(after.ok, "{}", after.err);
    assert_eq!(after.out, before.out);
    assert!(!after.err.contains(notice), "{}", after.err);
}

#[test]
fn register_on_a_sealed_vault_moves_it_to_the_server_with_the_master_password() {
    let dir = tempfile::tempdir().unwrap();
    let server =
        run_in_background(Arc::new(AppState::in_memory(Config::default()).unwrap())).unwrap();
    let url = server.url();
    let laptop = dir.path().join("laptop/snds/identity");
    let phone = dir.path().join("phone/snds/identity");
    let with = [("LOCKDOWN_MASTER_PASSWORD", MASTER)];
    let run = lockdown(&laptop, &["init", "--name", "Sean"], None, &with);
    assert!(run.ok, "{}", run.err);
    let (sk, rc) = kit_of(&run.out);
    let run = lockdown(
        &laptop,
        &[
            "add",
            "login",
            "--site",
            "https://bank.example",
            "--username",
            "me",
            "--password-stdin",
        ],
        Some("first secret\n"),
        &with,
    );
    assert!(run.ok, "{}", run.err);

    // An account password that isn't the master password is refused.
    let run = lockdown(
        &laptop,
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
        ],
        Some("another password entirely\n"),
        &with,
    );
    assert!(!run.ok);
    assert!(
        run.err
            .contains("the account's password is the master password"),
        "{}",
        run.err
    );

    let run = lockdown(
        &laptop,
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "me@example.com",
        ],
        None,
        &with,
    );
    assert!(run.ok, "{}", run.err);
    assert!(
        run.out.contains(&format!(
            "account me@example.com registered at {url}; your vault moved there"
        )),
        "{}",
        run.out
    );
    assert!(!run.out.contains("Secret Key:"), "{}", run.out);
    assert!(!data_of(&laptop).join("local-server").exists());
    let run = lockdown(&laptop, &["sync"], None, &with);
    assert!(run.ok, "{}", run.err);
    let run = lockdown(&laptop, &["status"], None, &with);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains(&url), "{}", run.out);

    // The phone comes back with the kit init printed and the master
    // password, which is the account's password now and the phone's master
    // password too (docs/plan/44-cli-join-sealed.md).
    let run = lockdown(
        &phone,
        &[
            "recover",
            "--server",
            &url,
            "--login",
            "me@example.com",
            "--password-stdin",
            "--kit-stdin",
        ],
        Some(&format!("{MASTER}\n{sk}\n{rc}\n")),
        &with,
    );
    assert!(run.ok, "{}", run.err);
    assert!(data_of(&phone).join("device-key.sealed.cbor").exists());
    assert!(!data_of(&phone).join("device-key.cbor").exists());
    let run = lockdown(&phone, &["list"], None, &with);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("bank.example"), "{}", run.out);

    // A vault on a server already can't move again from here.
    let run = lockdown(
        &laptop,
        &[
            "account",
            "register",
            "--server",
            &url,
            "--login",
            "again@example.com",
        ],
        None,
        &with,
    );
    assert!(!run.ok);
    assert!(
        run.err.contains("already syncs with a server"),
        "{}",
        run.err
    );
}
