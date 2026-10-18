// Passkeys in the background (docs/plan/19-passkeys.md §3): a page's
// create/get arrives from the passkey bridge in the tab's top frame, the
// app decides and signs, and the person says yes only in Lockdown's own
// frame. The page's address is the browser's (`sender.url`, `tabs.get`),
// never the page's word; the background holds no key and no vault data,
// only the open requests and the passkey summaries it was shown.

import type { PasskeyOption, PasskeyPlan, PickAnswer, SheetData } from "../shared/protocol"
import { DesktopError, type Link } from "./link"
import * as matcher from "./matcher"

/** What the app answers a passkey op with. */
type Outcome<T> =
  | { outcome: "done"; result: T }
  | { outcome: "confirm"; reason: string }
  | { outcome: "refused"; name: string; message: string }

interface PageError {
  name: string
  message: string
}

/** One open request from a page. */
interface Pending {
  rid: string
  tabId: number
  frameUrl: string
  op: "create" | "get"
  conditional: boolean
  request: Record<string, unknown>
  related: { origins: string[] } | null
  options: PasskeyOption[]
  plan: PasskeyPlan | null
  /** The RP ID the core resolved for the request. */
  rpId: string
  /** No app to ask: the sheet says so (locked). */
  locked: boolean
  created: number
}

const requests = new Map<string, Pending>()
const RID = /^[0-9a-f]{32}$/
/** A request nobody answers is dropped after this long. */
const REQUEST_MS = 10 * 60_000

function rid(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("")
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase()
  } catch {
    return ""
  }
}

function words(e: unknown): string {
  if (e instanceof DesktopError) return e.message
  return e instanceof Error ? e.message : String(e)
}

/** Sends the page its answer through the bridge and forgets the request. */
function finish(p: Pending, payload: { credential?: unknown; error?: PageError; handoff?: "same" | "modal" }) {
  requests.delete(p.rid)
  void chrome.tabs.sendMessage(p.tabId, { t: "pk_done", rid: p.rid, ...payload }, { frameId: 0 }).catch(() => undefined)
}

function expire() {
  const now = Date.now()
  for (const p of requests.values()) if (now - p.created > REQUEST_MS) finish(p, { error: { name: "NotAllowedError", message: "The request timed out." } })
}

/** The relying party's related-origins document (WebAuthn L3 §5.11), only
 *  when the page names an RP ID it isn't under; the core checks it. */
async function relatedFor(frameUrl: string, rpId: unknown): Promise<{ origins: string[] } | null> {
  if (typeof rpId !== "string" || !rpId) return null
  const host = hostOf(frameUrl)
  const rp = rpId.toLowerCase().replace(/\.$/, "")
  if (host === rp || host.endsWith(`.${rp}`) || !/^[a-z0-9.-]+$/.test(rp)) return null
  try {
    const r = await fetch(`https://${rp}/.well-known/webauthn`, { credentials: "omit", redirect: "error", cache: "no-store" })
    if (!r.ok) return null
    const text = await r.text()
    if (text.length > 65_536) return null
    const doc = JSON.parse(text) as { origins?: unknown }
    return Array.isArray(doc.origins) ? { origins: doc.origins.filter((o): o is string => typeof o === "string").slice(0, 100) } : null
  } catch {
    return null
  }
}

async function pageOf(p: { tabId: number; frameUrl: string; related: { origins: string[] } | null }, visible: boolean, confirmed: boolean) {
  const tab = await chrome.tabs.get(p.tabId)
  return { top: tab.url ?? "", frame: p.frameUrl, visible, user_confirmed: confirmed, related: p.related }
}

export class Passkeys {
  constructor(private link: Link) {}

  /** Whether Lockdown answers passkeys in pages now: paired, unlocked, and
   *  the person's setting on in the app. */
  async active(): Promise<boolean> {
    const s = await this.link.status()
    return s.link === "ready" && s.paired && !s.locked && s.passkeys
  }

  /** A page's create or get, from the bridge in its top frame. */
  async begin(
    sender: chrome.runtime.MessageSender,
    op: "create" | "get",
    mediation: string,
    request: Record<string, unknown>,
  ): Promise<{ handoff?: "same"; pending?: string; sheet?: boolean; credential?: unknown; error?: PageError }> {
    expire()
    const tabId = sender.tab?.id
    const frameUrl = sender.url ?? ""
    if (!tabId || sender.frameId !== 0 || !/^https?:/.test(frameUrl)) return { handoff: "same" }
    const conditional = mediation === "conditional"
    const s = await this.link.status()
    if (s.link !== "ready" || !s.paired || !s.passkeys) return { handoff: "same" }
    const p: Pending = {
      rid: rid(),
      tabId,
      frameUrl,
      op,
      conditional,
      request,
      related: await relatedFor(frameUrl, request.rp_id),
      options: [],
      plan: null,
      rpId: hostOf(frameUrl),
      locked: s.locked,
      created: Date.now(),
    }
    if (s.locked) {
      // A modal request gets a sheet that says so; anything else goes on.
      if (conditional) return { handoff: "same" }
      return this.open(p)
    }
    try {
      const page = await pageOf(p, false, false)
      if (op === "get") {
        const o = await this.link.call<Outcome<{ rp_id: string; options: PasskeyOption[] }>>("passkey_options", { page, request })
        if (o.outcome === "refused") return { error: { name: o.name, message: o.message } }
        if (o.outcome !== "done" || o.result.options.length === 0) return { handoff: "same" }
        p.options = o.result.options
        p.rpId = o.result.rp_id
        if (conditional) {
          // One conditional request per tab: a newer one replaces it.
          for (const old of requests.values()) if (old.tabId === tabId && old.conditional) finish(old, { error: { name: "AbortError", message: "Replaced by a newer request." } })
          requests.set(p.rid, p)
          return { pending: p.rid, sheet: false }
        }
        return this.open(p)
      }
      if (conditional) {
        // The automatic upgrade: the core allows it only after its own
        // recent password fill; otherwise the browser's own may try.
        const o = await this.link.call<Outcome<unknown>>("passkey_create", { page, request })
        return o.outcome === "done" ? { credential: o.result } : { handoff: "same" }
      }
      const o = await this.link.call<Outcome<PasskeyPlan>>("passkey_plan", { page, request })
      if (o.outcome === "refused") return { error: { name: o.name, message: o.message } }
      if (o.outcome !== "done") return { handoff: "same" }
      p.plan = o.result
      p.rpId = o.result.rp_id
      return this.open(p)
    } catch (e) {
      if (e instanceof DesktopError && (e.code === "passkeys_off" || e.code === "not_paired")) return { handoff: "same" }
      return { handoff: "same" }
    }
  }

  private open(p: Pending) {
    for (const old of requests.values()) if (old.tabId === p.tabId && !old.conditional) finish(old, { error: { name: "NotAllowedError", message: "Replaced by a newer request." } })
    requests.set(p.rid, p)
    return { pending: p.rid, sheet: true }
  }

  /** The page went away from a request (abort, timeout, navigation). */
  abort(sender: chrome.runtime.MessageSender, id: string) {
    const p = requests.get(id)
    if (p && p.tabId === sender.tab?.id) requests.delete(id)
  }

  forgetTab(tabId: number) {
    for (const [id, p] of requests) if (p.tabId === tabId) requests.delete(id)
  }

  private sheetFor(nonce: string, sender: chrome.runtime.MessageSender): Pending {
    const p = RID.test(nonce) ? requests.get(nonce) : undefined
    if (!p || p.conditional || sender.tab?.id !== p.tabId) throw new Error("this request isn't open any more")
    return p
  }

  /** What the sheet shows. No secret: titles, user names, the site. */
  sheet(nonce: string, sender: chrome.runtime.MessageSender): SheetData {
    const p = this.sheetFor(nonce, sender)
    return {
      kind: p.locked ? "locked" : p.op,
      host: hostOf(p.frameUrl),
      rp_id: p.rpId,
      rp_name: String(p.request.rp_name ?? ""),
      options: p.options,
      plan: p.plan,
    }
  }

  /** Whether the person can see the sheet: IOv2 from the frame, or the
   *  bridge's hit test of its host where the browser has none. */
  private async sheetVisible(p: Pending, frameVisible: boolean | null): Promise<boolean> {
    if (frameVisible !== null) return frameVisible
    const r = (await chrome.tabs.sendMessage(p.tabId, { t: "pk_measure", rid: p.rid }, { frameId: 0 }).catch(() => null)) as { visible?: boolean } | null
    return r?.visible === true
  }

  /** A pick in the sheet: the first answers "confirm", the second signs. */
  async sheetPick(nonce: string, sender: chrome.runtime.MessageSender, itemId: string, credentialId: string, confirmed: boolean, frameVisible: boolean | null): Promise<PickAnswer> {
    const p = this.sheetFor(nonce, sender)
    const visible = await this.sheetVisible(p, frameVisible)
    return this.use(p, itemId, credentialId, confirmed, visible)
  }

  /** A passkey picked from the inline menu for a conditional request in
   *  this tab; `visible` is the field's measurement. */
  async menuPick(tabId: number, frameId: number, itemId: string, credentialId: string, confirmed: boolean, visible: boolean): Promise<PickAnswer> {
    const p = [...requests.values()].find((r) => r.tabId === tabId && r.conditional)
    if (!p || frameId !== 0) return { refuse: "The page isn't asking for a passkey any more." }
    return this.use(p, itemId, credentialId, confirmed, visible)
  }

  /** The passkeys a login field's menu shows with its logins. */
  menuOptions(tabId: number, frameId: number): PasskeyOption[] {
    if (frameId !== 0) return []
    return [...requests.values()].find((r) => r.tabId === tabId && r.conditional)?.options ?? []
  }

  /** "Use a passkey from another device" in the inline menu. */
  menuOther(tabId: number) {
    const p = [...requests.values()].find((r) => r.tabId === tabId && r.conditional)
    if (p) finish(p, { handoff: "modal" })
  }

  private async use(p: Pending, itemId: string, credentialId: string, confirmed: boolean, visible: boolean): Promise<PickAnswer> {
    const offered = p.options.find((o) => o.item_id === itemId && o.credential_id === credentialId)
    if (!offered || p.op !== "get") return { refuse: "That passkey wasn't offered for this site." }
    const tab = await chrome.tabs.get(p.tabId)
    // The extension's own matcher first (the second deliberate action and
    // the cover rule), then the core decides again.
    const pre = await matcher.check({ top: tab.url ?? "", frame: p.frameUrl, field_visible: visible, user_confirmed: confirmed, kind: "passkey", decision: "exact" })
    if (pre.verdict === "refuse") return { refuse: pre.reason ?? "Lockdown won't sign in here." }
    if (pre.verdict === "confirm" && !confirmed) return { confirm: `Sign in to ${p.rpId} as ${offered.user_name || offered.title}?` }
    try {
      const o = await this.link.call<Outcome<unknown>>("passkey_get", {
        page: await pageOf(p, visible, confirmed),
        request: p.request,
        item_id: itemId,
        credential_id: credentialId,
        user_name: offered.user_name,
      })
      if (o.outcome === "done") {
        finish(p, { credential: o.result })
        return { done: true }
      }
      if (o.outcome === "confirm") return { confirm: o.reason }
      return { refuse: o.message }
    } catch (e) {
      return { refuse: words(e) }
    }
  }

  /** "Save passkey" in the sheet: one deliberate action makes it. */
  async sheetCreate(nonce: string, sender: chrome.runtime.MessageSender, frameVisible: boolean | null): Promise<PickAnswer> {
    const p = this.sheetFor(nonce, sender)
    if (p.op !== "create" || !p.plan || p.plan.excluded) return { refuse: "There's nothing to save." }
    const visible = await this.sheetVisible(p, frameVisible)
    if (!visible) return { refuse: "Something on the page covers Lockdown, so Lockdown won't make a passkey." }
    try {
      const o = await this.link.call<Outcome<unknown>>("passkey_create", { page: await pageOf(p, visible, true), request: p.request })
      if (o.outcome === "done") {
        finish(p, { credential: o.result })
        return { done: true }
      }
      if (o.outcome === "confirm") return { confirm: o.reason }
      if (o.name === "InvalidStateError") finish(p, { error: { name: o.name, message: o.message } })
      return { refuse: o.message }
    } catch (e) {
      return { refuse: words(e) }
    }
  }

  /** The sheet gives the request to the browser's own authenticator. */
  sheetHandoff(nonce: string, sender: chrome.runtime.MessageSender) {
    finish(this.sheetFor(nonce, sender), { handoff: "same" })
  }

  /** The person closed the sheet. An excluded credential is reported as
   *  the spec asks (InvalidStateError), after the person saw it. */
  sheetCancel(nonce: string, sender: chrome.runtime.MessageSender) {
    const p = this.sheetFor(nonce, sender)
    if (p.plan?.excluded) finish(p, { error: { name: "InvalidStateError", message: "Lockdown already holds a passkey for this account." } })
    else finish(p, { error: { name: "NotAllowedError", message: "The request was cancelled." } })
  }

  /** The locked sheet's "Try again", after the person unlocked Lockdown. */
  async sheetRetry(nonce: string, sender: chrome.runtime.MessageSender): Promise<SheetData> {
    const p = this.sheetFor(nonce, sender)
    const s = await this.link.status()
    if (!s.locked) {
      requests.delete(p.rid)
      const again = await this.begin({ tab: { id: p.tabId } as chrome.tabs.Tab, frameId: 0, url: p.frameUrl }, p.op, "optional", p.request)
      // The new request takes over the same sheet.
      const fresh = again.pending ? requests.get(again.pending) : undefined
      if (fresh) {
        requests.delete(fresh.rid)
        fresh.rid = p.rid
        requests.set(p.rid, fresh)
      } else if (again.handoff) {
        finish({ ...p }, { handoff: "same" })
      } else if (again.error) {
        finish({ ...p }, { error: again.error })
      }
    }
    return this.sheet(nonce, sender)
  }

  /** A Signal from the page: passed to the core, which checks the site. */
  async signal(sender: chrome.runtime.MessageSender, signal: Record<string, unknown>) {
    if (sender.frameId !== 0 || !sender.url || !(await this.active())) return
    await this.link.call("passkey_signal", { page: sender.url, signal }).catch(() => undefined)
  }
}
