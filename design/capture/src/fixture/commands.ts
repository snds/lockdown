// The fake core's answers: every command the desktop's clients send
// (apps/desktop/src/core/client.ts and the feature clients), answered from
// the world in world.ts and story.ts, in lockdown-app's shapes. Changes a
// reviewer makes (trash, restore, organize, dismiss) change the world, so
// the screens behave as the app does for the length of a page load.

import { MARKER as DEVTOOLS_MARKER } from "@/devtools/marker"
import type {
  ApprovalRoute,
  CodeAdvanced,
  Copied,
  DuplicateGroup,
  ExpiryReminder,
  ExportPreview,
  FieldChangeInput,
  GateInput,
  GeneratorPreview,
  ImportFormatView,
  ImportPreview,
  LinkTargetView,
  Probe,
  Recipe,
  StorageView,
  SyncOutcome,
  UserTemplateView,
  VersionView,
} from "@/core/types"
import type { ClockView, FederationMap, IncidentSummary, IncidentView, SetupGuide } from "@/features/bridges/bridges-client"
import type { EndpointStatus, PairedBrowser, PendingPairing } from "@/features/browsers/browser-client"
import type { DrillCheck, ExposureView, LinkView, RemindersView, SecurityKeyList } from "@/features/depth/depth-client"
import type { PasskeyApproval, PasskeyView } from "@/features/passkeys/passkey-client"
import type { HotkeyStatus, QuickHit } from "@/features/quick/quick-client"
import type { ScanOutcome } from "@/features/scan/scan-client"
import type { MirrorsView, RestoreDrillView } from "@/features/server/server-client"
import type { OpenedView, VaultSharingView } from "@/features/sharing/sharing-client"

import { fail, type Args, type Handler } from "../host/core"
import { uid } from "./ids"
import { PERSON, VAULTS, vaultId, type VaultKey } from "./seed"
import {
  CONTACTS,
  DEVICES,
  PLAYBOOKS,
  REQUESTS,
  SENDS,
  SESSIONS,
  backupStatus,
  emergencyList,
  securityReport,
  sharingOverview,
  status,
} from "./story"
import { NOW, ago, ahead } from "./time"
import { TEMPLATES, codesOf, detailOf, summaryOf, valueOf, vaultViews, type World, type WItem } from "./world"

const enc = new TextEncoder()
const bytes = (text: string) => enc.encode(text)
const str = (a: Args, k: string) => String(a[k] ?? "")

/** A once-page body (`splitOncePage`): NUL-terminated JSON notes, then the page. */
function oncePage(lines: string[], page: string): Uint8Array {
  const head = enc.encode(JSON.stringify(lines))
  const body = enc.encode(page)
  const out = new Uint8Array(head.length + 1 + body.length)
  out.set(head, 0)
  out.set(body, head.length + 1)
  return out
}

export const KIT = `Lockdown recovery kit
Made for: ${PERSON.login}
Server: ${PERSON.server}

Secret Key: A3-K7QF2M-9XWD4R-HT6PLC-8ZB3NV-Q5EY7J
Recovery code: RC-4F9K-2MXQ-7HTD-PL8B-3ZNW

Keep this page somewhere safe, away from your devices. With your account
password, it opens your vault on a new device. Lockdown has never seen
this Secret Key and cannot make it again.`

/** The master password the fixture's vault opens with. */
export const MASTER_PASSWORD = "Tidal-Vesper-Orbit-48"

/** A local account's kit page, as the core writes it (doc 39 §1.2). */
export const LOCAL_KIT = `LOCKDOWN RECOVERY KIT
=====================
Keep this page somewhere safe, away from this computer. It is the only way
back into your vault if you forget your master password.

Secret Key:     A3-K7QF2M-9XWD4R-HT6PLC-8ZB3NV-Q5EY7J
Recovery code:  RC4F-9K2M-XQ7H-TDPL-8B3Z-NW5V-6YEA-J2KD

Profile:        0190e6a0-5a3e-7c1d-9b2f-4e8a6c0d1f23
Made:           2026-10-29 on ${PERSON.device}

To use this kit, open Lockdown on this computer and choose
"Forgot your password? Use your recovery kit".
Lockdown never asks for this page in an email, a message or a web form.
`

/** What discovery finds around Sam: the account's server, and one on the network. */
const CANDIDATES = [
  {
    address: PERSON.server,
    sources: [{ kind: "settings" }],
    state: "ready",
    reason: null,
    fingerprint: "48211 90314 22587 61040 77302 15599",
    version: "0.1.0",
    policy_key: false,
    locked: false,
    in_use: true,
  },
  {
    address: "https://nas.rivera.example:8443",
    sources: [{ kind: "network" }],
    state: "ready",
    reason: null,
    fingerprint: "30918 44720 18263 90557 61124 08376",
    version: "0.1.0",
    policy_key: false,
    locked: false,
    in_use: false,
  },
  {
    address: "http://studio-nas.local:8787",
    sources: [{ kind: "network" }],
    state: "needs_https",
    reason: "Lockdown only talks to a server on another computer over https.",
    fingerprint: null,
    version: null,
    policy_key: false,
    locked: false,
    in_use: false,
  },
]

function typedCandidate(address: string) {
  const base = address.startsWith("http") ? address.replace(/\/$/, "") : `https://${address.replace(/\/$/, "")}`
  return { ...CANDIDATES[1]!, address: base, sources: [{ kind: "typed" }] }
}

/** A QR-looking grid (not a real code): the width, then one byte per module. */
function qrGrid(seed: number, width = 29): Uint8Array {
  const out = new Uint8Array(1 + width * width)
  out[0] = width
  let h = seed >>> 0
  const finder = (x: number, y: number) => {
    for (const [fx, fy] of [[0, 0], [width - 7, 0], [0, width - 7]] as const) {
      const dx = x - fx
      const dy = y - fy
      if (dx >= 0 && dx < 7 && dy >= 0 && dy < 7) return dx === 0 || dy === 0 || dx === 6 || dy === 6 || (dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4) ? 1 : 0
      if (dx >= -1 && dx <= 7 && dy >= -1 && dy <= 7) return 0
    }
    return null
  }
  for (let y = 0; y < width; y++) {
    for (let x = 0; x < width; x++) {
      const f = finder(x, y)
      h = (Math.imul(h, 1664525) + 1013904223) >>> 0
      out[1 + y * width + x] = f ?? (y === 6 || x === 6 ? (x + y) % 2 === 0 ? 1 : 0 : h >>> 31)
    }
  }
  return out
}

/** The same as rows of `1` and `0` (the add-device link's QR). */
function qrRows(seed: number): string[] {
  const g = qrGrid(seed)
  const w = g[0]!
  return Array.from({ length: w }, (_, y) => Array.from(g.subarray(1 + y * w, 1 + (y + 1) * w)).join(""))
}

const ROUTES: Record<string, ApprovalRoute> = {
  use: { route: "platform", method: "Touch ID" },
  approve: { route: "platform", method: "Touch ID" },
  reveal: { route: "password", method: "password" },
}

/** A crown jewel's reveal, export or class change asks for the master password (D101). */
function needPassword(gate: GateInput | undefined, what: string) {
  if (!gate?.password) fail("approval_needed", what, "password")
  if (gate.password.trim().toLowerCase() === "wrong") fail("refused", "The password didn't match.")
}

/** Builds the base answers over one world. */
export function commands(world: World): Record<string, Handler> {
  const dismissed = new Set<string>()
  const byId = (id: unknown): WItem => {
    const item = world.items.find((i) => i.id === id)
    if (!item) fail("refused", "There is no such item in this vault.")
    return item
  }
  const open = () => {
    if (world.lock.locked) fail("locked", "The vault is locked.")
  }
  const touchItem = (item: WItem) => {
    item.version += 1
    item.edited = 0
  }
  const vaultKey = (id: unknown): VaultKey => VAULTS.find((v) => v.id === id)?.key ?? "personal"
  const copied = (): Copied => {
    world.copies += 1
    return { clears_in_secs: world.settings.clipboard_clear_secs, copy: world.copies }
  }
  const addFrom = (input: Args, vault: VaultKey): string => {
    const kind = str(input, "kind")
    const changes = (input.fields as FieldChangeInput[] | undefined) ?? []
    const fields: Record<string, string> = {}
    for (const c of changes) {
      if (!c.field_id || c.action === "remove") continue
      if (c.secret) fields[c.field_id] = new TextDecoder().decode(Uint8Array.from(c.secret))
      else if (c.action === "generate") fields[c.field_id] = "Quartz-Meadow-Lantern-4471"
      else if (c.text !== undefined) fields[c.field_id] = c.text
      else if (c.flag !== undefined) fields[c.field_id] = String(c.flag)
    }
    const n = 100 + world.items.length
    const item: WItem = { n, id: uid("item", n), version: 1, kind, title: str(input, "title") || "Untitled", vault, edited: 0, fields, crown: Boolean(input.crownJewel) }
    world.items.push(item)
    return item.id
  }
  const editFrom = (input: Args): number => {
    const item = byId(input.id)
    if (item.crown) needPassword(input.gate as GateInput | undefined, "Saving a change to a crown jewel")
    if (typeof input.title === "string" && input.title) item.title = input.title
    for (const c of (input.changes as FieldChangeInput[] | undefined) ?? []) {
      if (!c.field_id) continue
      if (c.action === "remove") delete item.fields[c.field_id]
      else if (c.action === "set") {
        if (c.secret) item.fields[c.field_id] = new TextDecoder().decode(Uint8Array.from(c.secret))
        else if (c.text !== undefined) item.fields[c.field_id] = c.text
      } else if (c.action === "generate") item.fields[c.field_id] = "Quartz-Meadow-Lantern-4471"
    }
    touchItem(item)
    return item.version
  }
  const sharingVault = (id: unknown): VaultSharingView => {
    const v = sharingOverview(world).vaults.find((x) => x.vault_id === id)
    if (!v) fail("refused", "There is no such vault.")
    return v
  }
  const opened: OpenedView = {
    kind: "item",
    title: "Guest Wi-Fi",
    values: [
      { label: "Network name", secret: false },
      { label: "Password", secret: true },
    ],
    file: null,
  }

  return {
    // --- lifecycle and lock ------------------------------------------------------
    probe: (): Probe => ({
      root: "/Users/sam/Library/Application Support/Ecosystem",
      data: "/Users/sam/Library/Application Support/Lockdown",
      identity_present: world.device.identity,
      device_key_present: world.device.deviceKey,
      account_present: world.device.identity,
      device_name: PERSON.device,
      master_password: world.device.deviceKey && world.device.masterPassword !== false,
      local_account: world.device.local === true,
      name: world.device.deviceKey ? (world.device.local ? "Sam Rivera" : PERSON.login) : null,
      stage: world.device.stage ?? (world.device.deviceKey ? "done" : null),
    }),
    open_vault: () => {
      world.lock = { ...world.lock, locked: false, reason: null }
      return status(world)
    },
    unlock: () => {
      world.lock = { ...world.lock, locked: false, reason: null, idle_secs: 0 }
      return status(world)
    },
    lock: () => {
      world.lock = { ...world.lock, locked: true, reason: "requested" }
    },
    lock_state: () => world.lock,
    touch: () => {
      world.lock = { ...world.lock, idle_secs: 0 }
    },
    set_hidden: () => world.lock,
    set_policy: (a) => {
      const p = a.policy as { idle_secs: number | null; hidden_secs: number | null }
      world.lock = { ...world.lock, idle_timeout_secs: p.idle_secs, hidden_timeout_secs: p.hidden_secs }
      return world.lock
    },
    get_settings: () => world.settings,
    set_settings: (a) => {
      world.settings = { ...world.settings, ...(a.settings as object) }
      return world.settings
    },
    approval_route: (a) => ROUTES[str(a, "kind")] ?? ROUTES.use!,
    personas: () => [
      { id: uid("persona", 1), label: "Personal", kind: "personal", active: true },
      { id: uid("persona", 2), label: "Rivera Studio", kind: "work", active: false },
    ],
    status: () => {
      open()
      return status(world)
    },
    audit: () => [
      "The vault's history checks out: 1,842 entries, each signed by one of your devices.",
      "The server's public log has your account's keys at entry 48,211, and two mirrors agree.",
      "This device's key is in the identity's device list.",
      "The recovery kit was last drilled 5 months ago.",
    ],
    sync: (): SyncOutcome => ({
      pushed: 1,
      pulled: 3,
      merged: 0,
      conflict_copies: [],
      head_seq: 1845,
      rounds: 1,
      transparency: status(world).account!.transparency!,
      lines: ["Synced: 1 change sent, 3 received.", "The server's public log checks out (48,211 entries)."],
    }),

    // --- items -------------------------------------------------------------------
    list: () => {
      open()
      return world.items.map(summaryOf)
    },
    show: (a) => {
      open()
      return detailOf(byId(a.id))
    },
    reveal: (a) => {
      const item = byId(a.id)
      if (item.crown) needPassword(a.gate as GateInput | undefined, `Showing a field of “${item.title}”, a crown jewel`)
      const v = valueOf(item, str(a, "fieldId"))
      if (v === null) fail("refused", "This field couldn't be decrypted on this device.")
      return bytes(v)
    },
    copy_secret: (a) => {
      const item = byId(a.id)
      if (item.crown) needPassword(a.gate as GateInput | undefined, `Copying a field of “${item.title}”, a crown jewel`)
      return copied()
    },
    copy_code: () => copied(),
    clipboard_holds: (a) => a.copy === world.copies,
    clipboard_clear: () => true,
    templates: () => TEMPLATES,
    password_strength: (a) => {
      const n = (a.candidate as number[] | undefined)?.length ?? 0
      return n === 0 ? null : n < 8 ? 0 : n < 12 ? 2 : n < 16 ? 3 : 4
    },
    add_login: (a) => addFrom({ kind: "login", title: a.title ?? a.site, fields: [{ field_id: "user", action: "set", text: a.username }, { field_id: "url.0", action: "set", text: a.site }, { field_id: "pw", action: "set", secret: a.password }] }, "personal"),
    edit_login: (a) => editFrom({ id: a.id, title: a.title, changes: [] }),
    add_item: (a) => addFrom(a, vaultKey(a.vault)),
    edit_item: (a) => editFrom(a),
    trash: (a) => {
      byId(a.id).state = "trashed"
    },
    restore: (a) => {
      byId(a.id).state = "live"
    },
    purge: (a) => {
      world.items = world.items.filter((i) => i.id !== a.id)
    },
    archive: (a) => {
      byId(a.id).state = "archived"
    },
    unarchive: (a) => {
      byId(a.id).state = "live"
    },
    codes: () => codesOf(world.items),
    advance_code: (a): CodeAdvanced => {
      const item = byId(a.itemId)
      const uri = item.fields[str(a, "fieldId")] ?? ""
      const counter = Number(/counter=(\d+)/.exec(uri)?.[1] ?? 0) + 1
      item.fields[str(a, "fieldId")] = uri.replace(/counter=\d+/, `counter=${counter}`)
      touchItem(item)
      const code = codesOf([item]).find((c) => c.field_id === a.fieldId)!
      return { code: code.current, counter, version: item.version }
    },
    add_code: (a) => addFrom({ kind: "code", title: a.title ?? "New code", fields: [{ field_id: "code.0", action: "set", text: a.uri }] }, "personal"),
    note_use: () => undefined,
    open_site: () => undefined,
    open_clock_settings: () => false,

    // --- account and recovery --------------------------------------------------------
    // --- the first run, the master password, discovery (doc 39) -------------------------
    first_run_create: (a) => {
      if (str(a, "password").length < 12) return fail("refused", "This password protects everything else, so it needs to be at least fair. Add another word or two.")
      world.device = { identity: true, deviceKey: true, masterPassword: true, local: true, stage: "account_made" }
      world.lock = { ...world.lock, locked: false, reason: null }
      return oncePage(["Your vault is ready, and only your master password opens it."], LOCAL_KIT)
    },
    unlock_with_password: (a) => {
      if (str(a, "password") !== MASTER_PASSWORD) return fail("refused", "That isn't the master password for this vault.")
      world.lock = { ...world.lock, locked: false, reason: null, idle_secs: 0 }
      return status(world)
    },
    set_master_password: () => {
      world.device = { ...world.device, masterPassword: true }
      return oncePage(["Your master password now locks this vault."], "")
    },
    password_reset_with_kit: (a) => {
      if (!str(a, "secretKey").startsWith("A3-")) return fail("refused", "That kit doesn't open this vault. Check each character against the page.")
      world.lock = { ...world.lock, locked: false, reason: null, idle_secs: 0 }
      return status(world)
    },
    onboarding_stage: (a) => {
      world.device = { ...world.device, stage: str(a, "stage") }
    },
    set_name: (a) => str(a, "name").trim(),
    check_master_password: () => undefined,
    quick_unlock_status: () => ({ kind: "touch_id", available: false, why: "Touch ID unlock needs the signed Lockdown app." }),
    discovery_candidates: (a) => (a.network ? CANDIDATES : CANDIDATES.slice(0, 1)),
    discovery_check: (a) => typedCandidate(str(a, "address")),
    discovery_domain: () => [],
    discovery_confirm: (a) => typedCandidate(str(a, "address")),
    discovery_forget: () => undefined,
    move_to_server: () => {
      world.device = { ...world.device, local: false }
      return { pushed: 43, pulled: 0, merged: 0, conflict_copies: [], head_seq: 1842, rounds: 1, transparency: status(world).account?.transparency, lines: [] }
    },
    account_register: () => oncePage(["Your account is made on vault.rivera.example.", "Save, print or write down the kit now: it is shown once."], KIT),
    account_register_opted_out: () => oncePage(["Your account is made without a Secret Key."], KIT.replace(/^Secret Key:.*\n/m, "")),
    account_login: () => ["Signed in to vault.rivera.example as sam@rivera.example.", "Synced: 1,842 changes received."],
    recover: () => ["Recovered with the kit. This device is enrolled and its key is in the identity."],
    kit_create: () => oncePage(["A new kit replaces the old one: the old Secret Key and recovery code stop working."], KIT),
    kit_verify: (a) =>
      a.drill ? ["The kit opens your vault: the Secret Key and the recovery code both checked out.", "Drill recorded; the next reminder is in 6 months."] : ["The kit checks out."],
    device_list: () => DEVICES,
    device_revoke: (a) => [`Revoked ${DEVICES.find((d) => d.id === a.id)?.name ?? "the device"}. It can't open the vault or sync any more.`],

    // --- import, export, backups ---------------------------------------------------------
    import_formats: (): ImportFormatView[] => [
      { id: "lockdown", name: "Lockdown export or backup", how: "A .ldx or .ldbak file from Lockdown.", may_need_password: true },
      { id: "cxf", name: "Credential Exchange (CXF)", how: "The open format many password managers export.", may_need_password: false },
      { id: "csv", name: "A CSV of logins", how: "Columns for title, website, username and password.", may_need_password: false },
      { id: "kdbx", name: "KeePass database", how: "A .kdbx file and its master password.", may_need_password: true },
    ],
    import_detect: () => "csv",
    import_preview: (): ImportPreview => ({
      token: 7,
      format: "csv",
      format_name: "A CSV of logins",
      total: 26,
      by_kind: [
        { kind: "login", name: "Logins", count: 22 },
        { kind: "note", name: "Notes", count: 3 },
        { kind: "card", name: "Cards", count: 1 },
      ],
      attachments: 0,
      crown_jewels: 0,
      new: 24,
      updated: 0,
      unchanged: 2,
      problems: ["Row 14 has no website or title, so it was left out."],
      item_notes: ["2 rows match logins you already have and stay as they are."],
    }),
    import_commit: () => ({ imported: 24, updated: 0, unchanged: 2, notes: ["Row 14 was left out: no website or title."] }),
    import_cancel: () => undefined,
    import_items: () => ({ imported: 24, updated: 0, unchanged: 2, notes: [] }),
    export_preview: (a): ExportPreview => ({
      format: (a.format as ExportPreview["format"]) ?? "lockdown",
      items: 40,
      attachments: 4,
      crown_jewels_included: 0,
      crown_jewels_left_out: 2,
      left_out: [
        { id: world.items.find((i) => i.title === "Cold wallet")!.id, title: "Cold wallet", kind: "crypto-wallet", reason: "A crown jewel stays out unless you opt it in." },
        { id: world.items.find((i) => i.title === "Northwind Mail recovery codes")!.id, title: "Northwind Mail recovery codes", kind: "document", reason: "A crown jewel stays out unless you opt it in." },
      ],
      dropped: a.format === "csv" ? ["Passkeys, codes and attachments have no column in a CSV."] : [],
      encrypted: a.format !== "csv",
    }),
    export_choose: (a) => ({ token: 3, path: `/Users/sam/Desktop/lockdown-export.${a.format === "csv" ? "csv" : "ldx"}`, name: `lockdown-export.${a.format === "csv" ? "csv" : "ldx"}` }),
    export_save: (a) => {
      needPassword(a.gate as GateInput | undefined, "Exporting the vault")
      return { path: "/Users/sam/Desktop/lockdown-export.ldx", bytes: 1_204_331, preview: { format: "lockdown", items: 40, attachments: 4, crown_jewels_included: 0, crown_jewels_left_out: 2, left_out: [], dropped: [], encrypted: true } }
    },
    set_export_opt_in: (a) => {
      const item = byId(a.id)
      if (a.on) needPassword(a.gate as GateInput | undefined, "Letting a crown jewel into exports and backups")
      item.exportOptIn = Boolean(a.on)
      touchItem(item)
      return item.version
    },
    backup_status: () => backupStatus(world.settings.backup?.enabled ?? false),
    backup_pick_folder: () => ({ token: 4, path: "/Users/sam/Backups/Lockdown", name: "Lockdown" }),
    backup_setup: () => backupStatus(true),
    backup_now: () => ({ ...backupStatus(true), last_success_at: NOW }),
    backup_off: () => backupStatus(false),
    backup_restore_preview: () => ({
      token: 8,
      format: "lockdown",
      format_name: "Lockdown backup",
      total: 41,
      by_kind: [{ kind: "login", name: "Logins", count: 15 }],
      attachments: 4,
      crown_jewels: 0,
      new: 0,
      updated: 1,
      unchanged: 40,
      problems: [],
      item_notes: [],
    }),

    // --- organization ----------------------------------------------------------------------
    vaults: () => vaultViews(world),
    create_vault: (a) => uid("vault", 10 + str(a, "name").length),
    rename_vault: () => undefined,
    set_trash_days: () => undefined,
    add_folder: (a) => uid("folder", 50 + str(a, "name").length),
    change_folder: () => undefined,
    remove_folder: () => 0,
    organize: (a) => {
      const item = byId(a.id)
      if (Array.isArray(a.tags)) item.tags = a.tags as string[]
      if (typeof a.favourite === "boolean") item.favourite = a.favourite
      if (typeof a.folder === "string") item.folder = VAULTS.flatMap((v) => v.folders).find((f) => f.id === a.folder)
      touchItem(item)
      return item.version
    },
    rename_tag: () => 1,
    move_item: (a) => {
      const item = byId(a.id)
      if (item.crown) needPassword(a.gate as GateInput | undefined, "Moving a crown jewel to another vault")
      item.vault = vaultKey(a.vaultId)
      item.folder = undefined
      return item.id
    },
    set_crown_jewel: (a) => {
      const item = byId(a.id)
      if (!a.on) needPassword(a.gate as GateInput | undefined, "Taking an item out of the crown-jewel class")
      item.crown = Boolean(a.on)
      return item.id
    },
    resolve_conflict: (a) => {
      const keep = byId(a.keep)
      const other = byId(a.other)
      other.state = "trashed"
      keep.title = keep.title.replace(/ \(conflict copy from .*\)$/, "")
    },
    duplicates: (): DuplicateGroup[] => {
      const parcel = world.items.find((i) => i.title === "Parcelpost")!
      const quill = world.items.find((i) => i.title === "Quill Notes")!
      const home = world.items.filter((i) => i.title === "Home Wi-Fi" || i.title === "Home router")
      return [
        { reason: "same-password", words: "Parcelpost and Quill Notes share a password.", ids: [parcel.id, quill.id], mergeable: false },
        { reason: "same-login", words: "Home Wi-Fi and Home router are the same network.", ids: home.map((i) => i.id), mergeable: true },
      ]
    },
    merge_duplicates: () => 2,
    history: (a): VersionView[] => {
      const item = byId(a.id)
      const n = item.version
      return Array.from({ length: Math.min(n, 5) }, (_, i) => ({
        n: n - i,
        edited_at: ago(item.edited + i * 37),
        edited_by: i % 2 === 0 ? PERSON.device : "Sam's phone",
        changed: i === 0 ? ["Password"] : i === 1 ? ["Website", "Notes"] : ["Username"],
        readable: true,
        current: i === 0,
      }))
    },
    show_version: (a) => detailOf(byId(a.id)),
    restore_version: (a) => byId(a.id).version + 1,
    attach: () => uid("attachment", 40),
    attachment: () => bytes("%PDF-1.7 stand-in"),
    detach: (a) => byId(a.id).version + 1,
    attach_choose: () => ({ token: 5, path: "/Users/sam/Documents/receipt.pdf", name: "receipt.pdf" }),
    attach_chosen: () => uid("attachment", 41),
    attachment_save_choose: (a) => ({ token: 6, path: `/Users/sam/Downloads/${str(a, "name")}`, name: str(a, "name") }),
    attachment_save: () => undefined,
    // The reminders' toasts are their own states (desktop-reminder-*); elsewhere they would cover every screen.
    expiry_reminders: (): ExpiryReminder[] => [],
    expiry_reminders_due: (): ExpiryReminder[] =>
      world.items
        .map(summaryOf)
        .filter((s) => s.views.includes("expiring"))
        .map((s) => ({ id: s.id, title: s.title, kind: s.kind, date: s.expires_at ?? "", days: s.expires_in_days ?? 0 })),
    add_item_in: (a) => addFrom(a, vaultKey(a.vault)),

    // --- the security center ----------------------------------------------------------------
    security_report: () => securityReport(world, dismissed),
    security_check: () => securityReport(world, dismissed),
    security_dismiss: (a) => {
      const f = securityReport(world, dismissed).findings.find((x) => x.item_id === a.itemId && x.key === a.key)
      if (f) dismissed.add(f.id)
      return securityReport(world, dismissed)
    },
    security_restore: (a) => {
      for (const id of [...dismissed]) if (id.includes(str(a, "itemId"))) dismissed.delete(id)
      return securityReport(world, dismissed)
    },
    security_dismiss_alert: (a) => {
      dismissed.add(`alert:${String(a.seq)}`)
      return securityReport(world, dismissed)
    },
    security_change_password: (a) => {
      const item = byId(a.itemId)
      const site = Object.entries(item.fields).find(([k]) => k.startsWith("url."))?.[1] ?? "https://example.com"
      return { url: `${new URL(site).origin}/.well-known/change-password`, kind: "well_known", checked: true }
    },
    security_playbooks: () => PLAYBOOKS,

    // --- vault depth --------------------------------------------------------------------------
    user_templates: (): UserTemplateView[] => [
      {
        id: uid("template", 1),
        vault_id: vaultId("personal"),
        name: "Bike",
        summary: "A bike, its lock and its frame number",
        icon: "pedal_bike",
        fields: [
          { key: "lock", field_id: "tpl.lock", label: "Lock combination", kind: "hidden", secret: true },
          { key: "frame", field_id: "tpl.frame", label: "Frame number", kind: "text" },
        ],
        versions: 2,
      },
    ],
    save_user_template: () => uid("template", 2),
    delete_user_template: () => undefined,
    link_targets: (a): LinkTargetView[] =>
      world.items
        .filter((i) => vaultId(i.vault) === a.vaultId && i.id !== a.except && (i.state ?? "live") === "live")
        .slice(0, 8)
        .map((i) => ({ item_id: i.id, title: i.title, kind: i.kind, crown_jewel: i.crown ?? false, fields: Object.keys(i.fields).map((f) => ({ field_id: f, label: f, secret: f === "pw", code: f.startsWith("code.") })) })),
    generator_preview: (a): GeneratorPreview => {
      const r = a.recipe as Recipe
      // The core's own rule (crates/app/src/strength.rs `bits_word`): weak under 50 bits, fair under 75, else strong.
      const word = (bits: number): GeneratorPreview["strength"] => (bits < 50 ? "weak" : bits < 75 ? "fair" : "strong")
      if (r.style === "words") {
        const bits = Math.round(r.words * Math.log2(7776) * 10) / 10
        return { words: `${r.words} words${r.separator ? `, separated by “${r.separator}”` : ""}${r.number ? ", with a number" : ""}`, bits, strength: word(bits), site: null }
      }
      const bits = Math.round(r.length * (r.symbols ? 6.5 : 5.9) * 10) / 10
      return { words: `${r.length} characters: letters${r.digits ? ", digits" : ""}${r.symbols ? " and symbols" : ""}`, bits, strength: word(bits), site: null }
    },
    wifi_qr: (a) => qrGrid(byId(a.id).n * 7919),
    log_away: (a) => {
      const item = byId(a.id)
      item.away = [...(item.away ?? []), { place: str(a, "place"), at: NOW, remind_at: NOW + Number(a.remindInHours ?? 24) * 3600, due: false }]
      return item.version + 1
    },
    clear_away: (a) => {
      const item = byId(a.id)
      item.away = (item.away ?? []).filter((x) => x.at !== a.at)
      return item.version + 1
    },
    away_reminders: () => [],
    away_reminders_due: () => world.items.flatMap((i) => (i.away ?? []).filter((a) => a.remind_at <= NOW).map((a) => ({ ...a, item_id: i.id, title: i.title }))),
    storage_usage: (): StorageView => ({ used: 412_000_000, quota: 2_000_000_000, used_words: "412 MB", quota_words: "2 GB" }),

    // --- security depth -------------------------------------------------------------------------
    exposure: (a): ExposureView => {
      const item = byId(a.id)
      const high = item.crown || item.kind === "card" || item.kind === "bank-account"
      return {
        id: item.id,
        tier: item.crown ? "app-only" : high ? "ask" : "fill",
        source: item.crown ? "crown-jewel" : high ? "high-risk" : "default",
        default_tier: item.crown ? "app-only" : high ? "ask" : "fill",
        reason: item.crown ? "A crown jewel's values stay in the app." : high ? "Cards and bank details ask before they fill." : null,
        choices: item.crown ? ["app-only"] : ["app-only", "ask", "fill"],
      }
    },
    exposures: () => [],
    set_exposure: (a) => ({ id: str(a, "id"), tier: a.tier ?? "fill", source: "chosen", default_tier: "fill", reason: null, choices: ["app-only", "ask", "fill"] }),
    legacy_opt_in: () => false,
    set_legacy_opt_in: () => 2,
    reminders: (): RemindersView => ({
      password: { applies: true, due: false, last_at: ago(12), every_days: 14, next_at: ahead(2) },
      drill: { kit: "drilled", last_drilled_at: ago(150), next_at: ahead(30), due: false, kit_test_pending: false },
    }),
    password_check: () => ({
      password: { applies: true, due: false, last_at: NOW, every_days: 14, next_at: ahead(14) },
      drill: { kit: "drilled", last_drilled_at: ago(150), next_at: ahead(30), due: false, kit_test_pending: false },
    }),
    password_snooze: () => ({
      password: { applies: true, due: false, last_at: ago(12), every_days: 14, next_at: ahead(3) },
      drill: { kit: "drilled", last_drilled_at: ago(150), next_at: ahead(30), due: false, kit_test_pending: false },
    }),
    set_password_reminder_days: (a) => ({
      password: { applies: true, due: false, last_at: ago(12), every_days: Number(a.days ?? 14), next_at: ahead(2) },
      drill: { kit: "drilled", last_drilled_at: ago(150), next_at: ahead(30), due: false, kit_test_pending: false },
    }),
    drill_snooze: () => ({
      password: { applies: true, due: false, last_at: ago(12), every_days: 14, next_at: ahead(2) },
      drill: { kit: "drilled", last_drilled_at: ago(150), next_at: ahead(37), due: false, kit_test_pending: false },
    }),
    kit_test_skip: (a) => {
      if (!a.understood) fail("refused", "Skipping the kit test means you won't know the kit works until you need it.", "confirm_skip")
      return { password: { applies: true, due: false, last_at: ago(12), every_days: 14, next_at: ahead(2) }, drill: { kit: "created", last_drilled_at: null, next_at: ahead(30), due: false, kit_test_pending: false } }
    },
    drill_checks: (): DrillCheck[] => [
      { what: "kit", ok: true, words: "The recovery kit opens the vault (drilled 5 months ago)." },
      { what: "secret-key", ok: true, words: "The Secret Key on this device matches the kit." },
      { what: "contact", ok: true, words: "Alex Rivera can start emergency access, with a 7-day wait." },
      { what: "crown-jewels", ok: false, words: "2 crown jewels are left out of backups: only the kit and your password bring them back." },
    ],
    phrase_invite: (a) => ({ verify_id: uid("verify", 1), role: "inviter", login: str(a, "login"), account_id: CONTACTS[2]!.account_id, phrase: "maple anchor velvet", state: "waiting", words: "Say these three words to Priya; the check passes when she types them.", expires_at: ahead(0.01) }),
    phrase_incoming: () => [],
    phrase_answer: () => ({ verify_id: uid("verify", 1), role: "joiner", login: "priya@nair-design.example", account_id: CONTACTS[2]!.account_id, phrase: null, state: "matched", words: "The words matched: Priya's keys are the ones the server's log has.", expires_at: ahead(0.01) }),
    phrase_poll: () => ({ verify_id: uid("verify", 1), role: "inviter", login: "priya@nair-design.example", account_id: CONTACTS[2]!.account_id, phrase: "maple anchor velvet", state: "waiting", words: "Waiting for Priya to type the words.", expires_at: ahead(0.01) }),
    phrase_cancel: () => undefined,
    security_keys: (): SecurityKeyList => ({
      keys: [
        { id: uid("key", 1), name: "Blue key on the keyring", alg: "ES256", created_at: ago(300), last_used: ago(12) },
        { id: uid("key", 2), name: "Spare key in the drawer", alg: "EdDSA", created_at: ago(298), last_used: null },
      ],
      rp_id: "vault.rivera.example",
    }),
    security_key_add: () => undefined,
    security_key_remove: () => ({ keys: [], rp_id: "vault.rivera.example" }),
    link_offer: (): LinkView => ({ state: "waiting", code: "H7K2M-Q9XPD", qr: "lockdown:link:v1?c=H7K2MQ9XPD", qr_rows: qrRows(4242), expires_at: ahead(0.007) }),
    link_poll: (): LinkView => ({ state: "compare", code: "H7K2M-Q9XPD", qr: "lockdown:link:v1?c=H7K2MQ9XPD", expires_at: ahead(0.006), sas: "418 902", device_name: "Sam's new phone" }),
    link_approve: () => ["Sam's new phone joined. It can open the vault once it syncs."],
    link_decline: () => undefined,
    secret_key_status: () => ({ state: "kept", words: "This device keeps the Secret Key in the system keychain." }),
    join_start: (a) => ({ state: "compare", sas: "418 902", server: str(a, "server") || PERSON.server, secret_key: "sent", words: "Check the other device shows the same six digits." }),
    join_poll: () => ({ state: "approved", sas: "418 902", server: PERSON.server, secret_key: "sent", words: "Approved on the other device. Enter your master password to finish." }),
    join_finish: () => status(world),
    join_cancel: () => undefined,
    second_factor_open: () => undefined,
    second_factor_wait: () => null,

    // --- bridges -----------------------------------------------------------------------------------
    clock_status: (): ClockView => ({ checked_at: ago(0.1), offset_secs: 1.2, verdict: "in_step", correcting: false, words: "This computer's clock is within 2 seconds of the server's." }),
    clock_check: (): ClockView => ({ checked_at: NOW, offset_secs: 1.1, verdict: "in_step", correcting: false, words: "This computer's clock is within 2 seconds of the server's." }),
    federation_map: (): FederationMap => {
      const stack = world.items.find((i) => i.title === "Stackwise")!
      const mail = world.items.find((i) => i.title === "Northwind Mail")!
      const login = (i: WItem) => ({ item_id: i.id, title: i.title, username: i.fields.user ?? "", host: new URL(i.fields["url.0"] ?? "https://x.example").host, has_password: Boolean(i.fields.pw), second_step: Boolean(i.fields["code.0"]), crown_jewel: i.crown ?? false })
      return {
        providers: [{ id: "other:Northwind", name: "Northwind", account: login(mail), candidates: [login(mail)], sites: [login(stack)], warnings: [] }],
        pending: [{ id: 1, provider: "other:Northwind", provider_name: "Northwind", host: "boards.pinecone.example", logins: [], confirmed: false }],
      }
    },
    federation_accept: () => uid("item", 120),
    federation_dismiss: () => undefined,
    federation_set_account: () => undefined,
    site_fix: () => ({ never: false, fields: [] }),
    site_fix_set: (a) => a.fix,
    alias_overview: () => ({
      services: [
        { id: "relaymail", name: "Relaymail", key_hint: "An API key from Relaymail's settings", needs_domain: false, takes_domain: true, self_hostable: true },
        { id: "hushbox", name: "Hushbox", key_hint: "A token from Hushbox → Developers", needs_domain: true, takes_domain: true, self_hostable: false },
      ],
      configured: [{ item_id: uid("item", 130), title: "Relaymail", service: "relaymail", service_name: "Relaymail", domain: "relay.rivera.example", base: null, made: 14 }],
    }),
    alias_service_add: () => uid("item", 131),
    alias_generate: () => ({ address: "quiet.otter.7741@relay.rivera.example", service: "relaymail", service_name: "Relaymail", monitoring_problem: null }),
    alias_monitoring: () => ({ enabled: true, addresses: ["sam.rivera@northwind.example", "quiet.otter.7741@relay.rivera.example"], has_account: true }),
    alias_monitoring_set: (a) => ({ enabled: Boolean(a.on), addresses: ["sam.rivera@northwind.example"], has_account: true }),
    incidents: (): IncidentSummary[] => [
      { id: "lost-phone", title: "I lost my phone", when: "A phone with Lockdown on it is lost or stolen", scope: "none", started_at: null, steps_done: 0, steps: 5 },
      { id: "breached-email", title: "My email account was broken into", when: "Someone else got into the mailbox other sites send resets to", scope: "email", started_at: ago(3), steps_done: 2, steps: 6 },
      { id: "breached-site", title: "A site I use was breached", when: "A site says its passwords were stolen", scope: "item", started_at: null, steps_done: 0, steps: 4 },
    ],
    incident: (a): IncidentView => ({
      id: str(a, "id"),
      title: "My email account was broken into",
      when: "Someone else got into the mailbox other sites send resets to",
      scope: "email",
      picked: "sam.rivera@northwind.example",
      choices: [{ value: "sam.rivera@northwind.example", label: "sam.rivera@northwind.example" }],
      started_at: ago(3),
      finished: false,
      steps: [
        { id: "read", title: "Take the mailbox back", detail: "Change the mailbox's password from a device you trust, then sign out every other session there.", action: "read", done: true, items: [] },
        { id: "two-step", title: "Turn on two-step sign-in for the mailbox", detail: "A code or a key, so a password alone can't get in again.", action: "two_step", done: true, items: [] },
        {
          id: "change",
          title: "Change the passwords that reset through this mailbox",
          detail: "These logins use this address; whoever read the mailbox could reset them.",
          action: "change_passwords",
          done: false,
          items: ["Parcelpost", "Harbor Health", "Quill Notes"].map((t, i) => {
            const it = world.items.find((x) => x.title === t)!
            return { item_id: it.id, title: t, host: new URL(it.fields["url.0"]!).host, done: i === 0 }
          }),
        },
        { id: "sessions", title: "End sessions you don't know", detail: "On Devices, end any Lockdown session you don't recognise.", action: "sessions", done: false, items: [] },
        { id: "kit", title: "Make a new recovery kit", detail: "If the kit's page was ever emailed, make a new one.", action: "recovery_kit", done: false, items: [] },
        { id: "check", title: "Check again", detail: "Run the security center's check once you're done.", action: "check", done: false, items: [] },
      ],
    }),
    incident_mark: (a) => ({ id: str(a, "id"), title: "", when: "", scope: "none", picked: null, choices: [], started_at: NOW, steps: [], finished: false }),
    incident_reset: (a) => ({ id: str(a, "id"), title: "", when: "", scope: "none", picked: null, choices: [], started_at: null, steps: [], finished: false }),
    setup_guide: (a): SetupGuide | null => {
      const item = byId(a.itemId)
      if (item.title !== "Harbor Health") return null
      return {
        site: "Harbor Health",
        methods: ["authenticator app", "security key"],
        app_codes: true,
        passkeys: null,
        setup_url: "https://portal.harborhealth.example/help/two-step",
        passkey_url: null,
        recovery_url: "https://portal.harborhealth.example/help/recovery",
        notes: null,
        site_apps: [],
        steps: ["Sign in, then open Account → Security.", "Choose Authenticator app and copy the set-up key.", "Paste it into this login's One-time code field in Lockdown."],
        attribution: [{ name: "2fa.directory", licence: "MIT", url: "https://2fa.directory" }],
      }
    },
    guide_sources: () => ({ sources: [{ name: "2fa.directory", licence: "MIT", url: "https://2fa.directory" }], version: 112, published: "2026-10-01" }),
    emergency_sheet: () => ({
      made_at: NOW,
      title: "If something happens to Sam",
      intro: "This sheet says where things are and who to call. It holds no passwords.",
      sections: [
        { heading: "Lockdown", lines: ["The vault is on Sam's MacBook Pro and phone.", "Alex Rivera is the emergency contact, with a 7-day wait."] },
        { heading: "Accounts", lines: ["Bluefin Bank: checking and a Visa card.", "Hearthstone Mutual: home insurance, policy HM-2210-88413."] },
      ],
      blanks: ["Where the recovery kit is kept", "Who has a spare house key"],
    }),
    export_save_age: () => ({ path: "/Users/sam/Desktop/lockdown-export.age", bytes: 1_198_002, preview: { format: "lockdown", items: 40, attachments: 4, crown_jewels_included: 0, crown_jewels_left_out: 2, left_out: [], dropped: [], encrypted: true } }),

    // --- browsers and passkeys ---------------------------------------------------------------------
    browser_status: (): EndpointStatus => ({
      listening: true,
      problem: null,
      host_binary: "/Applications/Lockdown.app/Contents/MacOS/lockdown-native-host",
      browsers: [
        { browser: "chrome", name: "Google Chrome", connected: true },
        { browser: "firefox", name: "Firefox", connected: true },
        { browser: "edge", name: "Microsoft Edge", connected: false },
      ],
      peer_check: { by: "signature", detail: "The browser host is signed by the same developer as this app." },
    }),
    browser_pending: (): PendingPairing[] => [],
    browser_answer: () => undefined,
    browser_paired: (): PairedBrowser[] => [
      { client_id: uid("client", 1), name: "Chrome on Sam's MacBook Pro", family: "chromium", extension_id: "dbdblnolpiehhlabpmhobmkbjipmebpe", paired_at: ago(200) },
      { client_id: uid("client", 2), name: "Firefox on Sam's MacBook Pro", family: "firefox", extension_id: "extension@lockdown.example", paired_at: ago(40) },
    ],
    browser_unpair: () => undefined,
    browser_install: () => [{ browser: "chrome", manifest: "/Users/sam/Library/Application Support/Google/Chrome/NativeMessagingHosts/app.lockdown.json", registry_key: null }],
    passkeys: (a): PasskeyView[] =>
      (byId(a.itemId).passkeys ?? []).map((p, i) => ({
        credential_id: p.credential_id,
        rp_id: p.rp_id,
        user_name: p.user_name,
        user_display_name: p.user_display_name,
        provider: p.provider,
        aaguid: null,
        algorithm: "ES256",
        backup_eligible: true,
        backed_up: p.backed_up ?? true,
        created: p.created,
        last_used: p.last_used,
        hidden: p.hidden ?? null,
        only_sign_in: i === 0 && !byId(a.itemId).fields.pw,
      })),
    passkey_delete: () => 2,
    passkey_restore: () => 2,
    browser_passkey_pending: (): PasskeyApproval[] => [],
    // The automatic passkey upgrade's notices (D210, D312): none until a state says so.
    passkey_upgrades: () => [],
    passkey_upgrade_seen: () => 0,
    browser_passkey_answer: () => undefined,

    // --- sessions and server checks ----------------------------------------------------------------
    session_list: () => SESSIONS,
    session_end: () => undefined,
    session_end_others: () => SESSIONS.length - 1,
    server_restore_drill: (): RestoreDrillView => ({ reported: true, scheduled: true, last_run_at: ago(2), last_passed: true, last_success_at: ago(2), client_read_back: true, words: "The server restored last night's backup to a scratch copy and a test account read an item back." }),
    kt_mirrors: (): MirrorsView => ({ configured: ["https://kt.witness.example"], checks: status(world).account!.transparency!.mirrors! }),
    kt_mirrors_set: (a) => ({ configured: (a.urls as string[]) ?? [], checks: status(world).account!.transparency!.mirrors! }),

    // --- quick access, scan, hygiene -----------------------------------------------------------------
    quick_search: (a): QuickHit[] => {
      const q = str(a, "query").trim().toLowerCase()
      const rows = world.items.map(summaryOf).filter((r) => r.state === "live")
      const hits = q ? rows.filter((r) => [r.title, r.site, r.username].some((s) => s.toLowerCase().includes(q))) : rows.filter((r) => (r.last_used ?? 0) > ago(3))
      return hits.slice(0, Number(a.limit ?? 20)).map((r) => ({ ...r, exact: true }))
    },
    quick_code: (a) => bytes(codesOf([byId(a.itemId)])[0]?.current ?? "000000"),
    quick_fill: (a) => {
      const site = byId(a.itemId).fields["url.0"] ?? "https://example.com"
      return { url: site, host: new URL(site).host, waits_secs: 120 }
    },
    quick_hide: () => undefined,
    quick_open_main: () => undefined,
    quick_hotkey_status: (): HotkeyStatus => ({ on: true, registered: true, label: "⌥ Space" }),
    quick_hotkey_set: (a): HotkeyStatus => ({ on: Boolean(a.on), registered: Boolean(a.on), label: "⌥ Space" }),
    scan_frame: (): ScanOutcome => ({ found: false, kind: "none", decision: "unsupported", code_in_url: false, accounts: [] }),
    scan_text: (): ScanOutcome => ({
      found: true,
      kind: "device_code",
      decision: "refused",
      reason: "A code pasted or typed in isn't approved: only a code scanned from a screen in front of you is.",
      service: "Meadowlark Streaming",
      host: "watch.meadowlark.example",
      code: "WDJB-MJHT",
      code_in_url: false,
      accounts: [],
    }),
    activation_approve: () => ({ url: "https://watch.meadowlark.example/activate", host: "watch.meadowlark.example", service: "Meadowlark Streaming", code: "WDJB-MJHT", code_in_url: false }),
    activation_cancel: () => undefined,
    qr_contact_frame: () => null,
    capture_protect: (a) => Boolean(a.on),
    memory_report: () => ({ supported: true, pages_locked: 48, page_size: 16384, pages_locked_ever: 112, refused: 0, core_dumps_off: true }),

    // --- sharing ---------------------------------------------------------------------------------------
    sharing_overview: () => sharingOverview(world),
    contact_lookup: (a) => ({ ...CONTACTS[2]!, login: str(a, "login") || CONTACTS[2]!.login, verified: false }),
    contact_verify: (a) => ({ ...(CONTACTS.find((c) => c.account_id === a.account) ?? CONTACTS[2]!), verified: true }),
    contact_forget: () => undefined,
    vault_sharing: (a) => sharingVault(a.vaultId),
    vault_share: (a) => sharingVault(a.vaultId),
    vault_member_right: (a) => sharingVault(a.vaultId),
    vault_member_remove: (a) => sharingVault(a.vaultId),
    household_create: () => vaultId("family"),
    household_add: (a) => sharingVault(a.vaultId),
    item_share: (a) => ({ share_id: uid("share", 9), item_id: str(a, "id"), login: str(a, "login"), right: a.right, created_at: NOW }),
    item_unshare: () => undefined,
    send_item: () => bytes(`${PERSON.server}/s/7Hq2Lm9V#k=4f9K2mXq7HtDpL8b3ZnWqR`),
    send_text: () => bytes(`${PERSON.server}/s/9Xw4Rt8W#k=Zs6Kb3Nd7Hq2Lm9Vx4Rt`),
    sends: () => SENDS,
    send_delete: () => undefined,
    send_open: () => opened,
    opened_value: (a) => bytes(a.index === 0 ? "Rivera-Guest" : "sunny-porch-42"),
    opened_file: () => bytes(""),
    opened_save: () => uid("item", 140),
    opened_copy: () => copied(),
    link_copy: () => copied(),
    request_create: () => bytes(`${PERSON.server}/r/Lm9Vx4Rt#k=8Wp1Zs6Kb3Nd7Hq2`),
    requests: () => REQUESTS,
    request_open: () => ({ ...opened, title: "Wi-Fi at the studio", kind: "text", values: [{ label: "Answer", secret: true }] }),
    request_delete: () => undefined,
    request_label: () => "The studio's Wi-Fi password",
    request_answer: () => undefined,
    emergency_add: () => emergencyList(),
    emergency_list: () => emergencyList(),
    emergency_step: () => emergencyList(),
    emergency_open: () => emergencyList(),

    // --- the quantum-safe status line (docs/plan/30-pq-status-line.md) -------------------------
    pq_status: () => ({
      state: "waiting",
      reason: null,
      headline: "Quantum-safe signatures: waiting for Studio desktop",
      explanation: "This device needs the latest Lockdown. Once it's updated and opened, the stronger signatures switch on by themselves.",
      without_level_1: [{ id: uid("device", 4), name: "Studio desktop" }],
      without_level_2: [],
    }),

    // --- the web vault's own (crates/web/src/commands) ---------------------------------------------
    web_state: () => ({ ...world.web, server: world.web.signed_in ? PERSON.server : null }),
    web_sign_in: () => {
      world.web = { signed_in: true, locked: false, login: PERSON.login }
      world.lock = { ...world.lock, locked: false, reason: null }
      return status(world)
    },
    web_unlock: (a) => {
      if (str(a, "password").trim().toLowerCase() === "wrong") fail("refused", "The password didn't match.")
      world.web = { ...world.web, locked: false }
      world.lock = { ...world.lock, locked: false, reason: null }
      return status(world)
    },
    web_sign_out: () => {
      world.web = { signed_in: false, locked: true, login: null }
    },
    web_sign_in_waiting: () => ({ pending_id: uid("pending", 1), challenge: [1, 2, 3], rp_id: "vault.rivera.example", allow: [], timeout_ms: 120_000, expires_at: ahead(0.003), device_label: "Firefox on a Mac" }),
    web_sign_in_answer: () => status(world),
    web_sign_in_continue: () => null,
    web_join_start: () => ({ state: "compare", sas: "418 902", server: PERSON.server, secret_key: "sent", words: "Check the other device shows the same six digits." }),
    web_join_poll: () => ({ state: "approved", sas: "418 902", server: PERSON.server, secret_key: "sent", words: "Approved on the other device. Enter your master password to finish." }),
    web_join_finish: () => status(world),
    web_join_cancel: () => undefined,
    reveal_to_use: (a) => {
      const item = byId(a.id)
      if (item.crown) needPassword(a.gate as GateInput | undefined, `Copying a field of \u201c${item.title}\u201d, a crown jewel`)
      return bytes(valueOf(item, str(a, "fieldId")) ?? "")
    },
    code_now: (a) => bytes(codesOf([byId(a.itemId)]).find((c) => c.field_id === a.fieldId)?.current ?? ""),
    link_meta: () => ({ password: false, expires_at: ahead(6), views_left: 1 }),
    link_open: () => opened,
    link_value: (a) => bytes(a.index === 0 ? "Rivera-Guest" : "sunny-porch-42"),
    link_file: () => bytes(""),
    page_sign_in_request: () => ({ pending_id: uid("pending", 1), challenge: [1, 2, 3], rp_id: "vault.rivera.example", allow: [], timeout_ms: 120_000, expires_at: ahead(0.003), device_label: "Lockdown on linux-a7f3" }),
    page_sign_in_answer: () => undefined,
    page_key_request: () => ({ pending_id: uid("pending", 2), challenge: [1, 2, 3], rp_id: "vault.rivera.example", rp_name: "Lockdown", user_id: [1], user_name: PERSON.login, exclude: [], timeout_ms: 120_000 }),
    page_key_finish: () => undefined,
    security_key_add_page: () => `${PERSON.server}/vault/#/security-key/${uid("pending", 3)}`,
    // Paper (docs/plan/38-mac-fixes.md): the shell's print panel and the kit's save dialog are the OS's, never drawn here;
    // the fake answers as the shell does once the person printed or picked a place.
    print_page: () => undefined,
    kit_save_pdf: () => true,
    kit_pdf: () => bytes("%PDF-1.7"),
    // The developer tools (docs/plan/40-devtools.md), as a development build has them: the demo closed.
    "plugin:devtools|status": () => ({ marker: DEVTOOLS_MARKER, reset: true, demo: { open: false, items: 0 } }),
  }
}
