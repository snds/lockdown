//! LastPass's CSV export: `url,username,password,totp,extra,name,grouping,
//! fav`. A secure note has the url `http://sn`; a typed one ("Credit Card",
//! "Bank Account", "Wi-Fi Password", …) carries `NoteType:<type>` as the
//! first line of `extra`, then `Key:Value` lines, with `Notes:` last
//! (and everything after it is the note).

use lockdown_vault::{FieldKind, ItemKind};

use super::Parsed;
use crate::build::{Builder, month_from};
use crate::csv::Table;

/// Whether a CSV's headers are LastPass's.
pub fn is_lastpass_csv(t: &Table) -> bool {
    ["url", "username", "password", "extra", "name", "grouping"]
        .iter()
        .all(|h| t.headers.iter().any(|x| x == h))
}

/// The `Key:Value` lines of a typed secure note, and its free text.
fn note_fields(extra: &str) -> (Option<String>, Vec<(String, String)>, String) {
    let mut kind = None;
    let mut pairs = Vec::new();
    let mut notes = String::new();
    let mut lines = extra.lines();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("Notes:") {
            notes = std::iter::once(rest)
                .chain(lines.by_ref())
                .collect::<Vec<_>>()
                .join("\n");
            break;
        }
        match line.split_once(':') {
            Some(("NoteType", t)) => kind = Some(t.trim().to_string()),
            Some((k, v)) if !k.trim().is_empty() && k.len() < 40 => {
                pairs.push((k.trim().to_string(), v.trim().to_string()))
            }
            _ => {
                if !notes.is_empty() {
                    notes.push('\n');
                }
                notes.push_str(line);
            }
        }
    }
    (kind, pairs, notes)
}

/// Where a typed note's key lands for its kind.
fn field_for(kind: ItemKind, key: &str) -> Option<&'static str> {
    Some(match (kind, key) {
        (ItemKind::Card, "Name on Card") => "card.holder",
        (ItemKind::Card, "Type") => "card.type",
        (ItemKind::Card, "Number") => "card.number",
        (ItemKind::Card, "Security Code") => "card.cvc",
        (ItemKind::Card, "Expiration Date") => "card.expiry",
        (ItemKind::BankAccount, "Bank Name") => "bank.name",
        (ItemKind::BankAccount, "Account Number") => "bank.account_number",
        (ItemKind::BankAccount, "Routing Number") => "bank.routing",
        (ItemKind::BankAccount, "SWIFT Code") => "bank.swift",
        (ItemKind::BankAccount, "IBAN Number") => "bank.iban",
        (ItemKind::BankAccount, "Pin") => "bank.pin",
        (ItemKind::BankAccount, "Branch Address") => "bank.branch",
        (ItemKind::Identity, "First Name") => "name.given",
        (ItemKind::Identity, "Middle Name") => "name.middle",
        (ItemKind::Identity, "Last Name") => "name.family",
        (ItemKind::Identity, "Email Address") => "identity.email",
        (ItemKind::Identity, "Phone" | "Mobile Phone") => "address.tel",
        (ItemKind::Identity, "Address 1") => "address.street",
        (ItemKind::Identity, "City / Town") => "address.city",
        (ItemKind::Identity, "State") => "address.territory",
        (ItemKind::Identity, "Zip / Postal Code") => "address.postal_code",
        (ItemKind::Identity, "Country") => "address.country",
        (ItemKind::Server, "Hostname") => "server.host",
        (ItemKind::Server | ItemKind::Database, "Username") => "user",
        (ItemKind::Server | ItemKind::Database, "Password") => "pw",
        (ItemKind::Database, "Type") => "db.type",
        (ItemKind::Database, "Hostname") => "db.host",
        (ItemKind::Database, "Port") => "db.port",
        (ItemKind::Database, "Database") => "db.name",
        (ItemKind::WiFi, "SSID") => "wifi.ssid",
        (ItemKind::WiFi, "Password") => "wifi.passphrase",
        (ItemKind::WiFi, "Authentication") => "wifi.security",
        (ItemKind::SshKey, "Private Key") => "ssh.private_key",
        (ItemKind::SshKey, "Passphrase") => "ssh.passphrase",
        (ItemKind::SshKey, "Public Key") => "ssh.public_key",
        (ItemKind::SshKey, "Hostname") => "ssh.hosts",
        (ItemKind::SoftwareLicence, "License Key") => "licence.key",
        (ItemKind::SoftwareLicence, "Licensee") => "licence.licensed_to",
        (ItemKind::SoftwareLicence, "Version") => "licence.version",
        (ItemKind::SoftwareLicence, "Publisher") => "licence.publisher",
        (ItemKind::SoftwareLicence, "Website") => "licence.url",
        (ItemKind::SoftwareLicence, "Purchase Date") => "licence.purchased",
        (ItemKind::Membership, "Organization") => "member.organization",
        (ItemKind::Membership, "Membership Number") => "member.number",
        (ItemKind::Membership, "Member Name") => "member.name",
        (ItemKind::Membership, "Start Date") => "member.since",
        (ItemKind::Membership, "Expiration Date") => "expires_at",
        (ItemKind::Membership, "Website") => "member.url",
        (ItemKind::Membership, "Password") => "member.pin",
        (ItemKind::Document, "Number") => "doc.number",
        (ItemKind::Document, "Issuing Authority" | "State") => "doc.issuer",
        (ItemKind::Document, "Issued Date") => "doc.issued",
        (ItemKind::Document, "Expiration Date") => "expires_at",
        _ => return None,
    })
}

fn kind_of(note_type: &str) -> (ItemKind, Option<&'static str>) {
    match note_type {
        "Credit Card" => (ItemKind::Card, None),
        "Bank Account" => (ItemKind::BankAccount, None),
        "Address" => (ItemKind::Identity, None),
        "Server" => (ItemKind::Server, None),
        "Database" => (ItemKind::Database, None),
        "Wi-Fi Password" => (ItemKind::WiFi, None),
        "SSH Key" => (ItemKind::SshKey, None),
        "Software License" => (ItemKind::SoftwareLicence, None),
        "Membership" => (ItemKind::Membership, None),
        "Passport" => (ItemKind::Document, Some("Passport")),
        "Driver's License" => (ItemKind::Document, Some("Driver's licence")),
        "Social Security" | "Insurance" | "Health Insurance" => (ItemKind::Document, Some("Other")),
        _ => (ItemKind::Note, None),
    }
}

/// Reads a LastPass CSV.
pub fn read(t: &Table) -> Parsed {
    let get = |r: &[String], h: &str| {
        t.column(&[h])
            .and_then(|i| r.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let mut parsed = Parsed::default();
    for (n, r) in t.rows.iter().enumerate() {
        let id = format!("lastpass-{n}");
        let url = get(r, "url");
        let extra = get(r, "extra");
        let title = get(r, "name");
        let mut b;
        if url == "http://sn" {
            let (note_type, pairs, notes) = note_fields(&extra);
            let (kind, doc) = match &note_type {
                Some(t) => kind_of(t),
                None => (ItemKind::Note, None),
            };
            b = Builder::new(id, kind, &title);
            if let Some(d) = doc {
                b.field("doc.type", d);
            }
            if kind == ItemKind::Note && note_type.is_none() {
                b.note(&extra);
            } else {
                if kind == ItemKind::Note
                    && let Some(t) = &note_type
                {
                    b.report(format!(
                        "LastPass's '{t}' note type has no Lockdown kind; its fields came in on a note"
                    ));
                }
                for (k, v) in &pairs {
                    let hidden = [
                        "Password",
                        "Pin",
                        "PIN",
                        "Security Code",
                        "Number",
                        "Passphrase",
                    ]
                    .iter()
                    .any(|h| k.contains(h));
                    match field_for(kind, k) {
                        Some("card.expiry") => {
                            let m = month_from(v).unwrap_or_else(|| v.clone());
                            b.field_labelled("card.expiry", &m, Some(k));
                        }
                        Some(id) => {
                            b.field_labelled(id, v, Some(k));
                        }
                        None => {
                            let kind = if hidden {
                                FieldKind::Hidden
                            } else {
                                FieldKind::Text
                            };
                            b.custom(k, kind, v);
                        }
                    }
                }
                b.note(&notes);
            }
        } else {
            b = Builder::new(id, ItemKind::Login, &title);
            b.field("user", &get(r, "username"))
                .field("pw", &get(r, "password"))
                .totp(&get(r, "totp"));
            if url != "http://" {
                b.url(&url);
            }
            b.note(&extra);
        }
        b.folder(&get(r, "grouping").replace('\\', "/"))
            .favourite(get(r, "fav") == "1");
        parsed.items.push(b.finish());
    }
    parsed
}
