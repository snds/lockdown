//! The operator's recovery kit (`official-self-host`; 05-self-host §4;
//! docs/plan/25-self-host-2.md §6): one printable page that brings a dead
//! server back, as the person's own kit brings their identity back.
//!
//! It holds the server's **Backup Key** in large grouped type, every
//! character captioned (letter or digit, so nothing is misread off paper),
//! the key once more as plain text to type, where the backups are kept and
//! the commands that restore them. It is written by `lockdown-server backup
//! keygen --kit FILE` (or `backup kit --out FILE` for a key you already
//! have), readable only by its owner, and is meant to be printed and the
//! file deleted. The server never keeps the page or the key on disk.

use crate::backup::BackupKey;
use crate::pages::{STYLE, esc};

/// What the page says besides the key.
#[derive(Clone, Debug, Default)]
pub struct KitFacts {
    /// The server's address, as people reach it.
    pub server: String,
    /// Where the backups are written (a folder, a bucket, a note).
    pub backups: String,
}

const KIT_STYLE: &str = ".key{display:flex;flex-wrap:wrap;gap:.9rem 1.4rem;margin:1rem 0}.group{display:flex;gap:.25rem}\
.ch{display:flex;flex-direction:column;align-items:center;border:1px solid var(--line);border-radius:6px;min-width:2.1rem;padding:.25rem .2rem}\
.ch b{font:600 1.6rem/1.2 ui-monospace,Menlo,monospace}.ch span{font-size:.62rem;color:var(--muted)}\
@media print{body{background:#fff;color:#000}.noprint{display:none}}";

/// The page, as HTML (no script; one style sheet).
pub fn operator_kit_html(key: &BackupKey, facts: &KitFacts, now: i64) -> String {
    let text = key.display();
    let mut groups = String::new();
    for group in text.as_str().split('-').skip(1) {
        groups.push_str("<div class=\"group\">");
        for c in group.chars() {
            let what = if c.is_ascii_digit() {
                "digit"
            } else {
                "letter"
            };
            groups.push_str(&format!(
                "<div class=\"ch\"><b>{}</b><span>{what}</span></div>",
                esc(&c.to_string())
            ));
        }
        groups.push_str("</div>");
    }
    let server = if facts.server.trim().is_empty() {
        "(write the server's address here)".to_string()
    } else {
        facts.server.clone()
    };
    let backups = if facts.backups.trim().is_empty() {
        "(write where the backups are copied here)".to_string()
    } else {
        facts.backups.clone()
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Lockdown server recovery kit</title><style>{STYLE}{KIT_STYLE}</style></head><body><main>\
         <h1>Lockdown server recovery kit</h1>\
         <p>Made {}. Keep this page somewhere safe and away from the server, then delete the file. Anyone with this page and a backup has the whole server, but no one's vault: those stay locked under each person's own password and Secret Key.</p>\
         <div class=\"card\"><p>Server: <b>{}</b></p><p>Backups are kept: <b>{}</b></p></div>\
         <h2>Backup Key</h2><div class=\"key\" aria-label=\"Backup Key, character by character\">{groups}</div>\
         <p>Typed: <code>{}</code> (case, spaces and dashes don't matter).</p>\
         <h2>Bringing the server back</h2><ol>\
         <li>On the new machine, install the same or a newer <code>lockdown-server</code>.</li>\
         <li>Put the newest backup file there, then restore it into an empty data folder:<br><code>LOCKDOWN_BACKUP_KEY='LDBK-…' lockdown-server backup restore --in FILE --data /var/lib/lockdown</code><br>(with <code>--database-url</code> for an empty Postgres database, <code>--blob-dir</code> or the <code>--s3-*</code> settings for attachments).</li>\
         <li>Start the server on that folder. People's apps notice nothing, or say \u{201c}Rollback attempt blocked\u{201d} if the backup is older than what they saw: then sync again from a device that has the newer data.</li>\
         </ol>\
         <p class=\"muted\">Without this key no backup can be opened, by you or by anyone. The operator key of the admin page is not on this page: a new one can be made on the server at any time.</p>\
         <p class=\"noprint muted\">Print this page now (your browser's Print), then delete the file.</p>\
         </main></body></html>",
        crate::mail::when(now),
        esc(&server),
        esc(&backups),
        esc(text.as_str()),
    )
}

/// Writes the page readable only by its owner.
pub fn write(path: &std::path::Path, html: &str) -> std::io::Result<()> {
    crate::keys::write_private(path, html.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kit_carries_the_key_captioned_and_escapes_what_it_was_told() {
        let key = BackupKey::generate().unwrap();
        let html = operator_kit_html(
            &key,
            &KitFacts {
                server: "https://vault.example<script>".into(),
                backups: "/srv/backups".into(),
            },
            0,
        );
        let text = key.display();
        assert!(html.contains(&format!("<code>{}</code>", text.as_str())));
        assert!(html.contains("<span>letter</span>"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script"));
    }
}
