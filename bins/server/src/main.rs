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
//! lockdown-server security-keys list|clear --login NAME [--data DIR]
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
use lockdown_server::db::PgOptions;
use lockdown_server::keys::ServerKeys;
use lockdown_server::storage::SqlStorage;
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
    /// The account's security keys, for the operator: the way back in for
    /// someone who lost every key and every signed-in device.
    #[command(subcommand)]
    SecurityKeys(SecurityKeysCommand),
    /// The operator's admin page at `/admin`: its key, and invites.
    #[command(subcommand)]
    Admin(AdminCommand),
}

#[derive(Subcommand, Debug)]
enum AdminCommand {
    /// Sets the operator key that opens `/admin`: a new random one, printed
    /// once, or with `--stdin` one you type (16 characters or more). Only
    /// its hash is kept.
    SetKey {
        #[command(flatten)]
        store: StoreArgs,
        /// Read the key from standard input instead of making one.
        #[arg(long)]
        stdin: bool,
    },
    /// Removes the operator key: `/admin` is closed until a new one is set.
    ClearKey {
        #[command(flatten)]
        store: StoreArgs,
    },
    /// Lets one login name register while sign-ups are closed.
    Invite {
        #[command(flatten)]
        store: StoreArgs,
        /// The login name.
        #[arg(long)]
        login: String,
        /// How many days the invite lasts.
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
}

#[derive(Subcommand, Debug)]
enum SecurityKeysCommand {
    /// Lists an account's security keys.
    List {
        #[command(flatten)]
        store: StoreArgs,
        /// The account's login name.
        #[arg(long)]
        login: String,
    },
    /// Removes every security key from an account, so its next sign-in on
    /// a new device needs only the password (and, for the vault, the
    /// Secret Key or the recovery kit, which the server never had).
    Clear {
        #[command(flatten)]
        store: StoreArgs,
        /// The account's login name.
        #[arg(long)]
        login: String,
    },
}

#[derive(Subcommand, Debug)]
enum BackupCommand {
    /// Prints a new Backup Key. Keep it with your recovery kit; with
    /// `--kit FILE` the operator's printable recovery kit is written too.
    Keygen {
        #[command(flatten)]
        kit: KitArgs,
    },
    /// Writes the operator's printable recovery kit for a Backup Key you
    /// already have (from standard input or `LOCKDOWN_BACKUP_KEY`).
    Kit {
        #[command(flatten)]
        kit: KitArgs,
    },
    /// Writes a sealed snapshot of the database, the key file and the
    /// attachment files.
    Create {
        #[command(flatten)]
        store: StoreArgs,
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
    /// Writes a backup back into an empty data directory (and, with
    /// `--database-url`, an empty Postgres database).
    Restore {
        /// The backup file.
        #[arg(long = "in", value_name = "FILE")]
        input: PathBuf,
        #[command(flatten)]
        store: StoreArgs,
    },
}

/// The operator's printable kit.
#[derive(Args, Debug, Clone)]
struct KitArgs {
    /// Write the printable kit (HTML, readable only by you) here.
    #[arg(long, value_name = "FILE")]
    kit: Option<PathBuf>,
    /// The server's address, printed on the kit.
    #[arg(long, env = "LOCKDOWN_PUBLIC_URL", value_name = "URL")]
    public_url: Option<String>,
    /// Where the backups are kept, printed on the kit.
    #[arg(long, env = "LOCKDOWN_BACKUP_DIR", value_name = "WHERE")]
    backup_dir: Option<String>,
}

/// Where the store is: the data directory (key file; the SQLite database
/// unless `--database-url`), and where attachment bodies live.
#[derive(Args, Debug, Clone)]
struct StoreArgs {
    /// Where the database (SQLite) and the server key file live.
    #[arg(long, env = "LOCKDOWN_DATA", value_name = "DIR")]
    data: Option<PathBuf>,
    /// The server key file; default `server-key.cbor` in the data directory.
    #[arg(long, env = "LOCKDOWN_SERVER_KEY", value_name = "PATH")]
    server_key: Option<PathBuf>,
    /// Keep attachment bodies as files here instead of in the database.
    #[arg(long, env = "LOCKDOWN_BLOB_DIR", value_name = "DIR")]
    blob_dir: Option<PathBuf>,
    /// A Postgres database instead of the SQLite file
    /// (`postgres://user:password@host/db`; TLS is required away from this
    /// machine).
    #[arg(
        long,
        env = "LOCKDOWN_DATABASE_URL",
        value_name = "URL",
        hide_env_values = true
    )]
    database_url: Option<String>,
    /// Certificates (PEM) to trust for the database's TLS instead of the
    /// system's: the database server's private CA.
    #[arg(long, env = "LOCKDOWN_DATABASE_CA", value_name = "PEM")]
    database_ca: Option<PathBuf>,
    /// Keep attachment bodies in an S3-compatible bucket at this endpoint
    /// (`https://host[:port]`; plain `http://` only to this machine).
    #[arg(
        long,
        env = "LOCKDOWN_S3_ENDPOINT",
        value_name = "URL",
        conflicts_with = "blob_dir"
    )]
    s3_endpoint: Option<String>,
    /// The bucket (it must exist).
    #[arg(
        long,
        env = "LOCKDOWN_S3_BUCKET",
        value_name = "NAME",
        requires = "s3_endpoint"
    )]
    s3_bucket: Option<String>,
    /// The signing region.
    #[arg(long, env = "LOCKDOWN_S3_REGION", default_value = "us-east-1")]
    s3_region: String,
    /// The access key id.
    #[arg(
        long,
        env = "LOCKDOWN_S3_ACCESS_KEY",
        value_name = "ID",
        hide_env_values = true
    )]
    s3_access_key: Option<String>,
    /// The secret access key (better in the environment than on the
    /// command line).
    #[arg(
        long,
        env = "LOCKDOWN_S3_SECRET_KEY",
        value_name = "KEY",
        hide_env_values = true
    )]
    s3_secret_key: Option<String>,
    /// Put in front of every object name (`lockdown/`).
    #[arg(long, env = "LOCKDOWN_S3_PREFIX", default_value = "")]
    s3_prefix: String,
    /// Address the bucket as `bucket.host` instead of `host/bucket`.
    #[arg(long, env = "LOCKDOWN_S3_VIRTUAL_HOSTED")]
    s3_virtual_hosted: bool,
}

impl StoreArgs {
    /// The bucket for attachment bodies, when one is set (and checked).
    fn s3(&self) -> Result<Option<lockdown_server::s3::S3Bodies>, String> {
        let Some(endpoint) = &self.s3_endpoint else {
            return Ok(None);
        };
        let bodies = lockdown_server::s3::S3Bodies::new(lockdown_server::s3::S3Config {
            endpoint: endpoint.clone(),
            bucket: self
                .s3_bucket
                .clone()
                .ok_or("give the bucket with --s3-bucket")?,
            region: self.s3_region.clone(),
            access_key: self.s3_access_key.clone().unwrap_or_default(),
            secret_key: zeroize::Zeroizing::new(self.s3_secret_key.clone().unwrap_or_default()),
            prefix: self.s3_prefix.clone(),
            virtual_hosted: self.s3_virtual_hosted,
        })?;
        bodies.check().map_err(|e| match e {
            lockdown_server::storage::StorageError::Backend(m) => m,
            other => other.to_string(),
        })?;
        Ok(Some(bodies))
    }
}

#[derive(Args, Debug)]
struct ServeArgs {
    /// Address and port to listen on.
    #[arg(long, env = "LOCKDOWN_LISTEN", default_value = "127.0.0.1:8787")]
    listen: String,
    /// The data directory (default: the platform data directory,
    /// `lockdown-server` under it), the server key file (made on first
    /// run), the database and where attachment bodies live.
    #[command(flatten)]
    store: StoreArgs,
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
    /// A breach feed of your own, looked up by k-anonymity: a range file
    /// (`SHA1(address):Breach;Breach` lines, read on this machine) or a
    /// range service (`https://…`, sent the hash's first six hex digits
    /// only). No paid key needed.
    #[arg(
        long,
        env = "LOCKDOWN_BREACH_RANGE",
        value_name = "FILE|URL",
        conflicts_with = "breach_feed_url"
    )]
    breach_range: Option<String>,
    /// Breach details for the range feed: Have I Been Pwned's free breach
    /// list (`/api/v3/breaches`, JSON), saved to a file.
    #[arg(
        long,
        env = "LOCKDOWN_BREACH_CATALOG",
        value_name = "FILE",
        requires = "breach_range"
    )]
    breach_catalog: Option<PathBuf>,
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
    /// The server's public address (`https://vault.example`), for the links
    /// in email; never taken from a request.
    #[arg(long, env = "LOCKDOWN_PUBLIC_URL", value_name = "URL")]
    public_url: Option<String>,
    /// The SMTP relay for email: `smtps://host[:465]` (TLS) or
    /// `smtp://host[:587]` (STARTTLS; plain only to this machine).
    #[arg(
        long,
        env = "LOCKDOWN_SMTP_URL",
        value_name = "URL",
        requires = "smtp_from"
    )]
    smtp_url: Option<String>,
    /// The relay account.
    #[arg(
        long,
        env = "LOCKDOWN_SMTP_USER",
        value_name = "NAME",
        hide_env_values = true
    )]
    smtp_user: Option<String>,
    /// Its password (better in the environment).
    #[arg(
        long,
        env = "LOCKDOWN_SMTP_PASSWORD",
        value_name = "PASSWORD",
        hide_env_values = true
    )]
    smtp_password: Option<String>,
    /// The From address (`Lockdown <lockdown@example.com>`).
    #[arg(long, env = "LOCKDOWN_SMTP_FROM", value_name = "ADDRESS")]
    smtp_from: Option<String>,
    /// Certificates (PEM) to trust for the relay instead of the system's.
    #[arg(long, env = "LOCKDOWN_SMTP_CA", value_name = "PEM")]
    smtp_ca: Option<PathBuf>,
    /// Whether a device new to an account waits for the account's email to
    /// approve it (with SMTP set; accounts with security keys use those).
    #[arg(long, env = "LOCKDOWN_EMAIL_APPROVAL", value_enum, default_value_t = OnOff::On)]
    email_approval: OnOff,
    /// Back the server up into this folder on a schedule, and drill the
    /// restores (needs the Backup Key).
    #[arg(long, env = "LOCKDOWN_BACKUP_DIR", value_name = "DIR")]
    backup_dir: Option<PathBuf>,
    /// A file holding the Backup Key (a container secret), instead of
    /// `LOCKDOWN_BACKUP_KEY`.
    #[arg(long, env = "LOCKDOWN_BACKUP_KEY_FILE", value_name = "FILE")]
    backup_key_file: Option<PathBuf>,
    /// Hours between scheduled backups.
    #[arg(long, env = "LOCKDOWN_BACKUP_EVERY_HOURS", default_value_t = 24)]
    backup_every_hours: u64,
    /// How many scheduled backups are kept.
    #[arg(long, env = "LOCKDOWN_BACKUP_KEEP", default_value_t = 14)]
    backup_keep: usize,
    /// Hours between restore drills.
    #[arg(long, env = "LOCKDOWN_DRILL_EVERY_HOURS", default_value_t = 168)]
    drill_every_hours: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum OnOff {
    On,
    Off,
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

/// The store `args` names: Postgres with `--database-url`, else the SQLite
/// file in the data directory; attachment bodies as files with
/// `--blob-dir`. `exclusive` holds the database's advisory lock (a
/// serving server; one per database).
fn open_storage(args: &StoreArgs, exclusive: bool) -> Result<SqlStorage, String> {
    let storage = match &args.database_url {
        Some(url) => SqlStorage::open_postgres(pg_options(args, url, exclusive)?)
            .map_err(|e| e.to_string())?,
        None => {
            let data = data_dir(args.data.clone())?;
            SqlStorage::open(&data.join(backup::DB_NAME)).map_err(|e| e.to_string())?
        }
    };
    if let Some(bucket) = args.s3()? {
        return Ok(storage.with_bodies(std::sync::Arc::new(bucket)));
    }
    match &args.blob_dir {
        Some(dir) => storage.with_blob_dir(dir).map_err(|e| e.to_string()),
        None => Ok(storage),
    }
}

fn pg_options(args: &StoreArgs, url: &str, exclusive: bool) -> Result<PgOptions, String> {
    let mut options = PgOptions::new(url);
    options.exclusive = exclusive;
    if let Some(ca) = &args.database_ca {
        options.ca = Some(lockdown_tls::read_certs(ca).map_err(|e| e.to_string())?);
    }
    Ok(options)
}

fn key_path(args: &StoreArgs) -> Result<PathBuf, String> {
    match &args.server_key {
        Some(p) => Ok(p.clone()),
        None => Ok(data_dir(args.data.clone())?.join(backup::KEY_NAME)),
    }
}

fn serve(cli: ServeArgs) -> Result<(), String> {
    let data = data_dir(cli.store.data.clone())?;
    let key_path = key_path(&cli.store)?;
    let storage = open_storage(&cli.store, true)?;
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
    let database = match &cli.store.database_url {
        Some(_) => match storage.database_tls() {
            Ok(Some(v)) => format!("Postgres (--database-url), over {v}"),
            _ => "Postgres (--database-url), on this machine without TLS".to_string(),
        },
        None => format!("SQLite, {}", data.join(backup::DB_NAME).display()),
    };
    let attachments = storage
        .bodies()
        .map(|b| b.describe())
        .unwrap_or_else(|| "inside the database".into());
    let mut state = AppState::new(Box::new(storage), keys, config);
    if let Some(url) = &cli.smtp_url {
        let public_url = cli
            .public_url
            .as_deref()
            .ok_or("email needs the server's public address for its links: set --public-url")?;
        let mailer = lockdown_server::mail::SmtpMailer::new(lockdown_server::mail::SmtpConfig {
            url: url.clone(),
            user: cli.smtp_user.clone(),
            password: cli.smtp_password.clone().map(zeroize::Zeroizing::new),
            from: cli.smtp_from.clone().unwrap_or_default(),
            ca: match &cli.smtp_ca {
                Some(p) => Some(lockdown_tls::read_certs(p).map_err(|e| e.to_string())?),
                None => None,
            },
        })?;
        let describe = mailer.describe();
        let outbox = lockdown_server::mail::Outbox::start(Box::new(mailer))
            .map_err(|e| format!("couldn't start the mail thread: {e}"))?;
        state.mail = Some(lockdown_server::mail::MailSettings::new(
            outbox,
            public_url,
            cli.email_approval == OnOff::On,
            describe.clone(),
        )?);
        eprintln!("lockdown-server: email through {describe}");
    }
    let state = Arc::new(state);
    let mut breach_words = None;
    let feed: Option<Box<dyn lockdown_server::breach_monitor::BreachFeed>> =
        match (cli.breach_feed_url.as_deref(), cli.breach_range.as_deref()) {
            (Some(url), _) => Some(Box::new(
                lockdown_server::breach_monitor::HttpBreachFeed::new(url, cli.breach_feed_key)?,
            )),
            (None, Some(range)) => {
                let catalog = match &cli.breach_catalog {
                    Some(path) => lockdown_server::breach_range::Catalog::load(path)?,
                    None => lockdown_server::breach_range::Catalog::default(),
                };
                let feed = lockdown_server::breach_range::RangeBreachFeed::new(range, catalog)?;
                breach_words = Some(format!("over the operator's {}", feed.describe()));
                eprintln!(
                    "lockdown-server: email breach monitoring over the {}",
                    feed.describe()
                );
                Some(Box::new(feed))
            }
            (None, None) => None,
        };
    if let Some(feed) = feed {
        // A range file is local: no feed rate limit to keep under.
        let pause = if cli
            .breach_range
            .as_deref()
            .is_some_and(|r| !r.contains("://"))
        {
            Duration::ZERO
        } else {
            Duration::from_millis(cli.breach_scan_pause_ms)
        };
        lockdown_server::breach_monitor::spawn(
            state.clone(),
            feed,
            Duration::from_secs(cli.breach_scan_hours.max(1) * 3600),
            pause,
        )
        .map_err(|e| format!("couldn't start the breach monitor: {e}"))?;
        if breach_words.is_none() {
            breach_words = Some("over the configured breach feed (a paid key)".into());
        }
    }
    state.admin.set_facts(lockdown_server::admin::ServerFacts {
        database,
        attachments,
        tls: cli.tls_cert.is_some(),
        breach_feed: breach_words,
    });
    if let Some(dir) = &cli.backup_dir {
        let key = match &cli.backup_key_file {
            Some(file) => BackupKey::parse(&zeroize::Zeroizing::new(
                std::fs::read_to_string(file)
                    .map_err(|e| format!("couldn't read {}: {e}", file.display()))?,
            ))?,
            None => match std::env::var_os("LOCKDOWN_BACKUP_KEY").filter(|v| !v.is_empty()) {
                Some(text) => BackupKey::parse(&text.to_string_lossy())?,
                None => {
                    return Err(
                        "scheduled backups need the Backup Key: LOCKDOWN_BACKUP_KEY or --backup-key-file"
                            .into(),
                    );
                }
            },
        };
        let schedule = Arc::new(lockdown_server::drills::Schedule::new(
            dir,
            key,
            &key_path,
            Duration::from_secs(cli.backup_every_hours.max(1) * 3600),
            cli.backup_keep,
            Duration::from_secs(cli.drill_every_hours.max(1) * 3600),
        )?);
        state.admin.set_schedule(schedule.clone());
        lockdown_server::drills::spawn(state.clone(), schedule)
            .map_err(|e| format!("couldn't start the backup schedule: {e}"))?;
        eprintln!(
            "lockdown-server: backups every {} h into {}, restore drills every {} h",
            cli.backup_every_hours.max(1),
            dir.display(),
            cli.drill_every_hours.max(1)
        );
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
        BackupCommand::Keygen { kit } => {
            let key = BackupKey::generate()?;
            println!("{}", key.display().as_str());
            eprintln!(
                "This is your server's Backup Key. It is shown once and the server doesn't keep it: \
                 write it down with your recovery kit. Without it no backup can be opened."
            );
            write_kit(&key, &kit)
        }
        BackupCommand::Kit { kit } => {
            if kit.kit.is_none() {
                return Err("give the file to write with --kit".into());
            }
            let key = backup_key()?;
            write_kit(&key, &kit)
        }
        BackupCommand::Create { store, out } => {
            let key = backup_key()?;
            if store.database_url.is_none() {
                let data = data_dir(store.data.clone())?;
                if !data.join(backup::DB_NAME).exists() {
                    return Err(format!("there is no server database in {}", data.display()));
                }
            }
            let storage = open_storage(&store, false)?;
            let server_key = key_path(&store)?;
            let summary = backup::create(
                &backup::Sources {
                    storage: &storage,
                    server_key: &server_key,
                },
                &key,
                &out,
                lockdown_vault::Timestamp::now().0,
            )?;
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
        BackupCommand::Restore { input, store } => {
            let key = backup_key()?;
            let data = store
                .data
                .clone()
                .ok_or("give the data directory to restore into (--data or LOCKDOWN_DATA)")?;
            let target = backup::RestoreTarget {
                data: data.clone(),
                database: match &store.database_url {
                    Some(url) => Some(pg_options(&store, url, true)?),
                    None => None,
                },
                bodies: match (store.s3()?, &store.blob_dir) {
                    (Some(bucket), _) => Some(std::sync::Arc::new(bucket)
                        as std::sync::Arc<dyn lockdown_server::storage::BlobBodies>),
                    (None, Some(dir)) => Some(std::sync::Arc::new(
                        lockdown_server::storage::BlobDir::new(dir).map_err(|e| e.to_string())?,
                    )),
                    (None, None) => None,
                },
            };
            let summary = backup::restore(&input, &key, &target)?;
            println!("restored into {}: {summary}", data.display());
            Ok(())
        }
    }
}

/// Writes the operator's printable recovery kit when `--kit` names a file
/// (never over an existing one; user-only).
fn write_kit(key: &BackupKey, args: &KitArgs) -> Result<(), String> {
    let Some(path) = &args.kit else {
        return Ok(());
    };
    if path.exists() {
        return Err(format!(
            "{} is already there; pick another name",
            path.display()
        ));
    }
    let html = lockdown_server::kit::operator_kit_html(
        key,
        &lockdown_server::kit::KitFacts {
            server: args.public_url.clone().unwrap_or_default(),
            backups: args.backup_dir.clone().unwrap_or_default(),
        },
        lockdown_vault::Timestamp::now().0,
    );
    lockdown_server::kit::write(path, &html)
        .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    eprintln!(
        "The operator's recovery kit is in {}: print it, put it away from the server, and delete the file.",
        path.display()
    );
    Ok(())
}

fn run_admin(command: AdminCommand) -> Result<(), String> {
    match command {
        AdminCommand::SetKey { store, stdin } => {
            let storage = open_storage(&store, false)?;
            let key = if stdin {
                let mut line = zeroize::Zeroizing::new(String::new());
                std::io::stdin()
                    .take(4096)
                    .read_to_string(&mut line)
                    .map_err(|e| format!("couldn't read the key from standard input: {e}"))?;
                zeroize::Zeroizing::new(line.trim().to_string())
            } else {
                lockdown_server::admin::generate_key()?
            };
            storage.set_operator_key(&key).map_err(|e| e.to_string())?;
            if !stdin {
                println!("{}", key.as_str());
            }
            eprintln!(
                "The operator key for /admin is set{}. Only its hash is kept; a new one can be set the same way.",
                if stdin { "" } else { " (above, shown once)" }
            );
            Ok(())
        }
        AdminCommand::ClearKey { store } => {
            let storage = open_storage(&store, false)?;
            let had = storage.clear_operator_key().map_err(|e| e.to_string())?;
            println!(
                "{}",
                if had {
                    "the operator key is removed; /admin is closed"
                } else {
                    "there was no operator key"
                }
            );
            Ok(())
        }
        AdminCommand::Invite { store, login, days } => {
            let storage = open_storage(&store, false)?;
            let now = lockdown_vault::Timestamp::now().0;
            let days = days.clamp(1, 90);
            storage
                .put_invite(login.trim(), now, now + days * 86_400)
                .map_err(|e| e.to_string())?;
            println!("{} may register for {days} day(s)", login.trim());
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

fn run_security_keys(command: SecurityKeysCommand) -> Result<(), String> {
    use lockdown_server::storage::Storage;
    let (store, login, clear) = match command {
        SecurityKeysCommand::List { store, login } => (store, login, false),
        SecurityKeysCommand::Clear { store, login } => (store, login, true),
    };
    let storage = open_storage(&store, false)?;
    let account = storage
        .account_by_login(login.trim())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no account named {login} on this server"))?;
    let store = storage
        .account_security()
        .ok_or("this store keeps no security keys")?;
    if clear {
        let n = store
            .clear_keys(account.account_id)
            .map_err(|e| e.to_string())?;
        println!("removed {n} security key(s) from {login}");
    } else {
        for key in store.keys(account.account_id).map_err(|e| e.to_string())? {
            println!("{}  {}  added {}", key.alg, key.name, key.created_at);
        }
    }
    Ok(())
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
        Some(Command::SecurityKeys(c)) => run_security_keys(c),
        Some(Command::Admin(c)) => run_admin(c),
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
