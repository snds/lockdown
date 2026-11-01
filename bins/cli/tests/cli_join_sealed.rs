//! A computer added from the command line is sealed like the first one
//! (docs/plan/44-cli-join-sealed.md, D514): `account login` and `recover`
//! on an empty computer leave only the sealed key file, under the account's
//! password, which every later command then needs; without a terminal and
//! without that password they refuse and write nothing; a wrong password
//! writes nothing; the joined computer opens in the desktop core with the
//! same password; and a computer joined before this (a plain key file)
//! keeps opening with the notice until `password set` seals it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use lockdown_server::{AppState, Config, run_in_background};

const PASSWORD: &str = "correct horse battery staple";
const LOGIN: &str = "me@example.com";
const NEEDS_PASSWORD: &str = "Joining seals this computer with your account's password, which becomes its master password here. Give it with --master-password-stdin or LOCKDOWN_MASTER_PASSWORD, or run the command in a terminal to be asked.";
const NOT_THE_SAME: &str =
    "Your account's password becomes this computer's master password, so they are one password.";
const SEALED: &str = "this computer is locked with your account's password as its master password";
const NOTICE: &str = "This vault has no master password yet.";

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
        .env("HOSTNAME", "test-phone")
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

/// Every file and folder under `dir`, for "nothing was written".
fn everything_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !dir.exists() {
        return found;
    }
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

/// Only the sealed key file, never the plain one.
fn sealed_only(root: &Path) {
    let data = data_of(root);
    assert!(data.join("device-key.sealed.cbor").exists());
    assert!(!data.join("device-key.cbor").exists());
}

/// A join the identity refused after the sign-in: no identity, and the
/// keys kept sealed, so the next try is the same device (the server's log
/// already saw it sign in).
fn kept_sealed(root: &Path) {
    sealed_only(root);
    assert!(!root.exists());
}

/// The first computer: an account on the server with a kit and one item.
/// It is an install from before the master password, which is the quickest
/// way to an account here; the computers that join are what is tested.
fn first_computer(dir: &Path, url: &str) -> (String, String) {
    let laptop = dir.join("laptop/snds/identity");
    lockdown_devtools::legacy::make(&laptop, None).unwrap();
    let run = lockdown(
        &laptop,
        &[
            "account",
            "register",
            "--server",
            url,
            "--login",
            LOGIN,
            "--password-stdin",
        ],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    let value = |label: &str| {
        run.out
            .lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap_or_else(|| panic!("no {label} in {}", run.out))
            .trim()
            .to_string()
    };
    let kit = (value("Secret Key:"), value("Recovery code:"));
    let added = lockdown(
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
        &[],
    );
    assert!(added.ok, "{}", added.err);
    let synced = lockdown(&laptop, &["sync"], None, &[]);
    assert!(synced.ok, "{}", synced.err);
    kit
}

fn server() -> lockdown_server::RunningServer {
    // Several sign-ins in a few seconds; the login brake (D57) is loosened.
    let config = Config {
        per_login: (100, 10.0),
        ..Config::default()
    };
    run_in_background(Arc::new(AppState::in_memory(config).unwrap())).unwrap()
}

#[test]
fn without_a_terminal_or_a_master_password_a_join_refuses_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let phone = dir.path().join("phone/snds/identity");
    let url = "https://127.0.0.1:9";
    let login = ["account", "login", "--server", url, "--login", LOGIN];
    let recover = ["recover", "--server", url, "--login", LOGIN];
    let kit = [
        ("LOCKDOWN_SECRET_KEY", "A1-00000-00000-00000-00000-000000"),
        ("LOCKDOWN_RECOVERY_CODE", "0000"),
    ];
    // Nothing at all, and the account's password alone (the variable or the
    // stdin line): the account's password never seals by itself.
    for (args, stdin, env) in [
        (login.to_vec(), None, vec![]),
        (login.to_vec(), None, vec![("LOCKDOWN_PASSWORD", PASSWORD)]),
        (
            [&login[..], &["--password-stdin"]].concat(),
            Some(format!("{PASSWORD}\n")),
            vec![],
        ),
        (recover.to_vec(), None, kit.to_vec()),
        (
            recover.to_vec(),
            None,
            [&kit[..], &[("LOCKDOWN_PASSWORD", PASSWORD)]].concat(),
        ),
    ] {
        let run = lockdown(&phone, &args, stdin.as_deref(), &env);
        assert!(!run.ok, "{args:?}");
        assert!(run.err.contains(NEEDS_PASSWORD), "{args:?}: {}", run.err);
        assert!(!run.err.contains(PASSWORD), "{}", run.err);
        assert!(everything_in(dir.path()).is_empty(), "{args:?}");
    }
    // An account password that isn't the master password.
    let run = lockdown(
        &phone,
        &[&login[..], &["--password-stdin"]].concat(),
        Some("another password entirely\n"),
        &[("LOCKDOWN_MASTER_PASSWORD", PASSWORD)],
    );
    assert!(!run.ok);
    assert!(run.err.contains(NOT_THE_SAME), "{}", run.err);
    assert!(everything_in(dir.path()).is_empty());
}

#[test]
fn account_login_on_an_empty_computer_seals_it_with_the_account_password() {
    let dir = tempfile::tempdir().unwrap();
    let server = server();
    let url = server.url();
    let (sk, _) = first_computer(dir.path(), &url);
    let phone = dir.path().join("phone/snds/identity");
    let login = [
        "account",
        "login",
        "--server",
        url.as_str(),
        "--login",
        LOGIN,
        "--secret-key-stdin",
    ];
    let with = [("LOCKDOWN_MASTER_PASSWORD", PASSWORD)];

    // A wrong password: refused at the sign-in, nothing written.
    let run = lockdown(
        &phone,
        &login,
        Some(&format!("{sk}\n")),
        &[("LOCKDOWN_MASTER_PASSWORD", "wrong horse battery")],
    );
    assert!(!run.ok);
    assert!(run.err.contains("didn't go through"), "{}", run.err);
    assert!(everything_in(&dir.path().join("phone")).is_empty());

    // A wrong Secret Key: refused by the account slot; the keys stay sealed
    // for the next try, and nothing is in the clear.
    let run = lockdown(
        &phone,
        &login,
        Some("A1-00000-00000-00000-00000-000000\n"),
        &with,
    );
    assert!(!run.ok);
    assert!(run.err.contains("check the Secret Key"), "{}", run.err);
    kept_sealed(&phone);

    // The right ones: joined, sealed, and the password never printed.
    let run = lockdown(&phone, &login, Some(&format!("{sk}\n")), &with);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("now enrolled"), "{}", run.out);
    assert!(run.out.contains(SEALED), "{}", run.out);
    assert!(!run.out.contains(PASSWORD) && !run.err.contains(PASSWORD));
    assert!(!run.err.contains(NOTICE), "{}", run.err);
    sealed_only(&phone);
    assert_eq!(
        lockdown_app::onboarding::Onboarding::load(&data_of(&phone))
            .name
            .as_deref(),
        Some(LOGIN)
    );

    // Every later command needs the master password.
    let run = lockdown(&phone, &["sync"], None, &[]);
    assert!(!run.ok);
    assert!(
        run.err.contains("locked with a master password"),
        "{}",
        run.err
    );
    let run = lockdown(&phone, &["sync"], None, &with);
    assert!(run.ok, "{}", run.err);
    let run = lockdown(&phone, &["list"], None, &with);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("bank.example"), "{}", run.out);
    assert!(!run.err.contains(NOTICE), "{}", run.err);

    // The desktop core opens it with the same password, and not without.
    let app = core(&phone);
    assert!(app.unlock().is_err(), "opened without the master password");
    assert!(app.unlock_with_password("not the one at all").is_err());
    let status = app.unlock_with_password(PASSWORD).unwrap();
    assert_eq!(status.live_items, 1);
    app.lock().unwrap();
}

#[test]
fn recover_on_an_empty_computer_seals_it_under_the_password_and_the_kit() {
    let dir = tempfile::tempdir().unwrap();
    let server = server();
    let url = server.url();
    let (sk, rc) = first_computer(dir.path(), &url);
    let phone = dir.path().join("phone/snds/identity");
    let recover = [
        "--master-password-stdin",
        "recover",
        "--server",
        url.as_str(),
        "--login",
        LOGIN,
        "--kit-stdin",
    ];

    // A wrong password (the first stdin line): nothing written.
    let run = lockdown(
        &phone,
        &recover,
        Some(&format!("wrong horse battery\n{sk}\n{rc}\n")),
        &[],
    );
    assert!(!run.ok);
    assert!(run.err.contains("didn't go through"), "{}", run.err);
    assert!(everything_in(&dir.path().join("phone")).is_empty());

    // A wrong recovery code: refused after the sign-in; the keys stay
    // sealed for the next try, and nothing is in the clear.
    let run = lockdown(
        &phone,
        &recover,
        Some(&format!("{PASSWORD}\n{sk}\n{}0000\n", &rc[..rc.len() - 4])),
        &[],
    );
    assert!(!run.ok);
    assert!(run.err.contains("recovery kit didn't open"), "{}", run.err);
    kept_sealed(&phone);

    // The right kit: recovered, the vault pulled, sealed under the password
    // and under the kit (so a forgotten password can be reset here).
    let run = lockdown(
        &phone,
        &recover,
        Some(&format!("{PASSWORD}\n{sk}\n{rc}\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("recovered: this device"), "{}", run.out);
    assert!(run.out.contains("pulled (1 live items)"), "{}", run.out);
    assert!(run.out.contains(SEALED), "{}", run.out);
    assert!(!run.out.contains(PASSWORD) && !run.err.contains(PASSWORD));
    sealed_only(&phone);
    assert!(
        lockdown_app::local_unlock::profile_of(&data_of(&phone))
            .unwrap()
            .is_some(),
        "the kit slot"
    );
    let run = lockdown(&phone, &["list"], None, &[]);
    assert!(!run.ok);
    let run = lockdown(
        &phone,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", PASSWORD)],
    );
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("bank.example"), "{}", run.out);

    let app = core(&phone);
    assert!(app.unlock().is_err());
    assert_eq!(app.unlock_with_password(PASSWORD).unwrap().live_items, 1);
    app.lock().unwrap();
}

#[test]
fn a_computer_joined_before_this_still_opens_and_password_set_seals_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = server();
    let url = server.url();
    let (sk, _) = first_computer(dir.path(), &url);
    let phone = dir.path().join("phone/snds/identity");

    // Joined the way it was before: the plain key file (the desktop's join
    // still makes it, and seals right after; the old command line didn't).
    let app = core(&phone);
    app.account_login(&url, LOGIN, PASSWORD, Some(&sk)).unwrap();
    drop(app);
    let data = data_of(&phone);
    assert!(data.join("device-key.cbor").exists());

    let run = lockdown(&phone, &["sync"], None, &[]);
    assert!(run.ok, "{}", run.err);
    assert!(run.err.contains(NOTICE), "{}", run.err);
    let run = lockdown(&phone, &["list"], None, &[]);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("bank.example"), "{}", run.out);

    // `password set` checks the account's password at its server and seals.
    let run = lockdown(
        &phone,
        &["password", "set", "--password-stdin"],
        Some(&format!("{PASSWORD}\n")),
        &[],
    );
    assert!(run.ok, "{}", run.err);
    sealed_only(&phone);
    let run = lockdown(&phone, &["list"], None, &[]);
    assert!(!run.ok);
    let run = lockdown(
        &phone,
        &["list"],
        None,
        &[("LOCKDOWN_MASTER_PASSWORD", PASSWORD)],
    );
    assert!(run.ok, "{}", run.err);
    assert!(!run.err.contains(NOTICE), "{}", run.err);
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
fn at_a_terminal_a_join_asks_for_the_password_once_with_echo_off() {
    let dir = tempfile::tempdir().unwrap();
    let server = server();
    let url = server.url();
    let (sk, rc) = first_computer(dir.path(), &url);
    let phone = dir.path().join("phone/snds/identity");
    let kit = [
        ("LOCKDOWN_SECRET_KEY", sk.as_str()),
        ("LOCKDOWN_RECOVERY_CODE", rc.as_str()),
    ];
    let run = Terminal::open().run(
        &phone,
        &["recover", "--server", &url, "--login", LOGIN],
        &format!("{PASSWORD}\n"),
        &kit,
    );
    assert!(run.ok, "{}", run.err);
    assert!(
        run.err.contains(&format!(
            "Password for {LOGIN} at {url} (this computer's master password from now on): "
        )),
        "{}",
        run.err
    );
    // Asked once: the vault opened right after with the same password.
    assert_eq!(run.err.matches("Password for").count(), 1, "{}", run.err);
    assert!(!run.err.contains("Master password for"), "{}", run.err);
    assert!(run.out.contains("pulled (1 live items)"), "{}", run.out);
    assert!(!run.out.contains(PASSWORD) && !run.err.contains(PASSWORD));
    sealed_only(&phone);

    // Later commands at a terminal ask by the account's name.
    let run = Terminal::open().run(&phone, &["list"], &format!("{PASSWORD}\n"), &[]);
    assert!(run.ok, "{}", run.err);
    assert!(
        run.err.contains(&format!("Master password for {LOGIN}: ")),
        "{}",
        run.err
    );
}
