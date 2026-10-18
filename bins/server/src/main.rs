//! The `lockdown-server` binary: flags and environment in, the axum
//! server out. The library (`lockdown_server`) holds everything else so
//! tests can run the same server in process.
//!
//! ```text
//! lockdown-server [--listen ADDR] [--data DIR] [--server-key PATH]
//!                 [--session-ttl SECS] [--signups open|closed]
//!                 [--max-blob BYTES] [--blob-grace SECS]
//!                 [--breach-feed-url URL] [--breach-feed-key KEY]
//!                 [--breach-scan-hours N] [--breach-scan-pause-ms MS]
//! ```
//!
//! Every flag has an environment variable (`LOCKDOWN_LISTEN`,
//! `LOCKDOWN_DATA`, `LOCKDOWN_SERVER_KEY`, `LOCKDOWN_SESSION_TTL`,
//! `LOCKDOWN_SIGNUPS`, `LOCKDOWN_MAX_BLOB`, `LOCKDOWN_BLOB_GRACE`,
//! `LOCKDOWN_BREACH_FEED_URL`, `LOCKDOWN_BREACH_FEED_KEY`,
//! `LOCKDOWN_BREACH_SCAN_HOURS`, `LOCKDOWN_BREACH_SCAN_PAUSE_MS`) so a
//! container needs no command line. The email breach monitor runs only
//! with a feed URL (the HIBP v3 `breachedaccount` API needs a paid key).

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
    /// The email breach feed (HIBP v3 `breachedaccount` shape), e.g.
    /// `https://haveibeenpwned.com/api/v3/breachedaccount/`. Without it the
    /// breach monitor is off and nothing is sent anywhere.
    #[arg(long, env = "LOCKDOWN_BREACH_FEED_URL", value_name = "URL")]
    breach_feed_url: Option<String>,
    /// The feed's API key (HIBP needs a paid one).
    #[arg(
        long,
        env = "LOCKDOWN_BREACH_FEED_KEY",
        value_name = "KEY",
        hide_env_values = true
    )]
    breach_feed_key: Option<String>,
    /// Hours between breach checks of every account.
    #[arg(long, env = "LOCKDOWN_BREACH_SCAN_HOURS", default_value_t = 24)]
    breach_scan_hours: u64,
    /// Milliseconds between two accounts in one check, to stay under the
    /// feed's rate limit (HIBP's smallest plan allows ten a minute).
    #[arg(long, env = "LOCKDOWN_BREACH_SCAN_PAUSE_MS", default_value_t = 6500)]
    breach_scan_pause_ms: u64,
    /// The built web vault (`apps/web/dist`) to serve at `/vault/`.
    /// Without it no web vault is served.
    #[arg(long, env = "LOCKDOWN_WEB_VAULT", value_name = "DIR")]
    web_vault: Option<PathBuf>,
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
        web_vault: match cli.web_vault {
            Some(dir) if dir.join("index.html").is_file() => Some(dir),
            Some(dir) => {
                return Err(format!(
                    "{} has no index.html; build the web vault first (apps/web: pnpm build)",
                    dir.display()
                ));
            }
            None => None,
        },
        ..Config::default()
    };
    let web = config.web_vault.is_some();
    let state = Arc::new(AppState::new(Box::new(storage), keys, config));
    if let Some(url) = cli.breach_feed_url.as_deref() {
        let feed = lockdown_server::breach_monitor::HttpBreachFeed::new(url, cli.breach_feed_key)?;
        lockdown_server::breach_monitor::spawn(
            state.clone(),
            Box::new(feed),
            Duration::from_secs(cli.breach_scan_hours.max(1) * 3600),
            Duration::from_millis(cli.breach_scan_pause_ms),
        )
        .map_err(|e| format!("couldn't start the breach monitor: {e}"))?;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("couldn't start the runtime: {e}"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&cli.listen)
            .await
            .map_err(|e| format!("couldn't listen on {}: {e}", cli.listen))?;
        eprintln!(
            "lockdown-server listening on {} (data {}, signups {:?}, web vault {})",
            cli.listen,
            data.display(),
            cli.signups,
            if web { "at /vault/" } else { "off" }
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
