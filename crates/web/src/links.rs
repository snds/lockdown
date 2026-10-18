//! Share links and request links in a plain browser (docs/plan/18-sharing.md
//! §7, D198), with no account and no sign-in: what the server's `/s/{id}`
//! and `/r/{id}` pages hand to the web vault.
//!
//! The key of a share link is in the URL's fragment, which the browser
//! never sends to a server; the worker reads it from the page and this
//! module opens the link exactly as `lockdown-app` does on a device
//! (`lockdown-vault::send`): the server's metadata, the password's
//! Argon2id at the WASM floor when the link has one, the access proof,
//! one counted view, then the payload opened here. What it holds stays in
//! this module and leaves one value at a time, as bytes, when the person
//! asks. Answering a request is `lockdown-app`'s own sessionless call,
//! which checks the asker's key against the hash the link pins.
//!
//! Everything a link names is hostile until checked: its server must be
//! the page's own (the worker's HTTP refuses any other origin too), and a
//! title or label is only ever text on the page.

use lockdown_app::sharing::{OpenedFile, OpenedValue, OpenedView};
use lockdown_app::{AppError, AppResult};
use lockdown_sync::HttpTransport;
use lockdown_sync::sharing::SendOpen;
use lockdown_vault::Hash32;
use lockdown_vault::send::{
    SendKind, SendPassword, SendPayload, open_send, parse_send_link, send_access,
};
use serde::Serialize;
use serde_bytes::ByteBuf;
use zeroize::Zeroizing;

/// The longest title shown, in characters (the desktop's limit).
const TITLE_CHARS: usize = 120;

fn refused(message: impl Into<String>) -> AppError {
    AppError::refused(message)
}

/// What a share link is before it is opened (opening counts a view).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkMeta {
    /// Whether the sender set a password on it.
    pub password: bool,
    /// When it stops opening (seconds since the Unix epoch).
    pub expires_at: i64,
    /// How many more times it opens; `None` until it expires.
    pub views_left: Option<u32>,
}

/// The server a link names, if it is the page's own.
fn same_server(server: &str, origin: &str) -> AppResult<()> {
    if server.trim_end_matches('/') != origin.trim_end_matches('/') {
        return Err(refused(format!(
            "this link belongs to {server}; open it there, or in the Lockdown app"
        )));
    }
    Ok(())
}

/// Reads a share link's metadata without opening it.
pub fn link_meta(link: &str, origin: &str) -> AppResult<LinkMeta> {
    let (server, send_id, _secret) = parse_send_link(link).map_err(|e| refused(e.to_string()))?;
    same_server(&server, origin)?;
    let transport = HttpTransport::new(&server).map_err(|e| refused(e.to_string()))?;
    let meta = transport
        .send_meta(send_id)
        .map_err(|e| refused(e.to_string()))?;
    Ok(LinkMeta {
        password: meta.password.is_some(),
        expires_at: meta.expires_at,
        views_left: meta.views_left,
    })
}

/// Opens a share link: one view counted, the payload opened here.
pub fn open_link(
    link: &str,
    password: Option<&[u8]>,
    origin: &str,
) -> AppResult<(OpenedView, SendPayload)> {
    let (server, send_id, secret) = parse_send_link(link).map_err(|e| refused(e.to_string()))?;
    same_server(&server, origin)?;
    let transport = HttpTransport::new(&server).map_err(|e| refused(e.to_string()))?;
    let meta = transport
        .send_meta(send_id)
        .map_err(|e| refused(e.to_string()))?;
    let (access, pk) = match (&meta.password, password.filter(|p| !p.is_empty())) {
        (Some(_), None) => {
            return Err(refused("this link needs the password its sender gave you"));
        }
        (Some(p), Some(pw)) => {
            let full = SendPassword {
                salt: p.salt,
                params: p.params,
                verifier: Hash32::ZERO,
            };
            let (proof, pk) =
                send_access(send_id, pw, &full).map_err(|e| refused(e.to_string()))?;
            (Some(ByteBuf::from(proof.to_vec())), Some(pk))
        }
        (None, _) => (None, None),
    };
    let opened = transport
        .open_send(send_id, &SendOpen { access })
        .map_err(|e| refused(e.to_string()))?;
    let payload = open_send(send_id, &secret, pk.as_deref(), &opened.ciphertext)
        .map_err(|e| refused(e.to_string()))?;
    Ok((view_of(&payload), payload))
}

/// The opened payload as the page shows it: labels, never values.
pub fn view_of(payload: &SendPayload) -> OpenedView {
    let mut values = Vec::new();
    if payload.text.is_some() {
        values.push(OpenedValue {
            label: "Text".into(),
            secret: true,
        });
    }
    for f in &payload.fields {
        values.push(OpenedValue {
            label: f.label.clone(),
            secret: f.secret,
        });
    }
    OpenedView {
        kind: match payload.kind {
            SendKind::Text => "text",
            SendKind::File => "file",
            SendKind::Item => "item",
        }
        .into(),
        title: payload.title.chars().take(TITLE_CHARS).collect(),
        values,
        file: payload.file.as_ref().map(|f| OpenedFile {
            name: f.name.clone(),
            mime: f.mime.clone(),
            size: f.bytes.len() as u64,
        }),
    }
}

/// One value of the opened payload, by the index the view gave it.
pub fn value_of(payload: &SendPayload, index: usize) -> AppResult<Zeroizing<Vec<u8>>> {
    let mut all: Vec<&str> = Vec::new();
    if let Some(t) = &payload.text {
        all.push(t.as_str());
    }
    all.extend(payload.fields.iter().map(|f| f.value.as_str()));
    all.get(index)
        .map(|v| Zeroizing::new(v.as_bytes().to_vec()))
        .ok_or_else(|| refused("there's no such value"))
}

/// The opened payload's file.
pub fn file_of(payload: &SendPayload) -> AppResult<Zeroizing<Vec<u8>>> {
    payload
        .file
        .as_ref()
        .map(|f| Zeroizing::new(f.bytes.clone()))
        .ok_or_else(|| refused("this link carries no file"))
}

/// A request link's server, checked to be the page's own.
pub fn check_request_server(link: &str, origin: &str) -> AppResult<()> {
    let parsed =
        lockdown_vault::send::parse_request_link(link).map_err(|e| refused(e.to_string()))?;
    same_server(&parsed.server, origin)
}
