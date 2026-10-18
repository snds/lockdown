//! The SQL layer under every server table: one way to run a statement on
//! SQLite (the default, one file) or on Postgres (`--database-url`), so the
//! server keeps one set of queries and one set of rules for both
//! (03-architecture §10: "two SQL dialects in tests, and no
//! dialect-specific features"; docs/plan/25-self-host-2.md §2).
//!
//! Statements are written once, in the common subset both databases read:
//! numbered placeholders (`?1`), `INSERT … ON CONFLICT … DO NOTHING` or
//! `DO UPDATE SET c = excluded.c`, `COALESCE`, `CASE`, `CAST(… AS BIGINT)`
//! around a `SUM`. For Postgres the placeholders become `$1`; table
//! definitions are written in SQLite's words and their types mapped
//! (`INTEGER` → `BIGINT`, `BLOB` → `BYTEA`, `TEXT` → `TEXT COLLATE "C"`, so
//! text sorts by bytes as SQLite sorts it).
//!
//! Every value is one of four kinds ([`Value`]): null, a 64-bit integer,
//! text, bytes. A connection is held for the length of a [`Conn`] (both
//! backends keep one connection behind a lock, as the SQLite store always
//! has, so a read-then-write inside a [`Tx`] can't interleave with
//! another), and a [`Tx`] rolls back unless it is committed.
//!
//! Postgres is reached with `tokio-postgres` on a small runtime of its own;
//! the store's callers stay synchronous and wait for each answer, as they
//! wait on the SQLite lock. Away from this machine the connection must be
//! TLS (through `lockdown-tls`, TLS 1.3, the hybrid post-quantum group
//! first), unless the URL says `sslmode=disable` for a socket or loopback.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::storage::StorageError;

/// Which database a connection talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// SQLite, through `rusqlite`.
    Sqlite,
    /// Postgres, through `tokio-postgres`.
    Postgres,
}

/// One value in or out of a statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// SQL `NULL`.
    Null,
    /// A 64-bit integer.
    Int(i64),
    /// Text.
    Text(String),
    /// Bytes.
    Blob(Vec<u8>),
}

/// What went wrong in the SQL layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbError {
    /// A uniqueness, key or other constraint refused the write.
    Constraint(String),
    /// A statement that must return a row returned none.
    NoRows,
    /// Anything else the database or the driver said.
    Other(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Constraint(m) => write!(f, "a constraint refused the write: {m}"),
            DbError::NoRows => f.write_str("the query returned no row"),
            DbError::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for DbError {}

impl From<DbError> for StorageError {
    fn from(e: DbError) -> Self {
        StorageError::Backend(e.to_string())
    }
}

/// The SQL layer's result type.
pub type DbResult<T> = std::result::Result<T, DbError>;

/// A value that can be bound to a placeholder.
pub trait ToSql {
    /// The value to bind.
    fn to_value(&self) -> Value;
}

impl ToSql for Value {
    fn to_value(&self) -> Value {
        self.clone()
    }
}
impl ToSql for i64 {
    fn to_value(&self) -> Value {
        Value::Int(*self)
    }
}
impl ToSql for i32 {
    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }
}
impl ToSql for u32 {
    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }
}
impl ToSql for u8 {
    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }
}
impl ToSql for bool {
    fn to_value(&self) -> Value {
        Value::Int(i64::from(*self))
    }
}
impl ToSql for str {
    fn to_value(&self) -> Value {
        Value::Text(self.to_string())
    }
}
impl ToSql for String {
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}
impl ToSql for [u8] {
    fn to_value(&self) -> Value {
        Value::Blob(self.to_vec())
    }
}
impl ToSql for Vec<u8> {
    fn to_value(&self) -> Value {
        Value::Blob(self.clone())
    }
}
impl<const N: usize> ToSql for [u8; N] {
    fn to_value(&self) -> Value {
        Value::Blob(self.to_vec())
    }
}
impl<T: ToSql> ToSql for Option<T> {
    fn to_value(&self) -> Value {
        match self {
            Some(v) => v.to_value(),
            None => Value::Null,
        }
    }
}
impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}

/// Placeholder values for a statement, in order: `params![a, b]`.
macro_rules! params {
    () => {
        &[] as &[&dyn $crate::db::ToSql]
    };
    ($($p:expr),+ $(,)?) => {
        &[$(&$p as &dyn $crate::db::ToSql),+] as &[&dyn $crate::db::ToSql]
    };
}
pub(crate) use params;

/// A value that can be read out of a column.
pub trait FromValue: Sized {
    /// Reads it.
    fn from_value(v: &Value) -> DbResult<Self>;
}

fn wrong(want: &str, got: &Value) -> DbError {
    let kind = match got {
        Value::Null => "null",
        Value::Int(_) => "an integer",
        Value::Text(_) => "text",
        Value::Blob(_) => "bytes",
    };
    DbError::Other(format!("a column holds {kind} where {want} was expected"))
}

impl FromValue for i64 {
    fn from_value(v: &Value) -> DbResult<Self> {
        match v {
            Value::Int(n) => Ok(*n),
            other => Err(wrong("an integer", other)),
        }
    }
}
impl FromValue for u32 {
    fn from_value(v: &Value) -> DbResult<Self> {
        let n = i64::from_value(v)?;
        u32::try_from(n).map_err(|_| DbError::Other(format!("{n} doesn't fit a u32")))
    }
}
impl FromValue for String {
    fn from_value(v: &Value) -> DbResult<Self> {
        match v {
            Value::Text(t) => Ok(t.clone()),
            other => Err(wrong("text", other)),
        }
    }
}
impl FromValue for Vec<u8> {
    fn from_value(v: &Value) -> DbResult<Self> {
        match v {
            Value::Blob(b) => Ok(b.clone()),
            other => Err(wrong("bytes", other)),
        }
    }
}
impl FromValue for Value {
    fn from_value(v: &Value) -> DbResult<Self> {
        Ok(v.clone())
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: &Value) -> DbResult<Self> {
        match v {
            Value::Null => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}

/// One result row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row(Vec<Value>);

impl Row {
    /// Column `i`, read as `T`.
    pub fn get<T: FromValue>(&self, i: usize) -> DbResult<T> {
        let v = self
            .0
            .get(i)
            .ok_or_else(|| DbError::Other(format!("the row has no column {i}")))?;
        T::from_value(v)
    }

    /// Every column, in order.
    pub fn values(&self) -> &[Value] {
        &self.0
    }
}

// --- the handle ------------------------------------------------------------------------

/// A database: one connection behind a lock.
pub struct Db {
    inner: DbInner,
}

enum DbInner {
    Sqlite(Mutex<rusqlite::Connection>),
    Postgres(Mutex<PgConn>),
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Db({:?})", self.dialect())
    }
}

fn unpoison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock means a caller panicked mid-call; every write is in
    // a transaction that rolled back as the panic unwound.
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Db {
    /// Over an open SQLite connection.
    pub fn sqlite(conn: rusqlite::Connection) -> Self {
        Self {
            inner: DbInner::Sqlite(Mutex::new(conn)),
        }
    }

    /// Connects to Postgres.
    pub fn postgres(options: PgOptions) -> DbResult<Self> {
        let conn = PgConn::connect(options)?;
        Ok(Self {
            inner: DbInner::Postgres(Mutex::new(conn)),
        })
    }

    /// Which database this is.
    pub fn dialect(&self) -> Dialect {
        match self.inner {
            DbInner::Sqlite(_) => Dialect::Sqlite,
            DbInner::Postgres(_) => Dialect::Postgres,
        }
    }

    /// The connection, held until the [`Conn`] is dropped.
    pub fn conn(&self) -> Conn<'_> {
        match &self.inner {
            DbInner::Sqlite(m) => Conn(ConnInner::Sqlite(unpoison(m))),
            DbInner::Postgres(m) => Conn(ConnInner::Postgres(unpoison(m))),
        }
    }

    /// A transaction on the connection: rolled back unless committed.
    pub fn begin(&self) -> DbResult<Tx<'_>> {
        self.conn().transaction()
    }
}

/// A held connection.
pub struct Conn<'a>(ConnInner<'a>);

enum ConnInner<'a> {
    Sqlite(MutexGuard<'a, rusqlite::Connection>),
    Postgres(MutexGuard<'a, PgConn>),
}

impl<'a> Conn<'a> {
    /// Which database this is.
    pub fn dialect(&self) -> Dialect {
        match self.0 {
            ConnInner::Sqlite(_) => Dialect::Sqlite,
            ConnInner::Postgres(_) => Dialect::Postgres,
        }
    }

    /// Runs a statement; returns how many rows it changed.
    pub fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> DbResult<usize> {
        let values: Vec<Value> = params.iter().map(|p| p.to_value()).collect();
        match &self.0 {
            ConnInner::Sqlite(c) => sqlite_execute(c, sql, &values),
            ConnInner::Postgres(c) => c.execute(sql, &values),
        }
    }

    /// Runs a query; returns every row.
    pub fn query(&self, sql: &str, params: &[&dyn ToSql]) -> DbResult<Vec<Row>> {
        let values: Vec<Value> = params.iter().map(|p| p.to_value()).collect();
        match &self.0 {
            ConnInner::Sqlite(c) => sqlite_query(c, sql, &values),
            ConnInner::Postgres(c) => c.query(sql, &values),
        }
    }

    /// The first row through `f`; [`DbError::NoRows`] when there is none.
    pub fn query_row<T>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        f: impl FnOnce(&Row) -> DbResult<T>,
    ) -> DbResult<T> {
        match self.query(sql, params)?.first() {
            Some(row) => f(row),
            None => Err(DbError::NoRows),
        }
    }

    /// The first row through `f`, if there is one.
    pub fn query_opt<T>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        f: impl FnOnce(&Row) -> DbResult<T>,
    ) -> DbResult<Option<T>> {
        self.query(sql, params)?.first().map(f).transpose()
    }

    /// Every row through `f`, in order.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        f: impl FnMut(&Row) -> DbResult<T>,
    ) -> DbResult<Vec<T>> {
        self.query(sql, params)?.iter().map(f).collect()
    }

    /// Runs several statements with no placeholders: table definitions in
    /// SQLite's words, mapped for Postgres.
    pub fn execute_batch(&self, sql: &str) -> DbResult<()> {
        match &self.0 {
            ConnInner::Sqlite(c) => c.execute_batch(sql).map_err(sqlite_err),
            ConnInner::Postgres(c) => c.batch(&pg_ddl(sql)),
        }
    }

    /// The tables, by name.
    pub fn tables(&self) -> DbResult<Vec<String>> {
        let sql = match self.dialect() {
            Dialect::Sqlite => {
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            }
            Dialect::Postgres => {
                "SELECT CAST(table_name AS TEXT) FROM information_schema.tables WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name"
            }
        };
        self.query_map(sql, params![], |r| r.get(0))
    }

    /// A table's columns, in their order.
    pub fn columns(&self, table: &str) -> DbResult<Vec<String>> {
        if !table.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            return Err(DbError::Other(format!("'{table}' isn't a table name")));
        }
        match self.dialect() {
            Dialect::Sqlite => {
                self.query_map(&format!("PRAGMA table_info({table})"), params![], |r| {
                    r.get(1)
                })
            }
            Dialect::Postgres => self.query_map(
                "SELECT CAST(column_name AS TEXT) FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = ?1 ORDER BY ordinal_position",
                params![table],
                |r| r.get(0),
            ),
        }
    }

    /// Starts a transaction on this connection.
    pub fn transaction(self) -> DbResult<Tx<'a>> {
        match &self.0 {
            ConnInner::Sqlite(c) => c.execute_batch("BEGIN").map_err(sqlite_err)?,
            ConnInner::Postgres(c) => c.begin()?,
        }
        Ok(Tx {
            conn: self,
            open: true,
        })
    }
}

/// A transaction: every statement on [`Tx::deref`]'s connection, rolled
/// back on drop unless [`Tx::commit`] ran.
pub struct Tx<'a> {
    conn: Conn<'a>,
    open: bool,
}

impl<'a> std::ops::Deref for Tx<'a> {
    type Target = Conn<'a>;
    fn deref(&self) -> &Conn<'a> {
        &self.conn
    }
}

impl Tx<'_> {
    /// Makes the transaction's writes stick.
    pub fn commit(mut self) -> DbResult<()> {
        self.open = false;
        match &self.conn.0 {
            ConnInner::Sqlite(c) => c.execute_batch("COMMIT").map_err(sqlite_err),
            ConnInner::Postgres(c) => c.end("COMMIT"),
        }
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = match &self.conn.0 {
                ConnInner::Sqlite(c) => c.execute_batch("ROLLBACK").map_err(sqlite_err),
                ConnInner::Postgres(c) => c.end("ROLLBACK"),
            };
        }
    }
}

// --- SQLite ------------------------------------------------------------------------------

fn sqlite_err(e: rusqlite::Error) -> DbError {
    match e {
        rusqlite::Error::SqliteFailure(f, m)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            DbError::Constraint(m.unwrap_or_else(|| f.to_string()))
        }
        rusqlite::Error::QueryReturnedNoRows => DbError::NoRows,
        other => DbError::Other(other.to_string()),
    }
}

fn to_sqlite(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Int(n) => rusqlite::types::Value::Integer(*n),
        Value::Text(t) => rusqlite::types::Value::Text(t.clone()),
        Value::Blob(b) => rusqlite::types::Value::Blob(b.clone()),
    }
}

fn from_sqlite(v: rusqlite::types::ValueRef<'_>) -> DbResult<Value> {
    use rusqlite::types::ValueRef;
    Ok(match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::Int(n),
        ValueRef::Text(t) => Value::Text(
            std::str::from_utf8(t)
                .map_err(|_| DbError::Other("a text column isn't UTF-8".into()))?
                .to_string(),
        ),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        ValueRef::Real(_) => {
            return Err(DbError::Other(
                "a column holds a floating-point number; the server stores none".into(),
            ));
        }
    })
}

fn sqlite_execute(c: &rusqlite::Connection, sql: &str, values: &[Value]) -> DbResult<usize> {
    let mut stmt = c.prepare_cached(sql).map_err(sqlite_err)?;
    stmt.execute(rusqlite::params_from_iter(values.iter().map(to_sqlite)))
        .map_err(sqlite_err)
}

fn sqlite_query(c: &rusqlite::Connection, sql: &str, values: &[Value]) -> DbResult<Vec<Row>> {
    let mut stmt = c.prepare_cached(sql).map_err(sqlite_err)?;
    let n = stmt.column_count();
    let mut rows = stmt
        .query(rusqlite::params_from_iter(values.iter().map(to_sqlite)))
        .map_err(sqlite_err)?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().map_err(sqlite_err)? {
        let mut vals = Vec::with_capacity(n);
        for i in 0..n {
            vals.push(from_sqlite(r.get_ref(i).map_err(sqlite_err)?)?);
        }
        out.push(Row(vals));
    }
    Ok(out)
}

// --- the Postgres spelling ----------------------------------------------------------

/// `?N` placeholders as `$N` (a bare `?` takes the next number), outside
/// quoted text.
pub fn pg_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut chars = sql.chars().peekable();
    let mut next = 1u32;
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                out.push(c);
            }
            '?' => {
                let mut digits = String::new();
                while let Some(d) = chars.peek().copied().filter(char::is_ascii_digit) {
                    digits.push(d);
                    chars.next();
                }
                if digits.is_empty() {
                    out.push_str(&format!("${next}"));
                    next += 1;
                } else {
                    out.push('$');
                    out.push_str(&digits);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// A table definition in SQLite's words, in Postgres's.
pub fn pg_ddl(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 64);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match word.as_str() {
            "INTEGER" => out.push_str("BIGINT"),
            "BLOB" => out.push_str("BYTEA"),
            "TEXT" => out.push_str("TEXT COLLATE \"C\""),
            _ => out.push_str(word),
        }
        word.clear();
    };
    for c in sql.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

// --- Postgres -------------------------------------------------------------------------

/// How to reach a Postgres database.
#[derive(Clone)]
pub struct PgOptions {
    /// The `postgres://` URL (or key-value string) `tokio-postgres` reads.
    pub url: String,
    /// A schema to use instead of the URL's default (`search_path`); made
    /// when missing.
    pub schema: Option<String>,
    /// Drop the schema when the store goes (scratch stores: tests, drills).
    pub drop_schema_on_close: bool,
    /// Hold the schema's advisory lock, so a second server on the same
    /// tables refuses to start (`serve` sets it; tools that only read don't).
    pub exclusive: bool,
    /// Certificates to trust for the database's TLS instead of the system's.
    pub ca: Option<Vec<rustls::pki_types::CertificateDer<'static>>>,
}

impl std::fmt::Debug for PgOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The URL can carry a password: never printed.
        f.debug_struct("PgOptions")
            .field("schema", &self.schema)
            .field("exclusive", &self.exclusive)
            .finish_non_exhaustive()
    }
}

impl PgOptions {
    /// Options for `url` with nothing else set.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            schema: None,
            drop_schema_on_close: false,
            exclusive: false,
            ca: None,
        }
    }
}

/// Whether a schema name is one this layer will quote into a statement.
pub fn valid_schema(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !name.as_bytes()[0].is_ascii_digit()
}

/// The small runtime the store's own network I/O runs on (Postgres
/// connections, the S3 client): requests are sent from the caller's thread
/// and waited for there, so the store stays synchronous whether or not
/// its caller is inside the server's runtime.
pub(crate) fn io_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RT: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lockdown-io")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| format!("couldn't start the store's I/O runtime: {e}"))
}

/// Runs `fut` on [`io_runtime`] and waits for it on this thread.
pub(crate) fn run_io<T: Send + 'static>(
    fut: impl std::future::Future<Output = T> + Send + 'static,
) -> Result<T, String> {
    let task = io_runtime()?.spawn(fut);
    futures_executor::block_on(task).map_err(|e| format!("an I/O task failed: {e}"))
}

fn pg_runtime() -> DbResult<&'static tokio::runtime::Runtime> {
    io_runtime().map_err(DbError::Other)
}

fn pg_err(e: tokio_postgres::Error) -> DbError {
    if let Some(db) = e.as_db_error() {
        // Class 23: integrity constraint violations.
        if db.code().code().starts_with("23") {
            return DbError::Constraint(db.message().to_string());
        }
        return DbError::Other(format!("Postgres: {}", db.message()));
    }
    DbError::Other(format!("Postgres: {e}"))
}

struct PgState {
    client: Option<tokio_postgres::Client>,
    statements: HashMap<String, tokio_postgres::Statement>,
    in_tx: bool,
}

/// One Postgres connection, reconnected when it drops between transactions.
pub struct PgConn {
    options: PgOptions,
    state: RefCell<PgState>,
}

/// Whether a URL's hosts are this machine or a Unix socket.
fn pg_local(config: &tokio_postgres::Config) -> bool {
    use tokio_postgres::config::Host;
    !config.get_hosts().is_empty()
        && config.get_hosts().iter().all(|h| match h {
            Host::Tcp(name) => {
                name == "localhost"
                    || name
                        .parse::<std::net::IpAddr>()
                        .map(|ip| ip.is_loopback())
                        .unwrap_or(false)
            }
            #[cfg(unix)]
            Host::Unix(_) => true,
        })
}

impl PgConn {
    fn connect(options: PgOptions) -> DbResult<Self> {
        if let Some(s) = &options.schema
            && !valid_schema(s)
        {
            return Err(DbError::Other(format!(
                "'{s}' isn't a schema name this server uses (lowercase letters, digits, _)"
            )));
        }
        let conn = Self {
            options,
            state: RefCell::new(PgState {
                client: None,
                statements: HashMap::new(),
                in_tx: false,
            }),
        };
        conn.client()?;
        Ok(conn)
    }

    fn open_client(&self) -> DbResult<tokio_postgres::Client> {
        let config: tokio_postgres::Config = self
            .options
            .url
            .parse()
            .map_err(|_| DbError::Other("the database URL doesn't read".into()))?;
        let local = pg_local(&config);
        let no_tls = config.get_ssl_mode() == tokio_postgres::config::SslMode::Disable;
        if no_tls && !local {
            return Err(DbError::Other(
                "sslmode=disable is only for a database on this machine; the server speaks TLS to any other"
                    .into(),
            ));
        }
        let rt = pg_runtime()?;
        let client = if no_tls {
            let task = rt.spawn(async move {
                let (client, connection) = config.connect(tokio_postgres::NoTls).await?;
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                Ok::<_, tokio_postgres::Error>(client)
            });
            futures_executor::block_on(task)
                .map_err(|e| DbError::Other(format!("the database task failed: {e}")))?
                .map_err(pg_err)?
        } else {
            let trust = match &self.options.ca {
                Some(certs) => lockdown_tls::Trust::Only(certs.clone()),
                None => lockdown_tls::Trust::Platform,
            };
            let tls_config = lockdown_tls::client_config(&trust)
                .map_err(|e| DbError::Other(format!("the database's TLS: {e}")))?;
            let tls = pg_tls::MakeRustls::new(Arc::new(tls_config));
            let mut config = config;
            if !local {
                // Away from this machine TLS is required, whatever the URL's
                // `sslmode=prefer` would allow.
                config.ssl_mode(tokio_postgres::config::SslMode::Require);
            }
            let task = rt.spawn(async move {
                let (client, connection) = config.connect(tls).await?;
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                Ok::<_, tokio_postgres::Error>(client)
            });
            futures_executor::block_on(task)
                .map_err(|e| DbError::Other(format!("the database task failed: {e}")))?
                .map_err(pg_err)?
        };
        if let Some(schema) = &self.options.schema {
            futures_executor::block_on(client.batch_execute(&format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}; SET search_path TO {schema}"
            )))
            .map_err(pg_err)?;
        }
        if self.options.exclusive {
            let row = futures_executor::block_on(client.query_one(
                "SELECT pg_try_advisory_lock(hashtext('lockdown-server/' || current_schema()))",
                &[],
            ))
            .map_err(pg_err)?;
            let got: bool = row.try_get(0).map_err(pg_err)?;
            if !got {
                return Err(DbError::Other(
                    "another lockdown-server is using this database (its advisory lock is held); one server per database"
                        .into(),
                ));
            }
        }
        Ok(client)
    }

    /// The live client, reconnecting when the old one closed outside a
    /// transaction.
    fn client(&self) -> DbResult<std::cell::RefMut<'_, PgState>> {
        let mut state = self.state.borrow_mut();
        let closed = state.client.as_ref().is_none_or(|c| c.is_closed());
        if closed {
            if state.in_tx {
                state.client = None;
                return Err(DbError::Other(
                    "the database connection dropped in the middle of a transaction".into(),
                ));
            }
            state.statements.clear();
            state.client = Some(self.open_client()?);
        }
        Ok(state)
    }

    fn statement<'s>(
        state: &'s mut PgState,
        sql: &str,
    ) -> DbResult<(tokio_postgres::Statement, &'s tokio_postgres::Client)> {
        let client = state
            .client
            .as_ref()
            .ok_or_else(|| DbError::Other("no database connection".into()))?;
        if let Some(s) = state.statements.get(sql) {
            return Ok((s.clone(), client));
        }
        let stmt =
            futures_executor::block_on(client.prepare(&pg_placeholders(sql))).map_err(pg_err)?;
        if state.statements.len() > 512 {
            state.statements.clear();
        }
        state.statements.insert(sql.to_string(), stmt.clone());
        let client = state
            .client
            .as_ref()
            .ok_or_else(|| DbError::Other("no database connection".into()))?;
        Ok((stmt, client))
    }

    fn execute(&self, sql: &str, values: &[Value]) -> DbResult<usize> {
        let mut state = self.client()?;
        let (stmt, client) = Self::statement(&mut state, sql)?;
        let params: Vec<pg_types::PgParam<'_>> = values.iter().map(pg_types::PgParam).collect();
        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        let n = futures_executor::block_on(client.execute(&stmt, &refs)).map_err(pg_err)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    fn query(&self, sql: &str, values: &[Value]) -> DbResult<Vec<Row>> {
        let mut state = self.client()?;
        let (stmt, client) = Self::statement(&mut state, sql)?;
        let params: Vec<pg_types::PgParam<'_>> = values.iter().map(pg_types::PgParam).collect();
        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        let rows = futures_executor::block_on(client.query(&stmt, &refs)).map_err(pg_err)?;
        rows.iter().map(pg_types::row_of).collect()
    }

    fn batch(&self, sql: &str) -> DbResult<()> {
        let mut state = self.client()?;
        let client = state
            .client
            .as_ref()
            .ok_or_else(|| DbError::Other("no database connection".into()))?;
        futures_executor::block_on(client.batch_execute(sql)).map_err(pg_err)?;
        // A definition changed: prepared statements may name old shapes.
        state.statements.clear();
        Ok(())
    }

    fn begin(&self) -> DbResult<()> {
        let mut state = self.client()?;
        let client = state
            .client
            .as_ref()
            .ok_or_else(|| DbError::Other("no database connection".into()))?;
        futures_executor::block_on(client.batch_execute("BEGIN")).map_err(pg_err)?;
        state.in_tx = true;
        Ok(())
    }

    fn end(&self, how: &str) -> DbResult<()> {
        let mut state = self.state.borrow_mut();
        state.in_tx = false;
        let Some(client) = state.client.as_ref().filter(|c| !c.is_closed()) else {
            return Err(DbError::Other(
                "the database connection dropped before the transaction ended".into(),
            ));
        };
        futures_executor::block_on(client.batch_execute(how)).map_err(pg_err)
    }
}

impl Drop for PgConn {
    fn drop(&mut self) {
        if !self.options.drop_schema_on_close {
            return;
        }
        let state = self.state.get_mut();
        if let (Some(client), Some(schema)) = (state.client.as_ref(), &self.options.schema)
            && !client.is_closed()
        {
            let _ = futures_executor::block_on(
                client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE")),
            );
        }
    }
}

mod pg_types {
    //! Values to and from Postgres's types.

    use bytes::BytesMut;
    use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};

    use super::{DbError, DbResult, Row, Value};

    /// A [`Value`] bound to whatever type Postgres inferred for its
    /// placeholder (an integer to `BIGINT`, `INTEGER` or `BOOLEAN`; text to
    /// any text type; bytes to `BYTEA`).
    #[derive(Debug)]
    pub struct PgParam<'a>(pub &'a Value);

    type BoxError = Box<dyn std::error::Error + Sync + Send>;

    impl ToSql for PgParam<'_> {
        fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, BoxError> {
            match self.0 {
                Value::Null => Ok(IsNull::Yes),
                Value::Int(n) => match *ty {
                    Type::INT8 => n.to_sql(ty, out),
                    Type::INT4 => i32::try_from(*n)?.to_sql(ty, out),
                    Type::INT2 => i16::try_from(*n)?.to_sql(ty, out),
                    Type::BOOL => (*n != 0).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => n.to_string().as_str().to_sql(ty, out),
                    _ => Err(format!("an integer can't be bound to {ty}").into()),
                },
                Value::Text(t) => match *ty {
                    Type::TEXT | Type::VARCHAR | Type::NAME | Type::BPCHAR | Type::UNKNOWN => {
                        t.as_str().to_sql(ty, out)
                    }
                    _ => Err(format!("text can't be bound to {ty}").into()),
                },
                Value::Blob(b) => match *ty {
                    Type::BYTEA => b.as_slice().to_sql(ty, out),
                    _ => Err(format!("bytes can't be bound to {ty}").into()),
                },
            }
        }

        fn accepts(_: &Type) -> bool {
            true
        }

        to_sql_checked!();
    }

    fn pg_err(e: tokio_postgres::Error) -> DbError {
        DbError::Other(format!("Postgres: {e}"))
    }

    /// A row as [`Value`]s.
    pub fn row_of(row: &tokio_postgres::Row) -> DbResult<Row> {
        let mut out = Vec::with_capacity(row.len());
        for (i, col) in row.columns().iter().enumerate() {
            let v = match *col.type_() {
                Type::INT8 => row
                    .try_get::<_, Option<i64>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, Value::Int),
                Type::INT4 => row
                    .try_get::<_, Option<i32>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, |n| Value::Int(i64::from(n))),
                Type::INT2 => row
                    .try_get::<_, Option<i16>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, |n| Value::Int(i64::from(n))),
                Type::BOOL => row
                    .try_get::<_, Option<bool>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, |b| Value::Int(i64::from(b))),
                Type::TEXT | Type::VARCHAR | Type::NAME | Type::BPCHAR => row
                    .try_get::<_, Option<String>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, Value::Text),
                Type::BYTEA => row
                    .try_get::<_, Option<Vec<u8>>>(i)
                    .map_err(pg_err)?
                    .map_or(Value::Null, Value::Blob),
                ref other => {
                    return Err(DbError::Other(format!(
                        "a column of Postgres type {other} isn't one the server reads; cast it"
                    )));
                }
            };
            out.push(v);
        }
        Ok(Row(out))
    }
}

mod pg_tls {
    //! TLS for the database connection through `lockdown-tls`'s rustls
    //! configuration (TLS 1.3, hybrid post-quantum first).

    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use rustls::pki_types::ServerName;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_postgres::tls::{ChannelBinding, MakeTlsConnect, TlsConnect};

    /// Makes a connector per host.
    #[derive(Clone)]
    pub struct MakeRustls {
        config: Arc<rustls::ClientConfig>,
    }

    impl MakeRustls {
        pub fn new(config: Arc<rustls::ClientConfig>) -> Self {
            Self { config }
        }
    }

    impl<S> MakeTlsConnect<S> for MakeRustls
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        type Stream = RustlsStream<S>;
        type TlsConnect = RustlsConnect;
        type Error = io::Error;

        fn make_tls_connect(&mut self, domain: &str) -> io::Result<RustlsConnect> {
            let name = ServerName::try_from(domain.to_string())
                .map_err(|_| io::Error::other("the database host isn't a name TLS can check"))?;
            Ok(RustlsConnect {
                connector: tokio_rustls::TlsConnector::from(self.config.clone()),
                name,
            })
        }
    }

    /// Connects one stream.
    pub struct RustlsConnect {
        connector: tokio_rustls::TlsConnector,
        name: ServerName<'static>,
    }

    impl<S> TlsConnect<S> for RustlsConnect
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        type Stream = RustlsStream<S>;
        type Error = io::Error;
        type Future = Pin<Box<dyn Future<Output = io::Result<RustlsStream<S>>> + Send>>;

        fn connect(self, stream: S) -> Self::Future {
            Box::pin(async move {
                let tls = self.connector.connect(self.name, stream).await?;
                Ok(RustlsStream(Box::new(tls)))
            })
        }
    }

    /// The TLS stream.
    pub struct RustlsStream<S>(Box<tokio_rustls::client::TlsStream<S>>);

    impl<S> tokio_postgres::tls::TlsStream for RustlsStream<S>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        fn channel_binding(&self) -> ChannelBinding {
            // SCRAM's channel binding (tls-server-end-point) needs the
            // certificate's signature hash; without it the password exchange
            // is still SCRAM over TLS 1.3.
            ChannelBinding::none()
        }
    }

    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for RustlsStream<S> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut *self.0).poll_read(cx, buf)
        }
    }

    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for RustlsStream<S> {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut *self.0).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut *self.0).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut *self.0).poll_shutdown(cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_numbered_for_postgres_outside_quotes() {
        assert_eq!(
            pg_placeholders("SELECT a FROM t WHERE b = ?1 AND c = ?12 AND d = '?3'"),
            "SELECT a FROM t WHERE b = $1 AND c = $12 AND d = '?3'"
        );
        assert_eq!(pg_placeholders("VALUES (?, ?)"), "VALUES ($1, $2)");
    }

    #[test]
    fn definitions_map_their_types_for_postgres() {
        assert_eq!(
            pg_ddl(
                "CREATE TABLE t (id TEXT PRIMARY KEY, n INTEGER NOT NULL, b BLOB, integers_x TEXT)"
            ),
            "CREATE TABLE t (id TEXT COLLATE \"C\" PRIMARY KEY, n BIGINT NOT NULL, b BYTEA, integers_x TEXT COLLATE \"C\")"
        );
    }

    #[test]
    fn sqlite_values_round_trip_and_transactions_roll_back_unless_committed() {
        let db = Db::sqlite(rusqlite::Connection::open_in_memory().unwrap());
        db.conn()
            .execute_batch("CREATE TABLE t (k TEXT PRIMARY KEY, n INTEGER, b BLOB)")
            .unwrap();
        {
            let tx = db.begin().unwrap();
            tx.execute(
                "INSERT INTO t (k, n, b) VALUES (?1, ?2, ?3)",
                params!["a", 7i64, &b"x"[..]],
            )
            .unwrap();
        }
        assert_eq!(
            db.conn()
                .query_row("SELECT COUNT(*) FROM t", params![], |r| r.get::<i64>(0))
                .unwrap(),
            0
        );
        let tx = db.begin().unwrap();
        tx.execute(
            "INSERT INTO t (k, n, b) VALUES (?1, ?2, ?3)",
            params!["a", 7i64, None::<Vec<u8>>],
        )
        .unwrap();
        tx.commit().unwrap();
        let (n, b): (i64, Option<Vec<u8>>) = db
            .conn()
            .query_row("SELECT n, b FROM t WHERE k = ?1", params!["a"], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((n, b), (7, None));
        let again = db
            .conn()
            .execute("INSERT INTO t (k, n) VALUES (?1, ?2)", params!["a", 1i64]);
        assert!(matches!(again, Err(DbError::Constraint(_))));
        assert_eq!(
            db.conn()
                .execute(
                    "INSERT INTO t (k, n) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
                    params!["a", 1i64]
                )
                .unwrap(),
            0
        );
        assert_eq!(db.conn().columns("t").unwrap(), ["k", "n", "b"]);
        assert_eq!(db.conn().tables().unwrap(), ["t"]);
    }

    #[test]
    fn schema_names_are_plain_words_only() {
        assert!(valid_schema("lockdown_t_0a1b"));
        assert!(!valid_schema("1abc"));
        assert!(!valid_schema("a;drop"));
        assert!(!valid_schema(""));
    }
}
