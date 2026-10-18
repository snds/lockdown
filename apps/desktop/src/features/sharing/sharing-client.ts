import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type { Access, Copied, GateInput } from "@/core/types"

/** Another person, pinned on this device after their keys checked out against the server's log. */
export interface ContactView {
  account_id: string
  login: string
  persona: string
  /** Six groups of five digits, the same on their screen as their own. */
  fingerprint: string
  /** The same as a QR payload (`lockdown:contact:v1:…`), for scanning later. */
  qr: string
  /** Whether the two of you compared the numbers and they matched. */
  verified: boolean
  devices: number
  log_index: number
  /** Their identity signs with a post-quantum key beside Ed25519, pinned here. */
  post_quantum?: boolean
}

export interface MemberView {
  account_id: string
  login: string
  right: Access
  right_words: string
  owner: boolean
  you: boolean
  verified: boolean
}

export interface HistoryLine {
  at: number
  words: string
}

export interface VaultSharingView {
  vault_id: string
  shared: boolean
  kind: "shared" | "family"
  right: Access
  can_manage: boolean
  members: MemberView[]
  history: HistoryLine[]
  /** Why it can't be shared yet (a crown jewel inside). */
  blocked: string | null
}

export interface SentShareView {
  share_id: string
  item_id: string
  login: string
  right: Access
  created_at: number
}

export interface ReceivedView {
  share_id: string
  item_id: string
  from: string
  right: Access
}

export interface SendView {
  send_id: string
  created_at: number
  expires_at: number
  max_views: number | null
  views: number
  password: boolean
}

export interface OpenedView {
  kind: "text" | "file" | "item"
  title: string
  /** The labelled values by index; fetch one with `openedValue(index)`. */
  values: { label: string; secret: boolean }[]
  file: { name: string; mime: string; size: number } | null
}

export interface RequestView {
  request_id: string
  created_at: number
  expires_at: number
  answered_at: number | null
}

export type EmergencyState = "invited" | "accepted" | "requested" | "ready" | "released" | "declined" | "revoked"

export interface EmergencyView {
  grant_id: string
  /** You named the contact (else you are the contact). */
  mine: boolean
  other: string
  state: EmergencyState
  wait_days: number
  vaults: string[]
  release_at: number | null
  history: HistoryLine[]
}

export interface HouseholdView {
  vault_id: string
  name: string
  organizer: string
  organizing: boolean
  members: MemberView[]
}

export interface SharingOverview {
  online: boolean
  fingerprint: string
  fingerprint_qr: string
  contacts: ContactView[]
  vaults: VaultSharingView[]
  households: HouseholdView[]
  sent: SentShareView[]
  received: ReceivedView[]
  emergency: EmergencyView[]
  waiting_on_you: number
}

export type EmergencyAction = "accept" | "decline" | "request" | "stop" | "revoke"

/**
 * Sharing, households and emergency access (src-tauri/src/sharing.rs),
 * kept apart from the main client so the slice stays in its own folder. A
 * link (share or request) comes back as bytes, shown once and wiped; what
 * a link holds is fetched one value at a time, as bytes.
 */
export interface SharingClient {
  overview(): Promise<SharingOverview>
  lookup(login: string): Promise<ContactView>
  /** The fingerprint the other person read out, or scanned; refused when it differs. */
  verify(account: string, fingerprint: string): Promise<ContactView>
  forget(account: string): Promise<void>
  vault(vaultId: string): Promise<VaultSharingView>
  /** Hands the vault's key to someone pinned here, through the `Approve` gate. */
  shareVault(vaultId: string, login: string, right: Access, gate?: GateInput): Promise<VaultSharingView>
  setRight(vaultId: string, account: string, right: Access): Promise<VaultSharingView>
  /** The vault moves to new keys at once. */
  removeMember(vaultId: string, account: string): Promise<VaultSharingView>
  createHousehold(name: string): Promise<string>
  addToHousehold(vaultId: string, login: string, right: Access, gate?: GateInput): Promise<VaultSharingView>
  shareItem(id: string, login: string, right: "view" | "fill-only", gate?: GateInput): Promise<SentShareView>
  unshareItem(shareId: string): Promise<void>
  /** A link to one item's fields, through the `Approve` gate; the link once. */
  sendItem(id: string, days: number, maxViews: number | null, password: number[] | null, gate?: GateInput): Promise<Uint8Array>
  sendText(title: string, text: number[], days: number, maxViews: number | null, password: number[] | null): Promise<Uint8Array>
  sends(): Promise<SendView[]>
  deleteSend(sendId: string): Promise<void>
  openSend(link: number[], password: number[] | null): Promise<OpenedView>
  openedValue(index: number): Promise<Uint8Array>
  openedFile(): Promise<Uint8Array>
  saveOpened(): Promise<string>
  createRequest(label: string, days: number): Promise<Uint8Array>
  requests(): Promise<RequestView[]>
  openAnswer(requestId: string): Promise<OpenedView>
  deleteRequest(requestId: string): Promise<void>
  requestLabel(link: number[]): Promise<string>
  answerRequest(link: number[], title: string, secret: number[]): Promise<void>
  addEmergency(login: string, days: number, vaults: string[], gate?: GateInput): Promise<EmergencyView[]>
  emergency(): Promise<EmergencyView[]>
  emergencyStep(grant: string, action: EmergencyAction): Promise<EmergencyView[]>
  emergencyOpen(grant: string): Promise<EmergencyView[]>
  /** Copies one opened value on the shell's clipboard path, with its clear timer. */
  copyOpened(index: number, clearAfterSecs: number): Promise<Copied>
  /** Copies a link made here on the same path, so it is cleared like a password. */
  copyLink(link: number[], clearAfterSecs: number): Promise<Copied>
}

const bytes = async (cmd: string, args?: Record<string, unknown>) => new Uint8Array(await invoke<ArrayBuffer>(cmd, args))

export const tauriSharingClient: SharingClient = {
  overview: () => invoke("sharing_overview"),
  lookup: (login) => invoke("contact_lookup", { login }),
  verify: (account, fingerprint) => invoke("contact_verify", { account, fingerprint }),
  forget: (account) => invoke("contact_forget", { account }),
  vault: (vaultId) => invoke("vault_sharing", { vaultId }),
  shareVault: (vaultId, login, right, gate) => invoke("vault_share", { vaultId, login, right, gate }),
  setRight: (vaultId, account, right) => invoke("vault_member_right", { vaultId, account, right }),
  removeMember: (vaultId, account) => invoke("vault_member_remove", { vaultId, account }),
  createHousehold: (name) => invoke("household_create", { name }),
  addToHousehold: (vaultId, login, right, gate) => invoke("household_add", { vaultId, login, right, gate }),
  shareItem: (id, login, right, gate) => invoke("item_share", { id, login, right, gate }),
  unshareItem: (shareId) => invoke("item_unshare", { shareId }),
  sendItem: (id, days, maxViews, password, gate) => bytes("send_item", { id, days, maxViews, password, gate }),
  sendText: (title, text, days, maxViews, password) => bytes("send_text", { title, text, days, maxViews, password }),
  sends: () => invoke("sends"),
  deleteSend: (sendId) => invoke("send_delete", { sendId }),
  openSend: (link, password) => invoke("send_open", { link, password }),
  openedValue: (index) => bytes("opened_value", { index }),
  openedFile: () => bytes("opened_file"),
  saveOpened: () => invoke("opened_save"),
  createRequest: (label, days) => bytes("request_create", { label, days }),
  requests: () => invoke("requests"),
  openAnswer: (requestId) => invoke("request_open", { requestId }),
  deleteRequest: (requestId) => invoke("request_delete", { requestId }),
  requestLabel: (link) => invoke("request_label", { link }),
  answerRequest: (link, title, secret) => invoke("request_answer", { link, title, secret }),
  addEmergency: (login, days, vaults, gate) => invoke("emergency_add", { login, days, vaults, gate }),
  emergency: () => invoke("emergency_list"),
  emergencyStep: (grant, action) => invoke("emergency_step", { grant, action }),
  emergencyOpen: (grant) => invoke("emergency_open", { grant }),
  copyOpened: (index, clearAfterSecs) => invoke("opened_copy", { index, clearAfterSecs }),
  copyLink: (link, clearAfterSecs) => invoke("link_copy", { link, clearAfterSecs }),
}

const SharingClientContext = createContext<SharingClient>(tauriSharingClient)

export const SharingClientProvider = SharingClientContext.Provider

export function useSharingClient(): SharingClient {
  return useContext(SharingClientContext)
}

/** What each right lets someone do, in the words the core uses. */
export const RIGHT_WORDS: Record<Access, string> = {
  "fill-only": "Fill only: Lockdown fills it in the browser; it is never shown or copied",
  view: "View: see, copy and fill",
  edit: "Edit: see, fill and change items",
  manage: "Manage: change items and who has access",
}

/** Short names for a right, for a select. */
export const RIGHT_NAMES: Record<Access, string> = {
  "fill-only": "Fill only",
  view: "Can view",
  edit: "Can edit",
  manage: "Can manage",
}

/** What the person reads about an emergency grant's state. */
export function emergencyWords(e: EmergencyView): string {
  switch (e.state) {
    case "invited":
      return e.mine ? `Waiting for ${e.other} to agree.` : `${e.other} named you an emergency contact.`
    case "accepted":
      return e.mine ? `${e.other} can ask for access; you'd have ${e.wait_days} days to stop it.` : `You can ask for access; ${e.other} has ${e.wait_days} days to stop it.`
    case "requested":
      return e.mine ? `${e.other} asked for access. Stop it if you didn't expect this.` : `You asked for access; it opens after the wait unless ${e.other} stops it.`
    case "ready":
      return e.mine ? `${e.other}'s wait ran out; they can open your vaults now.` : "The wait ran out; you can open the vaults now."
    case "released":
      return e.mine ? `${e.other} opened your vaults.` : "You can read these vaults."
    case "declined":
      return e.mine ? `${e.other} said no or stepped down.` : "You stepped down."
    case "revoked":
      return e.mine ? "You ended this." : `${e.other} ended this.`
  }
}

/** What each side can do next, by the grant's state (the server holds the same rules). */
export function emergencyActions(g: EmergencyView): Array<{ action: EmergencyAction | "open"; label: string; destructive?: boolean }> {
  if (g.state === "declined" || g.state === "revoked") return []
  if (g.mine) {
    const out: Array<{ action: EmergencyAction | "open"; label: string; destructive?: boolean }> = []
    if (g.state === "requested" || g.state === "ready") out.push({ action: "stop", label: "Stop it" })
    out.push({ action: "revoke", label: "End this", destructive: true })
    return out
  }
  switch (g.state) {
    case "invited":
      return [
        { action: "accept", label: "Agree" },
        { action: "decline", label: "Say no", destructive: true },
      ]
    case "accepted":
      return [
        { action: "request", label: "Ask for access" },
        { action: "decline", label: "Step down", destructive: true },
      ]
    case "requested":
      return [{ action: "decline", label: "Step down", destructive: true }]
    case "ready":
      return [{ action: "open", label: "Open the vaults" }]
    default:
      return []
  }
}

/** UTF-8 bytes of some text, as the commands take them. */
export function utf8(text: string): number[] {
  return Array.from(new TextEncoder().encode(text))
}
