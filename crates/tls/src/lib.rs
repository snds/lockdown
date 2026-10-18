//! `lockdown-tls`: how Lockdown speaks TLS, on both ends
//! (03-architecture §1 "HTTPS (hybrid TLS)"; docs/plan/20-self-host-server.md §2).
//!
//! - **One provider.** rustls on its `ring` provider, with the hybrid
//!   post-quantum group [`hybrid::X25519MLKEM768`] put first. `ring` stays
//!   the only C/assembly in the TLS stack: the ML-KEM half is RustCrypto's
//!   `ml-kem`, already in the tree for the vault's hybrid wraps.
//! - **TLS 1.3 only** to and from Lockdown's own server. The hybrid group
//!   exists only in 1.3, and every client and proxy Lockdown supports
//!   speaks it.
//! - **Hybrid preferred, classical as the fallback the plan allows.** A
//!   client offers X25519MLKEM768 first and also X25519 (the bundle's
//!   reverse proxy may not have the hybrid group yet, 05-self-host §3).
//!   Lockdown's own server prefers the hybrid group and, under
//!   [`Groups::HybridOnly`], accepts nothing else.
//! - **Trust.** The operating system's store by default; a self-hosted
//!   server on a private CA is pinned with `LOCKDOWN_CA_CERT` (the
//!   certificates in that file and nothing else, [`Trust::from_env`]).
//! - **Certificates that reload.** [`ReloadingCert`] serves the current
//!   certificate and key and swaps in new files when asked (on SIGHUP or
//!   when the files change), keeping the old pair if the new one is bad.

pub mod hybrid;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use rustls::crypto::{CryptoProvider, SupportedKxGroup};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::{ClientConfig, NamedGroup, RootCertStore, ServerConfig};

pub use hybrid::X25519MLKEM768;

/// The environment variable naming a PEM file of certificates to trust
/// instead of the system store (a self-hosted server on its own CA).
pub const CA_CERT_ENV: &str = "LOCKDOWN_CA_CERT";

/// What can go wrong setting TLS up.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// A file couldn't be read.
    #[error("couldn't read {path}: {why}")]
    Read {
        /// The file.
        path: String,
        /// Why.
        why: String,
    },
    /// A file was read but holds no usable certificate or key.
    #[error("{path}: {why}")]
    Pem {
        /// The file.
        path: String,
        /// What is wrong with it.
        why: String,
    },
    /// rustls refused the setup.
    #[error("TLS setup failed: {0}")]
    Rustls(#[from] rustls::Error),
}

/// Which key-exchange groups an end offers or accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Groups {
    /// X25519MLKEM768 first, then X25519 and P-256 for peers without it.
    PreferHybrid,
    /// X25519MLKEM768 and nothing else.
    HybridOnly,
}

/// The groups for `groups`, most preferred first.
pub fn kx_groups(groups: Groups) -> Vec<&'static dyn SupportedKxGroup> {
    use rustls::crypto::ring::kx_group;
    match groups {
        Groups::PreferHybrid => vec![X25519MLKEM768, kx_group::X25519, kx_group::SECP256R1],
        Groups::HybridOnly => vec![X25519MLKEM768],
    }
}

/// rustls's `ring` provider with the hybrid group first ([`kx_groups`]).
pub fn provider(groups: Groups) -> CryptoProvider {
    CryptoProvider {
        kx_groups: kx_groups(groups),
        ..rustls::crypto::ring::default_provider()
    }
}

/// Installs [`provider`]`(PreferHybrid)` as the process's default, so any
/// rustls client built without an explicit provider (the breach-range and
/// change-password fetches, the server's breach feed) also offers the
/// hybrid group. Harmless when a default is already installed.
pub fn install_default_provider() {
    if CryptoProvider::get_default().is_none() {
        let _ = provider(Groups::PreferHybrid).install_default();
    }
}

/// A short name for a negotiated group, for logs and tests.
pub fn group_name(group: NamedGroup) -> &'static str {
    match group {
        NamedGroup::X25519MLKEM768 => "X25519MLKEM768",
        NamedGroup::X25519 => "X25519",
        NamedGroup::secp256r1 => "P-256",
        NamedGroup::secp384r1 => "P-384",
        _ => "other",
    }
}

/// Whether `group` is the post-quantum hybrid.
pub fn is_hybrid(group: NamedGroup) -> bool {
    group == NamedGroup::X25519MLKEM768
}

// --- the client ---------------------------------------------------------------

/// Which certificates a client trusts.
#[derive(Clone, Debug)]
pub enum Trust {
    /// The operating system's store and checks.
    Platform,
    /// These certificates and nothing else (a pinned private CA).
    Only(Vec<CertificateDer<'static>>),
}

impl Trust {
    /// [`Trust::Only`] with the certificates in `LOCKDOWN_CA_CERT` when it
    /// is set, else [`Trust::Platform`]. A set variable naming a missing
    /// or empty file is an error, never a silent fall back to the system.
    pub fn from_env() -> Result<Self, TlsError> {
        match std::env::var_os(CA_CERT_ENV).filter(|v| !v.is_empty()) {
            Some(path) => Ok(Trust::Only(read_certs(Path::new(&path))?)),
            None => Ok(Trust::Platform),
        }
    }
}

/// The client configuration for Lockdown's own server: TLS 1.3 only,
/// hybrid first with X25519 behind it, `trust` for the certificate.
pub fn client_config(trust: &Trust) -> Result<ClientConfig, TlsError> {
    let provider = Arc::new(provider(Groups::PreferHybrid));
    client_config_with(trust, provider)
}

/// [`client_config`] over another provider (tests pin the groups).
pub fn client_config_with(
    trust: &Trust,
    provider: Arc<CryptoProvider>,
) -> Result<ClientConfig, TlsError> {
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?;
    let config = match trust {
        Trust::Platform => {
            let verifier = rustls_platform_verifier::Verifier::new(provider)?;
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verifier))
                .with_no_client_auth()
        }
        Trust::Only(certs) => {
            let mut roots = RootCertStore::empty();
            for cert in certs {
                roots.add(cert.clone())?;
            }
            builder.with_root_certificates(roots).with_no_client_auth()
        }
    };
    Ok(config)
}

// --- the server -----------------------------------------------------------------

/// Reads every certificate in a PEM file.
pub fn read_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let bytes = std::fs::read(path).map_err(|e| TlsError::Read {
        path: path.display().to_string(),
        why: e.to_string(),
    })?;
    let certs = CertificateDer::pem_slice_iter(&bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::Pem {
            path: path.display().to_string(),
            why: format!("isn't a PEM certificate file ({e})"),
        })?;
    if certs.is_empty() {
        return Err(TlsError::Pem {
            path: path.display().to_string(),
            why: "holds no certificate".into(),
        });
    }
    Ok(certs)
}

/// Reads the first private key in a PEM file (PKCS#8, SEC1 or PKCS#1).
pub fn read_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    let bytes = std::fs::read(path).map_err(|e| TlsError::Read {
        path: path.display().to_string(),
        why: e.to_string(),
    })?;
    PrivateKeyDer::from_pem_slice(&bytes).map_err(|_| TlsError::Pem {
        path: path.display().to_string(),
        why: "holds no private key Lockdown can use (PKCS#8, SEC1 or PKCS#1 PEM)".into(),
    })
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

type Stamps = (Option<(SystemTime, u64)>, Option<(SystemTime, u64)>);

/// A certificate chain and key from two PEM files, served to every
/// handshake and swapped when the files change.
pub struct ReloadingCert {
    cert_path: PathBuf,
    key_path: PathBuf,
    provider: Arc<CryptoProvider>,
    current: RwLock<Arc<CertifiedKey>>,
    stamps: Mutex<Stamps>,
}

impl std::fmt::Debug for ReloadingCert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReloadingCert")
            .field("cert", &self.cert_path)
            .field("key", &self.key_path)
            .finish_non_exhaustive()
    }
}

impl ReloadingCert {
    /// Loads the pair; an unreadable file, a key that doesn't match the
    /// certificate or a key type rustls can't sign with is an error.
    pub fn load(
        cert_path: &Path,
        key_path: &Path,
        provider: Arc<CryptoProvider>,
    ) -> Result<Arc<Self>, TlsError> {
        let stamps = (stamp(cert_path), stamp(key_path));
        let pair = Self::read_pair(cert_path, key_path, &provider)?;
        Ok(Arc::new(Self {
            cert_path: cert_path.to_path_buf(),
            key_path: key_path.to_path_buf(),
            provider,
            current: RwLock::new(Arc::new(pair)),
            stamps: Mutex::new(stamps),
        }))
    }

    fn read_pair(
        cert_path: &Path,
        key_path: &Path,
        provider: &CryptoProvider,
    ) -> Result<CertifiedKey, TlsError> {
        let certs = read_certs(cert_path)?;
        let key = read_key(key_path)?;
        CertifiedKey::from_der(certs, key, provider).map_err(|e| TlsError::Pem {
            path: key_path.display().to_string(),
            why: format!("doesn't go with {} ({e})", cert_path.display()),
        })
    }

    /// Reads both files again and swaps them in. On any error the pair in
    /// use stays.
    pub fn reload(&self) -> Result<(), TlsError> {
        let stamps = (stamp(&self.cert_path), stamp(&self.key_path));
        let pair = Self::read_pair(&self.cert_path, &self.key_path, &self.provider)?;
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(pair);
        *self
            .stamps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = stamps;
        Ok(())
    }

    /// [`ReloadingCert::reload`] when either file's time or size moved
    /// since the last load; `Ok(true)` when it swapped.
    pub fn reload_if_changed(&self) -> Result<bool, TlsError> {
        let now = (stamp(&self.cert_path), stamp(&self.key_path));
        let changed = *self
            .stamps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            != now;
        if !changed {
            return Ok(false);
        }
        self.reload().map(|()| true)
    }

    /// The pair handshakes get now.
    pub fn current(&self) -> Arc<CertifiedKey> {
        self.current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ResolvesServerCert for ReloadingCert {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.current())
    }
}

/// The server configuration: TLS 1.3 only, `groups`, HTTP/1.1, the
/// certificate from `cert`.
pub fn server_config(cert: Arc<ReloadingCert>, groups: Groups) -> Result<ServerConfig, TlsError> {
    let provider = Arc::new(provider(groups));
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_cert_resolver(cert);
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

/// A throwaway private CA and a server certificate under it, for tests.
#[cfg(feature = "test-certs")]
pub mod testing {
    use std::path::{Path, PathBuf};

    use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
    use rustls::pki_types::CertificateDer;

    /// A CA and one leaf, as PEM text.
    #[derive(Clone, Debug)]
    pub struct TestPki {
        /// The CA certificate (what a client pins).
        pub ca_pem: String,
        /// The CA certificate, DER.
        pub ca_der: CertificateDer<'static>,
        /// The leaf certificate followed by the CA (a full chain).
        pub chain_pem: String,
        /// The leaf's private key (PKCS#8).
        pub key_pem: String,
    }

    /// Where [`TestPki::write`] put the files.
    #[derive(Clone, Debug)]
    pub struct TestPkiFiles {
        /// The CA certificate.
        pub ca: PathBuf,
        /// The leaf chain.
        pub cert: PathBuf,
        /// The leaf key.
        pub key: PathBuf,
    }

    /// A new CA and a leaf for `names` (DNS names or IP addresses).
    pub fn pki(names: &[&str]) -> TestPki {
        let ca_key = KeyPair::generate().expect("a CA key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Lockdown test CA");
        let ca_cert = ca_params.self_signed(&ca_key).expect("the CA signs itself");
        let issuer = Issuer::new(ca_params, ca_key);
        let leaf_key = KeyPair::generate().expect("a leaf key");
        let leaf_params =
            CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
                .expect("leaf params");
        let leaf = leaf_params
            .signed_by(&leaf_key, &issuer)
            .expect("the CA signs the leaf");
        TestPki {
            ca_pem: ca_cert.pem(),
            ca_der: ca_cert.der().clone(),
            chain_pem: format!("{}{}", leaf.pem(), ca_cert.pem()),
            key_pem: leaf_key.serialize_pem(),
        }
    }

    impl TestPki {
        /// Writes `ca.pem`, `cert.pem` and `key.pem` into `dir`.
        pub fn write(&self, dir: &Path) -> TestPkiFiles {
            let files = TestPkiFiles {
                ca: dir.join("ca.pem"),
                cert: dir.join("cert.pem"),
                key: dir.join("key.pem"),
            };
            std::fs::write(&files.ca, &self.ca_pem).expect("write ca.pem");
            std::fs::write(&files.cert, &self.chain_pem).expect("write cert.pem");
            std::fs::write(&files.key, &self.key_pem).expect("write key.pem");
            files
        }
    }
}
