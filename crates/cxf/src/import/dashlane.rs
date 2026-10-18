//! Dashlane's CSV export: a ZIP of `credentials.csv`, `securenotes.csv`,
//! `payments.csv`, `ids.csv` and `personalInfo.csv` (or any one of them on
//! its own). Headers are Dashlane's: `credentials.csv` is `username,
//! username2,username3,title,password,note,url,category,otpSecret` (older
//! files say `otpUrl`).

use lockdown_vault::{FieldKind, ItemKind};

use super::Parsed;
use crate::build::{Builder, month_of};
use crate::csv::{self, Table};
use crate::{CxfError, Limits, Result, unzip};

/// Whether a CSV table is one of Dashlane's, by its headers.
pub fn is_dashlane_csv(t: &Table) -> bool {
    let has = |h: &str| t.headers.iter().any(|x| x == h);
    (has("username2") && has("username3") && has("title"))
        || (has("cc_number") && has("account_holder"))
        || (has("place_of_issue") && has("number"))
        || (has("item_name") && has("first_name") && has("date_of_birth"))
}

/// Reads a Dashlane ZIP or a single Dashlane CSV.
pub fn read(bytes: &[u8], limits: &Limits) -> Result<Parsed> {
    let mut parsed = Parsed::default();
    if unzip::is_zip(bytes) {
        let entries = unzip::read(bytes, limits)?;
        let mut any = false;
        for e in &entries {
            let name = e.name.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
            if !name.ends_with(".csv") {
                continue;
            }
            any = true;
            let table = csv::parse(&e.bytes, limits)?;
            table_into(&table, &name, &mut parsed);
        }
        if !any {
            return Err(CxfError::Parse {
                format: "Dashlane",
                why: "there are no CSV files inside; is this a Dashlane export?".into(),
            });
        }
    } else {
        let table = csv::parse(bytes, limits)?;
        table_into(&table, "", &mut parsed);
    }
    Ok(parsed)
}

fn table_into(t: &Table, name: &str, parsed: &mut Parsed) {
    let has = |h: &str| t.headers.iter().any(|x| x == h);
    let get = |r: &[String], h: &str| {
        t.column(&[h])
            .and_then(|i| r.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let which = if has("username2") || name.starts_with("credentials") {
        "credentials"
    } else if has("cc_number") || name.starts_with("payments") {
        "payments"
    } else if has("place_of_issue") || name.starts_with("ids") {
        "ids"
    } else if has("first_name") || name.starts_with("personalinfo") {
        "personal"
    } else if name.starts_with("securenotes") || (has("title") && has("note")) {
        "notes"
    } else {
        parsed.problems.push(format!(
            "{} wasn't a Dashlane file this build knows and was left out",
            if name.is_empty() { "the file" } else { name }
        ));
        return;
    };
    for (n, r) in t.rows.iter().enumerate() {
        let id = format!("dashlane-{which}-{n}");
        let item = match which {
            "credentials" => {
                let mut b = Builder::new(id, ItemKind::Login, &get(r, "title"));
                b.field("user", &get(r, "username"))
                    .custom("Second username", FieldKind::Text, &get(r, "username2"))
                    .custom("Third username", FieldKind::Text, &get(r, "username3"))
                    .field("pw", &get(r, "password"))
                    .url(&get(r, "url"))
                    .totp(&get(r, "otpsecret"))
                    .totp(&get(r, "otpurl"))
                    .folder(&get(r, "category"))
                    .note(&get(r, "note"));
                b.finish()
            }
            "payments" => {
                let ty = get(r, "type");
                if ty == "bank" || !get(r, "account_number").is_empty() {
                    let mut b = Builder::new(id, ItemKind::BankAccount, &get(r, "account_name"));
                    b.field("bank.name", &get(r, "issuing_bank"))
                        .field("bank.holder", &get(r, "account_holder"))
                        .field("bank.account_number", &get(r, "account_number"))
                        .field("bank.routing", &get(r, "routing_number"))
                        .custom("Country", FieldKind::Text, &get(r, "country"));
                    b.finish()
                } else {
                    let mut b = Builder::new(id, ItemKind::Card, &get(r, "account_name"));
                    b.field("card.holder", &get(r, "account_holder"))
                        .field("card.number", &get(r, "cc_number"))
                        .field("card.cvc", &get(r, "code"))
                        .custom("Issuing bank", FieldKind::Text, &get(r, "issuing_bank"));
                    if let Some(m) =
                        month_of(&get(r, "expiration_month"), &get(r, "expiration_year"))
                    {
                        b.field("card.expiry", &m);
                    }
                    b.finish()
                }
            }
            "ids" => {
                let ty = get(r, "type");
                let title = if get(r, "name").is_empty() {
                    ty.clone()
                } else {
                    format!("{} ({})", get(r, "name"), ty)
                };
                let mut b = Builder::new(id, ItemKind::Document, &title);
                let doc = match ty.as_str() {
                    "passport" => "Passport",
                    "driver_license" => "Driver's licence",
                    "card" | "id_card" => "ID card",
                    _ => "Other",
                };
                b.field("doc.type", doc)
                    .field("doc.number", &get(r, "number"))
                    .field("doc.issued", &get(r, "issue_date"))
                    .field("expires_at", &get(r, "expiration_date"))
                    .field("doc.issuer", &get(r, "place_of_issue"))
                    .custom("Name", FieldKind::Text, &get(r, "name"))
                    .custom("State", FieldKind::Text, &get(r, "state"));
                b.finish()
            }
            "personal" => {
                let title = [get(r, "item_name"), get(r, "first_name"), get(r, "type")]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or_default();
                let mut b = Builder::new(id, ItemKind::Identity, &title);
                b.field("name.given", &get(r, "first_name"))
                    .field("name.middle", &get(r, "middle_name"))
                    .field("name.family", &get(r, "last_name"))
                    .field("identity.email", &get(r, "email"))
                    .field("address.tel", &get(r, "phone_number"))
                    .field("address.street", &get(r, "address"))
                    .field("address.city", &get(r, "city"))
                    .field("address.territory", &get(r, "state"))
                    .field("address.postal_code", &get(r, "zip"))
                    .field("address.country", &get(r, "country"))
                    .custom("Date of birth", FieldKind::Date, &get(r, "date_of_birth"))
                    .custom("Place of birth", FieldKind::Text, &get(r, "place_of_birth"))
                    .custom("Username", FieldKind::Text, &get(r, "login"))
                    .custom("Job title", FieldKind::Text, &get(r, "job_title"))
                    .custom("Website", FieldKind::Url, &get(r, "url"));
                b.finish()
            }
            _ => {
                let mut b = Builder::new(id, ItemKind::Note, &get(r, "title"));
                b.note(&get(r, "note"));
                b.finish()
            }
        };
        parsed.items.push(item);
    }
}
