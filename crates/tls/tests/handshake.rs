//! Handshakes between Lockdown's own TLS ends, in memory, and against an
//! independent implementation (OpenSSL 3.5 through Node) when one is on
//! the machine:
//!
//! - Lockdown's client and Lockdown's server agree on X25519MLKEM768.
//! - A classical-only peer on either side falls back to X25519.
//! - A server set to hybrid only refuses a classical-only client.
//! - A certificate swapped on disk is served after a reload; a bad one is
//!   refused and the old one stays.
//! - Interop: OpenSSL as the server and as the client, hybrid only, so a
//!   wrong share layout or secret order cannot pass.

use std::io::{Read, Write};
use std::sync::Arc;

use lockdown_tls::testing::pki;
use lockdown_tls::{Groups, ReloadingCert, Trust, client_config, server_config};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, NamedGroup, ServerConfig, ServerConnection};

/// Runs a handshake between `client` and `server` in memory, then sends a
/// line each way. Returns the group each end negotiated.
fn handshake(
    client: Arc<ClientConfig>,
    server: Arc<ServerConfig>,
    name: &str,
) -> Result<(NamedGroup, NamedGroup), rustls::Error> {
    let mut c = ClientConnection::new(client, ServerName::try_from(name.to_string()).unwrap())?;
    let mut s = ServerConnection::new(server)?;
    for _ in 0..20 {
        let mut buf = Vec::new();
        while c.wants_write() {
            c.write_tls(&mut buf).unwrap();
        }
        if !buf.is_empty() {
            s.read_tls(&mut buf.as_slice()).unwrap();
            s.process_new_packets()?;
        }
        let mut buf = Vec::new();
        while s.wants_write() {
            s.write_tls(&mut buf).unwrap();
        }
        if !buf.is_empty() {
            c.read_tls(&mut buf.as_slice()).unwrap();
            c.process_new_packets()?;
        }
        if !c.is_handshaking() && !s.is_handshaking() {
            break;
        }
    }
    assert!(!c.is_handshaking() && !s.is_handshaking(), "handshake stalled");
    c.writer().write_all(b"ping\n").unwrap();
    let mut buf = Vec::new();
    c.write_tls(&mut buf).unwrap();
    s.read_tls(&mut buf.as_slice()).unwrap();
    s.process_new_packets()?;
    let mut got = [0u8; 5];
    s.reader().read_exact(&mut got).unwrap();
    assert_eq!(&got, b"ping\n");
    Ok((
        c.negotiated_key_exchange_group().unwrap().name(),
        s.negotiated_key_exchange_group().unwrap().name(),
    ))
}

fn classical_provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

struct Ends {
    _dir: tempfile::TempDir,
    trust: Trust,
    cert: Arc<ReloadingCert>,
}

fn ends() -> Ends {
    let dir = tempfile::tempdir().unwrap();
    let files = pki(&["localhost", "127.0.0.1"]).write(dir.path());
    let cert = ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(Groups::PreferHybrid)),
    )
    .unwrap();
    let trust = Trust::Only(lockdown_tls::read_certs(&files.ca).unwrap());
    Ends {
        _dir: dir,
        trust,
        cert,
    }
}

#[test]
fn lockdowns_own_client_and_server_agree_on_the_hybrid_group() {
    let e = ends();
    let client = Arc::new(client_config(&e.trust).unwrap());
    let server = Arc::new(server_config(e.cert.clone(), Groups::PreferHybrid).unwrap());
    let (c, s) = handshake(client.clone(), server, "localhost").unwrap();
    assert_eq!(c, NamedGroup::X25519MLKEM768);
    assert_eq!(s, NamedGroup::X25519MLKEM768);

    // Hybrid only on the server: still the hybrid group, by IP name too.
    let strict = Arc::new(server_config(e.cert.clone(), Groups::HybridOnly).unwrap());
    let (c, s) = handshake(client, strict, "127.0.0.1").unwrap();
    assert_eq!((c, s), (NamedGroup::X25519MLKEM768, NamedGroup::X25519MLKEM768));
}

#[test]
fn a_classical_peer_falls_back_to_x25519_unless_the_server_is_hybrid_only() {
    let e = ends();
    // An older client (rustls's plain ring provider: X25519, P-256, P-384).
    let old_client = Arc::new(
        lockdown_tls::client_config_with(&e.trust, classical_provider()).unwrap(),
    );
    let server = Arc::new(server_config(e.cert.clone(), Groups::PreferHybrid).unwrap());
    let (c, s) = handshake(old_client.clone(), server, "localhost").unwrap();
    assert_eq!((c, s), (NamedGroup::X25519, NamedGroup::X25519));

    // A reverse proxy without the hybrid group, in front of Lockdown.
    let client = Arc::new(client_config(&e.trust).unwrap());
    let mut proxy = ServerConfig::builder_with_provider(classical_provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(e.cert.clone());
    proxy.alpn_protocols = vec![b"http/1.1".to_vec()];
    let (c, s) = handshake(client, Arc::new(proxy), "localhost").unwrap();
    assert_eq!((c, s), (NamedGroup::X25519, NamedGroup::X25519));

    // Hybrid only: the old client gets no session at all.
    let strict = Arc::new(server_config(e.cert.clone(), Groups::HybridOnly).unwrap());
    assert!(handshake(old_client, strict, "localhost").is_err());
}

#[test]
fn the_client_refuses_a_certificate_it_was_not_told_to_trust_and_a_wrong_name() {
    let e = ends();
    let server = Arc::new(server_config(e.cert.clone(), Groups::PreferHybrid).unwrap());
    let other = pki(&["localhost"]);
    let stranger = Arc::new(client_config(&Trust::Only(vec![other.ca_der])).unwrap());
    assert!(handshake(stranger, server.clone(), "localhost").is_err());
    let client = Arc::new(client_config(&e.trust).unwrap());
    assert!(handshake(client, server, "lockdown.example").is_err());
}

#[test]
fn a_swapped_certificate_is_served_after_a_reload_and_a_bad_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let first = pki(&["localhost"]);
    let files = first.write(dir.path());
    let cert = ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(Groups::PreferHybrid)),
    )
    .unwrap();
    let server = Arc::new(server_config(cert.clone(), Groups::PreferHybrid).unwrap());
    // A fresh client each time: a client that kept a session ticket would
    // resume without looking at the certificate again.
    let trusting = |ca: &rustls::pki_types::CertificateDer<'static>| {
        Arc::new(client_config(&Trust::Only(vec![ca.clone()])).unwrap())
    };
    assert!(handshake(trusting(&first.ca_der), server.clone(), "localhost").is_ok());
    assert!(!cert.reload_if_changed().unwrap(), "nothing changed yet");

    // A renewal: new files in the same place (different size, so the
    // change shows even within one second's timestamp).
    let second = pki(&["localhost", "lockdown.internal"]);
    std::fs::write(&files.cert, &second.chain_pem).unwrap();
    std::fs::write(&files.key, &second.key_pem).unwrap();
    assert!(cert.reload_if_changed().unwrap());
    assert!(handshake(trusting(&second.ca_der), server.clone(), "localhost").is_ok());
    assert!(handshake(trusting(&first.ca_der), server.clone(), "localhost").is_err());

    // A half-written renewal (a key that doesn't match) is refused and the
    // second pair keeps serving.
    let third = pki(&["localhost"]);
    std::fs::write(&files.key, &third.key_pem).unwrap();
    assert!(cert.reload().is_err());
    assert!(handshake(trusting(&second.ca_der), server, "localhost").is_ok());
}

#[test]
fn a_missing_or_empty_pinned_file_is_an_error_not_the_system_store() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.pem");
    std::fs::write(&empty, "").unwrap();
    assert!(lockdown_tls::read_certs(&empty).is_err());
    assert!(lockdown_tls::read_certs(&dir.path().join("nope.pem")).is_err());
    let junk = dir.path().join("junk.pem");
    std::fs::write(&junk, "-----BEGIN CERTIFICATE-----\n!!!\n-----END CERTIFICATE-----\n").unwrap();
    assert!(lockdown_tls::read_certs(&junk).is_err());
    assert!(lockdown_tls::read_key(&junk).is_err());
}

// --- interop with OpenSSL (through Node) -------------------------------------

/// Node with an OpenSSL that has X25519MLKEM768 (3.5 and later), or why not.
/// `LOCKDOWN_TLS_INTEROP=require` turns a missing one into a failure (CI).
fn node_with_mlkem() -> Option<std::path::PathBuf> {
    let required = std::env::var("LOCKDOWN_TLS_INTEROP").as_deref() == Ok("require");
    let probe = std::process::Command::new("node")
        .args([
            "-e",
            "const tls=require('tls');try{tls.createSecureContext({ecdhCurve:'X25519MLKEM768'});console.log('yes')}catch(e){console.log('no')}",
        ])
        .output();
    match probe {
        Ok(out) if String::from_utf8_lossy(&out.stdout).trim() == "yes" => Some("node".into()),
        _ => {
            assert!(
                !required,
                "LOCKDOWN_TLS_INTEROP=require, but no node with X25519MLKEM768 is on PATH"
            );
            eprintln!("skipping the OpenSSL interop: no node with X25519MLKEM768 on PATH");
            None
        }
    }
}

const NODE_SERVER: &str = r#"
const tls = require('tls'); const fs = require('fs');
const [cert, key] = process.argv.slice(1);
const server = tls.createServer({ cert: fs.readFileSync(cert), key: fs.readFileSync(key),
  ecdhCurve: 'X25519MLKEM768', minVersion: 'TLSv1.3' }, (s) => {
  s.on('data', (d) => { s.end('pong ' + d.toString().trim() + '\n'); });
});
server.listen(0, '127.0.0.1', () => { console.log(server.address().port); });
setTimeout(() => process.exit(0), 20000);
"#;

const NODE_CLIENT: &str = r#"
const tls = require('tls'); const fs = require('fs');
const [port, ca] = process.argv.slice(1);
const s = tls.connect({ host: '127.0.0.1', port: Number(port), servername: 'localhost',
  ca: fs.readFileSync(ca), ecdhCurve: 'X25519MLKEM768', minVersion: 'TLSv1.3' }, () => {
  s.write('ping\n');
});
s.on('data', (d) => { console.log(d.toString().trim()); s.end(); });
s.on('error', (e) => { console.error(e.message); process.exit(1); });
"#;

#[test]
fn openssl_as_the_server_negotiates_the_hybrid_with_lockdowns_client() {
    let Some(node) = node_with_mlkem() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ca = pki(&["localhost"]);
    let files = ca.write(dir.path());
    let mut child = std::process::Command::new(node)
        .args(["-e", NODE_SERVER])
        .arg(&files.cert)
        .arg(&files.key)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut port = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut port,
    )
    .unwrap();
    let port: u16 = port.trim().parse().unwrap();

    // Hybrid only on our side: nothing classical to fall back to.
    let provider = Arc::new(lockdown_tls::provider(Groups::HybridOnly));
    let config = lockdown_tls::client_config_with(&Trust::Only(vec![ca.ca_der]), provider).unwrap();
    let mut conn = ClientConnection::new(Arc::new(config), ServerName::try_from("localhost").unwrap()).unwrap();
    let mut sock = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    tls.write_all(b"ping\n").unwrap();
    let mut reply = String::new();
    let _ = tls.read_to_string(&mut reply);
    assert_eq!(reply.trim(), "pong ping");
    assert_eq!(
        conn.negotiated_key_exchange_group().unwrap().name(),
        NamedGroup::X25519MLKEM768
    );
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn openssl_as_the_client_negotiates_the_hybrid_with_lockdowns_server() {
    let Some(node) = node_with_mlkem() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let files = pki(&["localhost"]).write(dir.path());
    let cert = ReloadingCert::load(
        &files.cert,
        &files.key,
        Arc::new(lockdown_tls::provider(Groups::HybridOnly)),
    )
    .unwrap();
    let config = Arc::new(server_config(cert, Groups::HybridOnly).unwrap());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut conn = ServerConnection::new(config).unwrap();
        let mut line = Vec::new();
        {
            let mut tls = rustls::Stream::new(&mut conn, &mut sock);
            let mut byte = [0u8; 1];
            while tls.read(&mut byte).unwrap() == 1 && byte[0] != b'\n' {
                line.push(byte[0]);
            }
            tls.write_all(b"pong\n").unwrap();
            tls.flush().unwrap();
        }
        conn.send_close_notify();
        let _ = conn.write_tls(&mut sock);
        (
            String::from_utf8(line).unwrap(),
            conn.negotiated_key_exchange_group().unwrap().name(),
        )
    });
    let out = std::process::Command::new(node)
        .args(["-e", NODE_CLIENT])
        .arg(port.to_string())
        .arg(&files.ca)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "node's client failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "pong");
    let (line, group) = server.join().unwrap();
    assert_eq!(line, "ping");
    assert_eq!(group, NamedGroup::X25519MLKEM768);
}
