//! The `lockdown-server` binary: flags and environment in, the axum
//! server out. The library (`lockdown_server`) holds everything else so
//! tests can run the same server in process.
//!
//! ```text
//! lockdown-server [--listen ADDR] [--data DIR] [--server-key PATH]
//!                 [--tls-cert PEM --tls-key PEM] [--tls-pq-only]
//!                 [--tls-reload-secs N] [--hsts-max-age SECS]
//!                 [--redirect-http ADDR] [--insecure-http]
//!                 [--trusted-proxy CIDR]... [--blob-dir DIR]
//!                 [--session-ttl SECS] [--signups open|closed]
//!                 [--max-blob BYTES] [--blob-grace SECS]
//!                 [--breach-feed-url URL] [--breach-feed-key KEY]
//!                 [--breach-scan-hours N] [--breach-scan-pause-ms MS]
//! lockdown-server backup keygen | create --out FILE | verify --in FILE
//!                        | restore --in FILE --data DIR
//! lockdown-server healthcheck [--url URL]
//! ```
//!
//! Every flag has an environment variable (`LOCKDOWN_LISTEN`,
//! `LOCKDOWN_DATA`, `LOCKDOWN_SERVER_KEY`, `LOCKDOWN_TLS_CERT`,
//! `LOCKDOWN_TLS_KEY`, `LOCKDOWN_TLS_PQ_ONLY`, `LOCKDOWN_TLS_RELOAD_SECS`,
//! `LOCKDOWN_HSTS_MAX_AGE`, `LOCKDOWN_REDIRECT_HTTP`,
//! `LOCKDOWN_INSECURE_HTTP`, `LOCKDOWN_TRUSTED_PROXIES` (comma-separated),
//! `LOCKDOWN_BLOB_DIR`, `LOCKDOWN_SESSION_TTL`, `LOCKDOWN_SIGNUPS`,
//! `LOCKDOWN_MAX_BLOB`, `LOCKDOWN_BLOB_GRACE`, `LOCKDOWN_BREACH_FEED_URL`,
//! `LOCKDOWN_BREACH_FEED_KEY`, `LOCKDOWN_BREACH_SCAN_HOURS`,
//! `LOCKDOWN_BREACH_SCAN_PAUSE_MS`, `LOCKDOWN_BACKUP_KEY`) so a container
//! needs no command line. The email breach monitor runs only with a feed
//! URL (the HIBP v3 `breachedaccount` API needs a paid key).
//!
//! **Plain HTTP is for this machine and tests.** Without `--tls-cert` the
//! server listens on a loopback address only; anywhere else it refuses to
//! start unless `--insecure-http` says a reverse proxy in front of it does
//! the TLS (docs/self-host.md).

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use lockdown_server::backup::{self, BackupKey};
use lockdown_server::client_addr::Cidr;
use lockdown_server::keys::ServerKeys;
use lockdown_server::storage::SqliteStorage;
use lockdown_server::{AppState, Config};

/// The Lockdown server: one binary for cloud and self-host.
#[derive(Parser, Debug)]
#[command(
    name = "lockdown-server",
    version,
    about,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// The encrypted server backup under a Backup Key you hold.
    #[command(subcommand)]
    Backup(BackupCommand),
    /// Exits 0 when the server here answers `/healthz` (for a container's
    /// health check; the certificate isn't checked, only liveness).
    Healthcheck {
        /// The address to ask; default: from `--listen` and the TLS flags.
        #[arg(long)]
        url: Option<String>,
        /// Where the server listens.
        #[arg(long, env = "LOCKDOWN_LISTEN", default_value = "127.0.0.1:8787")]
        listen: String,
        /// Set when the server serves TLS itself.
        #[arg(long, env = "LOCKDOWN_TLS_CERT", value_name = "PEM")]
        tls_cert: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum BackupCommand {
    /// Prints a new Backup Key. Keep it with your recovery kit.
    Keygen,
    /// Writes a sealed snapshot of the database, the key file and the
    /// attachment files.
    Create {
        #[command(flatten)]
        place: DataPlace,
        /// The backup file to write (never over an existing one).
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },
    /// Opens a backup and checks everything in it, changing nothing.
    Verify {
        /// The backup file.
        #[arg(long = "in", value_name = "FILE")]
        input: PathBuf,
    },
    /// Writes a backup back into an empty data directory.
    Restore {
        /// The backup file.
        #[arg(long = "in", value_name = "FILE")]
        input: PathBuf,
        /// The (empty) data directory to restore into.
        #[arg(long, env = "LOCKDOWN_DATA", value_name = "DIR")]
        data: PathBuf,
        /// Where attachment files go; default `<data>/blobs`.
        #[arg(long, env = "LOCKDOWN_BLOB_DIR", value_name = "DIR")]
        blob_dir: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
struct DataPlace {
    /// Where the database lives.
    #[arg(long, env = "LOCKDOWN_DATA", value_name = "DIR")]
    data: Option<PathBuf>,
    /// The server key file; default `server-key.cbor` in the data directory.
    #[arg(long, env = "LOCKDOWN_SERVER_KEY", value_name = "PATH")]
    server_key: Option<PathBuf>,
    /// Where attachment files live, when they are kept as files.
    #[arg(long, env = "LOCKDOWN_BLOB_DIR", value_name = "DIR")]
    blob_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ServeArgs {
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
    /// The certificate chain (PEM) for TLS on the server itself.
    #[arg(
        long,
        env = "LOCKDOWN_TLS_CERT",
        value_name = "PEM",
        requires = "tls_key"
    )]
    tls_cert: Option<PathBuf>,
    /// The certificate's private key (PEM: PKCS#8, SEC1 or PKCS#1).
    #[arg(
        long,
        env = "LOCKDOWN_TLS_KEY",
        value_name = "PEM",
        requires = "tls_cert"
    )]
    tls_key: Option<PathBuf>,
    /// Accept only the hybrid post-quantum key exchange (X25519MLKEM768);
    /// clients without it get no connection.
    #[arg(long, env = "LOCKDOWN_TLS_PQ_ONLY")]
    tls_pq_only: bool,
    /// Seconds between checks for a renewed certificate on disk; 0 turns
    /// the check off (SIGHUP still reloads).
    #[arg(long, env = "LOCKDOWN_TLS_RELOAD_SECS", default_value_t = 60)]
    tls_reload_secs: u64,
    /// `Strict-Transport-Security` max-age in seconds over TLS; 0 sends none.
    #[arg(long, env = "LOCKDOWN_HSTS_MAX_AGE", default_value_t = 31_536_000)]
    hsts_max_age: u64,
    /// Also listen for plain HTTP here and redirect it to HTTPS.
    #[arg(
        long,
        env = "LOCKDOWN_REDIRECT_HTTP",
        value_name = "ADDR",
        requires = "tls_cert"
    )]
    redirect_http: Option<String>,
    /// Allow plain HTTP on an address other than this machine's: only when
    /// a reverse proxy in front of the server does TLS.
    #[arg(long, env = "LOCKDOWN_INSECURE_HTTP")]
    insecure_http: bool,
    /// A reverse proxy (IP or CIDR) whose `X-Forwarded-For` is believed.
    #[arg(
        long = "trusted-proxy",
        env = "LOCKDOWN_TRUSTED_PROXIES",
        value_delimiter = ',',
        value_name = "CIDR"
    )]
    trusted_proxies: Vec<String>,
    /// Keep attachment bodies as files here instead of in the database.
    #[arg(long, env = "LOCKDOWN_BLOB_DIR", value_name = "DIR")]
    blob_dir: Option<PathBuf>,
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
    /// How many bytes of attachments the vaults one account owns may hold;
    /// 0 is no limit.
    #[arg(long, env = "LOCKDOWN_ATTACHMENT_QUOTA", default_value_t = lockdown_server::DEFAULT_ATTACHMENT_QUOTA)]
    attachment_quota: u64,
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

fn data_dir(data: Option<PathBuf>) -> Result<PathBuf, String> {
    match data {
        Some(d) => Ok(d),
        None => default_data_dir()
            .ok_or_else(|| "no data directory: pass --data or set LOCKDOWN_DATA".into()),
    }
}

fn open_storage(
    data: &std::path::Path,
    blob_dir: Option<&std::path::Path>,
) -> Result<SqliteStorage, String> {
    let storage = SqliteStorage::open(&data.join(backup::DB_NAME)).map_err(|e| e.to_string())?;
    match blob_dir {
        Some(dir) => storage.with_blob_dir(dir).map_err(|e| e.to_string()),
        None => Ok(storage),
    }
}

fn serve(cli: ServeArgs) -> Result<(), String> {
    let data = data_dir(cli.data)?;
    let key_path = cli
        .server_key
        .unwrap_or_else(|| data.join(backup::KEY_NAME));
    let storage = open_storage(&data, cli.blob_dir.as_deref())?;
    let keys = ServerKeys::load_or_create(&key_path).map_err(|e| e.to_string())?;
    let trusted_proxies = cli
        .trusted_proxies
        .iter()
        .filter(|t| !t.trim().is_empty())
        .map(|t| Cidr::parse(t))
        .collect::<Result<Vec<_>, _>>()?;
    let config = Config {
        session_ttl: Duration::from_secs(cli.session_ttl),
        signups_open: cli.signups == Signups::Open,
        max_blob_bytes: cli.max_blob,
        blob_grace: Duration::from_secs(cli.blob_grace),
        attachment_quota: (cli.attachment_quota > 0).then_some(cli.attachment_quota),
        trusted_proxies,
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
    let groups = if cli.tls_pq_only {
        lockdown_tls::Groups::HybridOnly
    } else {
        lockdown_tls::Groups::PreferHybrid
    };
    let tls_files = cli.tls_cert.zip(cli.tls_key);
    let cert = match &tls_files {
        Some((cert, key)) => Some(
            lockdown_tls::ReloadingCert::load(cert, key, Arc::new(lockdown_tls::provider(groups)))
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("couldn't start the runtime: {e}"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&cli.listen)
            .await
            .map_err(|e| format!("couldn't listen on {}: {e}", cli.listen))?;
        let local = listener
            .local_addr()
            .map_err(|e| format!("couldn't read the listening address: {e}"))?;
        let stop = async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("lockdown-server: stopping");
        };
        let Some(cert) = cert else {
            if !local.ip().is_loopback() && !cli.insecure_http {
                return Err(format!(
                    "{local} isn't this machine, and there is no TLS: give --tls-cert and --tls-key, \
                     or --insecure-http if a reverse proxy in front of the server does TLS"
                ));
            }
            eprintln!(
                "lockdown-server listening on http://{local} — PLAIN HTTP, for this machine, tests, or behind a TLS proxy only (data {}, signups {:?}, web vault {})",
                data.display(),
                cli.signups,
                if web { "at /vault/" } else { "off" }
            );
            return lockdown_server::serve(listener, state, stop)
                .await
                .map_err(|e| e.to_string());
        };
        let config = lockdown_tls::server_config(cert.clone(), groups).map_err(|e| e.to_string())?;
        lockdown_server::tls::spawn_reloader(
            cert,
            Some(Duration::from_secs(cli.tls_reload_secs)),
        );
        if let Some(addr) = &cli.redirect_http {
            let plain = tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| format!("couldn't listen on {addr}: {e}"))?;
            let app = lockdown_server::tls::redirect_app(local.port());
            eprintln!("lockdown-server: plain HTTP on {addr} redirects to HTTPS");
            tokio::spawn(async move {
                let _ = axum::serve(plain, app).await;
            });
        }
        eprintln!(
            "lockdown-server listening on https://{local} (TLS 1.3, {}; data {}, signups {:?}, web vault {})",
            if cli.tls_pq_only {
                "X25519MLKEM768 only"
            } else {
                "X25519MLKEM768 preferred, X25519 for older clients"
            },
            data.display(),
            cli.signups,
            if web { "at /vault/" } else { "off" }
        );
        lockdown_server::serve_tls(
            listener,
            state,
            lockdown_server::TlsServing {
                config: Arc::new(config),
                hsts_max_age: cli.hsts_max_age,
                stats: Arc::new(lockdown_server::tls::TlsStats::default()),
            },
            stop,
        )
        .await
        .map_err(|e| e.to_string())
    })
}

/// The Backup Key from `LOCKDOWN_BACKUP_KEY`, or one line on standard input.
fn backup_key() -> Result<BackupKey, String> {
    if let Some(text) = std::env::var_os("LOCKDOWN_BACKUP_KEY").filter(|v| !v.is_empty()) {
        return BackupKey::parse(&text.to_string_lossy());
    }
    let mut line = zeroize::Zeroizing::new(String::new());
    std::io::stdin()
        .take(4096)
        .read_to_string(&mut line)
        .map_err(|e| format!("couldn't read the Backup Key from standard input: {e}"))?;
    if line.trim().is_empty() {
        return Err("give the Backup Key on standard input or in LOCKDOWN_BACKUP_KEY".into());
    }
    BackupKey::parse(&line)
}

fn run_backup(command: BackupCommand) -> Result<(), String> {
    match command {
        BackupCommand::Keygen => {
            let key = BackupKey::generate()?;
            println!("{}", key.display().as_str());
            eprintln!(
                "This is your server's Backup Key. It is shown once and the server doesn't keep it: \
                 write it down with your recovery kit. Without it no backup can be opened."
            );
            Ok(())
        }
        BackupCommand::Create { place, out } => {
            let key = backup_key()?;
            let data = data_dir(place.data)?;
            let sources = backup::Sources {
                db: data.join(backup::DB_NAME),
                server_key: place
                    .server_key
                    .unwrap_or_else(|| data.join(backup::KEY_NAME)),
                blob_dir: place.blob_dir,
            };
            if !sources.db.exists() {
                return Err(format!("there is no server database in {}", data.display()));
            }
            let summary = backup::create(&sources, &key, &out, lockdown_vault::Timestamp::now().0)?;
            println!("wrote {}: {summary}", out.display());
            Ok(())
        }
        BackupCommand::Verify { input } => {
            let key = backup_key()?;
            let (summary, facts) = backup::verify(&input, &key)?;
            println!("{summary}");
            println!(
                "every file matches its hash; the database opens: {} accounts, {} vaults, {} log entries",
                facts.accounts, facts.vaults, facts.log_size
            );
            Ok(())
        }
        BackupCommand::Restore {
            input,
            data,
            blob_dir,
        } => {
            let key = backup_key()?;
            let summary = backup::restore(&input, &key, &data, blob_dir.as_deref())?;
            println!("restored into {}: {summary}", data.display());
            Ok(())
        }
    }
}

fn healthcheck(url: Option<String>, listen: &str, tls: bool) -> Result<(), String> {
    let url = url.unwrap_or_else(|| {
        let addr = listen
            .replace("0.0.0.0", "127.0.0.1")
            .replace("[::]", "[::1]");
        format!("{}://{addr}/healthz", if tls { "https" } else { "http" })
    });
    lockdown_tls::install_default_provider();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        // Liveness only, on this machine: the certificate is for the
        // public name, not for 127.0.0.1.
        .tls_danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    let res = client.get(&url).send().map_err(|e| format!("{url}: {e}"))?;
    let ok = res.status().is_success();
    let body = res.text().unwrap_or_default();
    if ok && body.trim() == "ok" {
        Ok(())
    } else {
        Err(format!("{url} answered, but not ok"))
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Backup(b)) => run_backup(b),
        Some(Command::Healthcheck {
            url,
            listen,
            tls_cert,
        }) => healthcheck(url, &listen, tls_cert.is_some()),
        None => serve(cli.serve),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lockdown-server: {e}");
            ExitCode::FAILURE
        }
    }
}
