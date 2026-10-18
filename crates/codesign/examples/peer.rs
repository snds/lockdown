//! The other end of `tests/signed.rs`: plays the browser host. It connects
//! to the address it is given (a socket path on Unix, a pipe name on
//! Windows), checks the program there by its own signature's policy, sends
//! its verdict, prints the verdict it gets back, and exits 0 when it
//! trusted the other end, 3 when it refused it. CI signs copies of it with
//! throwaway certificates (`.github/workflows/ci.yml`, `codesign` jobs).

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use ecosystem_identity::handshake::{PeerChannel, PeerTransport};
use lockdown_codesign::{APP_IDENTIFIER, PeerCode, SignatureVerifier, own_policy};

fn main() -> ExitCode {
    let Some(address) = std::env::args().nth(1) else {
        eprintln!("usage: peer <socket path | pipe name>");
        return ExitCode::from(2);
    };
    let policy = Arc::new(own_policy(APP_IDENTIFIER));
    eprintln!("peer: policy {}: {}", policy.kind(), policy.describe());
    let (mut channel, code) = match connect(&address) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("peer: couldn't connect: {e}");
            return ExitCode::from(2);
        }
    };
    let verdict = SignatureVerifier::new(policy, code).check();
    let line = match &verdict {
        Ok(()) => "trusted".to_string(),
        Err(why) => format!("refused: {why}"),
    };
    eprintln!("peer: {line}");
    if channel.send(line.as_bytes()).is_err() {
        return ExitCode::from(2);
    }
    match channel.recv(Duration::from_secs(60)) {
        Ok(answer) => println!("{}", String::from_utf8_lossy(&answer)),
        Err(e) => println!("no answer: {e}"),
    }
    if verdict.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

#[cfg(unix)]
fn connect(address: &str) -> Result<(Box<dyn PeerChannel>, PeerCode), String> {
    let ch = lockdown_codesign::SocketTransport::at(std::path::Path::new(address))
        .connect(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    let code = ch.peer_code();
    Ok((Box::new(ch), code))
}

#[cfg(windows)]
fn connect(address: &str) -> Result<(Box<dyn PeerChannel>, PeerCode), String> {
    let ch = lockdown_pipe::PipeTransport::at(address)
        .connect(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    let code = ch
        .peer_process()
        .and_then(|p| p.exe.clone())
        .map_or(PeerCode::Unknown, PeerCode::Image);
    Ok((Box::new(ch), code))
}
