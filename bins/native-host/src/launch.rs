//! Opening the desktop app from the browser (docs/plan/37-cli-popup.md §2,
//! design review R16): the popup's **Open Lockdown** when no app answers.
//!
//! With nobody on the socket, the host has said `no_app`; it then reads
//! one request from the extension. Only `{"id", "op": "launch"}` is
//! answered, and only for an extension Lockdown answers. The app opened is
//! never one the request names:
//!
//! 1. the executable the endpoint file names, when its SHA-256 still
//!    matches the hash the app wrote there (a run that ended without
//!    removing the file); else
//! 2. the app installed beside this host (`lockdown-desktop`, the install
//!    layout every package uses and `EndpointConfig::for_app` assumes).
//!
//! The app starts detached, in a process group of its own, with nothing of
//! the browser's pipes, so it outlives the host. Nothing secret is read or
//! sent; the app asks for its own unlock as always.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ecosystem_identity::handshake::ProcessIdentityVerifier;
use lockdown_native_messaging::caller::Caller;
use lockdown_native_messaging::endpoint::EndpointFile;
use lockdown_native_messaging::framing::{read_message, write_message};
use serde_json::{Value, json};

/// The desktop app's file name beside the host.
pub const APP_BINARY: &str = if cfg!(windows) {
    "lockdown-desktop.exe"
} else {
    "lockdown-desktop"
};

/// What the extension shows when the app can't be opened from here.
pub const HOW: &str = "Lockdown couldn't open the app from the browser. Open Lockdown from your applications, then unlock it.";

/// The app to open: the endpoint file's, when its hash still matches, else
/// the one installed beside `host_exe`.
pub fn target(data: &Path, host_exe: Option<&Path>) -> Result<PathBuf, String> {
    if let Ok(file) = EndpointFile::read(data)
        && let Ok(want) = file.app_hash()
        && ProcessIdentityVerifier::hash_of(&file.app_exe).ok() == Some(want)
    {
        return Ok(file.app_exe);
    }
    if let Some(beside) = host_exe
        .and_then(Path::parent)
        .map(|dir| dir.join(APP_BINARY))
        .filter(|p| p.is_file())
    {
        return Ok(beside);
    }
    Err(HOW.to_string())
}

/// Starts `app` detached from this host and from the browser's pipes.
pub fn start(app: &Path) -> Result<(), String> {
    let mut cmd = Command::new(app);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: a browser ending the host's group on
        // disconnect leaves the app running.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map(|_| ()).map_err(|_| HOW.to_string())
}

/// Reads one request after `no_app` and answers it: `launch` opens the app
/// ([`target`], [`start`]); anything else is told there is no app. Ends
/// quietly when the extension closes the port instead.
pub fn answer<R: Read, W: Write>(caller: &Caller, data: &Path, input: &mut R, output: &mut W) {
    let own = std::env::current_exe().ok();
    answer_with(caller, data, own.as_deref(), input, output, &start);
}

/// [`answer`] with the host's own path and the starter given, for tests.
pub fn answer_with<R: Read, W: Write>(
    caller: &Caller,
    data: &Path,
    host_exe: Option<&Path>,
    input: &mut R,
    output: &mut W,
    starter: &dyn Fn(&Path) -> Result<(), String>,
) {
    let Ok(Some(message)) = read_message(input) else {
        return;
    };
    if message.len() > crate::MAX_FRAME {
        return;
    }
    let request: Value = serde_json::from_slice(&message).unwrap_or(Value::Null);
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let refuse = |output: &mut W, code: &str, words: &str| {
        let body = json!({ "id": id, "ok": false, "error": { "code": code, "message": words } });
        let _ = write_message(output, body.to_string().as_bytes());
    };
    if request.get("op").and_then(Value::as_str) != Some("launch") {
        refuse(
            output,
            "no_app",
            "Lockdown isn't running on this computer; open it and unlock",
        );
        return;
    }
    if !caller.is_allowed() {
        refuse(
            output,
            "refused",
            "this extension isn't one Lockdown answers",
        );
        return;
    }
    match target(data, host_exe).and_then(|app| starter(&app)) {
        Ok(()) => {
            let body = json!({ "id": id, "ok": true, "result": { "launched": true } });
            let _ = write_message(output, body.to_string().as_bytes());
        }
        Err(words) => refuse(output, "not_launched", &words),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lockdown_native_messaging::caller::Family;
    use lockdown_native_messaging::endpoint::ENDPOINT_V;
    use lockdown_native_messaging::{FIREFOX_ID, framing};

    use super::*;

    fn caller(id: &str) -> Caller {
        Caller {
            family: Family::Firefox,
            extension_id: id.into(),
        }
    }

    fn input(message: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        framing::write_message(&mut buf, message.as_bytes()).unwrap();
        buf
    }

    fn answer_of(out: &[u8]) -> Value {
        let mut buf = out;
        serde_json::from_slice(&framing::read_message(&mut buf).unwrap().unwrap()).unwrap()
    }

    /// Runs one request against `data` with the host at `host_exe`,
    /// recording what would have been started.
    fn ask(who: &str, data: &Path, host_exe: Option<&Path>, msg: &str) -> (Value, Vec<PathBuf>) {
        let started = RefCell::new(Vec::new());
        let starter = |p: &Path| {
            started.borrow_mut().push(p.to_path_buf());
            Ok(())
        };
        let mut out = Vec::new();
        answer_with(
            &caller(who),
            data,
            host_exe,
            &mut input(msg).as_slice(),
            &mut out,
            &starter,
        );
        (answer_of(&out), started.into_inner())
    }

    #[test]
    fn launch_opens_the_app_beside_the_host_and_nothing_a_request_names() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let host = bin.join("lockdown-native-host");
        std::fs::write(bin.join(APP_BINARY), b"the app").unwrap();
        let data = dir.path().join("data");
        let (answer, started) = ask(
            FIREFOX_ID,
            &data,
            Some(&host),
            r#"{"id":4,"op":"launch","path":"/bin/sh"}"#,
        );
        assert_eq!(
            answer,
            json!({"id": 4, "ok": true, "result": {"launched": true}})
        );
        assert_eq!(started, vec![bin.join(APP_BINARY)]);
    }

    #[test]
    fn launch_prefers_the_endpoint_files_app_only_while_its_hash_matches() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let app = dir.path().join("Lockdown");
        std::fs::write(&app, b"the app, version 1").unwrap();
        let hash = ProcessIdentityVerifier::hash_of(&app).unwrap();
        EndpointFile {
            v: ENDPOINT_V,
            socket: data.join("browser.sock"),
            app_exe: app.clone(),
            app_sha256: hash.iter().map(|b| format!("{b:02x}")).collect(),
        }
        .write(&data)
        .unwrap();
        assert_eq!(target(&data, None).unwrap(), app);
        // The file changed since the app wrote its hash: not opened.
        std::fs::write(&app, b"something else").unwrap();
        assert_eq!(target(&data, None).unwrap_err(), HOW);
    }

    #[test]
    fn no_app_to_find_says_how_and_other_requests_or_callers_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path().join("lockdown-native-host");
        let (answer, started) = ask(
            FIREFOX_ID,
            dir.path(),
            Some(&host),
            r#"{"id":1,"op":"launch"}"#,
        );
        assert_eq!(answer["error"]["code"], "not_launched");
        assert_eq!(answer["error"]["message"], HOW);
        assert!(started.is_empty());

        std::fs::write(dir.path().join(APP_BINARY), b"the app").unwrap();
        let (answer, started) = ask(
            FIREFOX_ID,
            dir.path(),
            Some(&host),
            r#"{"id":2,"op":"search"}"#,
        );
        assert_eq!(answer["id"], 2);
        assert_eq!(answer["error"]["code"], "no_app");
        assert!(started.is_empty());
        let (answer, started) = ask(
            "someone@else",
            dir.path(),
            Some(&host),
            r#"{"id":3,"op":"launch"}"#,
        );
        assert_eq!(answer["error"]["code"], "refused");
        assert!(started.is_empty());
    }
}
