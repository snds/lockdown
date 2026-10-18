//! Attachment bodies in an S3-compatible bucket (`single-container`;
//! docs/plan/25-self-host-2.md §3): MinIO, Garage, Ceph, AWS S3 or any
//! other store that speaks the S3 API with Signature Version 4.
//!
//! Only the bodies move. Each blob's row (vault, BLAKE3 name, size, when it
//! was uploaded, which items name it) stays in the database, so the
//! attachment quota (D234), content addressing and the grace period before
//! collection (D76) work exactly as they do with bodies in the database or
//! in files. An object is `<prefix><vault>/<BLAKE3 hex>`, written whole by
//! one `PUT` before its row, read by one `GET`, removed by one `DELETE`
//! after its row went. The bucket sees ciphertext named by its own hash and
//! the vault's id, which the server sees anyway (05-self-host §2).
//!
//! The client is small on purpose: path-style (or virtual-hosted) URLs,
//! SigV4 signed with the payload's SHA-256, over `reqwest` with TLS from
//! `lockdown-tls`'s provider. Plain `http://` only to this machine (a MinIO
//! beside the server); everything else is `https://`. The secret key is
//! never logged and is wiped when the client goes.

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use lockdown_vault::{Hash32, Id};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::storage::{BlobBodies, Result, StorageError};

/// The largest body this client sends in one `PUT` (S3's single-part limit
/// is 5 GiB; the server's own blob cap is far below it).
pub const MAX_OBJECT: usize = 5 << 30;

/// How to reach the bucket.
#[derive(Clone)]
pub struct S3Config {
    /// `https://host[:port]` (or `http://` to this machine).
    pub endpoint: String,
    /// The bucket.
    pub bucket: String,
    /// The signing region (`us-east-1` for MinIO's default).
    pub region: String,
    /// The access key id.
    pub access_key: String,
    /// The secret access key.
    pub secret_key: Zeroizing<String>,
    /// Put in front of every object name (`lockdown/`); may be empty.
    pub prefix: String,
    /// `https://bucket.host/…` instead of `https://host/bucket/…`.
    pub virtual_hosted: bool,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

/// Attachment bodies in a bucket.
pub struct S3Bodies {
    config: S3Config,
    scheme: String,
    host: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for S3Bodies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "S3Bodies({}/{})", self.host, self.config.bucket)
    }
}

fn s3_err(what: &str) -> StorageError {
    StorageError::Backend(format!("the attachment bucket: {what}"))
}

/// Whether a bucket name is one S3 allows (and so safe in a host or path).
fn valid_bucket(name: &str) -> bool {
    (3..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.as_bytes()[name.len() - 1].is_ascii_alphanumeric()
}

impl S3Bodies {
    /// A client for `config`. Refuses plain HTTP away from this machine and
    /// names that couldn't be a bucket or a prefix.
    pub fn new(config: S3Config) -> std::result::Result<Self, String> {
        if !valid_bucket(&config.bucket) {
            return Err(format!(
                "'{}' isn't an S3 bucket name (3 to 63 lowercase letters, digits, dots and dashes)",
                config.bucket
            ));
        }
        if !config
            .prefix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/' | b'.'))
            || config.prefix.starts_with('/')
            || config.prefix.contains("..")
        {
            return Err(
                "the S3 prefix may hold letters, digits, - _ . and /, and not start with /".into(),
            );
        }
        if config.access_key.trim().is_empty() || config.secret_key.trim().is_empty() {
            return Err("the S3 access key and secret key are both needed".into());
        }
        let url = url::Url::parse(config.endpoint.trim())
            .map_err(|_| format!("'{}' isn't an S3 endpoint URL", config.endpoint))?;
        let host = url
            .host_str()
            .ok_or("the S3 endpoint names no host")?
            .to_string();
        let local = host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false);
        match url.scheme() {
            "https" => {}
            "http" if local => {}
            "http" => {
                return Err(
                    "the S3 endpoint must be https:// (plain http:// only to this machine)".into(),
                );
            }
            other => return Err(format!("an S3 endpoint can't be {other}://")),
        }
        if url.path() != "/" || url.query().is_some() {
            return Err("give the S3 endpoint as scheme, host and port only".into());
        }
        let host = match url.port() {
            Some(p) => format!("{host}:{p}"),
            None => host,
        };
        let host = if config.virtual_hosted {
            format!("{}.{host}", config.bucket)
        } else {
            host
        };
        lockdown_tls::install_default_provider();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("lockdown-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("couldn't make the S3 client: {e}"))?;
        Ok(Self {
            scheme: url.scheme().to_string(),
            host,
            config,
            client,
        })
    }

    /// The object's path, as sent and as signed.
    fn path(&self, key: Option<&str>) -> String {
        let mut path = String::from("/");
        if !self.config.virtual_hosted {
            path.push_str(&self.config.bucket);
            path.push('/');
        }
        if let Some(key) = key {
            path.push_str(&uri_encode(key, false));
        }
        path
    }

    fn key(&self, vault_id: Id, hash: Hash32) -> String {
        format!("{}{vault_id}/{}", self.config.prefix, hash.to_hex())
    }

    /// One signed request; the status and the body.
    fn send(&self, method: reqwest::Method, path: &str, body: Vec<u8>) -> Result<(u16, Vec<u8>)> {
        let now = lockdown_vault::Timestamp::now().0;
        let payload_hash = hex(&Sha256::digest(&body));
        let headers = vec![
            ("host".to_string(), self.host.clone()),
            ("x-amz-content-sha256".to_string(), payload_hash.clone()),
            ("x-amz-date".to_string(), amz_date(now)),
        ];
        let authorization = sign(
            &SigningInput {
                method: method.as_str(),
                path,
                query: "",
                headers: &headers,
                payload_hash: &payload_hash,
                now,
                region: &self.config.region,
                access_key: &self.config.access_key,
            },
            &self.config.secret_key,
        );
        let url = format!("{}://{}{path}", self.scheme, self.host);
        let mut req = self
            .client
            .request(method, url)
            .header("authorization", authorization)
            .body(body);
        for (name, value) in headers.iter().filter(|(n, _)| n != "host") {
            req = req.header(name.as_str(), value.as_str());
        }
        let outcome = crate::db::run_io(async move {
            let res = req.send().await.map_err(|e| {
                if e.is_timeout() {
                    "it didn't answer in time".to_string()
                } else if e.is_connect() {
                    "it couldn't be reached".to_string()
                } else {
                    "the request failed".to_string()
                }
            })?;
            let status = res.status().as_u16();
            let bytes = res
                .bytes()
                .await
                .map_err(|_| "its answer was cut off".to_string())?;
            Ok::<_, String>((status, bytes.to_vec()))
        })
        .map_err(|e| s3_err(&e))?;
        outcome.map_err(|e| s3_err(&e))
    }

    /// Checks the bucket is there and the keys may use it (at start, so a
    /// typo is found before the first upload).
    pub fn check(&self) -> Result<()> {
        let (status, _) = self.send(reqwest::Method::HEAD, &self.path(None), Vec::new())?;
        match status {
            200 => Ok(()),
            301 | 307 | 400 => Err(s3_err(
                "it answered for another region or address style; check --s3-region and --s3-virtual-hosted",
            )),
            403 => Err(s3_err("it refused the access key")),
            404 => Err(s3_err("there is no such bucket; make it first")),
            other => Err(s3_err(&format!("it answered {other}"))),
        }
    }
}

impl BlobBodies for S3Bodies {
    fn put(&self, vault_id: Id, hash: Hash32, body: &[u8]) -> Result<()> {
        if body.len() > MAX_OBJECT {
            return Err(s3_err("an attachment is larger than one upload may be"));
        }
        let path = self.path(Some(&self.key(vault_id, hash)));
        let (status, _) = self.send(reqwest::Method::PUT, &path, body.to_vec())?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(s3_err(&format!("it refused an upload ({status})")))
        }
    }

    fn get(&self, vault_id: Id, hash: Hash32) -> Result<Option<Vec<u8>>> {
        let path = self.path(Some(&self.key(vault_id, hash)));
        let (status, body) = self.send(reqwest::Method::GET, &path, Vec::new())?;
        match status {
            200 => Ok(Some(body)),
            404 => Ok(None),
            other => Err(s3_err(&format!("it refused a download ({other})"))),
        }
    }

    fn delete(&self, vault_id: Id, hash: Hash32) -> Result<()> {
        let path = self.path(Some(&self.key(vault_id, hash)));
        let (status, _) = self.send(reqwest::Method::DELETE, &path, Vec::new())?;
        if (200..300).contains(&status) || status == 404 {
            Ok(())
        } else {
            Err(s3_err(&format!("it refused a removal ({status})")))
        }
    }

    fn describe(&self) -> String {
        format!(
            "S3 bucket {} at {}://{}{}",
            self.config.bucket,
            self.scheme,
            self.host,
            if self.config.prefix.is_empty() {
                String::new()
            } else {
                format!(" under {}", self.config.prefix)
            }
        )
    }
}

// --- Signature Version 4 ---------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

/// S3's URI encoding: everything but unreserved characters as `%XX`, and
/// `/` kept in an object key's path.
pub fn uri_encode(text: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else if b == b'/' && !encode_slash {
            out.push('/');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `YYYYMMDDTHHMMSSZ` for seconds since the Unix epoch.
pub fn amz_date(secs: i64) -> String {
    let date = lockdown_sync::engine::civil_date(secs).replace('-', "");
    let rem = secs.rem_euclid(86_400);
    format!(
        "{date}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// What a request's signature covers.
pub struct SigningInput<'a> {
    /// `GET`, `PUT`, …
    pub method: &'a str,
    /// The URI-encoded path.
    pub path: &'a str,
    /// The canonical query string (sorted, encoded); empty for none.
    pub query: &'a str,
    /// Every signed header, lowercase names, trimmed values.
    pub headers: &'a [(String, String)],
    /// The payload's SHA-256, hex (the `x-amz-content-sha256` value).
    pub payload_hash: &'a str,
    /// When (seconds since the Unix epoch; also in `x-amz-date`).
    pub now: i64,
    /// The region.
    pub region: &'a str,
    /// The access key id.
    pub access_key: &'a str,
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// The `Authorization` header's value.
pub fn sign(input: &SigningInput<'_>, secret_key: &str) -> String {
    let mut headers: Vec<(String, String)> = input
        .headers
        .iter()
        .map(|(n, v)| (n.to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(n, v)| format!("{n}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        input.method, input.path, input.query, input.payload_hash
    );
    let date = amz_date(input.now);
    let day = &date[..8];
    let scope = format!("{day}/{}/s3/aws4_request", input.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let k_secret = Zeroizing::new(format!("AWS4{secret_key}"));
    let k_date = Zeroizing::new(hmac(k_secret.as_bytes(), day.as_bytes()));
    let k_region = Zeroizing::new(hmac(&k_date, input.region.as_bytes()));
    let k_service = Zeroizing::new(hmac(&k_region, b"s3"));
    let k_signing = Zeroizing::new(hmac(&k_service, b"aws4_request"));
    let signature = hex(&hmac(&k_signing, string_to_sign.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        input.access_key
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "GET Object" example of AWS's Signature Version 4 documentation
    /// for S3 (header-based authentication).
    #[test]
    fn the_signature_matches_the_published_example() {
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let headers = vec![
            (
                "host".to_string(),
                "examplebucket.s3.amazonaws.com".to_string(),
            ),
            ("range".to_string(), "bytes=0-9".to_string()),
            ("x-amz-content-sha256".to_string(), empty.to_string()),
            ("x-amz-date".to_string(), "20130524T000000Z".to_string()),
        ];
        // 2013-05-24T00:00:00Z.
        let now = 1_369_353_600;
        assert_eq!(amz_date(now), "20130524T000000Z");
        let auth = sign(
            &SigningInput {
                method: "GET",
                path: "/test.txt",
                query: "",
                headers: &headers,
                payload_hash: empty,
                now,
                region: "us-east-1",
                access_key: "AKIAIOSFODNN7EXAMPLE",
            },
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn keys_are_encoded_and_bad_settings_refused() {
        assert_eq!(uri_encode("a b/c~d", false), "a%20b/c~d");
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
        let config = |endpoint: &str, bucket: &str| S3Config {
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            region: "us-east-1".into(),
            access_key: "ak".into(),
            secret_key: Zeroizing::new("sk".into()),
            prefix: "lockdown/".into(),
            virtual_hosted: false,
        };
        assert!(S3Bodies::new(config("http://127.0.0.1:9000", "attachments")).is_ok());
        assert!(S3Bodies::new(config("http://minio.lan:9000", "attachments")).is_err());
        assert!(S3Bodies::new(config("https://s3.example.com", "Bad_Bucket")).is_err());
        assert!(S3Bodies::new(config("https://s3.example.com/path", "attachments")).is_err());
        let mut sneaky = config("https://s3.example.com", "attachments");
        sneaky.prefix = "../x".into();
        assert!(S3Bodies::new(sneaky).is_err());
        let b = S3Bodies::new(config("https://s3.example.com:9443", "attachments")).unwrap();
        let vault = Id::new();
        assert_eq!(
            b.path(Some(&b.key(vault, Hash32([0xab; 32])))),
            format!("/attachments/lockdown/{vault}/{}", "ab".repeat(32))
        );
        assert!(b.describe().contains("s3.example.com:9443"));
    }
}
