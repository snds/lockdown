//! Every format Lockdown reads, behind one door: [`detect`] guesses the
//! format from the bytes and the file name, [`read`] parses it into items
//! with what couldn't be carried in words. Nothing here touches a vault;
//! the app lands the items after the person has seen the preview.
//!
//! Each reader treats the file as hostile: the whole file and everything
//! it unpacks to is size-capped ([`Limits`]), a key derivation the file
//! asks for is capped before it runs, and a value over the per-field cap
//! is left out and reported. A reader that panics on input nobody foresaw
//! refuses the file instead of taking the app down.

pub mod bitwarden;
pub mod dashlane;
pub mod enpass;
pub mod kdbx;
pub mod keeper;
pub mod lastpass;
pub mod onepux;
pub mod otp;
pub mod proton;
pub mod steam;

use lockdown_vault::FieldValue;

use crate::convert::ImportedItem;
use crate::{CxfError, Limits, Result, archive, csv, model, unzip};

/// The formats Lockdown reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// Lockdown's own backup or export (`.lockdown`, plain or sealed).
    Lockdown,
    /// FIDO Credential Exchange Format, JSON.
    Cxf,
    /// 1Password's unencrypted export.
    OnePux,
    /// Bitwarden JSON, plain or password-protected.
    Bitwarden,
    /// Proton Pass: ZIP, JSON or CSV.
    Proton,
    /// KeePass KDBX 3.1 or 4.
    Kdbx,
    /// Dashlane's CSV export (ZIP or one CSV).
    Dashlane,
    /// Keeper JSON or CSV.
    Keeper,
    /// LastPass CSV.
    LastPass,
    /// Enpass JSON.
    Enpass,
    /// Any CSV with the usual login columns.
    Csv,
    /// Google Authenticator's transfer links (`otpauth-migration://`).
    GoogleAuthenticator,
    /// Aegis's vault export, plain or encrypted.
    Aegis,
    /// A 2FAS backup, plain or encrypted.
    TwoFas,
    /// Ente Auth's export, plain text or encrypted.
    Ente,
    /// Proton Authenticator's export.
    ProtonAuthenticator,
    /// Bitwarden Authenticator's JSON or CSV.
    BitwardenAuthenticator,
    /// Steam Guard: SDA maFiles (one, or the folder as a ZIP), the Steam
    /// app's JSON.
    Steam,
    /// WinAuth's text export.
    WinAuth,
    /// Any file of `otpauth://` links, one per line.
    OtpLinks,
}

impl Format {
    /// Every format, in the order the import wizard lists them.
    pub const ALL: [Format; 20] = [
        Format::OnePux,
        Format::Bitwarden,
        Format::Proton,
        Format::Kdbx,
        Format::Dashlane,
        Format::Keeper,
        Format::LastPass,
        Format::Enpass,
        Format::Cxf,
        Format::Csv,
        Format::Lockdown,
        Format::GoogleAuthenticator,
        Format::Aegis,
        Format::TwoFas,
        Format::Ente,
        Format::ProtonAuthenticator,
        Format::BitwardenAuthenticator,
        Format::Steam,
        Format::WinAuth,
        Format::OtpLinks,
    ];

    /// The name a shell sends.
    pub fn id(self) -> &'static str {
        match self {
            Self::Lockdown => "lockdown",
            Self::Cxf => "cxf",
            Self::OnePux => "1pux",
            Self::Bitwarden => "bitwarden",
            Self::Proton => "proton",
            Self::Kdbx => "kdbx",
            Self::Dashlane => "dashlane",
            Self::Keeper => "keeper",
            Self::LastPass => "lastpass",
            Self::Enpass => "enpass",
            Self::Csv => "csv",
            Self::GoogleAuthenticator => "google-authenticator",
            Self::Aegis => "aegis",
            Self::TwoFas => "2fas",
            Self::Ente => "ente",
            Self::ProtonAuthenticator => "proton-authenticator",
            Self::BitwardenAuthenticator => "bitwarden-authenticator",
            Self::Steam => "steam",
            Self::WinAuth => "winauth",
            Self::OtpLinks => "otpauth",
        }
    }

    /// The format a shell named, or `None`.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    /// Its name, in plain words.
    pub fn name(self) -> &'static str {
        match self {
            Self::Lockdown => "A Lockdown backup",
            Self::Cxf => "A CXF file (credential exchange)",
            Self::OnePux => "1Password (.1pux)",
            Self::Bitwarden => "Bitwarden (.json)",
            Self::Proton => "Proton Pass (.zip, .json or .csv)",
            Self::Kdbx => "KeePass or KeePassXC (.kdbx)",
            Self::Dashlane => "Dashlane (.zip or .csv)",
            Self::Keeper => "Keeper (.json or .csv)",
            Self::LastPass => "LastPass (.csv)",
            Self::Enpass => "Enpass (.json)",
            Self::Csv => "Another app's CSV",
            Self::GoogleAuthenticator => "Google Authenticator (transfer codes)",
            Self::Aegis => "Aegis (.json)",
            Self::TwoFas => "2FAS (.2fas)",
            Self::Ente => "Ente Auth (.txt or encrypted file)",
            Self::ProtonAuthenticator => "Proton Authenticator (.json)",
            Self::BitwardenAuthenticator => "Bitwarden Authenticator (.json or .csv)",
            Self::Steam => "Steam Guard (SDA maFiles or the Steam app's file)",
            Self::WinAuth => "WinAuth (.txt)",
            Self::OtpLinks => "A list of otpauth:// links",
        }
    }

    /// One sentence on how to get the file out of the other app.
    pub fn how(self) -> &'static str {
        match self {
            Self::Lockdown => "A file Lockdown saved: an export or one of your backups.",
            Self::Cxf => {
                "Many apps can save a CXF file; look for \"credential exchange\" in their export menu."
            }
            Self::OnePux => {
                "In 1Password: File, Export, choose your account, and pick the 1PUX format."
            }
            Self::Bitwarden => {
                "In Bitwarden: Tools, Export vault, format .json. A password-protected file works too; you'll type its password here."
            }
            Self::Proton => "In Proton Pass: Settings, Export, without PGP encryption.",
            Self::Kdbx => "Your KeePass database file itself; you'll type its password here.",
            Self::Dashlane => "In Dashlane: Settings, Export data, CSV. Pick the whole .zip.",
            Self::Keeper => "In Keeper: Settings, Export, JSON (preferred) or CSV.",
            Self::LastPass => "In LastPass: Advanced options, Export, and save the CSV.",
            Self::Enpass => "In Enpass: File, Export, .json.",
            Self::Csv => "A CSV with columns like name, url, username, password, notes and totp.",
            Self::GoogleAuthenticator => {
                "In Google Authenticator: Transfer accounts, Export. Scan each QR code with your camera app and paste the otpauth-migration:// links into a text file, one per line."
            }
            Self::Aegis => {
                "In Aegis: Settings, Import & Export, Export, JSON. An encrypted export works too; you'll type its password here."
            }
            Self::TwoFas => {
                "In 2FAS: Settings, 2FAS Backup, Export to file. A password-protected file works too."
            }
            Self::Ente => {
                "In Ente Auth: Settings, Data, Export codes. A plain text file or an encrypted one; you'll type its password here."
            }
            Self::ProtonAuthenticator => {
                "In Proton Authenticator: Settings, Export, without a password."
            }
            Self::BitwardenAuthenticator => {
                "In Bitwarden Authenticator: Settings, Export, as JSON or CSV."
            }
            Self::Steam => {
                "From Steam Desktop Authenticator: ZIP the whole maFiles folder (with manifest.json) or pick one .maFile. An encrypted folder needs its passkey."
            }
            Self::WinAuth => "In WinAuth: right-click, Export, as a text file without a password.",
            Self::OtpLinks => "A text file with one otpauth:// link per line.",
        }
    }

    /// Whether the file may be locked with a password of its own.
    pub fn may_need_password(self) -> bool {
        matches!(
            self,
            Self::Lockdown
                | Self::Bitwarden
                | Self::Kdbx
                | Self::Aegis
                | Self::TwoFas
                | Self::Ente
                | Self::Steam
        )
    }
}

/// What a file held: the items, and what about the file as a whole
/// couldn't be carried (each item's own notes are on the item).
#[derive(Debug, Default)]
pub struct Parsed {
    /// The items, in the file's order.
    pub items: Vec<ImportedItem>,
    /// About the file, in words: items in the source's trash, attachments
    /// a format doesn't carry, sheets of a ZIP this build doesn't know.
    pub problems: Vec<String>,
    /// Only from a Lockdown backup: the folders its items are filed in,
    /// by id, so a restore puts them back where they were.
    pub folders: Vec<KeptFolder>,
}

/// A folder a Lockdown backup carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptFolder {
    /// Its id in the vault it came from.
    pub id: lockdown_vault::Id,
    /// Its name.
    pub name: String,
    /// The folder it sits in.
    pub parent: Option<lockdown_vault::Id>,
}

pub(crate) fn json_err(format: &'static str) -> impl Fn(serde_json::Error) -> CxfError {
    move |e| CxfError::Parse {
        format,
        why: e.to_string(),
    }
}

/// The format a file most likely is, from its first bytes and its name.
pub fn detect(bytes: &[u8], file_name: &str) -> Option<Format> {
    let name = file_name.to_ascii_lowercase();
    if bytes.starts_with(&kdbx::MAGIC) {
        return Some(Format::Kdbx);
    }
    if archive::is_sealed(bytes)
        || bytes.starts_with(b"age-encryption.org/v1\n")
        || bytes.starts_with(b"-----BEGIN AGE ENCRYPTED FILE-----")
    {
        return Some(Format::Lockdown);
    }
    if name.ends_with(".1pux") {
        return Some(Format::OnePux);
    }
    if let Some(f) = detect_authenticator(bytes, &name) {
        return Some(f);
    }
    if unzip::is_zip(bytes) {
        let entries = unzip::read(bytes, &Limits::default()).ok()?;
        let has = |f: &dyn Fn(&str) -> bool| entries.iter().any(|e| f(&e.name));
        if has(&|n| n == "export.data") {
            return Some(Format::OnePux);
        }
        if has(&|n| n.ends_with(".maFile")) {
            return Some(Format::Steam);
        }
        if has(&|n| n.contains("Proton Pass/")) {
            return Some(Format::Proton);
        }
        if has(&|n| n.ends_with("credentials.csv") || n.ends_with("securenotes.csv")) {
            return Some(Format::Dashlane);
        }
        return None;
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') {
        let keys: std::collections::BTreeMap<String, serde::de::IgnoredAny> =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)).ok()?;
        let has = |k: &str| keys.contains_key(k);
        return Some(if has("format") && head.contains(archive::FORMAT_NAME) {
            Format::Lockdown
        } else if has("exporterRpId") {
            Format::Cxf
        } else if has("userId") && has("vaults") {
            Format::Proton
        } else if has("passwordProtected") || has("encrypted") {
            Format::Bitwarden
        } else if has("records") || has("shared_folders") {
            Format::Keeper
        } else if has("items") && has("folders") {
            Format::Enpass
        } else {
            return None;
        });
    }
    if name.ends_with(".csv") || head.contains(',') {
        let table = csv::parse(&bytes[..bytes.len().min(64 * 1024)], &Limits::default()).ok()?;
        return Some(if lastpass::is_lastpass_csv(&table) {
            Format::LastPass
        } else if proton::is_proton_csv(&table) {
            Format::Proton
        } else if dashlane::is_dashlane_csv(&table) {
            Format::Dashlane
        } else {
            Format::Csv
        });
    }
    None
}

/// The authenticator apps' files, by their own marks (docs/plan/23-bridges.md §1).
fn detect_authenticator(bytes: &[u8], name: &str) -> Option<Format> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("otpauth-migration://") {
        return Some(Format::GoogleAuthenticator);
    }
    if trimmed.starts_with("otpauth://") || trimmed.starts_with("steam://") {
        return Some(Format::OtpLinks);
    }
    if name.ends_with(".mafile") {
        return Some(Format::Steam);
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }
    let keys: std::collections::BTreeMap<String, serde::de::IgnoredAny> =
        serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)).ok()?;
    let has = |k: &str| keys.contains_key(k);
    if has("header") && has("db") {
        Some(Format::Aegis)
    } else if has("schemaVersion") && (has("services") || has("servicesEncrypted")) {
        Some(Format::TwoFas)
    } else if has("kdfParams") && has("encryptedData") {
        Some(Format::Ente)
    } else if has("version") && has("entries") && head.contains("\"uri\"") {
        Some(Format::ProtonAuthenticator)
    } else if has("encrypted")
        && has("items")
        && !has("folders")
        && !has("collections")
        && head.contains("\"totp\"")
    {
        Some(Format::BitwardenAuthenticator)
    } else if steam::is_steam_json(&keys) {
        Some(Format::Steam)
    } else {
        None
    }
}

/// Reads `bytes` as `format`. `password` opens a file that has one (a
/// sealed Lockdown backup, a password-protected Bitwarden export, a
/// KeePass database); without it such a file answers
/// [`CxfError::PasswordNeeded`].
pub fn read(
    bytes: &[u8],
    format: Format,
    password: Option<&[u8]>,
    limits: &Limits,
) -> Result<Parsed> {
    let run = || -> Result<Parsed> {
        match format {
            Format::Lockdown => archive::read(bytes, password, limits),
            Format::Cxf => {
                let header = model::parse(bytes, limits)?;
                Ok(Parsed {
                    items: crate::convert::to_items(&header)?,
                    ..Parsed::default()
                })
            }
            Format::OnePux => onepux::read(bytes, limits),
            Format::Bitwarden => bitwarden::read(bytes, password, limits),
            Format::Proton => {
                if unzip::is_zip(bytes) || looks_like_json(bytes) {
                    proton::read(bytes, limits)
                } else {
                    Ok(proton::read_csv(&csv::parse(bytes, limits)?))
                }
            }
            Format::Kdbx => kdbx::read(bytes, password, limits),
            Format::Dashlane => dashlane::read(bytes, limits),
            Format::Keeper => {
                if looks_like_json(bytes) {
                    keeper::read_json(bytes, limits)
                } else {
                    Ok(keeper::read_csv(&csv::rows(bytes, limits)?))
                }
            }
            Format::LastPass => Ok(lastpass::read(&csv::parse(bytes, limits)?)),
            Format::Enpass => enpass::read(bytes, limits),
            Format::Csv => Ok(Parsed {
                items: csv::import_logins(&csv::parse(bytes, limits)?),
                ..Parsed::default()
            }),
            Format::GoogleAuthenticator => otp::read_google(bytes),
            Format::Aegis => otp::read_aegis(bytes, password, limits),
            Format::TwoFas => otp::read_2fas(bytes, password),
            Format::Ente => otp::read_ente(bytes, password),
            Format::ProtonAuthenticator => otp::read_proton_authenticator(bytes),
            Format::BitwardenAuthenticator => otp::read_bitwarden_authenticator(bytes, limits),
            Format::Steam => steam::read(bytes, password, limits),
            Format::WinAuth => otp::read_otpauth_lines(bytes, "WinAuth"),
            Format::OtpLinks => otp::read_otpauth_lines(bytes, "otpauth links"),
        }
    };
    let mut parsed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).map_err(|_| {
            CxfError::Parse {
                format: format.id(),
                why: "the file has a shape Lockdown didn't expect; nothing was read".into(),
            }
        })??;
    if parsed.items.len() > limits.max_items {
        return Err(CxfError::TooMany {
            what: "items",
            found: parsed.items.len(),
            limit: limits.max_items,
        });
    }
    for item in &mut parsed.items {
        cap_fields(item, limits);
    }
    Ok(parsed)
}

pub(crate) fn looks_like_json(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace() && **b != 0xef && **b != 0xbb && **b != 0xbf)
        == Some(&b'{')
}

fn too_big(v: &FieldValue, cap: usize) -> bool {
    match v {
        FieldValue::Text(t) => t.len() > cap,
        FieldValue::Url { value, .. } => value.len() > cap,
        FieldValue::Bytes(b) => b.len() > cap,
        FieldValue::Boolean(_) | FieldValue::Ref(_) => false,
    }
}

/// Leaves out (and reports) any value over the per-field cap, and any
/// file over the per-attachment cap.
fn cap_fields(item: &mut ImportedItem, limits: &Limits) {
    let cap = limits.max_field_bytes;
    let mut dropped = Vec::new();
    item.fields.retain(|f| {
        let keep = f.field_id == "title" || !too_big(&f.value, cap);
        if !keep {
            dropped.push(f.label.clone());
        }
        keep
    });
    for h in &mut item.history {
        h.retain(|f| !too_big(&f.value, cap));
    }
    if item.title.len() > cap {
        item.title = item.title.chars().take(200).collect();
    }
    for label in dropped {
        item.report.push(format!(
            "'{label}' is longer than {} KiB and was left out",
            cap / 1024
        ));
    }
    let max = limits.max_attachment_bytes;
    let mut big = Vec::new();
    item.attachments.retain(|a| {
        let keep = a.bytes.len() <= max;
        if !keep {
            big.push(a.name.clone());
        }
        keep
    });
    for name in big {
        item.report.push(format!(
            "the file '{name}' is over {} MiB and was left out",
            max / (1024 * 1024)
        ));
    }
}
