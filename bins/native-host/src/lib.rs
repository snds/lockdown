//! `lockdown-native-host`: the small program a browser starts when the
//! Lockdown extension calls `runtime.connectNative("design.snds.lockdown")`
//! (docs/plan/16-browser-extension.md §2).
//!
//! What it does, in order:
//!
//! 1. Reads which extension started it from its arguments
//!    ([`Caller::from_args`]).
//! 2. Reads the endpoint file the desktop app wrote in Lockdown's data
//!    directory, connects to the socket (Linux, macOS) or named pipe
//!    (Windows, `lockdown-pipe`) it names, and checks the process on the
//!    other end is that executable with that SHA-256
//!    ([`ProcessIdentityVerifier`], or `lockdown-pipe`'s check by the same
//!    rules); the desktop checks this host the same way from its side.
//! 3. Sends a [`HostHello`] naming the caller, relays the desktop's first
//!    frame (ready, or refused) to the browser, and then relays each
//!    browser message to the desktop and each answer back, one at a time.
//!
//! It never parses a request beyond its `id`, never decrypts, keeps
//! nothing, and writes no log: a fill's value passes through it once, in
//! memory, on its way from the desktop to the extension.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::time::Duration;

use ecosystem_identity::handshake::{PeerChannel, PeerVerifier};
use lockdown_native_messaging::caller::{Caller, HostHello};
use lockdown_native_messaging::framing::{read_message, write_message};
use serde_json::json;

pub use ecosystem_identity::handshake::ProcessIdentityVerifier;

/// How long one answer may take: a fill that waits on Touch ID or Windows
/// Hello is the slowest case.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(180);

/// How long the desktop has to answer the hello.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// The largest frame the socket carries (the identity crate's framing).
pub const MAX_FRAME: usize = ecosystem_identity::handshake::wire::MAX_FRAME;

/// Writes a message the host itself says to the extension: `{"event":
/// <event>, "message": <words>}`. Used when there is no desktop to answer.
pub fn say<W: Write>(out: &mut W, event: &str, message: &str) {
    let body = json!({ "event": event, "message": message }).to_string();
    let _ = write_message(out, body.as_bytes());
}

/// An answer the host gives in the desktop's place, for one request.
fn refuse<W: Write>(out: &mut W, request: &[u8], code: &str, message: &str) {
    let id = serde_json::from_slice::<serde_json::Value>(request)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(serde_json::Value::Null);
    let body = json!({ "id": id, "ok": false, "error": { "code": code, "message": message } });
    let _ = write_message(out, body.to_string().as_bytes());
}

/// Checks the channel's peer, sends the hello, and relays until the
/// browser closes the port or the desktop goes away.
pub fn relay<R: Read, W: Write, C: PeerChannel>(
    caller: Caller,
    verifier: &dyn PeerVerifier,
    mut channel: C,
    input: &mut R,
    output: &mut W,
) -> Result<(), String> {
    if let Err(e) = verifier.verify(&channel.peer()) {
        say(
            output,
            "untrusted_app",
            &format!("the program on Lockdown's socket isn't the Lockdown app ({e})"),
        );
        return Err(e.to_string());
    }
    let hello = serde_json::to_vec(&HostHello::new(caller)).map_err(|e| e.to_string())?;
    channel.send(&hello).map_err(|e| e.to_string())?;
    let first = match channel.recv(HELLO_TIMEOUT) {
        Ok(f) => f,
        Err(e) => {
            say(output, "closed", "the Lockdown app didn't answer");
            return Err(e.to_string());
        }
    };
    write_message(output, &first).map_err(|e| e.to_string())?;
    if serde_json::from_slice::<serde_json::Value>(&first)
        .ok()
        .and_then(|v| v.get("event").and_then(|e| e.as_str()).map(str::to_string))
        .as_deref()
        != Some("ready")
    {
        return Ok(());
    }
    loop {
        let message = match read_message(input) {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        if message.len() > MAX_FRAME {
            refuse(
                output,
                &message,
                "too_large",
                "that message is larger than Lockdown accepts",
            );
            continue;
        }
        if let Err(e) = channel.send(&message) {
            refuse(
                output,
                &message,
                "closed",
                "the Lockdown app closed the connection",
            );
            return Err(e.to_string());
        }
        match channel.recv(ANSWER_TIMEOUT) {
            Ok(answer) => write_message(output, &answer).map_err(|e| e.to_string())?,
            Err(e) => {
                refuse(output, &message, "closed", "the Lockdown app didn't answer");
                return Err(e.to_string());
            }
        }
    }
}

/// Reads the endpoint file in `data`, or says Lockdown isn't running.
fn endpoint<W: Write>(
    data: &std::path::Path,
    output: &mut W,
) -> Result<lockdown_native_messaging::endpoint::EndpointFile, String> {
    lockdown_native_messaging::endpoint::EndpointFile::read(data).map_err(|_| {
        say(
            output,
            "no_app",
            "Lockdown isn't running on this computer; open it and unlock",
        );
        "no endpoint file".to_string()
    })
}

/// The desktop app the endpoint file names, by path and hash: the only
/// program this host relays to.
fn the_app(
    endpoint: &lockdown_native_messaging::endpoint::EndpointFile,
) -> Result<ecosystem_identity::handshake::verify::AllowedBinary, String> {
    Ok(ecosystem_identity::handshake::verify::AllowedBinary {
        app: "lockdown".into(),
        path: endpoint.app_exe.clone(),
        sha256: Some(endpoint.app_hash().map_err(|e| e.to_string())?),
    })
}

/// Connects to the desktop named by the endpoint file in `data` and
/// relays: over the Unix socket it names (Linux, macOS), checked by the
/// identity crate's process identity.
#[cfg(unix)]
pub fn run<R: Read, W: Write>(
    caller: Caller,
    data: &std::path::Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), String> {
    use ecosystem_identity::handshake::PeerTransport;
    use ecosystem_identity::handshake::unix::UnixSocketTransport;

    let endpoint = endpoint(data, output)?;
    let verifier = ProcessIdentityVerifier::new(vec![the_app(&endpoint)?]);
    let channel = match UnixSocketTransport::at(&endpoint.socket).connect(HELLO_TIMEOUT) {
        Ok(c) => c,
        Err(_) => {
            say(
                output,
                "no_app",
                "Lockdown isn't running on this computer; open it and unlock",
            );
            return Err("nobody on the socket".into());
        }
    };
    relay(caller, &verifier, channel, input, output)
}

/// Connects to the desktop named by the endpoint file in `data` and
/// relays: over the named pipe it names (Windows), checked by the same
/// rules as the socket (user, path, SHA-256) through `lockdown-pipe`.
#[cfg(windows)]
pub fn run<R: Read, W: Write>(
    caller: Caller,
    data: &std::path::Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), String> {
    use ecosystem_identity::handshake::PeerTransport;
    use lockdown_pipe::{PipeTransport, PipeVerifier};

    let endpoint = endpoint(data, output)?;
    let verifier = PipeVerifier::new(vec![the_app(&endpoint)?]);
    let name = endpoint.socket.to_string_lossy();
    if !name.starts_with(lockdown_pipe::PIPE_PREFIX) {
        say(
            output,
            "untrusted_app",
            "the endpoint file doesn't name a Lockdown pipe",
        );
        return Err("not a Lockdown pipe".into());
    }
    let channel = match PipeTransport::at(&name).connect(HELLO_TIMEOUT) {
        Ok(c) => c,
        Err(_) => {
            say(
                output,
                "no_app",
                "Lockdown isn't running on this computer; open it and unlock",
            );
            return Err("nobody on the pipe".into());
        }
    };
    let bound = verifier.bind(channel.peer_process());
    relay(caller, &bound, channel, input, output)
}

/// On a platform with neither a Unix socket nor a named pipe, the host
/// says so.
#[cfg(not(any(unix, windows)))]
pub fn run<R: Read, W: Write>(
    _caller: Caller,
    _data: &std::path::Path,
    _input: &mut R,
    output: &mut W,
) -> Result<(), String> {
    say(
        output,
        "unsupported",
        "the Lockdown app can't talk to the browser on this system yet",
    );
    Err("unsupported platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecosystem_identity::handshake::{AllowAll, ChannelTransport, PeerTransport, Scripted};
    use lockdown_native_messaging::caller::Family;
    use lockdown_native_messaging::{FIREFOX_ID, framing};

    fn caller() -> Caller {
        Caller {
            family: Family::Firefox,
            extension_id: FIREFOX_ID.into(),
        }
    }

    fn browser_input(messages: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for m in messages {
            framing::write_message(&mut buf, m.as_bytes()).unwrap();
        }
        buf
    }

    fn outputs(mut buf: &[u8]) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        while let Some(m) = framing::read_message(&mut buf).unwrap() {
            out.push(serde_json::from_slice(&m).unwrap());
        }
        out
    }

    #[test]
    fn the_host_says_hello_then_relays_each_request_and_answer() {
        let t = ChannelTransport::new();
        let desk = t.clone();
        let desktop = std::thread::spawn(move || {
            let mut ch = desk.accept(Duration::from_secs(5)).unwrap();
            let hello: HostHello =
                serde_json::from_slice(&ch.recv(Duration::from_secs(5)).unwrap()).unwrap();
            assert_eq!(hello.caller.extension_id, FIREFOX_ID);
            ch.send(br#"{"event":"ready","challenge":"00"}"#).unwrap();
            for _ in 0..2 {
                let req: serde_json::Value =
                    serde_json::from_slice(&ch.recv(Duration::from_secs(5)).unwrap()).unwrap();
                let answer = json!({ "id": req["id"], "ok": true, "result": req["op"] });
                ch.send(answer.to_string().as_bytes()).unwrap();
            }
        });
        let ch = t.connect(Duration::from_secs(5)).unwrap();
        let input = browser_input(&[r#"{"id":1,"op":"status"}"#, r#"{"id":2,"op":"search"}"#]);
        let mut output = Vec::new();
        relay(caller(), &AllowAll, ch, &mut input.as_slice(), &mut output).unwrap();
        desktop.join().unwrap();
        let out = outputs(&output);
        assert_eq!(out[0]["event"], "ready");
        assert_eq!(out[1], json!({"id": 1, "ok": true, "result": "status"}));
        assert_eq!(out[2], json!({"id": 2, "ok": true, "result": "search"}));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn an_untrusted_peer_gets_nothing() {
        let t = ChannelTransport::new();
        let desk = t.clone();
        let listener = std::thread::spawn(move || {
            let mut ch = desk.accept(Duration::from_secs(5)).unwrap();
            // The host hangs up without a hello.
            assert!(ch.recv(Duration::from_secs(2)).is_err());
        });
        let ch = t.connect(Duration::from_secs(5)).unwrap();
        let refuse = Scripted::new([Err("not the app".to_string())]);
        let input = browser_input(&[r#"{"id":1,"op":"status"}"#]);
        let mut output = Vec::new();
        assert!(relay(caller(), &refuse, ch, &mut input.as_slice(), &mut output).is_err());
        listener.join().unwrap();
        let out = outputs(&output);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["event"], "untrusted_app");
    }

    #[test]
    fn a_refusal_from_the_desktop_ends_the_relay() {
        let t = ChannelTransport::new();
        let desk = t.clone();
        let desktop = std::thread::spawn(move || {
            let mut ch = desk.accept(Duration::from_secs(5)).unwrap();
            ch.recv(Duration::from_secs(5)).unwrap();
            ch.send(br#"{"event":"refused","message":"not an extension Lockdown answers"}"#)
                .unwrap();
        });
        let ch = t.connect(Duration::from_secs(5)).unwrap();
        let input = browser_input(&[r#"{"id":1,"op":"status"}"#]);
        let mut output = Vec::new();
        relay(caller(), &AllowAll, ch, &mut input.as_slice(), &mut output).unwrap();
        desktop.join().unwrap();
        let out = outputs(&output);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["event"], "refused");
    }

    #[test]
    fn oversized_requests_are_answered_by_the_host_and_a_gone_desktop_is_reported() {
        let t = ChannelTransport::new();
        let desk = t.clone();
        let desktop = std::thread::spawn(move || {
            let mut ch = desk.accept(Duration::from_secs(5)).unwrap();
            ch.recv(Duration::from_secs(5)).unwrap();
            ch.send(br#"{"event":"ready"}"#).unwrap();
            // Reads the small request, then goes away without answering.
            ch.recv(Duration::from_secs(5)).unwrap();
        });
        let ch = t.connect(Duration::from_secs(5)).unwrap();
        let big = format!(r#"{{"id":7,"op":"x","pad":"{}"}}"#, "a".repeat(MAX_FRAME));
        let input = browser_input(&[&big, r#"{"id":8,"op":"status"}"#]);
        let mut output = Vec::new();
        assert!(relay(caller(), &AllowAll, ch, &mut input.as_slice(), &mut output).is_err());
        desktop.join().unwrap();
        let out = outputs(&output);
        assert_eq!(out[1]["id"], 7);
        assert_eq!(out[1]["error"]["code"], "too_large");
        assert_eq!(out[2]["id"], 8);
        assert_eq!(out[2]["error"]["code"], "closed");
    }

    #[test]
    fn without_an_endpoint_file_the_host_says_lockdown_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let mut output = Vec::new();
        assert!(run(caller(), dir.path(), &mut [].as_slice(), &mut output).is_err());
        assert_eq!(outputs(&output)[0]["event"], "no_app");
    }
}
