//! The `lockdown-server` binary: flags and environment in, the axum
//! server out. The library (`lockdown_server`) holds everything else so
//! tests can run the same server in process.
//!
//! ```text
//! lockdown-server [--listen ADDR] [--data DIR] [--server-key PATH]
//!                 [--session-ttl SECS] [--signups open|closed]
//!                 [--max-blob BYTES] [--blob-grace SECS]
//! ```
//!
//! Every flag has an environment variable (`LOCKDOWN_LISTEN`,
//! `LOCKDOWN_DATA`, `LOCKDOWN_SERVER_KEY`, `LOCKDOWN_SESSION_TTL`,
//! `LOCKDOWN_SIGNUPS`, `LOCKDOWN_MAX_BLOB`, `LOCKDOWN_BLOB_GRACE`) so a
//! container needs no command line.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use lockdown_server::keys::ServerKeys;
use lockdown_server::storage::SqliteStorage;
use lockdown_server::{AppState, Config};

/// The Lockdown server: one binary for cloud and self-host.
#[derive(Parser, Debug)]
#[command(name = "lockdown-server", version, about)]
struct Cli {
    /// Address and port to listen on.
    #[arg(long, env = "LOCKDOWN_LISTEN", default_value = "127.0.0.1:8787")]
    listen: String,
    /// Where the database lives. Default: the platform data directory,
    /// `lockdown-server` under it.
    #[arg(long, env = "LOCKDOWN_DATA", value_name = "DIR")]
    data: Option<PathBuf>,
    /// The server key file (token key and OPAQUE setup), made on first
    /// run. Default: `server-key.cbor` in the data directory.
    #[arg(long, env = "LOCKDOWN_SERVER_KEY", value_name = "PATH")]
    server_key: Option<PathBuf>,
    /// How long a session token lives, in seconds.
    #[arg(long, env = "LOCKDOWN_SESSION_TTL", default_value_t = 8 * 60 * 60)]
    session_ttl: u64,
    /// Whether new accounts may register.
    #[arg(long, env = "LOCKDOWN_SIGNUPS", value_enum, default_value_t = Signups::Open)]
    signups: Signups,
    /// The largest attachment blob accepted, in bytes.
    #[arg(long, env = "LOCKDOWN_MAX_BLOB", default_value_t = lockdown_server::DEFAULT_MAX_BLOB_BYTES)]
    max_blob: u64,
    /// How long a blob nothing names is kept before it is collected, in seconds.
    #[arg(long, env = "LOCKDOWN_BLOB_GRACE", default_value_t = lockdown_server::DEFAULT_BLOB_GRACE.as_secs())]
    blob_grace: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Signups {
    Open,
    Closed,
}

fn default_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir).join("snds").join("lockdown-server"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(local).join("snds").join("lockdown-server"));
    }
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("snds")
                .join("lockdown-server")
        })
}

fn run(cli: Cli) -> Result<(), String> {
    let data = match cli.data {
        Some(d) => d,
        None => default_data_dir().ok_or("no data directory: pass --data or set LOCKDOWN_DATA")?,
    };
    let key_path = cli
        .server_key
        .unwrap_or_else(|| data.join("server-key.cbor"));
    let storage =
        SqliteStorage::open(&data.join("lockdown-server.sqlite")).map_err(|e| e.to_string())?;
    let keys = ServerKeys::load_or_create(&key_path).map_err(|e| e.to_string())?;
    let config = Config {
        session_ttl: Duration::from_secs(cli.session_ttl),
        signups_open: cli.signups == Signups::Open,
        max_blob_bytes: cli.max_blob,
        blob_grace: Duration::from_secs(cli.blob_grace),
        ..Config::default()
    };
    let state = Arc::new(AppState::new(Box::new(storage), keys, config));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("couldn't start the runtime: {e}"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&cli.listen)
            .await
            .map_err(|e| format!("couldn't listen on {}: {e}", cli.listen))?;
        eprintln!(
            "lockdown-server listening on {} (data {}, signups {:?})",
            cli.listen,
            data.display(),
            cli.signups
        );
        lockdown_server::serve(listener, state, async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("lockdown-server: stopping");
        })
        .await
        .map_err(|e| e.to_string())
    })
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lockdown-server: {e}");
            ExitCode::FAILURE
        }
    }
}
