// The fixture in the shape the design's frames draw (foundation): items,
// requests and a long vault that a state opts into with `setup` or
// `answers`. Nothing here changes the seed, so the states that don't ask
// for it draw what they always drew. Every brand is invented and every site
// a `.example` (the harness's rule): the design's own sample names are never
// copied in.
//
// What the design draws, and where it is:
//   a login with a password, a passkey, a five-character code
//     and a protected recovery note ......... SHAPE.login (withShape)
//   a login that is a passkey and nothing else  SHAPE.passkeyLogin (withShape)
//   a card, for the card art .................. SHAPE.card (the seed's)
//   a secure note with a network and a password SHAPE.note (withShape)
//   a code seconds from expiry ................ expiringCode
//   findings: weak, reused, no second step .... the seed's report (story.ts)
//   two vaults with folders and tags .......... the seed's Personal and Family
//   a long title .............................. SHAPE.longTitle (the seed's)
//   a passkey request and a pairing request ... PASSKEY_REQUEST, PAIRING_REQUEST
//   a vault of 300 rows ....................... longVault
//   the clock ................................. pinned by the capture (time.ts)

import type { PendingPairing } from "@/features/browsers/browser-client"
import type { PasskeyApproval } from "@/features/passkeys/passkey-client"

import type { Handler } from "../host/core"
import { uid } from "./ids"
import { FOLDERS, type SeedItem, type SeedPasskey } from "./seed"
import { ago } from "./time"
import { codesOf, type WItem, type World } from "./world"

/** The titles a state opens: one name for each thing the design draws. */
export const SHAPE = {
  login: "Harrier Games",
  passkeyLogin: "Larkspur Docs",
  card: "Bluefin Visa",
  note: "Cabin network",
  longTitle: "Riverside Community Gardening Association: Members Portal for Volunteer Scheduling and Plot Allocation (2026 season)",
} as const

const passkey = (n: number, rp_id: string, user: string, display: string, created: number, used: number): SeedPasskey => ({
  credential_id: uid("credential", n).replace(/-/g, "").slice(0, 22),
  rp_id,
  user_name: user,
  user_display_name: display,
  provider: "Lockdown",
  created: ago(created),
  last_used: ago(used),
  backed_up: true,
})

/** The shaped items. Their numbers start at 900, clear of the seed's. */
const SHAPED: SeedItem[] = [
  {
    n: 900,
    kind: "login",
    title: SHAPE.login,
    vault: "personal",
    folder: FOLDERS.gaming,
    tags: ["2fa", "gaming"],
    favourite: true,
    edited: 9,
    used: 0.2,
    uses: 126,
    fields: {
      user: "sam_rivera",
      pw: "harbor-Lantern-0wl-73-softly",
      "code.0": "otpauth://steam/Harrier%20Games:sam_rivera?secret=MFZWIZLTMFZQ&issuer=Harrier%20Games&encoder=steam&digits=5",
      "url.0": "https://store.harriergames.example",
    },
    rules: { "url.0": "base_domain" },
    custom: [{ field_id: "custom.0", label: "Recovery note", kind: "hidden", value: "R47K2 · the code on the card in the desk drawer, second envelope" }],
    passkeys: [passkey(900, "harriergames.example", "sam_rivera", "Sam Rivera", 120, 2)],
  },
  {
    n: 901,
    kind: "login",
    title: SHAPE.passkeyLogin,
    vault: "personal",
    tags: ["work"],
    edited: 45,
    used: 1,
    uses: 58,
    fields: { user: "sam@rivera.example", "url.0": "https://docs.larkspur.example", sign_in_method: "Passkey" },
    passkeys: [passkey(901, "larkspur.example", "sam@rivera.example", "Sam Rivera", 45, 1)],
  },
  {
    n: 902,
    kind: "note",
    title: SHAPE.note,
    vault: "family",
    folder: FOLDERS.home,
    tags: ["travel"],
    edited: 33,
    used: 12,
    uses: 4,
    fields: { body: "The router is in the hall cupboard, top shelf. Guests use the same network.\n\nIf it drops, switch it off at the wall for a minute." },
    custom: [
      { field_id: "custom.0", label: "Network", kind: "text", value: "Rivera-Cabin" },
      { field_id: "custom.1", label: "Password", kind: "hidden", value: "pine4 ridge8 ember2 lake6" },
    ],
  },
]

const held = (s: SeedItem): WItem => ({ ...s, id: uid("item", s.n), version: Math.max(1, Math.round(s.edited / 40)) })

/** Adds the shaped items to the vault, ahead of nothing: the list sorts them in. */
export const withShape = (w: World) => {
  w.items = [...w.items, ...SHAPED.map(held)]
}

// --- a vault of 300 rows --------------------------------------------------------

const FIRST = ["Amber", "Birch", "Cinder", "Dapple", "Ember", "Fallow", "Garnet", "Hollow", "Indigo", "Juniper", "Kelp", "Lumen", "Marrow", "Nettle", "Ochre", "Pewter", "Quarry", "Rowan", "Sable", "Thistle", "Umber", "Vellum", "Willow", "Yarrow", "Zephyr"]
const SECOND = ["Atlas", "Bakery", "Cycles", "Dental", "Electric", "Freight", "Grocer", "Hardware", "Insurance", "Journal", "Kitchen", "Library"]
const slug = (s: string) => s.toLowerCase().replace(/[^a-z]+/g, "")

/** Row `i` of the long vault: mostly logins, with a card, a note, a network and a code now and then. */
function row(i: number): SeedItem {
  const title = `${FIRST[i % FIRST.length]} ${SECOND[Math.floor(i / FIRST.length) % SECOND.length]}`
  const host = `${slug(title)}.example`
  const base = { n: 1000 + i, title, vault: i % 5 === 4 ? ("family" as const) : ("personal" as const), edited: 1 + ((i * 7) % 400), used: (i * 3) % 90, uses: (i * 11) % 200 }
  if (i % 23 === 7) return { ...base, kind: "card", fields: { "card.number": "4111 1111 1111 1111", "card.holder": "SAM RIVERA", "card.expiry": "2029-08", "card.cvc": "123", "card.type": "Visa" } }
  if (i % 19 === 5) return { ...base, kind: "note", fields: { body: `Notes for ${title}.` } }
  if (i % 29 === 11) return { ...base, kind: "wi-fi", fields: { "wifi.ssid": slug(title), "wifi.passphrase": "maple4 harbor9 kettle2", "wifi.security": "WPA3" } }
  const fields: Record<string, string> = { user: i % 3 === 0 ? "sam.rivera@northwind.example" : "sam_rivera", pw: `long-Vault-${i}-row-password`, "url.0": `https://${host}` }
  if (i % 6 === 1) fields["code.0"] = `otpauth://totp/${encodeURIComponent(title)}:sam_rivera?secret=JBSWY3DPEHPK3PXP&issuer=${encodeURIComponent(title)}`
  return { ...base, kind: "login", favourite: i % 37 === 0, fields, ...(i % 9 === 2 ? { passkeys: [passkey(1000 + i, host, "sam_rivera", "Sam Rivera", 30 + i, i % 20)] } : {}) }
}

/** A vault of exactly 300 rows in place of the seed's, for long lists and their scrolling. */
export const longVault = (w: World) => {
  w.items = Array.from({ length: 300 }, (_, i) => held(row(i)))
}

// --- a code seconds from expiry ---------------------------------------------------

/** The Codes view with one item's code `secondsLeft` from its end, where the countdown turns critical. */
export const expiringCode =
  (title: string, secondsLeft = 4) =>
  (w: World): Record<string, Handler> => ({
    codes: () => codesOf(w.items).map((c) => (c.title === title && c.seconds_left !== null ? { ...c, seconds_left: secondsLeft } : c)),
  })

// --- requests that wait for the person ---------------------------------------------

/** A browser asking to be paired, as `browser_pending` answers it. */
export const PAIRING_REQUEST: PendingPairing = { request_id: "pr-1", code: "482913", name: "Chrome on Sam's MacBook Pro", family: "chromium", extension_id: "dbdblnolpiehhlabpmhobmkbjipmebpe", expires_in_secs: 118 }
/** A site asking to sign in with a passkey, as `browser_passkey_pending` answers it. */
export const PASSKEY_REQUEST: PasskeyApproval = { request_id: "pk-1", browser: "Firefox", text: "Sign in to pixelforge.example as sam_r", expires_in_secs: 117 }
