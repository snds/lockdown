//! `lockdown-shared-fixture`: two people and a shared vault on a running
//! server, for the web vault's end-to-end spec of what is held back
//! (docs/plan/49-shared-vault-quiet.md). A developer tool: never in a
//! release.
//!
//! - `setup --server URL --home DIR`: Alice and Bob register; Alice's only
//!   vault is shared with Bob, who writes "Bob one". Prints Alice's login,
//!   her Secret Key and the vault's id, one per line.
//! - `write --home DIR --title T [--db FILE]`: Bob writes a note into it
//!   and syncs; with `--db`, the head he just wrote is bent at once (as
//!   `bend` does), before anyone else syncs it.
//! - `bend --db FILE --home DIR`: the server's newest head of the vault gets
//!   a bent device signature, as a hostile server would; the good head is
//!   kept in DIR.
//! - `restore --db FILE --home DIR`: the good head goes back.
//! - `roll-back --db FILE --home DIR`: the server drops the vault's newest
//!   head, as a server that lost or hid it would; `put-back` returns it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lockdown_app::{App, GateInput, LockPolicy, Paths};
use lockdown_vault::ManifestHead;

const PASSWORD: &str = "correct horse battery staple";

fn person(home: &Path, name: &str) -> App {
    let paths = Paths::resolve(Some(home.join(name).join("identity")), None)
        .unwrap_or_else(|e| panic!("{e}"));
    let app = App::start(paths, None, LockPolicy::default());
    app.open_or_create_plain()
        .unwrap_or_else(|e| panic!("{e:?}"));
    app
}

fn login(home: &Path, name: &str) -> String {
    let stamp = std::fs::read_to_string(home.join("stamp")).unwrap_or_default();
    format!("{name}-{}@example.com", stamp.trim())
}

fn setup(server: &str, home: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
    let stamp = format!("{}", lockdown_vault::Timestamp::now().0);
    std::fs::write(home.join("stamp"), &stamp).map_err(|e| e.to_string())?;
    let alice = person(home, "alice");
    let bob = person(home, "bob");
    let reg = alice
        .account_register(server, &login(home, "alice"), PASSWORD, None)
        .map_err(|e| e.message)?;
    let page =
        String::from_utf8(reg.kit.ok_or("no kit")?.expose().to_vec()).map_err(|e| e.to_string())?;
    let secret_key = page
        .lines()
        .find_map(|l| l.strip_prefix("Secret Key:"))
        .ok_or("no Secret Key on the kit")?
        .trim()
        .to_string();
    alice.sync().map_err(|e| e.message)?;
    bob.account_register(server, &login(home, "bob"), PASSWORD, None)
        .map_err(|e| e.message)?;
    bob.sync().map_err(|e| e.message)?;
    let vault = alice.vaults().map_err(|e| e.message)?[0].id.clone();
    alice
        .add_login(
            "https://mail.example.com",
            "alice",
            b"pw".to_vec(),
            Some("Mail"),
        )
        .map_err(|e| e.message)?;
    alice
        .lookup_contact(&login(home, "bob"))
        .map_err(|e| e.message)?;
    alice
        .share_vault(
            &vault,
            &login(home, "bob"),
            "edit",
            GateInput {
                password: Some(PASSWORD.into()),
                confirmed: false,
            },
        )
        .map_err(|e| e.message)?;
    bob.sync().map_err(|e| e.message)?;
    std::fs::write(home.join("vault"), &vault).map_err(|e| e.to_string())?;
    write(home, "Bob one")?;
    Ok(vec![login(home, "alice"), secret_key, vault])
}

fn write(home: &Path, title: &str) -> Result<Vec<String>, String> {
    let bob = person(home, "bob");
    let vault = std::fs::read_to_string(home.join("vault")).map_err(|e| e.to_string())?;
    bob.sync().map_err(|e| e.message)?;
    bob.add_item_in(&vault, "note", Some(title), false, Vec::new())
        .map_err(|e| e.message)?;
    bob.sync().map_err(|e| e.message)?;
    Ok(vec![format!("Bob wrote {title}")])
}

fn bend(db: &Path, home: &Path) -> Result<Vec<String>, String> {
    let vault = std::fs::read_to_string(home.join("vault")).map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    let (seq, body): (i64, Vec<u8>) = conn
        .query_row(
            "SELECT seq, body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&vault],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    std::fs::write(home.join("good-head"), &body).map_err(|e| e.to_string())?;
    let mut head = ManifestHead::from_bytes(&body).map_err(|e| e.to_string())?;
    head.sig[5] ^= 1;
    conn.execute(
        "UPDATE heads SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
        rusqlite::params![head.to_bytes().map_err(|e| e.to_string())?, vault, seq],
    )
    .map_err(|e| e.to_string())?;
    Ok(vec![format!("bent manifest {seq}")])
}

fn restore(db: &Path, home: &Path) -> Result<Vec<String>, String> {
    let vault = std::fs::read_to_string(home.join("vault")).map_err(|e| e.to_string())?;
    let body = std::fs::read(home.join("good-head")).map_err(|e| e.to_string())?;
    let head = ManifestHead::from_bytes(&body).map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE heads SET body = ?1 WHERE vault_id = ?2 AND seq = ?3",
        rusqlite::params![body, vault, head.seq as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(vec![format!("restored manifest {}", head.seq)])
}

fn roll_back(db: &Path, home: &Path) -> Result<Vec<String>, String> {
    let vault = std::fs::read_to_string(home.join("vault")).map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    let (seq, hash, body): (i64, Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT seq, hash, body FROM heads WHERE vault_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&vault],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let before: Vec<u8> = conn
        .query_row(
            "SELECT hash FROM heads WHERE vault_id = ?1 AND seq = ?2",
            rusqlite::params![vault, seq - 1],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    std::fs::write(home.join("dropped-head"), &body).map_err(|e| e.to_string())?;
    std::fs::write(home.join("dropped-hash"), &hash).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM heads WHERE vault_id = ?1 AND seq = ?2",
        rusqlite::params![vault, seq],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE vaults SET head_seq = ?1, head_hash = ?2 WHERE vault_id = ?3",
        rusqlite::params![seq - 1, before, vault],
    )
    .map_err(|e| e.to_string())?;
    Ok(vec![format!("dropped manifest {seq}")])
}

fn put_back(db: &Path, home: &Path) -> Result<Vec<String>, String> {
    let vault = std::fs::read_to_string(home.join("vault")).map_err(|e| e.to_string())?;
    let body = std::fs::read(home.join("dropped-head")).map_err(|e| e.to_string())?;
    let hash = std::fs::read(home.join("dropped-hash")).map_err(|e| e.to_string())?;
    let head = ManifestHead::from_bytes(&body).map_err(|e| e.to_string())?;
    let conn = rusqlite::Connection::open(db).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO heads (vault_id, seq, hash, body) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![vault, head.seq as i64, hash, body],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE vaults SET head_seq = ?1, head_hash = ?2 WHERE vault_id = ?3",
        rusqlite::params![head.seq as i64, hash, vault],
    )
    .map_err(|e| e.to_string())?;
    Ok(vec![format!("put back manifest {}", head.seq)])
}

const USAGE: &str = "usage: lockdown-shared-fixture setup --server URL --home DIR | write --home DIR --title T [--db FILE] | bend --db FILE --home DIR | restore --db FILE --home DIR | roll-back --db FILE --home DIR | put-back --db FILE --home DIR";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let (mut server, mut home, mut db, mut title) = (None, None, None, None);
    while let Some(flag) = args.next() {
        let value = args.next();
        match flag.as_str() {
            "--server" => server = value,
            "--home" => home = value.map(PathBuf::from),
            "--db" => db = value.map(PathBuf::from),
            "--title" => title = value,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(home) = home else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let out = match (cmd.as_str(), server, db, title) {
        ("setup", Some(server), _, _) => setup(&server, &home),
        ("write", _, db, Some(title)) => write(&home, &title).and_then(|mut out| {
            if let Some(db) = db {
                out.extend(bend(&db, &home)?);
            }
            Ok(out)
        }),
        ("bend", _, Some(db), _) => bend(&db, &home),
        ("restore", _, Some(db), _) => restore(&db, &home),
        ("roll-back", _, Some(db), _) => roll_back(&db, &home),
        ("put-back", _, Some(db), _) => put_back(&db, &home),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match out {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lockdown-shared-fixture: {e}");
            ExitCode::FAILURE
        }
    }
}
