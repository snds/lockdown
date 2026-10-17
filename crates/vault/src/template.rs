//! The per-kind templates (02-model §4, §5): for every [`ItemKind`], the
//! ordered fields a person fills in, as data. A template says, per field,
//! which control a shell draws, whether it is required, whether it makes
//! the item a crown jewel (02-model §3), whether it is ever filled into a
//! page, and whether the password generator applies. Every shell draws its
//! editor and its detail from this table; [`crate::compose`] checks what a
//! shell sends against it.
//!
//! Field ids are the ones the importers already write (`user`, `pw`,
//! `url.0`, `code.0`, `body`, `card.*`, `name.*`, `address.*`; see
//! `lockdown-cxf`), so an imported item and a hand-made one are the same
//! shape, and the CXF mapping covers what it covered before.
//!
//! The common `title` is not in these lists: it is every item's first
//! field and is passed on its own. Fields a person adds by hand
//! ("Add field", 02-model §5 custom fields) are `custom.<n>` and take one
//! of [`CUSTOM_KINDS`].

use lockdown_crypto::generator::PasswordRule;

use crate::ItemKind;
use crate::crown_jewel::{Classify, CrownJewelDefault, LoginRole, crown_jewel_default};
use crate::field::FieldKind;

/// The control a shell draws for a field, and so how its text is checked.
/// Several controls store as the same [`FieldKind`] (a number, a choice and
/// a paragraph are all `text`); the template, keyed by field id, is what
/// tells them apart, so no schema change was needed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    /// One line of text.
    Text,
    /// Several lines (Markdown where the field is notes or a note's body).
    Multiline,
    /// A masked secret on one line.
    Secret,
    /// A masked secret over several lines (a private key, a recovery phrase).
    SecretMultiline,
    /// A web address.
    Url,
    /// An email address.
    Email,
    /// A phone number.
    Phone,
    /// Digits only (a port).
    Number,
    /// A calendar date, `YYYY-MM-DD`.
    Date,
    /// A month, `YYYY-MM` (a card's expiry).
    Month,
    /// One of the template's options.
    Select,
    /// Yes or no.
    Boolean,
    /// A one-time code set-up: an `otpauth://` link or a bare secret.
    Totp,
    /// Passkey material: made by a provider, shown never, set never.
    Passkey,
}

impl Control {
    /// How a value of this control is stored.
    pub fn storage(self) -> FieldKind {
        match self {
            Self::Text | Self::Multiline | Self::Number | Self::Select => FieldKind::Text,
            Self::Secret | Self::SecretMultiline => FieldKind::Hidden,
            Self::Url => FieldKind::Url,
            Self::Email => FieldKind::Email,
            Self::Phone => FieldKind::Phone,
            Self::Date | Self::Month => FieldKind::Date,
            Self::Boolean => FieldKind::Boolean,
            Self::Totp => FieldKind::Totp,
            Self::Passkey => FieldKind::Passkey,
        }
    }

    /// Whether a shell gets the value only through `reveal`.
    pub fn is_secret(self) -> bool {
        matches!(
            self.storage(),
            FieldKind::Hidden | FieldKind::Totp | FieldKind::Passkey
        )
    }

    /// The name a shell keys on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Multiline => "multiline",
            Self::Secret => "secret",
            Self::SecretMultiline => "secret-multiline",
            Self::Url => "url",
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Number => "number",
            Self::Date => "date",
            Self::Month => "month",
            Self::Select => "select",
            Self::Boolean => "boolean",
            Self::Totp => "totp",
            Self::Passkey => "passkey",
        }
    }
}

/// Whether the extension or an OS provider may put a field into a page
/// (02-model §3, §4; brief §6 rule 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fill {
    /// Filled on the person's pick.
    Fill,
    /// Only after a second deliberate action (codes, cards, passkeys).
    Confirm,
    /// Never put into a page.
    Never,
}

impl Fill {
    /// The name a shell keys on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Confirm => "confirm",
            Self::Never => "never",
        }
    }
}

/// One field of a template.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FieldTemplate {
    /// The stable id (`pw`, `card.number`, `wifi.ssid`).
    pub field_id: &'static str,
    /// The label, in plain words.
    pub label: &'static str,
    /// What a shell draws.
    pub control: Control,
    /// An item of this kind is refused without it.
    pub required: bool,
    /// Holding this field makes the item a crown jewel by default (02-model §3).
    pub crown_jewel: bool,
    /// Whether it is ever filled into a page.
    pub fill: Fill,
    /// The generator's rule, where a generated value makes sense.
    pub generator: Option<PasswordRule>,
    /// The choices of a [`Control::Select`].
    pub options: &'static [&'static str],
    /// The heading the detail groups it under.
    pub section: &'static str,
    /// Drawn in groups of four (card and account numbers).
    pub grouped: bool,
    /// One line of help under the control, or empty.
    pub hint: &'static str,
}

const fn field(field_id: &'static str, label: &'static str, control: Control) -> FieldTemplate {
    FieldTemplate {
        field_id,
        label,
        control,
        required: false,
        crown_jewel: false,
        fill: Fill::Never,
        generator: None,
        options: &[],
        section: "Details",
        grouped: false,
        hint: "",
    }
}

impl FieldTemplate {
    const fn required(mut self) -> Self {
        self.required = true;
        self
    }
    const fn crown(mut self) -> Self {
        self.crown_jewel = true;
        self
    }
    const fn fill(mut self, fill: Fill) -> Self {
        self.fill = fill;
        self
    }
    const fn generate(mut self, rule: PasswordRule) -> Self {
        self.generator = Some(rule);
        self
    }
    const fn options(mut self, options: &'static [&'static str]) -> Self {
        self.options = options;
        self
    }
    const fn section(mut self, section: &'static str) -> Self {
        self.section = section;
        self
    }
    const fn grouped(mut self) -> Self {
        self.grouped = true;
        self
    }
    const fn hint(mut self, hint: &'static str) -> Self {
        self.hint = hint;
        self
    }
}

/// Markdown notes, last on every template but a Note's (02-model §4 "common to every type").
const NOTES: FieldTemplate = field("notes", "Notes", Control::Multiline).section("Notes");

/// One kind's template.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Template {
    /// The kind.
    pub kind: ItemKind,
    /// Its name, singular, in plain words ("Wi-Fi network").
    pub name: &'static str,
    /// One line on what it is for, for the "New item" list.
    pub summary: &'static str,
    /// Whether a person makes one by hand. A passkey comes from a site
    /// through a provider; a consent record is the app's own.
    pub creatable: bool,
    /// The fields, in the order an editor and a detail show them.
    pub fields: &'static [FieldTemplate],
    /// The field the quick copy takes (the palette and a row's copy
    /// button): always a secret or a code, so it goes through the core.
    pub primary: Option<&'static str>,
    /// A plain field shown under the title in lists.
    pub subtitle: Option<&'static str>,
}

impl Template {
    /// One field by id.
    pub fn field(&self, field_id: &str) -> Option<&'static FieldTemplate> {
        self.fields.iter().find(|f| f.field_id == field_id)
    }

    /// Whether an item of this kind is a crown jewel by default (02-model
    /// §3), from the same classifier the rest of the crate uses.
    pub fn crown_jewel_default(&self) -> CrownJewelDefault {
        crown_jewel_default(Classify {
            kind: self.kind,
            field_id: None,
            login_role: LoginRole::Everyday,
        })
    }
}

/// The kinds a person may give a field they add by hand, with the word
/// for each (02-model §5).
pub const CUSTOM_KINDS: &[(FieldKind, &str)] = &[
    (FieldKind::Text, "Text"),
    (FieldKind::Hidden, "Secret"),
    (FieldKind::Url, "Web address"),
    (FieldKind::Email, "Email"),
    (FieldKind::Phone, "Phone"),
    (FieldKind::Date, "Date"),
    (FieldKind::Boolean, "Yes or no"),
];

const STRONG: PasswordRule = PasswordRule::STRONG;
const READABLE: PasswordRule = PasswordRule::READABLE;

const LOGIN: &[FieldTemplate] = &[
    field("user", "Username", Control::Text)
        .fill(Fill::Fill)
        .section("Sign-in"),
    field("pw", "Password", Control::Secret)
        .fill(Fill::Fill)
        .generate(STRONG)
        .section("Sign-in"),
    field("code.0", "One-time code", Control::Totp)
        .fill(Fill::Confirm)
        .section("Sign-in")
        .hint("Paste the otpauth:// link or the set-up key the site showed."),
    field("url.0", "Website", Control::Url).section("Websites"),
    NOTES,
];

const PASSKEY: &[FieldTemplate] = &[
    field("passkey.0.rp_id", "Website", Control::Text).section("Passkey"),
    field("passkey.0.user_name", "User name", Control::Text).section("Passkey"),
    field("passkey.0.user_display_name", "Display name", Control::Text).section("Passkey"),
    field("passkey.0.credential_id", "Credential", Control::Passkey).section("Passkey"),
    field("passkey.0.user_handle", "User handle", Control::Passkey).section("Passkey"),
    field("passkey.0.key", "Private key", Control::Passkey)
        .fill(Fill::Confirm)
        .section("Passkey"),
    NOTES,
];

const CODE: &[FieldTemplate] = &[
    field("code.0", "Set-up key", Control::Totp)
        .required()
        .fill(Fill::Confirm)
        .section("One-time code")
        .hint("Paste the otpauth:// link, or the set-up key on its own."),
    NOTES,
];

const WIFI: &[FieldTemplate] = &[
    field("wifi.ssid", "Network name", Control::Text)
        .required()
        .section("Network"),
    field("wifi.passphrase", "Password", Control::Secret)
        .generate(READABLE)
        .section("Network"),
    field("wifi.security", "Security", Control::Select)
        .options(&[
            "WPA3",
            "WPA2/WPA3",
            "WPA2",
            "WPA",
            "WEP",
            "Open",
            "Enterprise",
        ])
        .section("Network"),
    field("wifi.hidden", "Hidden network", Control::Boolean)
        .section("Network")
        .hint("The network doesn't announce its name."),
    field("wifi.eap_identity", "Enterprise identity", Control::Text)
        .section("Network")
        .hint("Only for Enterprise networks."),
    NOTES,
];

const SSH_KEY: &[FieldTemplate] = &[
    field("ssh.private_key", "Private key", Control::SecretMultiline)
        .required()
        .section("Private"),
    field("ssh.passphrase", "Passphrase", Control::Secret)
        .generate(STRONG)
        .section("Private"),
    field("ssh.public_key", "Public key", Control::Multiline).section("Public"),
    field("ssh.fingerprint", "Fingerprint", Control::Text).section("Public"),
    field("ssh.key_type", "Key type", Control::Select)
        .options(&["Ed25519", "ECDSA", "RSA", "Other"])
        .section("Public"),
    field("ssh.hosts", "Allowed hosts", Control::Multiline).section("Use"),
    NOTES,
];

const API_CREDENTIAL: &[FieldTemplate] = &[
    field("api.key", "Key or token", Control::Secret)
        .required()
        .section("Credential"),
    field("api.secret", "Secret", Control::Secret).section("Credential"),
    field("api.type", "Type", Control::Select)
        .options(&["Bearer token", "Basic", "HMAC", "Other"])
        .section("Credential"),
    field("api.host", "Host name", Control::Text),
    field("api.scopes", "Scopes", Control::Text),
    field("expires_at", "Expires", Control::Date),
    field("api.rotation", "Rotation note", Control::Multiline),
    NOTES,
];

const DOCUMENT: &[FieldTemplate] = &[
    field("doc.type", "Document type", Control::Select).options(&[
        "Passport",
        "Driver's licence",
        "ID card",
        "Contract",
        "Certificate",
        "Other",
    ]),
    field("doc.number", "Number", Control::Text),
    field("doc.issuer", "Issued by", Control::Text),
    field("doc.issued", "Issued", Control::Date),
    field("expires_at", "Expires", Control::Date),
    NOTES,
];

const IDENTITY: &[FieldTemplate] = &[
    field("name.given", "Given name", Control::Text)
        .fill(Fill::Fill)
        .section("Name"),
    field("name.middle", "Middle name", Control::Text)
        .fill(Fill::Fill)
        .section("Name"),
    field("name.family", "Family name", Control::Text)
        .fill(Fill::Fill)
        .section("Name"),
    field("identity.email", "Email", Control::Email)
        .fill(Fill::Fill)
        .section("Contact"),
    field("address.tel", "Phone", Control::Phone)
        .fill(Fill::Fill)
        .section("Contact"),
    field("address.street", "Street", Control::Text)
        .fill(Fill::Fill)
        .section("Address"),
    field("address.city", "City", Control::Text)
        .fill(Fill::Fill)
        .section("Address"),
    field("address.territory", "State or region", Control::Text)
        .fill(Fill::Fill)
        .section("Address"),
    field("address.postal_code", "Postal code", Control::Text)
        .fill(Fill::Fill)
        .section("Address"),
    field("address.country", "Country", Control::Text)
        .fill(Fill::Fill)
        .section("Address"),
    NOTES,
];

const CARD: &[FieldTemplate] = &[
    field("card.number", "Card number", Control::Secret)
        .required()
        .grouped()
        .fill(Fill::Confirm)
        .section("Card"),
    field("card.holder", "Name on card", Control::Text)
        .fill(Fill::Confirm)
        .section("Card"),
    field("card.expiry", "Expires", Control::Month)
        .fill(Fill::Confirm)
        .section("Card"),
    field("card.cvc", "Security code", Control::Secret)
        .fill(Fill::Confirm)
        .section("Card"),
    field("card.pin", "PIN", Control::Secret).section("Card"),
    field("card.type", "Brand", Control::Text)
        .hint("Left empty, it is worked out from the number."),
    NOTES,
];

const BANK_ACCOUNT: &[FieldTemplate] = &[
    field("bank.name", "Bank", Control::Text).section("Account"),
    field("bank.holder", "Account holder", Control::Text).section("Account"),
    field("bank.account_number", "Account number", Control::Secret)
        .grouped()
        .section("Account"),
    field("bank.routing", "Routing or sort code", Control::Text).section("Account"),
    field("bank.iban", "IBAN", Control::Secret)
        .grouped()
        .section("International"),
    field("bank.swift", "SWIFT or BIC", Control::Text).section("International"),
    field("bank.branch", "Branch", Control::Text),
    field("bank.pin", "PIN", Control::Secret),
    NOTES,
];

const NOTE: &[FieldTemplate] = &[field("body", "Note", Control::Multiline)
    .section("Note")
    .hint("Markdown works: # for a heading, * for a list.")];

const CRYPTO_WALLET: &[FieldTemplate] = &[
    field("seed", "Recovery phrase", Control::SecretMultiline)
        .required()
        .crown()
        .section("Wallet")
        .hint("The words in order, with spaces between them."),
    field("wallet.password", "Wallet password", Control::Secret)
        .generate(STRONG)
        .section("Wallet"),
    field("wallet.name", "Wallet app", Control::Text),
    field("wallet.network", "Network", Control::Text),
    field("wallet.address", "Address", Control::Text),
    NOTES,
];

const SERVER: &[FieldTemplate] = &[
    field("server.host", "Address", Control::Text)
        .required()
        .section("Server"),
    field("server.port", "Port", Control::Number).section("Server"),
    field("user", "Username", Control::Text)
        .fill(Fill::Fill)
        .section("Sign-in"),
    field("pw", "Password", Control::Secret)
        .fill(Fill::Fill)
        .generate(STRONG)
        .section("Sign-in"),
    field("server.admin", "Admin console", Control::Url),
    NOTES,
];

const DATABASE: &[FieldTemplate] = &[
    field("db.type", "Database type", Control::Select)
        .options(&[
            "PostgreSQL",
            "MySQL",
            "MariaDB",
            "SQL Server",
            "Oracle",
            "SQLite",
            "MongoDB",
            "Redis",
            "Other",
        ])
        .section("Database"),
    field("db.host", "Server", Control::Text).section("Database"),
    field("db.port", "Port", Control::Number).section("Database"),
    field("db.name", "Database name", Control::Text).section("Database"),
    field("user", "Username", Control::Text).section("Sign-in"),
    field("pw", "Password", Control::Secret)
        .generate(STRONG)
        .section("Sign-in"),
    field("db.options", "Connection options", Control::Text),
    NOTES,
];

const SOFTWARE_LICENCE: &[FieldTemplate] = &[
    field("licence.key", "Licence key", Control::SecretMultiline)
        .required()
        .section("Licence"),
    field("licence.licensed_to", "Licensed to", Control::Text).section("Licence"),
    field("licence.email", "Registered email", Control::Email).section("Licence"),
    field("licence.version", "Version", Control::Text),
    field("licence.publisher", "Publisher", Control::Text),
    field("licence.url", "Download page", Control::Url),
    field("licence.purchased", "Purchased", Control::Date),
    field("expires_at", "Expires", Control::Date),
    NOTES,
];

const MEMBERSHIP: &[FieldTemplate] = &[
    field("member.organization", "Organization", Control::Text).section("Membership"),
    field("member.name", "Member name", Control::Text).section("Membership"),
    field("member.number", "Member number", Control::Text).section("Membership"),
    field("member.pin", "PIN", Control::Secret).section("Membership"),
    field("member.since", "Member since", Control::Date),
    field("expires_at", "Expires", Control::Date),
    field("member.url", "Website", Control::Url),
    NOTES,
];

const CUSTOM: &[FieldTemplate] = &[NOTES];

const fn template(
    kind: ItemKind,
    name: &'static str,
    summary: &'static str,
    fields: &'static [FieldTemplate],
    primary: Option<&'static str>,
    subtitle: Option<&'static str>,
) -> Template {
    Template {
        kind,
        name,
        summary,
        creatable: true,
        fields,
        primary,
        subtitle,
    }
}

/// Every kind's template, in the order a "New item" list shows them (the
/// hub type first, then the order people reach for them).
pub const TEMPLATES: [Template; 18] = [
    template(
        ItemKind::Login,
        "Login",
        "A website or app account",
        LOGIN,
        Some("pw"),
        Some("user"),
    ),
    template(
        ItemKind::Code,
        "One-time code",
        "The six-digit codes a site asks for after the password",
        CODE,
        Some("code.0"),
        None,
    ),
    template(
        ItemKind::Card,
        "Payment card",
        "A credit or debit card",
        CARD,
        Some("card.number"),
        Some("card.holder"),
    ),
    template(
        ItemKind::Note,
        "Note",
        "Anything you want to keep private",
        NOTE,
        None,
        None,
    ),
    template(
        ItemKind::WiFi,
        "Wi-Fi network",
        "A network name and its password",
        WIFI,
        Some("wifi.passphrase"),
        Some("wifi.ssid"),
    ),
    template(
        ItemKind::Identity,
        "Identity",
        "Your name, email, phone and address, for forms",
        IDENTITY,
        None,
        Some("identity.email"),
    ),
    template(
        ItemKind::Document,
        "Document",
        "A passport, licence or contract, with its dates",
        DOCUMENT,
        None,
        Some("doc.type"),
    ),
    template(
        ItemKind::BankAccount,
        "Bank account",
        "Account and routing numbers",
        BANK_ACCOUNT,
        Some("bank.account_number"),
        Some("bank.name"),
    ),
    template(
        ItemKind::CryptoWallet,
        "Crypto wallet",
        "A recovery phrase, always kept as a crown jewel",
        CRYPTO_WALLET,
        None,
        Some("wallet.network"),
    ),
    template(
        ItemKind::SshKey,
        "SSH key",
        "A key for signing in to servers",
        SSH_KEY,
        None,
        Some("ssh.key_type"),
    ),
    template(
        ItemKind::ApiCredential,
        "API key",
        "A key or token a program uses",
        API_CREDENTIAL,
        Some("api.key"),
        Some("api.host"),
    ),
    template(
        ItemKind::Server,
        "Server",
        "A machine you sign in to",
        SERVER,
        Some("pw"),
        Some("server.host"),
    ),
    template(
        ItemKind::Database,
        "Database",
        "Where a database lives and how to sign in",
        DATABASE,
        Some("pw"),
        Some("db.host"),
    ),
    template(
        ItemKind::SoftwareLicence,
        "Software licence",
        "A licence key and who it is for",
        SOFTWARE_LICENCE,
        Some("licence.key"),
        Some("licence.publisher"),
    ),
    template(
        ItemKind::Membership,
        "Membership",
        "A club, loyalty or library card",
        MEMBERSHIP,
        None,
        Some("member.organization"),
    ),
    template(
        ItemKind::Custom,
        "Custom item",
        "Start empty and add your own fields",
        CUSTOM,
        None,
        None,
    ),
    Template {
        creatable: false,
        ..template(
            ItemKind::Passkey,
            "Passkey",
            "Made by a site when you choose a passkey",
            PASSKEY,
            None,
            Some("passkey.0.user_name"),
        )
    },
    Template {
        creatable: false,
        ..template(
            ItemKind::Consent,
            "Consent record",
            "The app's own record of a choice you made",
            &[],
            None,
            None,
        )
    },
];

/// The template for `kind`.
pub fn template_for(kind: ItemKind) -> &'static Template {
    TEMPLATES
        .iter()
        .find(|t| t.kind == kind)
        .expect("every kind has a template (tested)")
}

/// A card's brand from its number's leading digits, for the `card.type`
/// field when the person left it empty (02-model §4 "issuer (detected)").
pub fn card_brand(number: &str) -> Option<&'static str> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    let prefix = |n: usize| digits.get(..n).and_then(|p| p.parse::<u32>().ok());
    match (prefix(1), prefix(2), prefix(4), prefix(6)) {
        (_, Some(34 | 37), _, _) => Some("American Express"),
        (Some(4), _, _, _) => Some("Visa"),
        (_, Some(51..=55), _, _) => Some("Mastercard"),
        (_, _, Some(2221..=2720), _) => Some("Mastercard"),
        (_, _, Some(6011), _) | (_, Some(65), _, _) => Some("Discover"),
        (_, _, _, Some(622126..=622925)) => Some("Discover"),
        (_, _, Some(3528..=3589), _) => Some("JCB"),
        (_, Some(36 | 38 | 39), _, _) | (_, _, Some(3000..=3059), _) => Some("Diners Club"),
        (_, _, Some(5018 | 5020 | 5038 | 5893 | 6304 | 6759 | 6761..=6763), _) => Some("Maestro"),
        (_, Some(62), _, _) => Some("UnionPay"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_template_and_ids_are_unique() {
        for kind in ItemKind::ALL {
            let n = TEMPLATES.iter().filter(|t| t.kind == kind).count();
            assert_eq!(n, 1, "{kind:?}");
            let t = template_for(kind);
            let mut ids: Vec<&str> = t.fields.iter().map(|f| f.field_id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), t.fields.len(), "{kind:?} repeats a field id");
            assert!(!ids.contains(&"title"), "title is common, not templated");
            for f in t.fields {
                assert!(!f.field_id.starts_with("custom."), "{}", f.field_id);
                assert_eq!(
                    f.control == Control::Select,
                    !f.options.is_empty(),
                    "{} options",
                    f.field_id
                );
                if f.generator.is_some() {
                    assert_eq!(
                        f.control,
                        Control::Secret,
                        "{}: only a secret is generated",
                        f.field_id
                    );
                }
            }
        }
        assert!(!template_for(ItemKind::Passkey).creatable);
        assert!(!template_for(ItemKind::Consent).creatable);
        assert_eq!(TEMPLATES.iter().filter(|t| t.creatable).count(), 16);
    }

    #[test]
    fn the_crown_jewel_data_matches_the_classifier() {
        for t in &TEMPLATES {
            for f in t.fields {
                let classified = crown_jewel_default(Classify {
                    kind: t.kind,
                    field_id: Some(f.field_id),
                    login_role: LoginRole::Everyday,
                });
                assert_eq!(
                    f.crown_jewel,
                    classified == CrownJewelDefault::Default,
                    "{:?} {}",
                    t.kind,
                    f.field_id
                );
            }
        }
        let wallet = template_for(ItemKind::CryptoWallet);
        assert_eq!(wallet.crown_jewel_default(), CrownJewelDefault::Default);
        assert!(wallet.field("seed").unwrap().crown_jewel);
        assert_eq!(
            template_for(ItemKind::Login).crown_jewel_default(),
            CrownJewelDefault::Never
        );
    }

    #[test]
    fn fill_rules_follow_the_kind_and_the_model() {
        for t in &TEMPLATES {
            for f in t.fields {
                if !t.kind.autofill_allowed() {
                    assert_eq!(f.fill, Fill::Never, "{:?} {}", t.kind, f.field_id);
                }
                if f.control == Control::Totp || f.control == Control::Passkey {
                    assert_ne!(
                        f.fill,
                        Fill::Fill,
                        "codes and passkeys need a second action"
                    );
                }
            }
        }
        for id in ["card.number", "card.cvc", "card.expiry"] {
            assert_eq!(
                template_for(ItemKind::Card).field(id).unwrap().fill,
                Fill::Confirm
            );
        }
        for kind in [
            ItemKind::SshKey,
            ItemKind::BankAccount,
            ItemKind::CryptoWallet,
        ] {
            assert!(
                template_for(kind)
                    .fields
                    .iter()
                    .all(|f| f.fill == Fill::Never)
            );
        }
    }

    #[test]
    fn primary_is_a_secret_and_subtitle_is_plain() {
        for t in &TEMPLATES {
            if let Some(p) = t.primary {
                let f = t
                    .field(p)
                    .unwrap_or_else(|| panic!("{:?} primary {p}", t.kind));
                assert!(
                    f.control.is_secret(),
                    "{:?} primary {p} must go through the core",
                    t.kind
                );
            }
            if let Some(s) = t.subtitle {
                let f = t
                    .field(s)
                    .unwrap_or_else(|| panic!("{:?} subtitle {s}", t.kind));
                assert!(
                    !f.control.is_secret(),
                    "{:?} subtitle {s} is shown in lists",
                    t.kind
                );
            }
        }
    }

    #[test]
    fn the_importers_ids_are_the_templates_ids() {
        let login = template_for(ItemKind::Login);
        for id in ["user", "pw", "url.0", "code.0", "notes"] {
            assert!(login.field(id).is_some(), "{id}");
        }
        let card = template_for(ItemKind::Card);
        for id in [
            "card.number",
            "card.holder",
            "card.type",
            "card.cvc",
            "card.pin",
            "card.expiry",
        ] {
            assert!(card.field(id).is_some(), "{id}");
        }
        assert!(template_for(ItemKind::Note).field("body").is_some());
        assert_eq!(
            template_for(ItemKind::WiFi)
                .field("wifi.passphrase")
                .unwrap()
                .generator,
            Some(PasswordRule::READABLE)
        );
    }

    #[test]
    fn card_brands_are_spotted() {
        assert_eq!(card_brand("4111 1111 1111 1111"), Some("Visa"));
        assert_eq!(card_brand("5555555555554444"), Some("Mastercard"));
        assert_eq!(card_brand("2223003122003222"), Some("Mastercard"));
        assert_eq!(card_brand("378282246310005"), Some("American Express"));
        assert_eq!(card_brand("6011111111111117"), Some("Discover"));
        assert_eq!(card_brand("3530111333300000"), Some("JCB"));
        assert_eq!(card_brand("6200000000000005"), Some("UnionPay"));
        assert_eq!(card_brand("1234"), None);
        assert_eq!(card_brand(""), None);
    }
}
