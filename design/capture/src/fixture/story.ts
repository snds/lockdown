// Everything around the items: devices, sessions, the security center's
// findings, sharing, backups, the account. Words follow the core's own
// sentences (crates/app/src/security.rs, crates/security/src/playbook.rs),
// so a reviewer reads what the app would say.

import type {
  BackupStatus,
  DeviceInfo,
  Finding,
  Playbook,
  SecurityReport,
  Status,
} from "@/core/types"
import type { SessionView } from "@/features/sessions/live-client"
import type { ContactView, EmergencyView, RequestView, SendView, SharingOverview } from "@/features/sharing/sharing-client"

import { uid } from "./ids"
import { PERSON, vaultId } from "./seed"
import { NOW, ago, ahead } from "./time"
import type { World, WItem } from "./world"

export const DEVICES: DeviceInfo[] = [
  { id: uid("device", 1), name: PERSON.device, platform: "macos", enrolled_at: ago(400), revoked_at: null, this_device: true },
  { id: uid("device", 2), name: "Sam's phone", platform: "ios", enrolled_at: ago(398), revoked_at: null, this_device: false },
  { id: uid("device", 3), name: "Family iPad", platform: "ipados", enrolled_at: ago(210), revoked_at: null, this_device: false },
  { id: uid("device", 4), name: "Studio desktop", platform: "windows", enrolled_at: ago(120), revoked_at: null, this_device: false },
  // The one nobody recognises: enrolled two days ago.
  { id: uid("device", 5), name: "linux-a7f3", platform: "linux", enrolled_at: ago(2), revoked_at: null, this_device: false },
  { id: uid("device", 6), name: "Old ThinkPad", platform: "linux", enrolled_at: ago(800), revoked_at: ago(390), this_device: false },
]

export const SESSIONS: SessionView[] = [
  {
    id: uid("session", 1),
    device_id: DEVICES[0]!.id,
    device_name: PERSON.device,
    platform: "macos",
    address: "198.51.100.23",
    client: "lockdown/0.9.0 (macos)",
    created_at: ago(20),
    last_seen: ago(0.001),
    expires_at: ahead(10),
    current: true,
  },
  {
    id: uid("session", 2),
    device_id: DEVICES[1]!.id,
    device_name: "Sam's phone",
    platform: "ios",
    address: "198.51.100.23",
    client: "lockdown/0.9.0 (ios)",
    created_at: ago(14),
    last_seen: ago(0.05),
    expires_at: ahead(16),
    current: false,
  },
  {
    id: uid("session", 3),
    device_id: DEVICES[3]!.id,
    device_name: "Studio desktop",
    platform: "windows",
    address: "203.0.113.80",
    client: "lockdown/0.8.2 (windows)",
    created_at: ago(9),
    last_seen: ago(1.2),
    expires_at: ahead(21),
    current: false,
  },
  {
    id: uid("session", 4),
    device_id: DEVICES[4]!.id,
    device_name: "linux-a7f3",
    platform: "linux",
    address: "192.0.2.144",
    client: "lockdown/0.9.0 (linux)",
    created_at: ago(2),
    last_seen: ago(0.3),
    expires_at: ahead(28),
    current: false,
  },
  {
    id: uid("session", 5),
    device_id: uid("device", 9),
    device_name: null,
    platform: null,
    address: "198.51.100.23",
    client: "lockdown-web/0.9.0 (Firefox)",
    created_at: ago(0.2),
    last_seen: ago(0.15),
    expires_at: ahead(0.8),
    current: false,
  },
]

export function status(world: World): Status {
  return {
    root: "/Users/sam/Library/Application Support/Ecosystem",
    data: "/Users/sam/Library/Application Support/Lockdown",
    profile_id: uid("profile", 1),
    manager: null,
    recovery: "kit made, drilled 5 months ago",
    device_id: DEVICES[0]!.id,
    device_name: PERSON.device,
    persona_id: uid("persona", 1),
    persona_label: "Personal",
    vault_id: vaultId("personal"),
    head_seq: 1842,
    head_hash: "b3f1c09e7a2d4c8f",
    live_items: world.items.filter((i) => (i.state ?? "live") === "live").length,
    devices: DEVICES.filter((d) => !d.revoked_at).length,
    devices_revoked: DEVICES.filter((d) => d.revoked_at).length,
    identity_seq: 37,
    account: {
      server: PERSON.server,
      login: PERSON.login,
      synced_seq: 1842,
      synced_rev: 911,
      last_sync_at: ago(0.004),
      session_expires_at: ahead(10),
      transparency: {
        log_size: 48_211,
        root_prefix: "9c4e1b07",
        audited: 48_211,
        checked_at: ago(0.004),
        mirrors: [
          { url: "https://mirror-one.rivera.example", named_by: "server", at: ago(0.004), state: "agrees", size: 48_211, checked_at: ago(0.004) },
          { url: "https://kt.witness.example", named_by: "settings", at: ago(0.004), state: "agrees", size: 48_209, checked_at: ago(0.01) },
        ],
      },
    },
  }
}

const byTitle = (world: World, title: string): WItem => {
  const item = world.items.find((i) => i.title === title)
  if (!item) throw new Error(`fixture: no item called ${title}`)
  return item
}

function onItem(item: WItem, kind: string, severity: Finding["severity"], title: string, playbook: string, detail: string, field: string | null = null): Finding {
  const urls = Object.entries(item.fields).filter(([k]) => /^url\.\d+$/.test(k)).map(([, v]) => v)
  return {
    id: field ? `${kind}:${item.id}:${field}` : `${kind}:${item.id}`,
    kind: kind as Finding["kind"],
    severity,
    title,
    detail,
    key: field ? `${kind}:${field}` : kind,
    item_id: item.id,
    item_title: item.title,
    item_kind: item.kind,
    field_id: field,
    site: item.crown ? null : (urls[0] ?? null),
    related: [],
    setup_url: null,
    change_password: item.kind === "login" && urls.length > 0 && ["breached", "reused", "weak"].includes(kind),
    playbook,
    alert: null,
    at: null,
    dismissed: null,
  }
}

/** Findings in all three groups (urgent, important, suggested), and one dismissed. */
export function securityReport(world: World, dismissed: Set<string>): SecurityReport {
  const parcel = byTitle(world, "Parcelpost")
  const quill = byTitle(world, "Quill Notes")
  const meadow = byTitle(world, "Meadowlark Streaming")
  const harbor = byTitle(world, "Harbor Health")
  const tern = byTitle(world, "Tern Airlines")
  const alerts = byTitle(world, "Bluefin Bank alerts")
  const bank = byTitle(world, "Bluefin Bank")
  const findings: Finding[] = [
    onItem(parcel, "breached", "urgent", "Password found in a data breach", "breached-password", "This password has been seen 2,318 times in data breaches. Change it here and anywhere else you use it.", "pw"),
    {
      ...onItem(parcel, "exposed_email", "urgent", "Email address in a data breach", "exposed-email", "sam.rivera@northwind.example was in the Parcelpost breach of parcelpost.example (2026-08-14). What was taken: email addresses, passwords, phone numbers."),
      id: "alert:41",
      key: "alert:41",
      item_id: null,
      item_title: null,
      item_kind: null,
      alert: 41,
      at: ago(3),
      site: "https://parcelpost.example",
    },
    {
      ...onItem(parcel, "new_sign_in", "important", "New device signed in", "new-sign-in", "“linux-a7f3” signed in to your Lockdown account for the first time. If that wasn't you, revoke it in Devices."),
      id: "alert:42",
      key: "alert:42",
      item_id: null,
      item_title: null,
      item_kind: null,
      site: null,
      alert: 42,
      at: ago(2),
    },
    { ...onItem(parcel, "reused", "important", "Password used in more than one place", "reused-password", "The same password is also saved in “Quill Notes”.", "pw"), related: [{ id: quill.id, title: quill.title }] },
    { ...onItem(quill, "reused", "important", "Password used in more than one place", "reused-password", "The same password is also saved in “Parcelpost”.", "pw"), related: [{ id: parcel.id, title: parcel.title }] },
    onItem(meadow, "weak", "important", "Easy-to-guess password", "weak-password", "This password would be guessed quickly.", "pw"),
    {
      ...onItem(alerts, "lookalike", "important", "Website looks like another site", "lookalike-site", "secure.b1uefin-bank.example looks a lot like bluefin-bank.example, where your login “Bluefin Bank” is. Fake sites copy real addresses to collect passwords."),
      related: [{ id: bank.id, title: bank.title }],
    },
    {
      ...onItem(harbor, "missing_2fa", "suggested", "Two-step sign-in is available", "missing-2fa", "Harbor Health lets you add a second step when you sign in with an authenticator app or a security key. Lockdown can keep the code with this login."),
      setup_url: "https://portal.harborhealth.example/help/two-step",
    },
    {
      ...onItem(tern, "missing_2fa", "suggested", "Two-step sign-in is available", "missing-2fa", "Tern Airlines lets you add a second step when you sign in with text message or email."),
      setup_url: "https://www.ternair.example/account/security",
    },
  ]
  const live = findings.filter((f) => !dismissed.has(f.id))
  const gone = findings.filter((f) => dismissed.has(f.id)).map((f) => ({ ...f, dismissed: { at: ago(1), until: null } }))
  const snoozed = { ...onItem(byTitle(world, "Stackwise"), "missing_2fa", "suggested", "Two-step sign-in is available", "missing-2fa", "Stackwise lets you add a second step when you sign in with an authenticator app."), dismissed: { at: ago(4), until: ahead(26) } }
  const count = (s: Finding["severity"]) => live.filter((f) => f.severity === s).length
  const passwords = world.items.filter((i) => i.fields.pw).length
  return {
    findings: live,
    dismissed: [...gone, snoozed],
    counts: { urgent: count("urgent"), important: count("important"), suggested: count("suggested") },
    passwords,
    // Two passwords changed since the last check, so they wait for the next one.
    breach: { enabled: true, source: "Pwned Passwords (k-anonymity range API)", checked_at: ago(0.3), checked: passwords - 2, unchecked: 2, message: null },
    alerts: { account: true, checked_at: ago(0.3), message: null },
    directory: { source: "2fa.directory", licence: "MIT", date: "2026-10-01", sites: 3_412 },
    generated_at: NOW,
  }
}

export const PLAYBOOKS: Playbook[] = [
  {
    id: "breached-password",
    title: "This password has shown up in a data breach",
    summary: "Criminals share lists of passwords stolen from websites. This exact password is on one of those lists, so anyone can try it.",
    steps: [
      "Change this password now. Use Change password to go to the site, and let Lockdown make the new one.",
      "If you used the same password anywhere else, change it there too. Lockdown lists those places as reused passwords.",
      "Turn on two-step sign-in for this site if it offers it.",
      "Look at the site's security or recent-activity page for sign-ins you don't recognise, and sign out other sessions if it lets you.",
      "Come back and choose Check now. The warning goes away once the new password is saved.",
    ],
  },
  {
    id: "reused-password",
    title: "The same password is used in more than one place",
    summary: "If any one of these sites is breached, whoever gets the password can try it on all the others.",
    steps: [
      "Start with the most important account: your email, your bank, anything that holds money or can reset other passwords.",
      "Change its password to a new one Lockdown makes. Every site should have its own.",
      "Do the same for the other places in the list.",
      "Choose Check now. The warning clears when no two items share a password.",
    ],
  },
  {
    id: "weak-password",
    title: "This password is easy to guess",
    summary: "It is short, common, or made of words and patterns that guessing tools try first.",
    steps: ["Change it to a new password Lockdown makes for you.", "You don't need to remember it: Lockdown fills it in or copies it for you.", "Choose Check now to confirm it is no longer marked."],
  },
  {
    id: "missing-2fa",
    title: "This site offers two-step sign-in",
    summary: "With two-step sign-in a stolen password isn't enough: the site also asks for a code or a key that only you have.",
    steps: [
      "Open the site's set-up page with Turn it on, and follow its steps.",
      "If the site offers an authenticator app code, choose that one: Lockdown can keep the code with this login. Edit the item and add the set-up key the site shows you.",
      "Save the backup codes the site gives you, in a Note in Lockdown.",
      "If you already use two-step sign-in here (by text message, for example), dismiss this. Choose Check now once a code is saved to confirm.",
    ],
  },
  {
    id: "lookalike-site",
    title: "This website's address looks like another site's",
    summary: "Its address is very close to a well-known site's or to one of your other logins. Fake sites copy real addresses to collect passwords.",
    steps: [
      "Compare the two addresses letter by letter.",
      "If you didn't mean to save this site, don't sign in there. Move this item to the trash.",
      "If you typed a real password into it, change that password on the real site now and turn on two-step sign-in there.",
      "If both sites are genuine and yours, dismiss this warning.",
    ],
  },
  {
    id: "exposed-email",
    title: "Your email address was in a data breach",
    summary: "A website you have, or had, an account with lost its list of customers, and your email address was on it.",
    steps: [
      "Change your password on the site that was breached. If you no longer use it, close the account if you can.",
      "If the breach included passwords, change that password anywhere else you used it.",
      "Be wary of emails about the breach: criminals use them to trick people. Go to sites directly, not through links in emails.",
      "Turn on two-step sign-in for your email account; it is the key that can reset every other password.",
      "Dismiss this once you have done the steps that apply.",
    ],
  },
  {
    id: "new-sign-in",
    title: "A new device signed in to your Lockdown account",
    summary: "A device this one hasn't seen before signed in with your account. If it was you, there is nothing to do.",
    steps: [
      "If it was you (a new computer or phone), dismiss this.",
      "If it wasn't you, go to Devices and revoke that device now.",
      "Then change your Lockdown account password and make a new recovery kit.",
      "Look at Devices again: only your own devices should be listed.",
    ],
  },
]

export function backupStatus(on = true): BackupStatus {
  if (!on) {
    return { enabled: false, folder: null, interval_hours: 24, keep_last: 10, key_set: false, last_success_at: null, last_error: null, next_due_at: null, crown_jewels_left_out: 0, backups: [] }
  }
  return {
    enabled: true,
    folder: "/Users/sam/Backups/Lockdown",
    interval_hours: 24,
    keep_last: 10,
    key_set: true,
    last_success_at: ago(0.4),
    last_error: null,
    next_due_at: ahead(0.6),
    crown_jewels_left_out: 2,
    backups: [0.4, 1.4, 2.4, 3.4, 5.4, 9.4].map((d, i) => ({ name: `lockdown-backup-${new Date(ago(d) * 1000).toISOString().slice(0, 10)}.ldbak`, bytes: 1_240_000 - i * 9_000, made_at: ago(d) })),
  }
}

// --- sharing ------------------------------------------------------------------------

const fp = (seed: string) => {
  let h = [...seed].reduce((a, c) => (a * 131 + c.charCodeAt(0)) >>> 0, 17)
  const groups: string[] = []
  for (let i = 0; i < 6; i++) {
    h = (Math.imul(h, 1103515245) + 12345) >>> 0
    groups.push(String(h % 100000).padStart(5, "0"))
  }
  return groups.join(" ")
}

export const CONTACTS: ContactView[] = [
  { account_id: uid("account", 2), login: "alex@rivera.example", persona: "Alex Rivera", fingerprint: fp("alex"), qr: "lockdown:contact:v1:alex", verified: true, devices: 2, log_index: 18_204, post_quantum: true },
  { account_id: uid("account", 3), login: "jordan@rivera.example", persona: "Jordan Rivera", fingerprint: fp("jordan"), qr: "lockdown:contact:v1:jordan", verified: true, devices: 1, log_index: 21_977, post_quantum: true },
  { account_id: uid("account", 4), login: "priya@nair-design.example", persona: "Priya Nair", fingerprint: fp("priya"), qr: "lockdown:contact:v1:priya", verified: false, devices: 3, log_index: 40_115 },
]

export const MY_FINGERPRINT = fp("sam")

export function sharingOverview(world: World): SharingOverview {
  const school = world.items.find((i) => i.title === "Kids' school portal")!
  const harbor = world.items.find((i) => i.title === "Harbor Health")!
  const members = [
    { account_id: PERSON.accountId, login: PERSON.login, right: "manage" as const, right_words: "Can manage", owner: true, you: true, verified: true },
    { account_id: CONTACTS[0]!.account_id, login: "alex@rivera.example", right: "manage" as const, right_words: "Can manage", owner: false, you: false, verified: true },
    { account_id: CONTACTS[1]!.account_id, login: "jordan@rivera.example", right: "edit" as const, right_words: "Can edit", owner: false, you: false, verified: true },
  ]
  return {
    online: true,
    fingerprint: MY_FINGERPRINT,
    fingerprint_qr: "lockdown:contact:v1:sam",
    contacts: CONTACTS,
    vaults: [
      {
        vault_id: vaultId("family"),
        shared: true,
        kind: "family",
        right: "manage",
        can_manage: true,
        members,
        history: [
          { at: ago(200), words: "You made Family a household vault." },
          { at: ago(199), words: "Alex Rivera joined, can manage." },
          { at: ago(120), words: "Jordan Rivera joined, can edit." },
        ],
        blocked: null,
      },
      { vault_id: vaultId("personal"), shared: false, kind: "shared", right: "manage", can_manage: true, members: [members[0]!], history: [], blocked: "Personal holds crown jewels (Cold wallet, Northwind Mail recovery codes), which never leave your own vaults." },
      { vault_id: vaultId("work"), shared: false, kind: "shared", right: "manage", can_manage: true, members: [members[0]!], history: [], blocked: null },
    ],
    households: [{ vault_id: vaultId("family"), name: "Family", organizer: PERSON.login, organizing: true, members }],
    sent: [{ share_id: uid("share", 1), item_id: harbor.id, login: "alex@rivera.example", right: "view", created_at: ago(30) }],
    received: [{ share_id: uid("share", 2), item_id: school.id, from: "jordan@rivera.example", right: "view" }],
    emergency: emergencyList(),
    waiting_on_you: 1,
  }
}

export function emergencyList(): EmergencyView[] {
  return [
    {
      grant_id: uid("grant", 1),
      mine: true,
      other: "alex@rivera.example",
      state: "accepted",
      wait_days: 7,
      vaults: [vaultId("personal"), vaultId("family")],
      release_at: null,
      history: [
        { at: ago(90), words: "You named Alex Rivera as your emergency contact, with a 7-day wait." },
        { at: ago(89), words: "Alex Rivera accepted." },
      ],
    },
    {
      grant_id: uid("grant", 2),
      mine: false,
      other: "jordan@rivera.example",
      state: "invited",
      wait_days: 14,
      vaults: [],
      release_at: null,
      history: [{ at: ago(1), words: "Jordan Rivera named you as their emergency contact." }],
    },
  ]
}

export const SENDS: SendView[] = [
  { send_id: uid("send", 1), created_at: ago(1), expires_at: ahead(6), max_views: 1, views: 0, password: true },
  { send_id: uid("send", 2), created_at: ago(4), expires_at: ahead(3), max_views: 5, views: 2, password: false },
  { send_id: uid("send", 3), created_at: ago(9), expires_at: ago(2), max_views: null, views: 7, password: false },
]

export const REQUESTS: RequestView[] = [
  { request_id: uid("request", 1), created_at: ago(2), expires_at: ahead(5), answered_at: ago(1) },
  { request_id: uid("request", 2), created_at: ago(0.5), expires_at: ahead(6.5), answered_at: null },
]
