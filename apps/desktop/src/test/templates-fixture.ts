import type { FieldTemplateView, TemplateView, Templates } from "@/core/types"

/** A slice of the core's templates (crates/vault/src/template.rs), as `templates` sends them. */
const f = (field_id: string, label: string, control: FieldTemplateView["control"], over: Partial<FieldTemplateView> = {}): FieldTemplateView => ({
  field_id,
  label,
  control,
  secret: ["secret", "secret-multiline", "totp", "passkey"].includes(control),
  required: false,
  crown_jewel: false,
  fill: "never",
  generator: null,
  options: [],
  section: "Details",
  grouped: false,
  hint: "",
  ...over,
})

const NOTES = f("notes", "Notes", "multiline", { section: "Notes" })
const STRONG = { length: 20, symbols: true, unambiguous: false, words: "20 characters: letters, digits and symbols" }
const READABLE = { length: 20, symbols: false, unambiguous: true, words: "20 characters: letters and digits, none that look alike" }

const t = (kind: string, name: string, fields: FieldTemplateView[], over: Partial<TemplateView> = {}): TemplateView => ({
  kind,
  id: over.variant ? `${kind}/${over.variant}` : kind,
  variant: "",
  icon: "category",
  fills: true,
  name,
  summary: `About a ${name.toLowerCase()}`,
  creatable: true,
  crown_jewel: false,
  autofill: true,
  primary: null,
  subtitle: null,
  fields,
  ...over,
})

export const TEMPLATES: Templates = {
  kinds: [
    t(
      "login",
      "Login",
      [
        f("user", "Username", "text", { fill: "fill", section: "Sign-in" }),
        f("pw", "Password", "secret", { fill: "fill", generator: STRONG, section: "Sign-in" }),
        f("code.0", "One-time code", "totp", { fill: "confirm", section: "Sign-in" }),
        f("url.0", "Website", "url", { section: "Websites" }),
        NOTES,
      ],
      { primary: "pw", subtitle: "user" },
    ),
    t(
      "wi-fi",
      "Wi-Fi network",
      [
        f("wifi.ssid", "Network name", "text", { required: true, section: "Network" }),
        f("wifi.passphrase", "Password", "secret", { generator: READABLE, section: "Network" }),
        f("wifi.security", "Security", "select", { options: ["WPA3", "WPA2"], section: "Network" }),
        f("wifi.hidden", "Hidden network", "boolean", { section: "Network" }),
        NOTES,
      ],
      { primary: "wifi.passphrase", subtitle: "wifi.ssid", autofill: true },
    ),
    t(
      "card",
      "Payment card",
      [
        f("card.number", "Card number", "secret", { required: true, grouped: true, fill: "confirm", section: "Card" }),
        f("card.holder", "Name on card", "text", { section: "Card" }),
        f("card.expiry", "Expires", "month", { section: "Card" }),
        f("card.cvc", "Security code", "secret", { section: "Card" }),
        f("card.type", "Brand", "text"),
        NOTES,
      ],
      { primary: "card.number", subtitle: "card.holder" },
    ),
    t("note", "Note", [f("body", "Note", "multiline", { section: "Note" })]),
    t(
      "identity",
      "Passport",
      [
        f("passport.number", "Passport number", "secret", { required: true, section: "Passport" }),
        f("passport.country", "Issuing country", "text", { section: "Passport" }),
        f("expires_at", "Expires", "date", { section: "Passport" }),
        NOTES,
      ],
      { variant: "passport", icon: "flight", primary: "passport.number", subtitle: "passport.country" },
    ),
    t("custom", "Custom item", [NOTES], { autofill: false }),
    t(
      "crypto-wallet",
      "Crypto wallet",
      [f("seed", "Recovery phrase", "secret-multiline", { required: true, crown_jewel: true, section: "Wallet" }), f("wallet.network", "Network", "text"), NOTES],
      { crown_jewel: true, autofill: false },
    ),
    t("passkey", "Passkey", [f("passkey.0.rp_id", "Website", "text", { section: "Passkey" }), f("passkey.0.key", "Private key", "passkey", { section: "Passkey" })], {
      creatable: false,
    }),
  ],
  custom_kinds: [
    { kind: "text", label: "Text" },
    { kind: "hidden", label: "Secret" },
    { kind: "date", label: "Date" },
    { kind: "totp", label: "One-time code" },
    { kind: "linked", label: "Link to another item" },
  ],
}
