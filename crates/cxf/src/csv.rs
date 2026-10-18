//! CSV: a small RFC 4180 reader that never panics, an injection-safe
//! writer, and a login importer over the header names the common managers
//! and browsers use (02-model §11).
//!
//! [`parse`] is the fuzz boundary: one pure function from bytes to a
//! [`Table`], with every size capped by [`Limits`]. Nothing in it indexes
//! without a check, and every error is a value.

use lockdown_vault::{FieldKind, FieldValue, ItemKind, PlainField};

use crate::convert::ImportedItem;
use crate::model::b64url;
use crate::{CxfError, Limits, Result};

/// A parsed CSV file.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Table {
    /// The first row, lowercased and trimmed.
    pub headers: Vec<String>,
    /// Every other row, each padded or truncated to the header count.
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// The column index of the first header in `names`.
    pub fn column(&self, names: &[&str]) -> Option<usize> {
        names
            .iter()
            .find_map(|n| self.headers.iter().position(|h| h == n))
    }
}

/// Parses CSV bytes (UTF-8, lossy; `\r\n` or `\n`; quoted fields with `""`
/// escapes) within `limits`. A trailing newline is fine; a lone `\r` is a
/// newline too.
pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Table> {
    let mut iter = rows(bytes, limits)?.into_iter();
    let headers: Vec<String> = iter
        .next()
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let width = headers.len();
    let rows = iter
        .map(|mut r| {
            r.resize(width, String::new());
            r
        })
        .collect();
    Ok(Table { headers, rows })
}

/// Every row as read, the first one included and nothing padded: for
/// files with no header row (Keeper's CSV).
pub fn rows(bytes: &[u8], limits: &Limits) -> Result<Vec<Vec<String>>> {
    limits.check_size(bytes)?;
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let too_long = |cell: &String| -> Result<()> {
        if cell.len() > limits.max_field_bytes {
            return Err(CxfError::TooLarge {
                found: cell.len(),
                limit: limits.max_field_bytes,
            });
        }
        Ok(())
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        cell.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                _ => cell.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    too_long(&cell)?;
                    row.push(std::mem::take(&mut cell));
                }
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    too_long(&cell)?;
                    row.push(std::mem::take(&mut cell));
                    if !(row.len() == 1 && row[0].is_empty()) {
                        rows.push(std::mem::take(&mut row));
                        if rows.len() > limits.max_items + 1 {
                            return Err(CxfError::TooMany {
                                what: "rows",
                                found: rows.len(),
                                limit: limits.max_items,
                            });
                        }
                    } else {
                        row.clear();
                    }
                }
                _ => cell.push(c),
            }
        }
        if cell.len() > limits.max_field_bytes {
            return Err(CxfError::TooLarge {
                found: cell.len(),
                limit: limits.max_field_bytes,
            });
        }
    }
    if in_quotes {
        return Err(CxfError::Parse {
            format: "CSV",
            why: "a quoted field never closed".into(),
        });
    }
    if !cell.is_empty() || !row.is_empty() {
        too_long(&cell)?;
        row.push(cell);
        rows.push(row);
    }
    if rows.len() > limits.max_items + 1 {
        return Err(CxfError::TooMany {
            what: "rows",
            found: rows.len(),
            limit: limits.max_items,
        });
    }
    Ok(rows)
}

/// Quotes a cell for output and defuses spreadsheet formula injection: a
/// cell starting with `=`, `+`, `-`, `@`, tab or carriage return gets a
/// leading apostrophe, as OWASP recommends.
pub fn escape_cell(value: &str) -> String {
    let defused = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if defused.contains([',', '"', '\n', '\r']) || defused != value {
        format!("\"{}\"", defused.replace('"', "\"\""))
    } else {
        defused
    }
}

/// Writes rows as CSV with `\r\n` line ends.
pub fn write(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(
        &headers
            .iter()
            .map(|h| escape_cell(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("\r\n");
    for r in rows {
        out.push_str(
            &r.iter()
                .map(|c| escape_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    out
}

/// Imports logins from a table with the usual headers (Bitwarden,
/// 1Password, Chrome, Firefox, KeePassXC CSV exports all fit).
pub fn import_logins(table: &Table) -> Vec<ImportedItem> {
    let title = table.column(&["name", "title", "account"]);
    let url = table.column(&["url", "login_uri", "website", "urls"]);
    let user = table.column(&["username", "login_username", "user name", "login"]);
    let pw = table.column(&["password", "login_password"]);
    let notes = table.column(&["notes", "note", "comments"]);
    let totp = table.column(&["totp", "login_totp", "otpauth", "one-time password"]);
    let get = |r: &[String], c: Option<usize>| {
        c.and_then(|i| r.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    table
        .rows
        .iter()
        .enumerate()
        .map(|(n, r)| {
            let mut fields = Vec::new();
            let mut report = Vec::new();
            let name = get(r, title)
                .or_else(|| get(r, url))
                .unwrap_or_else(|| format!("Imported login {}", n + 1));
            fields.push(PlainField::text("title", "title", &name));
            if let Some(u) = get(r, url) {
                for (i, one) in u.split([',', ' ']).filter(|s| !s.is_empty()).enumerate() {
                    fields.push(PlainField::url(&format!("url.{i}"), one, "exact_host"));
                }
            }
            if let Some(u) = get(r, user) {
                fields.push(PlainField::text("user", "username", &u));
            }
            if let Some(p) = get(r, pw) {
                fields.push(PlainField::hidden("pw", "password", &p));
            }
            if let Some(t) = get(r, totp) {
                let uri = if t.starts_with("otpauth://") {
                    t
                } else {
                    format!("otpauth://totp/{name}?secret={t}")
                };
                match lockdown_codes::parse_uri(&uri) {
                    Ok(_) => fields.push(PlainField::new(
                        "code.0",
                        FieldKind::Totp,
                        "one-time code",
                        FieldValue::Text(uri),
                    )),
                    Err(e) => report.push(format!("one-time code not imported: {e}")),
                }
            }
            if let Some(nt) = get(r, notes) {
                fields.push(PlainField::text("notes", "notes", &nt));
            }
            ImportedItem {
                fields,
                report,
                ..ImportedItem::new(
                    b64url(format!("csv:{}", n + 1).as_bytes()),
                    ItemKind::Login,
                    name,
                )
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_newlines_and_bom() {
        let csv = "\u{feff}name,url,username,password,notes\r\nExample,https://example.com,me,\"p,w\"\"x\",\"two\nlines\"\nBare,,,,\n";
        let t = parse(csv.as_bytes(), &Limits::default()).unwrap();
        assert_eq!(t.headers, ["name", "url", "username", "password", "notes"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][3], "p,w\"x");
        assert_eq!(t.rows[0][4], "two\nlines");
        assert_eq!(t.rows[1], ["Bare", "", "", "", ""]);
        let items = import_logins(&t);
        assert_eq!(items[0].title, "Example");
        assert_eq!(items[0].fields.len(), 5);
        assert_eq!(items[1].title, "Bare");
    }

    #[test]
    fn hostile_inputs_are_errors_not_panics() {
        let l = Limits {
            max_bytes: 1024,
            max_items: 2,
            max_field_bytes: 8,
            ..Limits::default()
        };
        assert!(matches!(
            parse(b"a,b\n\"nc", &l),
            Err(CxfError::Parse { .. })
        ));
        assert!(matches!(
            parse(b"a\n123456789", &l),
            Err(CxfError::TooLarge { .. })
        ));
        assert!(matches!(
            parse(b"a\n1\n2\n3\n", &l),
            Err(CxfError::TooMany { what: "rows", .. })
        ));
        assert!(matches!(
            parse(&[b'x'; 2000], &l),
            Err(CxfError::TooLarge { .. })
        ));
        // Garbage bytes, empty input, only newlines, a lone quote: all values.
        for input in [&b"\xff\xfe\x00"[..], b"", b"\n\n\r\n", b"\"\"", b",,,\n,,,"] {
            let _ = parse(input, &l);
        }
        assert_eq!(parse(b"", &l).unwrap(), Table::default());
    }

    #[test]
    fn the_writer_defuses_formulas_and_round_trips() {
        assert_eq!(escape_cell("=1+1"), "\"'=1+1\"");
        assert_eq!(escape_cell("@cmd"), "\"'@cmd\"");
        assert_eq!(escape_cell("plain"), "plain");
        assert_eq!(escape_cell("a,b"), "\"a,b\"");
        let out = write(&["name", "password"], &[vec!["x".into(), "p\"q".into()]]);
        let back = parse(out.as_bytes(), &Limits::default()).unwrap();
        assert_eq!(back.rows[0][1], "p\"q");
    }
}
