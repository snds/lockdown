// The one fixture every surface draws from: Sam Rivera's vaults. Every
// person, site, number and key here is invented. Sites use `.example`
// domains or brands that do not exist; card numbers are the networks'
// published test numbers; keys and phrases are random words and hex.

import type { Access, AttachmentView, AwayView, FolderView } from "@/core/types"

import { uid } from "./ids"
import { ago, ahead, dateIn, monthIn } from "./time"

export const PERSON = {
  name: "Sam Rivera",
  login: "sam@rivera.example",
  server: "https://vault.rivera.example",
  device: "Sam's MacBook Pro",
  accountId: uid("account", 1),
}

export type VaultKey = "personal" | "family" | "work"

export interface SeedVault {
  key: VaultKey
  id: string
  name: string
  primary: boolean
  trash_days: number
  folders: FolderView[]
}

const folder = (n: number, name: string, parent: FolderView | null = null): FolderView => ({
  id: uid("folder", n),
  name,
  parent: parent?.id ?? null,
  path: parent ? `${parent.path} / ${name}` : name,
})

const finance = folder(1, "Finance")
const taxes = folder(2, "Taxes", finance)
const travel = folder(3, "Travel")
const gaming = folder(4, "Gaming")
const home = folder(5, "Home")
const kids = folder(6, "Kids")
const infra = folder(7, "Infrastructure")
const clients = folder(8, "Clients")

export const FOLDERS = { finance, taxes, travel, gaming, home, kids, infra, clients }

export const VAULTS: SeedVault[] = [
  { key: "personal", id: uid("vault", 1), name: "Personal", primary: true, trash_days: 30, folders: [finance, taxes, travel, gaming] },
  { key: "family", id: uid("vault", 2), name: "Family", primary: false, trash_days: 30, folders: [home, kids] },
  { key: "work", id: uid("vault", 3), name: "Work", primary: false, trash_days: 60, folders: [infra, clients] },
]

export const vaultId = (key: VaultKey) => VAULTS.find((v) => v.key === key)!.id

export interface SeedPasskey {
  credential_id: string
  rp_id: string
  user_name: string
  user_display_name: string
  provider: string
  created: number
  last_used: number | null
  hidden?: { reason: string; at: number }
  backed_up?: boolean
}

/** A field added by hand (`custom.N`). */
export interface SeedCustom {
  field_id: string
  label: string
  kind: string
  value: string
}

export interface SeedItem {
  n: number
  kind: string
  /** A variant's template id (`identity/passport`); the kind's own when left out. */
  template?: string
  title: string
  vault: VaultKey
  folder?: FolderView
  tags?: string[]
  favourite?: boolean
  state?: "live" | "trashed" | "archived"
  crown?: boolean
  /** Days since the last change. */
  edited: number
  /** Days since this device last used it. */
  used?: number
  uses?: number
  access?: Access
  /** Field id → value, as typed. Which are secrets comes from the template. */
  fields: Record<string, string>
  /** A web address's match rule, by field id. */
  rules?: Record<string, string>
  custom?: SeedCustom[]
  passkeys?: SeedPasskey[]
  attachments?: AttachmentView[]
  away?: AwayView[]
  reprompt?: boolean
  exportOptIn?: boolean
  /** Signed by the bridge manifest (none in the vault model yet; the seam exists). */
  tier?: "standard" | "official" | "tolerated"
}

const totp = (issuer: string, account: string, secret: string, extra = "") =>
  `otpauth://totp/${encodeURIComponent(issuer)}:${encodeURIComponent(account)}?secret=${secret}&issuer=${encodeURIComponent(issuer)}${extra}`

const pk = (n: number, rp_id: string, user: string, display: string, created: number, used: number | null, over: Partial<SeedPasskey> = {}): SeedPasskey => ({
  credential_id: uid("credential", n).replace(/-/g, "").slice(0, 22),
  rp_id,
  user_name: user,
  user_display_name: display,
  provider: "Lockdown",
  created: ago(created),
  last_used: used === null ? null : ago(used),
  backed_up: true,
  ...over,
})

const att = (n: number, name: string, mime: string, size: number): AttachmentView => ({ id: uid("attachment", n), name, mime, size })

/** About forty items across every kind, in three vaults. */
export const ITEMS: SeedItem[] = [
  // --- logins ---------------------------------------------------------------
  {
    n: 1,
    kind: "login",
    title: "Northwind Mail",
    vault: "personal",
    tags: ["2fa", "email"],
    favourite: true,
    edited: 12,
    used: 0.1,
    uses: 214,
    fields: {
      user: "sam.rivera@northwind.example",
      pw: "tidal-Vespers-0rbit-48-quietly",
      "code.0": totp("Northwind", "sam.rivera@northwind.example", "JBSWY3DPEHPK3PXP"),
      "url.0": "https://mail.northwind.example",
      "url.1": "https://accounts.northwind.example",
      notes: "Recovery mailbox for most other accounts. Recovery codes are in their own item.",
    },
    rules: { "url.0": "base_domain", "url.1": "exact_host" },
    passkeys: [pk(1, "northwind.example", "sam.rivera@northwind.example", "Sam Rivera", 200, 3)],
  },
  {
    n: 2,
    kind: "login",
    title: "Bluefin Bank",
    vault: "personal",
    folder: finance,
    tags: ["finance"],
    favourite: true,
    edited: 41,
    used: 2,
    uses: 63,
    fields: {
      user: "samrivera",
      pw: "Correct-Lantern-Harbor-91!",
      "code.0": totp("Bluefin Bank", "samrivera", "KRSXG5CTMVRXEZLU"),
      "url.0": "https://secure.bluefin-bank.example",
    },
    rules: { "url.0": "exact_host" },
  },
  {
    n: 3,
    kind: "login",
    title: "Parcelpost",
    vault: "personal",
    tags: ["shopping"],
    edited: 400,
    used: 9,
    uses: 17,
    fields: {
      user: "sam.rivera@northwind.example",
      pw: "summer2019parcel",
      "url.0": "https://parcelpost.example",
      "url.1": "https://track.parcelpost.example",
      "url.2": "https://m.parcelpost.example/app",
    },
    rules: { "url.0": "base_domain", "url.1": "exact_host", "url.2": "starts_with" },
  },
  {
    n: 4,
    kind: "login",
    title: "Pixelforge",
    vault: "personal",
    folder: gaming,
    tags: ["gaming"],
    edited: 30,
    used: 1,
    uses: 40,
    fields: { user: "sam_r", "url.0": "https://pixelforge.example", sign_in_method: "Passkey" },
    passkeys: [
      pk(2, "pixelforge.example", "sam_r", "Sam R.", 90, 1),
      pk(3, "pixelforge.example", "sam_r", "Sam R. (old laptop)", 400, 220, { hidden: { reason: "The site said it no longer knows this passkey.", at: ago(5) } }),
    ],
  },
  {
    n: 5,
    kind: "login",
    title: "Meadowlark Streaming",
    vault: "family",
    folder: home,
    tags: ["shared"],
    edited: 220,
    used: 4,
    uses: 88,
    fields: { user: "riverafamily@northwind.example", pw: "meadow123", "url.0": "https://watch.meadowlark.example" },
    away: [{ place: "Hotel TV in Lisbon", at: ago(6), remind_at: ago(1), due: true }],
  },
  {
    n: 6,
    kind: "login",
    title: "Harbor Health",
    vault: "personal",
    edited: 95,
    used: 30,
    uses: 6,
    fields: { user: "sam.rivera@northwind.example", pw: "Gl@cier-Mint-Ferry-2207", "url.0": "https://portal.harborhealth.example" },
  },
  {
    n: 7,
    kind: "login",
    title: "Tern Airlines",
    vault: "personal",
    folder: travel,
    tags: ["travel"],
    edited: 3,
    used: 3,
    uses: 12,
    fields: { user: "SRIVERA4471", pw: "Ferry-Cobalt-Wren-6610", "url.0": "https://www.ternair.example" },
  },
  {
    // A conflict copy (D58): edited on two devices before either synced.
    n: 8,
    kind: "login",
    title: "Tern Airlines (conflict copy from Sam's phone)",
    vault: "personal",
    folder: travel,
    tags: ["travel"],
    edited: 2.9,
    fields: { user: "SRIVERA4471", pw: "Ferry-Cobalt-Wren-6611", "url.0": "https://www.ternair.example", notes: "Changed the password at the airport." },
  },
  {
    n: 9,
    kind: "login",
    title: "Quill Notes",
    vault: "personal",
    edited: 380,
    used: 60,
    uses: 4,
    fields: { user: "sam.rivera@northwind.example", pw: "summer2019parcel", "url.0": "https://app.quillnotes.example" },
  },
  {
    // A long title and long values: the stress row.
    n: 10,
    kind: "login",
    title: "Riverside Community Gardening Association: Members Portal for Volunteer Scheduling and Plot Allocation (2026 season)",
    vault: "family",
    folder: home,
    edited: 18,
    used: 18,
    uses: 2,
    fields: {
      user: "samantha.alexandra.rivera-delacroix.garden.volunteer.coordinator@riverside-community-gardening-association.example",
      pw: "Plot-Seventeen-Compost-Rotation-Tuesday-Mornings-Only-2026",
      "url.0": "https://members.riverside-community-gardening-association.example/volunteers/scheduling/plots/allocation?season=2026&view=calendar",
      notes:
        "Plot 17, north bed. Water on Tuesdays and Fridays before 9 am. The shed code changes every season; ask the coordinator. Tools go back in the blue cabinet, never the green one, because the green one floods when it rains hard.",
    },
  },
  {
    n: 11,
    kind: "login",
    title: "Cobalt Cloud console",
    vault: "work",
    folder: infra,
    tags: ["work/internal"],
    edited: 7,
    used: 0.5,
    uses: 130,
    fields: {
      user: "sam@rivera-studio.example",
      pw: "Anvil-Juniper-Static-3391",
      "code.0": totp("Cobalt Cloud", "sam@rivera-studio.example", "MFRGGZDFMZTWQ2LK"),
      "url.0": "https://console.cobaltcloud.example",
    },
  },
  {
    n: 12,
    kind: "login",
    title: "Stackwise",
    vault: "work",
    folder: clients,
    tags: ["work/clients"],
    edited: 50,
    used: 14,
    uses: 9,
    fields: { user: "sam@rivera-studio.example", pw: "Velvet-Orchard-Kite-5820", "url.0": "https://stackwise.example", sign_in_method: "Another account", sign_in_provider: "Northwind" },
  },
  {
    n: 13,
    kind: "login",
    title: "Kids' school portal",
    vault: "family",
    folder: kids,
    edited: 25,
    used: 10,
    uses: 15,
    access: "view",
    fields: { user: "rivera.parent", pw: "Crayon-Maple-Bus-7731", "url.0": "https://parents.elmwood-school.example" },
  },
  {
    n: 14,
    kind: "login",
    title: "Old gym account",
    vault: "personal",
    state: "trashed",
    edited: 8,
    fields: { user: "samr", pw: "gym-2017-gym", "url.0": "https://members.ironpeak-gym.example" },
  },
  {
    // Saved from a link in an email: its address imitates the bank's.
    n: 43,
    kind: "login",
    title: "Bluefin Bank alerts",
    vault: "personal",
    folder: finance,
    edited: 11,
    fields: { user: "samrivera", pw: "Hollow-Prism-Dune-1184", "url.0": "https://secure.b1uefin-bank.example/alerts" },
  },
  // --- codes ----------------------------------------------------------------
  {
    n: 15,
    kind: "code",
    title: "Kestrel Arcade · Steam Guard",
    vault: "personal",
    folder: gaming,
    tags: ["gaming"],
    edited: 60,
    used: 0.3,
    uses: 77,
    fields: { "code.0": "otpauth://steam/Kestrel%20Arcade:sam_rivera?secret=NB2W45DFOIZA&issuer=Kestrel%20Arcade&encoder=steam&digits=5" },
  },
  {
    n: 16,
    kind: "code",
    title: "Lockdown server",
    vault: "personal",
    edited: 140,
    uses: 3,
    fields: { "code.0": totp("vault.rivera.example", "sam@rivera.example", "GEZDGNBVGY3TQOJQ") },
  },
  {
    n: 17,
    kind: "code",
    title: "Driftwood Forum",
    vault: "personal",
    edited: 300,
    used: 45,
    uses: 5,
    fields: { "code.0": totp("Driftwood", "sam_r", "ORSXG5BAMNXWIZJA", "&digits=8") },
  },
  {
    n: 18,
    kind: "code",
    title: "Office VPN token",
    vault: "work",
    folder: infra,
    edited: 33,
    used: 1,
    uses: 210,
    fields: { "code.0": "otpauth://hotp/Rivera%20Studio:sam?secret=OBQXG43XN5ZGIZLE&issuer=Rivera%20Studio&counter=212" },
  },
  // --- passkey made by a site, on its own -----------------------------------
  {
    n: 19,
    kind: "passkey",
    title: "Lumen Photo forum",
    vault: "personal",
    edited: 70,
    used: 20,
    uses: 11,
    fields: {
      "passkey.0.rp_id": "forum.lumenphoto.example",
      "passkey.0.user_name": "samrivera",
      "passkey.0.user_display_name": "Sam Rivera",
      "passkey.0.credential_id": "AbF3kQ9xLm2vR8sT1wYz4Q",
      "passkey.0.user_handle": "dXNlci0zODQ5Mg",
      "passkey.0.key": "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg",
    },
    passkeys: [pk(4, "forum.lumenphoto.example", "samrivera", "Sam Rivera", 70, 20)],
  },
  // --- cards and bank ---------------------------------------------------------
  {
    n: 20,
    kind: "card",
    title: "Bluefin Visa",
    vault: "personal",
    folder: finance,
    edited: 200,
    used: 6,
    uses: 31,
    reprompt: true,
    fields: { "card.number": "4111 1111 1111 1111", "card.holder": "SAM RIVERA", "card.expiry": monthIn(1), "card.cvc": "123", "card.pin": "4096", "card.type": "Visa" },
  },
  {
    n: 21,
    kind: "card",
    title: "Family Mastercard",
    vault: "family",
    edited: 90,
    used: 12,
    uses: 19,
    fields: { "card.number": "5555 5555 5555 4444", "card.holder": "ALEX RIVERA", "card.expiry": monthIn(26), "card.cvc": "737", "card.type": "Mastercard" },
  },
  {
    n: 22,
    kind: "bank-account",
    title: "Bluefin checking",
    vault: "personal",
    folder: finance,
    edited: 500,
    fields: {
      "bank.name": "Bluefin Bank",
      "bank.holder": "Sam Rivera",
      "bank.account_number": "0042 7781 9930",
      "bank.routing": "011000015",
      "bank.iban": "GB33 BUKB 2020 1555 5555 55",
      "bank.swift": "BLFNUS33",
      "bank.branch": "Harbourfront",
    },
  },
  // --- Wi-Fi ---------------------------------------------------------------------
  {
    n: 23,
    kind: "wi-fi",
    title: "Home Wi-Fi",
    vault: "family",
    folder: home,
    favourite: true,
    edited: 150,
    used: 40,
    uses: 9,
    fields: { "wifi.ssid": "Rivera-Home", "wifi.passphrase": "kettle7 harbor2 maple9 window4", "wifi.security": "WPA3" },
  },
  {
    n: 24,
    kind: "wi-fi",
    title: "Guest Wi-Fi",
    vault: "family",
    folder: home,
    edited: 150,
    used: 2,
    uses: 14,
    fields: { "wifi.ssid": "Rivera-Guest", "wifi.passphrase": "sunny-porch-42", "wifi.security": "WPA2/WPA3" },
  },
  {
    n: 25,
    kind: "wi-fi",
    template: "wi-fi/router",
    title: "Home router",
    vault: "family",
    folder: home,
    edited: 365,
    fields: {
      "wifi.ssid": "Rivera-Home",
      "wifi.passphrase": "kettle7 harbor2 maple9 window4",
      "wifi.security": "WPA3",
      "router.admin": "http://192.168.1.1",
      user: "admin",
      pw: "Beacon-Lattice-0219",
      "router.model": "Fieldstone AX3000",
    },
  },
  {
    n: 26,
    kind: "wi-fi",
    title: "Previous apartment Wi-Fi",
    vault: "personal",
    state: "archived",
    edited: 700,
    fields: { "wifi.ssid": "Unit4B", "wifi.passphrase": "elevator-moss-11", "wifi.security": "WPA2" },
  },
  // --- notes -------------------------------------------------------------------
  {
    n: 27,
    kind: "note",
    title: "Spare keys and the alarm",
    vault: "family",
    folder: home,
    edited: 21,
    fields: {
      body: "# Spare keys\n\n* Front door: with Jordan, and in the lockbox by the shed\n* Car: kitchen drawer, left side\n\n# The alarm\n\nThe panel is behind the coat rack. **Hold** the star key for three seconds to silence a false alarm, then call the monitoring line on the sticker.",
    },
  },
  {
    n: 28,
    kind: "note",
    title: "Passport renewal checklist",
    vault: "personal",
    folder: travel,
    tags: ["travel"],
    edited: 5,
    fields: { body: "1. Photo booth at the pharmacy\n2. Form from the consulate site\n3. Old passport goes in the envelope too\n\nBook the appointment before the summer rush." },
  },
  // --- identities ----------------------------------------------------------------
  {
    n: 29,
    kind: "identity",
    title: "Sam Rivera",
    vault: "personal",
    edited: 80,
    used: 7,
    uses: 23,
    fields: {
      "name.given": "Sam",
      "name.family": "Rivera",
      "identity.email": "sam.rivera@northwind.example",
      "address.tel": "+1 555 0142",
      "address.street": "48 Alder Lane",
      "address.city": "Port Elm",
      "address.territory": "WA",
      "address.postal_code": "98000",
      "address.country": "United States",
    },
  },
  {
    n: 30,
    kind: "identity",
    template: "identity/passport",
    title: "Passport",
    vault: "personal",
    folder: travel,
    edited: 1800,
    fields: {
      "passport.number": "X1234567",
      "passport.type": "P",
      "passport.country": "United States",
      "passport.authority": "Passport Office",
      "passport.issued": dateIn(-3600),
      expires_at: dateIn(52),
      "name.given": "Sam",
      "name.family": "Rivera",
      "passport.nationality": "American",
      "passport.birth_date": "1988-04-17",
      "passport.birth_place": "Port Elm",
      "passport.sex": "X",
    },
    attachments: [att(1, "passport-photo-page.jpg", "image/jpeg", 412_008)],
  },
  {
    n: 31,
    kind: "identity",
    template: "identity/contact",
    title: "Alex Rivera",
    vault: "family",
    edited: 300,
    fields: { "name.given": "Alex", "name.family": "Rivera", "identity.email": "alex.rivera@northwind.example", "address.tel": "+1 555 0177", "contact.birthday": "1987-09-02" },
  },
  // --- documents -------------------------------------------------------------------
  {
    n: 32,
    kind: "document",
    template: "document/insurance",
    title: "Home insurance",
    vault: "family",
    folder: home,
    edited: 40,
    fields: {
      "insurance.type": "Home",
      "insurance.insurer": "Hearthstone Mutual",
      "insurance.policy_number": "HM-2210-88413",
      "insurance.holder": "Sam and Alex Rivera",
      "insurance.start": dateIn(-325),
      expires_at: dateIn(40),
      "insurance.premium": "$1,240 a year",
      "insurance.phone": "+1 555 0190",
      "insurance.url": "https://claims.hearthstone-mutual.example",
    },
    attachments: [att(2, "policy-2026.pdf", "application/pdf", 1_830_512), att(3, "house-inventory.xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", 96_220)],
  },
  {
    n: 33,
    kind: "document",
    template: "document/recovery-codes",
    title: "Northwind Mail recovery codes",
    vault: "personal",
    crown: true,
    edited: 200,
    fields: {
      "recovery.account": "sam.rivera@northwind.example",
      recovery_codes: "8f3k-29dm\nq7xp-41ra\nn2vb-88tc\nw6jh-03ky\nz9lp-57es\nc4rt-62mu",
      "recovery.made": dateIn(-200),
    },
  },
  // --- keys ----------------------------------------------------------------------------
  {
    n: 34,
    kind: "ssh-key",
    title: "Studio deploy key",
    vault: "work",
    folder: infra,
    tags: ["work/internal"],
    edited: 120,
    fields: {
      "ssh.private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n-----END OPENSSH PRIVATE KEY-----",
      "ssh.public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIK0wmN/Cr3JXqmLW7u+g9pTh+5M deploy@rivera-studio",
      "ssh.fingerprint": "SHA256:Xq3v9mJkP0w2rT8sLzYbN4cH6dF1gA5eK7uR2oQ9iWc",
      "ssh.key_type": "Ed25519",
      "ssh.hosts": "build.rivera-studio.example\ngit.rivera-studio.example",
    },
  },
  {
    n: 35,
    kind: "ssh-key",
    template: "ssh-key/pgp",
    title: "Personal signing key",
    vault: "personal",
    edited: 600,
    fields: {
      "pgp.private_key": "-----BEGIN PGP PRIVATE KEY BLOCK-----\nlFgEZo3x0hYJKwYBBAHaRw8BAQdA9kqT\n-----END PGP PRIVATE KEY BLOCK-----",
      "pgp.public_key": "-----BEGIN PGP PUBLIC KEY BLOCK-----\nmDMEZo3x0hYJKwYBBAHaRw8BAQdA9kqT\n-----END PGP PUBLIC KEY BLOCK-----",
      "pgp.fingerprint": "7A3C 91D4 0B2E 55F8 C6A1  3E90 D4B7 2F61 08CA 9E35",
      "pgp.user_id": "Sam Rivera <sam@rivera.example>",
      "pgp.algorithm": "Ed25519",
      expires_at: dateIn(400),
    },
  },
  {
    n: 36,
    kind: "api-credential",
    title: "Cobalt Cloud API token",
    vault: "work",
    folder: infra,
    tags: ["work/internal"],
    edited: 80,
    fields: { "api.key": "cc_live_7Hq2Lm9Vx4Rt8Wp1Zs6Kb3Nd", "api.type": "Bearer token", "api.host": "api.cobaltcloud.example", "api.scopes": "deploy:write logs:read", expires_at: dateIn(9) },
  },
  // --- the crown jewel -------------------------------------------------------------------
  {
    n: 37,
    kind: "crypto-wallet",
    title: "Cold wallet",
    vault: "personal",
    folder: finance,
    crown: true,
    edited: 260,
    fields: {
      seed: "orbit lantern velvet maple ferry quartz anchor willow ember cobalt harbor tidal",
      "wallet.password": "Spindle-Opal-Thaw-7782",
      "wallet.name": "Ledgerline Vault",
      "wallet.network": "Bitcoin",
      "wallet.address": "bc1qexample7xk2m9t0w4q8r3n5v6c7z8l2p0j4h6g",
    },
  },
  // --- the rest ---------------------------------------------------------------------------------
  {
    n: 38,
    kind: "server",
    title: "Build server",
    vault: "work",
    folder: infra,
    edited: 15,
    used: 1,
    uses: 52,
    fields: { "server.host": "build.rivera-studio.example", "server.port": "22", user: "deploy", pw: "Granite-Owl-Sprocket-4410", "server.admin": "https://build.rivera-studio.example/admin" },
  },
  {
    n: 39,
    kind: "database",
    title: "Analytics database",
    vault: "work",
    folder: clients,
    edited: 22,
    fields: { "db.type": "PostgreSQL", "db.host": "db.rivera-studio.example", "db.port": "5432", "db.name": "analytics", user: "report_reader", pw: "Lichen-Bramble-Pylon-2093" },
  },
  {
    n: 40,
    kind: "software-licence",
    title: "Lumen Photo Editor",
    vault: "personal",
    edited: 410,
    fields: { "licence.key": "LMN-7Q4K-PX92-HB3T-ZW8R", "licence.licensed_to": "Sam Rivera", "licence.email": "sam.rivera@northwind.example", "licence.version": "6", "licence.publisher": "Lumen Labs", "licence.purchased": dateIn(-410) },
  },
  {
    n: 41,
    kind: "membership",
    title: "Trailhead Climbing Gym",
    vault: "personal",
    edited: 100,
    used: 3,
    uses: 40,
    fields: { "member.organization": "Trailhead Climbing Gym", "member.name": "Sam Rivera", "member.number": "TH-00418", "member.pin": "5521", "member.since": dateIn(-900), expires_at: dateIn(120) },
  },
  {
    n: 42,
    kind: "custom",
    title: "Bike lock",
    vault: "personal",
    edited: 33,
    fields: { notes: "The blue U-lock in the garage." },
    custom: [
      { field_id: "custom.0", label: "Combination", kind: "hidden", value: "3-9-1-4" },
      { field_id: "custom.1", label: "Frame number", kind: "text", value: "WTU 118 C4471" },
    ],
  },
]

/** Facts that change with time but are part of the story. */
export const STORY = {
  lastSync: ago(0.01),
  enrolled: ago(400),
  kitMade: ago(380),
  lastDrill: ago(150),
  nextBackup: ahead(0.2),
}
