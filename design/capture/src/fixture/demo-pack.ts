// The developer demo's items (docs/plan/40-devtools.md §3), read from the
// default pack itself (crates/devtools/demo/default-pack.json) into the
// fixture's item shape, so the harness draws what the demo holds. The
// reading follows Lockdown's CXF import (crates/cxf/src/convert.rs
// `to_item`): the item's credentials, then the Lockdown extension's
// fields. Keys the demo makes when it opens (passkeys, the SSH key) are
// drawn as made-up stand-ins here.

import type { AttachmentView, FolderView } from "@/core/types"

import packText from "../../../../crates/devtools/demo/default-pack.json?raw"
import { uid } from "./ids"
import type { SeedCustom, SeedItem, SeedPasskey } from "./seed"
import { NOW } from "./time"

type Value = { text?: string; flag?: boolean; url?: string }
type ExtField = { id: string; kind: string; label: string; section?: string; value: Value }
type Editable = { value: string }
type Credential = Record<string, unknown> & { type: string }
interface CxfItem {
  id: string
  title: string
  favorite?: boolean
  tags?: string[]
  modifiedAt?: number
  scope?: { urls: string[] }
  credentials: Credential[]
  extensions?: { name: string; kind?: string; fields: ExtField[] }[]
}
interface Collection {
  title: string
  items: { item: string }[]
}
interface Extras {
  vault?: string
  crown_jewel?: boolean
  state?: "live" | "archived" | "trashed"
  files?: { name: string; mime: string; text?: string; size?: number }[]
  fresh?: string[]
}
interface Pack {
  name: string
  cxf: { accounts: { items: CxfItem[]; collections: Collection[] }[] }
  items?: Record<string, Extras>
}

export const DEMO_PACK = JSON.parse(packText) as Pack

const v = (c: Credential, key: string): string | undefined => (c[key] as Editable | undefined)?.value

/** CXF's Wi-Fi words back to the template's (`WIFI_TYPES`). */
const WIFI: Record<string, string> = { "wpa3-personal": "WPA3", "wpa2-personal": "WPA2", "wpa-personal": "WPA", wep: "WEP", unsecured: "Open" }

/** Each credential's fields, by the template ids the import gives them. */
function credentialFields(c: Credential, out: Record<string, string>, files: AttachmentView[]) {
  const put = (id: string, value: string | undefined) => {
    if (value !== undefined) out[id] = value
  }
  switch (c.type) {
    case "basic-auth":
      put("user", v(c, "username"))
      put("pw", v(c, "password"))
      break
    case "totp": {
      const n = Object.keys(out).filter((k) => k.startsWith("code.")).length
      const issuer = String(c.issuer ?? "")
      const user = String(c.username ?? "")
      out[`code.${n}`] =
        `otpauth://totp/${encodeURIComponent(issuer)}:${encodeURIComponent(user)}?secret=${c.secret}&issuer=${encodeURIComponent(issuer)}&digits=${c.digits}&period=${c.period}`
      break
    }
    case "note":
      put("body", v(c, "content"))
      break
    case "credit-card":
      put("card.number", v(c, "number"))
      put("card.holder", v(c, "fullName"))
      put("card.type", v(c, "cardType"))
      put("card.cvc", v(c, "verificationNumber"))
      put("card.pin", v(c, "pin"))
      put("card.expiry", v(c, "expiryDate"))
      break
    case "person-name":
      for (const k of ["title", "given", "middle", "family", "generation"]) put(`name.${k}`, v(c, k))
      break
    case "address":
      put("address.street", v(c, "streetAddress"))
      put("address.city", v(c, "city"))
      put("address.territory", v(c, "territory"))
      put("address.country", v(c, "country"))
      put("address.postal_code", v(c, "postalCode"))
      put("address.tel", v(c, "tel"))
      break
    case "wifi":
      put("wifi.ssid", v(c, "ssid"))
      put("wifi.passphrase", v(c, "passphrase"))
      put("wifi.security", WIFI[v(c, "networkSecurityType") ?? ""])
      if (v(c, "hidden") === "true") out["wifi.hidden"] = "true"
      break
    case "passport":
      put("passport.country", v(c, "issuingCountry"))
      put("passport.type", v(c, "passportType"))
      put("passport.number", v(c, "passportNumber"))
      put("passport.national_id", v(c, "nationalIdentificationNumber"))
      put("passport.nationality", v(c, "nationality"))
      put("passport.birth_date", v(c, "birthDate"))
      put("passport.birth_place", v(c, "birthPlace"))
      put("passport.sex", v(c, "sex"))
      put("passport.issued", v(c, "issueDate"))
      put("expires_at", v(c, "expiryDate"))
      put("passport.authority", v(c, "issuingAuthority"))
      break
    case "drivers-license":
      put("dl.birth_date", v(c, "birthDate"))
      put("dl.issued", v(c, "issueDate"))
      put("expires_at", v(c, "expiryDate"))
      put("dl.authority", v(c, "issuingAuthority"))
      put("dl.territory", v(c, "territory"))
      put("dl.country", v(c, "country"))
      put("dl.number", v(c, "licenseNumber"))
      put("dl.class", v(c, "licenseClass"))
      break
    case "identity-document":
      put("id.country", v(c, "issuingCountry"))
      put("id.number", v(c, "documentNumber"))
      put("id.personal_number", v(c, "identificationNumber"))
      put("id.nationality", v(c, "nationality"))
      put("id.birth_date", v(c, "birthDate"))
      put("id.issued", v(c, "issueDate"))
      put("expires_at", v(c, "expiryDate"))
      put("id.authority", v(c, "issuingAuthority"))
      break
    case "file":
      files.push({ id: uid("attachment", 900 + files.length), name: String(c.name), mime: "application/octet-stream", size: Number(c.decryptedSize ?? 0) })
      break
  }
}

/** The kind the import gives an item with no Lockdown extension. */
function kindOf(creds: Credential[]): string {
  const has = (t: string) => creds.some((c) => c.type === t)
  if (has("wifi")) return "wi-fi"
  if (has("basic-auth") || has("passkey")) return "login"
  switch (creds[0]?.type) {
    case "credit-card":
      return "card"
    case "note":
      return "note"
    case "totp":
      return "code"
    case "person-name":
    case "address":
    case "drivers-license":
    case "passport":
    case "identity-document":
      return "identity"
    default:
      return "custom"
  }
}

const mimeOf = (name: string) => (name.endsWith(".txt") ? "text/plain" : name.endsWith(".pdf") ? "application/pdf" : "application/octet-stream")

/** The pack's items in the fixture's shape, numbered from `first`. */
export function demoItems(first = 1000): SeedItem[] {
  const account = DEMO_PACK.cxf.accounts[0]!
  const folderOf = new Map<string, FolderView>()
  account.collections.forEach((c, i) => {
    const folder: FolderView = { id: uid("folder", 900 + i), name: c.title.split("/").pop() ?? c.title, parent: null, path: c.title.replaceAll("/", " / ") }
    for (const l of c.items) folderOf.set(l.item, folder)
  })
  return account.items.map((item, i) => {
    const extras = DEMO_PACK.items?.[item.id] ?? {}
    const ext = item.extensions?.find((e) => e.name === "design.snds.lockdown.fields")
    const fields: Record<string, string> = {}
    const attachments: AttachmentView[] = []
    item.scope?.urls.forEach((u, n) => (fields[`url.${n}`] = u))
    for (const c of item.credentials) credentialFields(c, fields, attachments)
    const custom: SeedCustom[] = []
    const passkeys: SeedPasskey[] = []
    let template: string | undefined
    for (const f of ext?.fields ?? []) {
      const value = f.value.text ?? f.value.url ?? (f.value.flag === undefined ? "" : String(f.value.flag))
      if (f.id === "lockdown.template") template = value
      else if (f.id.startsWith("custom.")) custom.push({ field_id: f.id, label: f.label, kind: f.kind, value })
      else if (f.id.startsWith("passkey.")) {
        const [, n, part] = f.id.split(".")
        const k = Number(n)
        passkeys[k] ??= { credential_id: uid("credential", 900 + i * 4 + k).replace(/-/g, "").slice(0, 22), rp_id: "", user_name: "", user_display_name: "", provider: "Lockdown", created: 0, last_used: null }
        if (part === "rp_id" || part === "user_name" || part === "user_display_name") passkeys[k][part] = value
      } else fields[f.id] = value
    }
    const kind = ext?.kind ?? kindOf(item.credentials)
    if (extras.fresh?.includes("ssh-key")) {
      fields["ssh.private_key"] = "-----BEGIN OPENSSH PRIVATE KEY-----\n(made when the demo opens)\n-----END OPENSSH PRIVATE KEY-----"
      fields["ssh.public_key"] = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDemoMadeWhenTheDemoOpensNotARealKey Build-server-key@demo.test"
      fields["ssh.fingerprint"] = "SHA256:DemoMadeWhenTheDemoOpens0000000000000000000"
    }
    for (const f of extras.files ?? []) {
      attachments.push({ id: uid("attachment", 950 + i), name: f.name, mime: f.mime || mimeOf(f.name), size: f.size ?? new TextEncoder().encode(f.text ?? "").length })
    }
    const edited = Math.max(0, Math.round((NOW - (item.modifiedAt ?? NOW)) / 86_400))
    const seed: SeedItem = {
      n: first + i,
      kind,
      ...(template ? { template: `${kind}/${template}` } : {}),
      title: item.title,
      vault: extras.vault === "Work" ? "work" : "personal",
      ...(folderOf.get(item.id) ? { folder: folderOf.get(item.id) } : {}),
      tags: item.tags ?? [],
      favourite: item.favorite ?? false,
      state: extras.state ?? "live",
      crown: extras.crown_jewel ?? kind === "crypto-wallet",
      edited,
      fields,
      ...(custom.length ? { custom } : {}),
      ...(passkeys.length ? { passkeys: passkeys.filter(Boolean) } : {}),
      ...(attachments.length ? { attachments } : {}),
    }
    return seed
  })
}
