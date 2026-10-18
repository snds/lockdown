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
    /// A link to another item's field in the same vault (02-model §5
    /// `linked`): the value is `<item id>#<field id>`, the field's own
    /// value is read from there, through that item's gates.
    Linked,
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
            Self::Linked => FieldKind::Linked,
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
            Self::Linked => "linked",
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
    /// The field may appear more than once, numbered from `.0` (a Login's
    /// websites: `url.0`, `url.1`, …); every copy takes this template.
    pub repeats: bool,
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
        repeats: false,
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
    const fn repeats(mut self) -> Self {
        self.repeats = true;
        self
    }

    /// The `<n>`th copy's id of a repeating field (`url.0` → `url.<n>`).
    pub fn numbered(&self, n: usize) -> Option<String> {
        if !self.repeats {
            return None;
        }
        let stem = self.field_id.strip_suffix(".0")?;
        Some(format!("{stem}.{n}"))
    }
}

/// The fields every item carries whatever its kind (02-model §4 "common to
/// every type"): its folder, its tags and whether it is a favourite. They
/// are sealed like any field, never listed in a kind's template (a shell
/// draws them as the item's organization, not as its details), and an
/// edit that leaves them out keeps them (`crate::compose`).
pub const COMMON: &[FieldTemplate] = &[
    field(crate::organization::ORG_FOLDER, "Folder", Control::Text).section("Organization"),
    field(crate::organization::ORG_TAGS, "Tags", Control::Multiline)
        .section("Organization")
        .hint("One tag per line; a slash nests one under another (work/clients)."),
    field(
        crate::organization::ORG_FAVOURITE,
        "Favourite",
        Control::Boolean,
    )
    .section("Organization"),
];

/// A common field by id.
pub fn common_field(field_id: &str) -> Option<&'static FieldTemplate> {
    COMMON.iter().find(|f| f.field_id == field_id)
}

/// Markdown notes, last on every template but a Note's (02-model §4 "common to every type").
const NOTES: FieldTemplate = field("notes", "Notes", Control::Multiline).section("Notes");

/// One template: a kind's own, or one of its variants (D231). A variant
/// is a template of its own (a Passport, a Vehicle, a Router) on a kind
/// whose rules it shares (fill, autofill, crown jewel, expiry); an item
/// made from one keeps the variant's name in [`TEMPLATE_FIELD`], so an
/// older build still opens it as its kind, with the variant's fields under
/// "Other fields".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Template {
    /// The kind.
    pub kind: ItemKind,
    /// The variant's name (`passport`), or empty for the kind's own.
    pub variant: &'static str,
    /// A Material Symbols icon name for lists and menus.
    pub icon: &'static str,
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
    /// The template's id as shells name it: the kind (`identity`), or the
    /// kind and the variant (`identity/passport`).
    pub fn id(&self) -> String {
        if self.variant.is_empty() {
            self.kind.as_str().to_string()
        } else {
            format!("{}/{}", self.kind.as_str(), self.variant)
        }
    }

    /// Whether any of its fields is ever put into a page. A Contact's are
    /// not (the contacts list is never used to fill; 02-model §4 Identity).
    pub fn fills(&self) -> bool {
        self.kind.autofill_allowed() && self.fields.iter().any(|f| f.fill != Fill::Never)
    }

    const fn variant(mut self, variant: &'static str) -> Self {
        self.variant = variant;
        self
    }

    /// One field by id.
    pub fn field(&self, field_id: &str) -> Option<&'static FieldTemplate> {
        if let Some(f) = self.fields.iter().find(|f| f.field_id == field_id) {
            return Some(f);
        }
        // A later copy of a repeating field (`url.3`) takes the first's template.
        let (stem, n) = field_id.rsplit_once('.')?;
        if n.is_empty() || n == "0" || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        self.fields
            .iter()
            .find(|f| f.repeats && f.field_id.strip_suffix(".0") == Some(stem))
    }

    /// One field by id, or a field every item carries ([`COMMON`]).
    pub fn field_or_common(&self, field_id: &str) -> Option<&'static FieldTemplate> {
        self.field(field_id).or_else(|| common_field(field_id))
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
    (FieldKind::Totp, "One-time code"),
    (FieldKind::Linked, "Link to another item"),
];

/// The reserved field that names an item's template variant (D231): its
/// text is the variant (`passport`), or for an item made from a person's
/// own template, `user:<template item id>`. Kept by Lockdown, never named
/// in an edit (`crate::compose::is_reserved`).
pub const TEMPLATE_FIELD: &str = "lockdown.template";

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
    field("sign_in_method", "Sign-in method", Control::Select)
        .options(SIGN_IN_METHODS)
        .section("Sign-in")
        .hint("How you get into this account; left empty, a password."),
    field("sign_in_provider", "Signed in with", Control::Text)
        .section("Sign-in")
        .hint("The account or company you sign in through, like Google, Apple or your work."),
    field("url.0", "Website", Control::Url)
        .repeats()
        .section("Websites"),
    NOTES,
];

/// How a Login is entered (02-model §4 `sign_in_method`), in plain words:
/// password, passkey, federated (another account) and single sign-on.
pub const SIGN_IN_METHODS: &[&str] = &["Password", "Passkey", "Another account", "Single sign-on"];

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
        .options(WIFI_SECURITY)
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
    field("card.reprompt", "Ask before using the security code", Control::Boolean)
        .section("Card")
        .hint("Lockdown asks for your fingerprint or password before it shows, copies or fills the security code."),
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

// --- the variants (D231) ----------------------------------------------------

const ADDRESS: &[FieldTemplate] = &[
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
    field("address.tel", "Phone", Control::Phone)
        .fill(Fill::Fill)
        .section("Contact"),
    NOTES,
];

const BUSINESS: &[FieldTemplate] = &[
    field("business.name", "Company name", Control::Text)
        .required()
        .fill(Fill::Fill)
        .section("Business"),
    field("business.department", "Department", Control::Text).section("Business"),
    field("business.tax_id", "Tax or VAT number", Control::Text).section("Business"),
    field("business.registration", "Company number", Control::Text).section("Business"),
    field("identity.email", "Email", Control::Email)
        .fill(Fill::Fill)
        .section("Contact"),
    field("address.tel", "Phone", Control::Phone)
        .fill(Fill::Fill)
        .section("Contact"),
    field("business.website", "Website", Control::Url).section("Contact"),
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

const VEHICLE: &[FieldTemplate] = &[
    field("vehicle.make", "Make", Control::Text).section("Vehicle"),
    field("vehicle.model", "Model", Control::Text).section("Vehicle"),
    field("vehicle.year", "Year", Control::Number).section("Vehicle"),
    field("vehicle.colour", "Colour", Control::Text).section("Vehicle"),
    field("vehicle.plate", "Number plate", Control::Text).section("Vehicle"),
    field("vehicle.vin", "VIN", Control::Text)
        .section("Vehicle")
        .hint("The 17-character vehicle identification number."),
    field("expires_at", "Registration expires", Control::Date).section("Registration"),
    field("vehicle.insurer", "Insurer", Control::Text).section("Insurance"),
    field("vehicle.policy", "Policy number", Control::Text).section("Insurance"),
    NOTES,
];

const SEXES: &[&str] = &["Female", "Male", "X"];

const PASSPORT: &[FieldTemplate] = &[
    field("passport.number", "Passport number", Control::Secret)
        .required()
        .section("Passport"),
    field("passport.type", "Type", Control::Text)
        .section("Passport")
        .hint("The letter on the photo page, usually P."),
    field("passport.country", "Issuing country", Control::Text).section("Passport"),
    field("passport.authority", "Issuing authority", Control::Text).section("Passport"),
    field("passport.issued", "Issued", Control::Date).section("Passport"),
    field("expires_at", "Expires", Control::Date).section("Passport"),
    field("name.given", "Given names", Control::Text).section("Holder"),
    field("name.family", "Surname", Control::Text).section("Holder"),
    field("passport.nationality", "Nationality", Control::Text).section("Holder"),
    field("passport.birth_date", "Date of birth", Control::Date).section("Holder"),
    field("passport.birth_place", "Place of birth", Control::Text).section("Holder"),
    field("passport.sex", "Sex", Control::Select)
        .options(SEXES)
        .section("Holder"),
    field("passport.national_id", "Personal number", Control::Text).section("Holder"),
    NOTES,
];

const DRIVER_LICENCE: &[FieldTemplate] = &[
    field("dl.number", "Licence number", Control::Secret)
        .required()
        .section("Licence"),
    field("dl.class", "Categories", Control::Text)
        .section("Licence")
        .hint("The vehicle classes it covers, like B or C1."),
    field("dl.country", "Country", Control::Text).section("Licence"),
    field("dl.territory", "State or region", Control::Text).section("Licence"),
    field("dl.authority", "Issued by", Control::Text).section("Licence"),
    field("dl.issued", "Issued", Control::Date).section("Licence"),
    field("expires_at", "Expires", Control::Date).section("Licence"),
    field("name.given", "Given names", Control::Text).section("Holder"),
    field("name.family", "Surname", Control::Text).section("Holder"),
    field("dl.birth_date", "Date of birth", Control::Date).section("Holder"),
    NOTES,
];

const NATIONAL_ID: &[FieldTemplate] = &[
    field("id.number", "Number", Control::Secret)
        .required()
        .section("Identity document"),
    field("id.country", "Issuing country", Control::Text).section("Identity document"),
    field("id.authority", "Issued by", Control::Text).section("Identity document"),
    field("id.issued", "Issued", Control::Date).section("Identity document"),
    field("expires_at", "Expires", Control::Date).section("Identity document"),
    field("name.given", "Given names", Control::Text).section("Holder"),
    field("name.family", "Surname", Control::Text).section("Holder"),
    field("id.nationality", "Nationality", Control::Text).section("Holder"),
    field("id.birth_date", "Date of birth", Control::Date).section("Holder"),
    field("id.personal_number", "Personal number", Control::Text).section("Holder"),
    NOTES,
];

/// A person in your address book: kept apart from the identities forms
/// are filled from, and never filled itself (02-model §4 Identity).
const CONTACT: &[FieldTemplate] = &[
    field("name.given", "Given name", Control::Text).section("Name"),
    field("name.family", "Family name", Control::Text).section("Name"),
    field("contact.company", "Company", Control::Text).section("Name"),
    field("identity.email", "Email", Control::Email).section("Contact"),
    field("address.tel", "Phone", Control::Phone).section("Contact"),
    field("contact.birthday", "Birthday", Control::Date).section("Contact"),
    field("address.street", "Street", Control::Text).section("Address"),
    field("address.city", "City", Control::Text).section("Address"),
    field("address.territory", "State or region", Control::Text).section("Address"),
    field("address.postal_code", "Postal code", Control::Text).section("Address"),
    field("address.country", "Country", Control::Text).section("Address"),
    NOTES,
];

const BLOOD_TYPES: &[&str] = &["A+", "A-", "B+", "B-", "AB+", "AB-", "O+", "O-"];

const MEDICAL_RECORD: &[FieldTemplate] = &[
    field("med.patient", "Patient", Control::Text).section("Record"),
    field("med.provider", "Doctor or clinic", Control::Text).section("Record"),
    field("med.record_number", "Record number", Control::Text).section("Record"),
    field("med.insurer", "Health insurer", Control::Text).section("Record"),
    field("med.blood_type", "Blood type", Control::Select)
        .options(BLOOD_TYPES)
        .section("Health"),
    field("med.allergies", "Allergies", Control::Multiline).section("Health"),
    field("med.medications", "Medications", Control::Multiline).section("Health"),
    field("med.conditions", "Conditions", Control::Multiline).section("Health"),
    field("med.emergency_contact", "Emergency contact", Control::Text).section("In an emergency"),
    field("med.emergency_phone", "Their phone", Control::Phone).section("In an emergency"),
    NOTES,
];

const INSURANCE: &[FieldTemplate] = &[
    field("insurance.type", "Cover", Control::Select)
        .options(&["Health", "Car", "Home", "Life", "Travel", "Pet", "Other"])
        .section("Policy"),
    field("insurance.insurer", "Insurer", Control::Text).section("Policy"),
    field("insurance.policy_number", "Policy number", Control::Text).section("Policy"),
    field("insurance.holder", "Policy holder", Control::Text).section("Policy"),
    field("insurance.start", "Starts", Control::Date).section("Policy"),
    field("expires_at", "Renews", Control::Date).section("Policy"),
    field("insurance.premium", "Premium", Control::Text).section("Policy"),
    field("insurance.phone", "Claims phone", Control::Phone).section("Claims"),
    field("insurance.url", "Insurer's website", Control::Url).section("Claims"),
    NOTES,
];

/// Recovery or backup codes for an account: a crown jewel by default
/// (02-model §3), from the `recovery_codes` field the classifier knows.
const RECOVERY_CODES: &[FieldTemplate] = &[
    field("recovery.account", "For", Control::Text)
        .section("Recovery")
        .hint("The account these codes get you back into."),
    field("recovery_codes", "Recovery codes", Control::SecretMultiline)
        .required()
        .crown()
        .section("Recovery")
        .hint("One code per line, as the site showed them."),
    field("recovery.made", "Made", Control::Date).section("Recovery"),
    NOTES,
];

const OUTDOOR_LICENCE: &[FieldTemplate] = &[
    field("outdoor.holder", "Name", Control::Text).section("Licence"),
    field("outdoor.number", "Licence number", Control::Text).section("Licence"),
    field("outdoor.activities", "Covers", Control::Text)
        .section("Licence")
        .hint("Fishing, hunting, a park pass…"),
    field("outdoor.authority", "Issued by", Control::Text).section("Licence"),
    field("outdoor.territory", "Where it's valid", Control::Text).section("Licence"),
    field("outdoor.issued", "Issued", Control::Date).section("Licence"),
    field("expires_at", "Expires", Control::Date).section("Licence"),
    NOTES,
];

const EMAIL_ACCOUNT: &[FieldTemplate] = &[
    field("email.address", "Email address", Control::Email)
        .required()
        .section("Account"),
    field("pw", "Password", Control::Secret)
        .generate(STRONG)
        .section("Account"),
    field("email.provider", "Provider", Control::Text).section("Account"),
    field("email.imap_host", "Incoming server", Control::Text).section("Servers"),
    field("email.imap_port", "Incoming port", Control::Number).section("Servers"),
    field("email.smtp_host", "Outgoing server", Control::Text).section("Servers"),
    field("email.smtp_port", "Outgoing port", Control::Number).section("Servers"),
    field("email.security", "Connection security", Control::Select)
        .options(&["SSL/TLS", "STARTTLS", "None"])
        .section("Servers"),
    NOTES,
];

const ROUTER: &[FieldTemplate] = &[
    field("wifi.ssid", "Network name", Control::Text)
        .required()
        .section("Network"),
    field("wifi.passphrase", "Password", Control::Secret)
        .generate(READABLE)
        .section("Network"),
    field("wifi.security", "Security", Control::Select)
        .options(WIFI_SECURITY)
        .section("Network"),
    field("wifi.hidden", "Hidden network", Control::Boolean).section("Network"),
    field("router.admin", "Admin page", Control::Url).section("Router"),
    field("user", "Admin username", Control::Text).section("Router"),
    field("pw", "Admin password", Control::Secret)
        .generate(STRONG)
        .section("Router"),
    field("router.model", "Model", Control::Text).section("Router"),
    NOTES,
];

const PGP_KEY: &[FieldTemplate] = &[
    field("pgp.private_key", "Private key", Control::SecretMultiline)
        .required()
        .section("Private"),
    field("pgp.passphrase", "Passphrase", Control::Secret)
        .generate(STRONG)
        .section("Private"),
    field("pgp.public_key", "Public key", Control::Multiline).section("Public"),
    field("pgp.fingerprint", "Fingerprint", Control::Text).section("Public"),
    field("pgp.user_id", "User ID", Control::Text)
        .section("Public")
        .hint("The name and email the key is made out to."),
    field("pgp.algorithm", "Algorithm", Control::Select)
        .options(&["Ed25519", "RSA", "ECDSA", "Other"])
        .section("Public"),
    field("expires_at", "Expires", Control::Date).section("Public"),
    NOTES,
];

/// Wi-Fi's security choices, shared by a network and a router.
pub const WIFI_SECURITY: &[&str] = &[
    "WPA3",
    "WPA2/WPA3",
    "WPA2",
    "WPA",
    "WEP",
    "Open",
    "Enterprise",
];

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
        variant: "",
        icon: "category",
        name,
        summary,
        creatable: true,
        fields,
        primary,
        subtitle,
    }
}

impl Template {
    const fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }
}

/// Every template, in the order a "New item" list shows them: each kind's
/// own (the hub type first, then the order people reach for them), its
/// variants after it, and last the three no one makes by hand.
pub const TEMPLATES: &[Template] = &[
    template(
        ItemKind::Login,
        "Login",
        "A website or app account",
        LOGIN,
        Some("pw"),
        Some("user"),
    )
    .icon("key"),
    template(
        ItemKind::Code,
        "One-time code",
        "The six-digit codes a site asks for after the password",
        CODE,
        Some("code.0"),
        None,
    )
    .icon("timer"),
    template(
        ItemKind::Card,
        "Payment card",
        "A credit or debit card",
        CARD,
        Some("card.number"),
        Some("card.holder"),
    )
    .icon("credit_card"),
    template(
        ItemKind::Note,
        "Note",
        "Anything you want to keep private",
        NOTE,
        None,
        None,
    )
    .icon("sticky_note_2"),
    template(
        ItemKind::WiFi,
        "Wi-Fi network",
        "A network name and its password",
        WIFI,
        Some("wifi.passphrase"),
        Some("wifi.ssid"),
    )
    .icon("wifi"),
    template(
        ItemKind::WiFi,
        "Router",
        "A network and the router's own admin sign-in",
        ROUTER,
        Some("wifi.passphrase"),
        Some("wifi.ssid"),
    )
    .variant("router")
    .icon("router"),
    template(
        ItemKind::Identity,
        "Identity",
        "Your name, email, phone and address, for forms",
        IDENTITY,
        None,
        Some("identity.email"),
    )
    .icon("badge"),
    template(
        ItemKind::Identity,
        "Address",
        "A postal address, for forms",
        ADDRESS,
        None,
        Some("address.city"),
    )
    .variant("address")
    .icon("home"),
    template(
        ItemKind::Identity,
        "Business",
        "A company's name, numbers and address, for forms",
        BUSINESS,
        None,
        Some("business.name"),
    )
    .variant("business")
    .icon("business"),
    template(
        ItemKind::Identity,
        "Passport",
        "The facts from a passport's photo page",
        PASSPORT,
        Some("passport.number"),
        Some("passport.country"),
    )
    .variant("passport")
    .icon("flight"),
    template(
        ItemKind::Identity,
        "Driver's licence",
        "A driving licence's number, categories and dates",
        DRIVER_LICENCE,
        Some("dl.number"),
        Some("dl.territory"),
    )
    .variant("driver-licence")
    .icon("id_card"),
    template(
        ItemKind::Identity,
        "National ID",
        "An ID card, or a social security or national insurance number",
        NATIONAL_ID,
        Some("id.number"),
        Some("id.country"),
    )
    .variant("national-id")
    .icon("fingerprint"),
    template(
        ItemKind::Identity,
        "Vehicle",
        "A car's plate, VIN, registration and insurance",
        VEHICLE,
        None,
        Some("vehicle.plate"),
    )
    .variant("vehicle")
    .icon("directions_car"),
    template(
        ItemKind::Identity,
        "Contact",
        "Someone else's details; never used to fill a form",
        CONTACT,
        None,
        Some("identity.email"),
    )
    .variant("contact")
    .icon("contact_page"),
    template(
        ItemKind::Document,
        "Document",
        "A passport, licence or contract, with its dates",
        DOCUMENT,
        None,
        Some("doc.type"),
    )
    .icon("description"),
    template(
        ItemKind::Document,
        "Medical record",
        "Your doctor, blood type, allergies and medications",
        MEDICAL_RECORD,
        None,
        Some("med.provider"),
    )
    .variant("medical-record")
    .icon("medical_information"),
    template(
        ItemKind::Document,
        "Insurance policy",
        "A policy's number, cover, renewal date and claims line",
        INSURANCE,
        None,
        Some("insurance.insurer"),
    )
    .variant("insurance")
    .icon("health_and_safety"),
    template(
        ItemKind::Document,
        "Recovery codes",
        "An account's backup codes, always kept as a crown jewel",
        RECOVERY_CODES,
        Some("recovery_codes"),
        Some("recovery.account"),
    )
    .variant("recovery-codes")
    .icon("lock_reset"),
    template(
        ItemKind::Document,
        "Outdoor licence",
        "A fishing or hunting licence, or a park pass",
        OUTDOOR_LICENCE,
        None,
        Some("outdoor.activities"),
    )
    .variant("outdoor-licence")
    .icon("forest"),
    template(
        ItemKind::BankAccount,
        "Bank account",
        "Account and routing numbers",
        BANK_ACCOUNT,
        Some("bank.account_number"),
        Some("bank.name"),
    )
    .icon("account_balance"),
    template(
        ItemKind::CryptoWallet,
        "Crypto wallet",
        "A recovery phrase, always kept as a crown jewel",
        CRYPTO_WALLET,
        None,
        Some("wallet.network"),
    )
    .icon("account_balance_wallet"),
    template(
        ItemKind::SshKey,
        "SSH key",
        "A key for signing in to servers",
        SSH_KEY,
        None,
        Some("ssh.key_type"),
    )
    .icon("terminal"),
    template(
        ItemKind::SshKey,
        "PGP key",
        "A key for signing and encrypting email and files",
        PGP_KEY,
        None,
        Some("pgp.user_id"),
    )
    .variant("pgp")
    .icon("encrypted"),
    template(
        ItemKind::ApiCredential,
        "API key",
        "A key or token a program uses",
        API_CREDENTIAL,
        Some("api.key"),
        Some("api.host"),
    )
    .icon("api"),
    template(
        ItemKind::Server,
        "Server",
        "A machine you sign in to",
        SERVER,
        Some("pw"),
        Some("server.host"),
    )
    .icon("dns"),
    template(
        ItemKind::Server,
        "Email account",
        "An address, its password and its mail servers",
        EMAIL_ACCOUNT,
        Some("pw"),
        Some("email.address"),
    )
    .variant("email-account")
    .icon("mail"),
    template(
        ItemKind::Database,
        "Database",
        "Where a database lives and how to sign in",
        DATABASE,
        Some("pw"),
        Some("db.host"),
    )
    .icon("database"),
    template(
        ItemKind::SoftwareLicence,
        "Software licence",
        "A licence key and who it is for",
        SOFTWARE_LICENCE,
        Some("licence.key"),
        Some("licence.publisher"),
    )
    .icon("license"),
    template(
        ItemKind::Membership,
        "Membership",
        "A club, loyalty or library card",
        MEMBERSHIP,
        None,
        Some("member.organization"),
    )
    .icon("card_membership"),
    template(
        ItemKind::Custom,
        "Custom item",
        "Start empty and add your own fields",
        CUSTOM,
        None,
        None,
    )
    .icon("category"),
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
        .icon("passkey")
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
    Template {
        creatable: false,
        ..template(
            ItemKind::VaultInfo,
            "Vault details",
            "The vault's own name, folders and trash setting",
            &[],
            None,
            None,
        )
    },
    Template {
        creatable: false,
        ..template(
            ItemKind::Template,
            "Template",
            "A kind of item you made: its name and its fields",
            &[],
            None,
            None,
        )
    },
];

/// The kind's own template.
pub fn template_for(kind: ItemKind) -> &'static Template {
    TEMPLATES
        .iter()
        .find(|t| t.kind == kind && t.variant.is_empty())
        .expect("every kind has a template (tested)")
}

/// One of `kind`'s variants by name; the kind's own for an empty name.
pub fn template_variant(kind: ItemKind, variant: &str) -> Option<&'static Template> {
    TEMPLATES
        .iter()
        .find(|t| t.kind == kind && t.variant == variant)
}

/// A template by its id ([`Template::id`]): `identity` or `identity/passport`.
pub fn template_named(id: &str) -> Option<&'static Template> {
    let (kind, variant) = id.split_once('/').unwrap_or((id, ""));
    let kind = ItemKind::ALL.into_iter().find(|k| k.as_str() == kind)?;
    template_variant(kind, variant)
}

/// The variant an item's fields name in [`TEMPLATE_FIELD`], if any.
pub fn variant_of(fields: &[crate::PlainField]) -> Option<&str> {
    fields
        .iter()
        .find(|f| f.field_id == TEMPLATE_FIELD)
        .and_then(|f| match &f.value {
            crate::FieldValue::Text(t) => Some(t.as_str()),
            _ => None,
        })
}

/// The template an item of `kind` holding `fields` is drawn from: its
/// variant where it names one this build knows, else the kind's own (an
/// item from a newer build, or from a person's own template, shows its
/// fields under the kind's).
pub fn template_of(kind: ItemKind, fields: &[crate::PlainField]) -> &'static Template {
    variant_of(fields)
        .and_then(|v| template_variant(kind, v))
        .unwrap_or_else(|| template_for(kind))
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
            let n = TEMPLATES
                .iter()
                .filter(|t| t.kind == kind && t.variant.is_empty())
                .count();
            assert_eq!(n, 1, "{kind:?}");
        }
        let mut template_ids: Vec<String> = TEMPLATES.iter().map(Template::id).collect();
        template_ids.sort_unstable();
        template_ids.dedup();
        assert_eq!(template_ids.len(), TEMPLATES.len(), "a template id twice");
        for t in TEMPLATES {
            let mut ids: Vec<&str> = t.fields.iter().map(|f| f.field_id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), t.fields.len(), "{} repeats a field id", t.id());
            assert!(!ids.contains(&"title"), "title is common, not templated");
            assert!(
                t.variant
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{}",
                t.variant
            );
            assert!(
                !t.icon.is_empty()
                    && t.icon
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            );
            for f in t.fields {
                assert!(!f.field_id.starts_with("custom."), "{}", f.field_id);
                assert!(!f.field_id.starts_with("lockdown."), "{}", f.field_id);
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
                // A field id two templates of one kind share means the
                // same thing in both (a passport's names are a person's).
                for other in TEMPLATES.iter().filter(|o| o.kind == t.kind) {
                    if let Some(o) = other.fields.iter().find(|o| o.field_id == f.field_id) {
                        assert_eq!(
                            o.control.storage(),
                            f.control.storage(),
                            "{} differs between {} and {}",
                            f.field_id,
                            t.id(),
                            other.id()
                        );
                    }
                }
            }
            if !t.variant.is_empty() {
                assert!(t.creatable, "{}", t.id());
                assert_eq!(template_named(&t.id()), Some(t));
            }
        }
        assert!(!template_for(ItemKind::Passkey).creatable);
        assert!(!template_for(ItemKind::Consent).creatable);
        assert!(!template_for(ItemKind::VaultInfo).creatable);
        assert!(!template_for(ItemKind::Template).creatable);
        // 02-model §4 and the matrix: 25 or more typed templates.
        let creatable = TEMPLATES.iter().filter(|t| t.creatable).count();
        assert_eq!(creatable, 30);
        assert!(creatable - 1 >= 25, "25+ typed, the custom item aside");
        assert_eq!(
            template_named("identity"),
            Some(template_for(ItemKind::Identity))
        );
        assert!(template_named("identity/spaceship").is_none());
        assert!(template_named("spaceship").is_none());
    }

    #[test]
    fn an_item_finds_its_variant_and_falls_back_to_its_kind() {
        use crate::{FieldValue, PlainField};
        let marker = |v: &str| PlainField::text(TEMPLATE_FIELD, "Template", v);
        assert_eq!(
            template_of(ItemKind::Identity, &[marker("passport")]).name,
            "Passport"
        );
        // A variant this build doesn't know, or one of another kind: the kind's own.
        assert_eq!(
            template_of(ItemKind::Identity, &[marker("starship")]).variant,
            ""
        );
        assert_eq!(
            template_of(ItemKind::Document, &[marker("passport")]).variant,
            ""
        );
        assert_eq!(template_of(ItemKind::Identity, &[]).variant, "");
        let not_text = PlainField::new(
            TEMPLATE_FIELD,
            crate::FieldKind::Text,
            "Template",
            FieldValue::Boolean(true),
        );
        assert_eq!(variant_of(&[not_text]), None);
        // Contacts are never filled; the other identities are.
        assert!(!template_named("identity/contact").unwrap().fills());
        assert!(template_named("identity/address").unwrap().fills());
        assert!(template_for(ItemKind::Identity).fills());
        assert!(!template_named("identity/vehicle").unwrap().fills());
    }

    #[test]
    fn a_logins_websites_repeat_and_the_common_fields_resolve() {
        let login = template_for(ItemKind::Login);
        let first = login.field("url.0").unwrap();
        assert!(first.repeats);
        assert_eq!(login.field("url.4").unwrap().field_id, "url.0");
        assert_eq!(first.numbered(2).as_deref(), Some("url.2"));
        // Only repeating fields repeat; `.0` names itself; junk is junk.
        assert!(login.field("code.1").is_none());
        assert!(login.field("url.").is_none());
        assert!(login.field("url.x").is_none());
        assert!(login.field("org.tags").is_none());
        assert_eq!(
            login.field_or_common("org.tags").unwrap().control,
            Control::Multiline
        );
        assert_eq!(
            login.field("sign_in_method").unwrap().options,
            SIGN_IN_METHODS
        );
        for f in COMMON {
            assert!(f.field_id.starts_with("org."));
            assert!(!f.control.is_secret());
        }
    }

    #[test]
    fn the_crown_jewel_data_matches_the_classifier() {
        for t in TEMPLATES {
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
        let codes = template_named("document/recovery-codes").unwrap();
        assert!(codes.field("recovery_codes").unwrap().crown_jewel);
    }

    #[test]
    fn fill_rules_follow_the_kind_and_the_model() {
        for t in TEMPLATES {
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
        for t in TEMPLATES {
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
