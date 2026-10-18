// The background (an MV3 service worker on Chromium, an event page on
// Firefox). It holds no vault data: it links to the Lockdown app, routes
// what the page's content script, the extension's own frames and the popup
// ask, and checks who asked (docs/plan/16-browser-extension.md §5).

import type { ToBackground, ToContent, Measured, MenuKind, PickWhat } from "../shared/messages"
import { extensionOrigin } from "../shared/messages"
import type { FillAnswer, LinkState, LoginCandidate, MenuData, PageMatches, PasswordSuggestion, PickAnswer, SaveOffer, WalletEntry } from "../shared/protocol"
import * as bridges from "./bridges"
import { DesktopError, Link } from "./link"
import * as matcher from "./matcher"
import { Passkeys } from "./passkeys"

const link = new Link()
const passkeys = new Passkeys(link)
const fixes = new bridges.Fixes(link)
const federation = new bridges.Federation(link)

/** One open menu or prompt, registered by the content script that owns the
 *  field. The frame the extension shows inside the page may act only for a
 *  nonce registered in its own tab. */
interface Menu {
  tabId: number
  frameId: number
  frameUrl: string
  kind: MenuKind
  created: number
  /** What the menu offered, so a pick is checked against it. */
  offered: Map<string, LoginCandidate | WalletEntry>
  /** The passkeys it offered, as `item:credential`. */
  passkeys: Set<string>
  /** The field takes a new password, so the menu may suggest one. */
  newPassword: boolean
}

const menus = new Map<string, Menu>()
const MAX_MENUS_PER_TAB = 24
const NONCE = /^[0-9a-f]{32}$/

/** A save offer waiting for the person, per tab. The password is not here:
 *  the app holds it until they answer (D-row in the doc). */
const offers = new Map<number, { offer: SaveOffer; at: number }>()
const OFFER_MS = 170_000

function isExtensionPage(sender: chrome.runtime.MessageSender, page?: string): boolean {
  const url = sender.url ?? ""
  const origin = extensionOrigin()
  if (!url.startsWith(origin + "/")) return false
  return page === undefined || new URL(url).pathname === `/${page}`
}

function isContentScript(sender: chrome.runtime.MessageSender): boolean {
  return !!sender.tab?.id && sender.frameId !== undefined && !isExtensionPage(sender) && /^https?:/.test(sender.url ?? "")
}

function words(e: unknown): string {
  if (e instanceof DesktopError) return e.message
  return e instanceof Error ? e.message : String(e)
}

function menuFor(nonce: string, sender: chrome.runtime.MessageSender): Menu {
  const m = NONCE.test(nonce) ? menus.get(nonce) : undefined
  if (!m || sender.tab?.id !== m.tabId) throw new Error("this menu isn't open any more")
  return m
}

function sendToFrame<T>(m: { tabId: number; frameId: number }, msg: ToContent): Promise<T> {
  return chrome.tabs.sendMessage(m.tabId, msg, { frameId: m.frameId }) as Promise<T>
}

async function topUrl(tabId: number): Promise<string> {
  const tab = await chrome.tabs.get(tabId)
  return tab.url ?? ""
}

/** The page's logins, each double-checked by the extension's own matcher. */
async function matchesFor(frameUrl: string): Promise<PageMatches> {
  const m = await link.call<PageMatches>("matches", { frame: frameUrl })
  const logins: LoginCandidate[] = []
  for (const c of m.logins) {
    // A crown jewel comes without its address (D113); the app's decision stands.
    if (c.saved === null || matcher.isMatch(await matcher.decision(c.saved, frameUrl, c.rule))) logins.push(c)
  }
  return { ...m, logins }
}

async function linkState(): Promise<LinkState> {
  return link.status()
}

async function menuData(m: Menu): Promise<MenuData> {
  const host = (() => {
    try {
      return new URL(m.frameUrl).host
    } catch {
      return ""
    }
  })()
  const kind = m.kind === "prompt" ? "login" : m.kind
  const base: MenuData = { kind, link: await linkState(), host, logins: [], wallet: [], lookalike_of: [], note: null, passkeys: [] }
  try {
    const never = await fixes.neverWhy(m.frameUrl)
    if (never) {
      base.note = never
      m.offered = new Map()
      m.passkeys = new Set()
      return base
    }
    if (kind === "login" || kind === "code") {
      const pm = await matchesFor(m.frameUrl)
      base.lookalike_of = pm.lookalike_of
      base.logins = kind === "code" ? pm.logins.filter((c) => c.code_field) : pm.logins.filter((c) => c.password)
      // A page asking for a passkey with conditional mediation: its
      // passkeys join its logins in the menu.
      if (kind === "login") base.passkeys = passkeys.menuOptions(m.tabId, m.frameId)
      if (base.logins.length === 0 && base.passkeys.length === 0 && pm.lookalike_of.length === 0)
        base.note = kind === "code" ? `No saved login for ${host} has a one-time code.` : `No saved login for ${host}.`
    } else {
      const w = await link.call<{ entries: WalletEntry[] }>("wallet")
      base.wallet = w.entries.filter((e) => e.kind === kind)
      if (base.wallet.length === 0) base.note = kind === "card" ? "No payment cards in Lockdown." : "No identities in Lockdown."
    }
  } catch (e) {
    base.note = words(e)
    base.link = await linkState()
  }
  // A sign-up's or a change's password field: the strong password Lockdown
  // would make for it, named by the rules it follows; made only on a pick.
  if (kind === "login" && m.newPassword) base.suggest = await link.call<PasswordSuggestion>("password_rule", { frame: m.frameUrl }).catch(() => null)
  m.offered = new Map<string, LoginCandidate | WalletEntry>([...base.logins.map((c) => [c.item_id, c] as const), ...base.wallet.map((w) => [w.item_id, w] as const)])
  m.passkeys = new Set(base.passkeys.map((p) => `${p.item_id}:${p.credential_id}`))
  return base
}

/** A passkey sign-in picked in the menu (conditional mediation): the
 *  field and the menu measured as for a fill, then the passkey's own path. */
async function pickPasskey(m: Menu, itemId: string, credentialId: string, confirmed: boolean, frameVisible: boolean | null): Promise<PickAnswer> {
  if (!m.passkeys.has(`${itemId}:${credentialId}`)) return { refuse: "That passkey wasn't offered for this field." }
  const measured = await sendToFrame<Measured>(m, { t: "measure", nonce: nonceOf(m) })
  if (!measured?.ok) return { refuse: "The field isn't on the page any more." }
  if (frameVisible === null && measured.menu_visible === false)
    return { refuse: "Something on the page covers Lockdown's menu, so Lockdown won't sign in." }
  return passkeys.menuPick(m.tabId, m.frameId, itemId, credentialId, confirmed, measured.visible)
}

/** One fill, from a pick in the menu, the popup or the shortcut. */
async function pick(m: Menu, itemId: string, what: PickWhat, field: string | null, confirmed: boolean, frameVisible: boolean | null): Promise<PickAnswer> {
  // A "never fill here" fix (the app refuses such a fill too).
  const never = await fixes.neverWhy(m.frameUrl)
  if (never) return { refuse: never }
  if (frameVisible === false) return { refuse: "Something on the page covers Lockdown's menu, so Lockdown won't fill." }
  if (what === "passkey") return pickPasskey(m, itemId, field ?? "", confirmed, frameVisible)
  const offered = m.offered.get(itemId)
  if (!offered) return { refuse: "That item wasn't offered for this field." }
  const measured = await sendToFrame<Measured>(m, { t: "measure", nonce: nonceOf(m) })
  if (!measured?.ok) return { refuse: "The field isn't on the page any more." }
  if (frameVisible === null && measured.menu_visible === false)
    return { refuse: "Something on the page covers Lockdown's menu, so Lockdown won't fill." }
  const top = await topUrl(m.tabId)
  const decision = "decision" in offered ? (offered.saved ? await matcher.decision(offered.saved, m.frameUrl, offered.rule) : offered.decision) : "exact"
  const pre = await matcher.check({ top, frame: m.frameUrl, field_visible: measured.visible, user_confirmed: confirmed, kind: what, decision })
  if (pre.verdict === "refuse") return { refuse: pre.reason ?? "Lockdown won't fill here." }
  if (pre.verdict === "confirm" && !confirmed) return { confirm: pre.reason ?? "Confirm before filling." }
  const answer = await link.call<FillAnswer>("fill", {
    item_id: itemId,
    what,
    field,
    top,
    frame: m.frameUrl,
    field_visible: measured.visible,
    user_confirmed: confirmed,
  })
  if (answer.verdict === "fill") {
    const filled = await sendToFrame<{ filled: number }>(m, { t: "apply", nonce: nonceOf(m), what, values: answer.values })
    for (const v of answer.values) v.value = ""
    return filled?.filled ? { done: true } : { refuse: "The page changed before Lockdown could fill it." }
  }
  if (answer.verdict === "confirm") return { confirm: answer.reason }
  return { refuse: answer.reason }
}

/** A strong password for a new-password field, picked in the menu: the
 *  field and the menu measured as for a fill, then made in the app under
 *  the site's rules and written into the field and its repeat. */
async function suggest(m: Menu, frameVisible: boolean | null): Promise<PickAnswer> {
  if (frameVisible === false) return { refuse: "Something on the page covers Lockdown's menu, so Lockdown won't fill." }
  const measured = await sendToFrame<Measured>(m, { t: "measure", nonce: nonceOf(m) })
  if (!measured?.ok) return { refuse: "The field isn't on the page any more." }
  if (!measured.visible) return { refuse: "The field is hidden or covered, so Lockdown won't fill it." }
  if (frameVisible === null && measured.menu_visible === false)
    return { refuse: "Something on the page covers Lockdown's menu, so Lockdown won't fill." }
  const made = await link.call<{ value: string }>("suggest", { frame: m.frameUrl })
  const filled = await sendToFrame<{ filled: number }>(m, { t: "apply_new", nonce: nonceOf(m), value: made.value })
  made.value = ""
  return filled?.filled ? { done: true } : { refuse: "The page changed before Lockdown could fill it." }
}

function nonceOf(m: Menu): string {
  for (const [n, x] of menus) if (x === m) return n
  return ""
}

function register(nonce: string, kind: MenuKind, sender: chrome.runtime.MessageSender, newPassword = false) {
  if (!NONCE.test(nonce) || !sender.tab?.id || sender.frameId === undefined || !sender.url) throw new Error("bad registration")
  const tabId = sender.tab.id
  const mine = [...menus.entries()].filter(([, m]) => m.tabId === tabId)
  if (mine.length >= MAX_MENUS_PER_TAB) {
    mine.sort((a, b) => a[1].created - b[1].created)
    menus.delete(mine[0][0])
  }
  menus.set(nonce, { tabId, frameId: sender.frameId, frameUrl: sender.url, kind, created: Date.now(), offered: new Map(), passkeys: new Set(), newPassword: kind === "login" && newPassword })
}

async function offerSave(username: string, password: string, sender: chrome.runtime.MessageSender): Promise<SaveOffer | { error: string }> {
  const tabId = sender.tab?.id
  if (!tabId || !sender.url) return { error: "no tab" }
  let user = username
  if (!user) {
    // A change-password form rarely shows the username: take the one login
    // this site has, if there is exactly one.
    const pm = await matchesFor(sender.url)
    if (pm.logins.length === 1 && pm.logins[0].username) user = pm.logins[0].username
  }
  if (!user) return { error: "no username to save under" }
  const offer = await link.call<SaveOffer>("offer_save", { page: sender.url, username: user, password })
  if (offer.offer_id) {
    offers.set(tabId, { offer, at: Date.now() })
    // A page that stays put shows the prompt now; one that navigates asks
    // for it when its top frame loads.
    void chrome.tabs.sendMessage(tabId, { t: "show_prompt" } satisfies ToContent, { frameId: 0 }).catch(() => undefined)
  }
  return { kind: offer.kind, title: offer.title }
}

function pendingOffer(tabId: number): SaveOffer | null {
  const o = offers.get(tabId)
  if (!o) return null
  if (Date.now() - o.at > OFFER_MS) {
    offers.delete(tabId)
    return null
  }
  return o.offer
}

async function activeTab(tabId?: number): Promise<chrome.tabs.Tab | undefined> {
  if (tabId !== undefined) return chrome.tabs.get(tabId)
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true })
  return tab
}

/** Finds the login field in a tab's top frame and registers it. */
async function targetLogin(tabId: number): Promise<Menu | null> {
  const r = (await chrome.tabs.sendMessage(tabId, { t: "target_login" } satisfies ToContent, { frameId: 0 }).catch(() => null)) as { nonce?: string } | null
  if (!r?.nonce) return null
  return menus.get(r.nonce) ?? null
}

async function route(msg: ToBackground, sender: chrome.runtime.MessageSender): Promise<unknown> {
  switch (msg.t) {
    // --- the content script -------------------------------------------------
    case "register":
      if (!isContentScript(sender)) throw new Error("refused")
      register(msg.nonce, msg.kind, sender, msg.new_password === true)
      return { ok: true }
    case "unregister": {
      if (!isContentScript(sender)) throw new Error("refused")
      const m = menus.get(msg.nonce)
      if (m && m.tabId === sender.tab?.id && m.frameId === sender.frameId) menus.delete(msg.nonce)
      return { ok: true }
    }
    case "offer_save":
      if (!isContentScript(sender)) throw new Error("refused")
      if (typeof msg.password !== "string" || !msg.password) return { error: "nothing to save" }
      return offerSave(String(msg.username ?? ""), msg.password, sender)
    case "top_ready":
      if (!isContentScript(sender) || sender.frameId !== 0) return { prompt: false }
      // The federated map: a provider's sign-in page loading in this tab
      // (or in a window it opened) after a site's page.
      void federation.topLoaded(sender.tab!, sender.url!).catch(() => undefined)
      return { prompt: pendingOffer(sender.tab!.id!) !== null }
    case "fixes":
      if (!isContentScript(sender)) throw new Error("refused")
      return fixes.forFrame(sender.url!)
    case "provider_button":
      if (!isContentScript(sender)) throw new Error("refused")
      await federation.button(sender.tab!, String(msg.name ?? ""))
      return { ok: true }

    // --- the menu and prompt frames -----------------------------------------
    case "menu_data":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      return menuData(menuFor(msg.nonce, sender))
    case "menu_pick": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      if (m.kind === "prompt") throw new Error("refused")
      try {
        const answer = await pick(m, msg.item_id, msg.what, msg.field ?? null, msg.confirmed === true, msg.frame_visible)
        if (answer.done) void sendToFrame(m, { t: "close", nonce: msg.nonce }).catch(() => undefined)
        return answer
      } catch (e) {
        return { refuse: words(e) }
      }
    }
    case "menu_suggest": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      if (!m.newPassword) return { refuse: "This field doesn't take a new password." }
      try {
        const answer = await suggest(m, msg.frame_visible)
        if (answer.done) void sendToFrame(m, { t: "close", nonce: msg.nonce }).catch(() => undefined)
        return answer
      } catch (e) {
        return { refuse: words(e) }
      }
    }
    case "menu_passkey_other": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      passkeys.menuOther(m.tabId)
      await sendToFrame(m, { t: "close", nonce: msg.nonce }).catch(() => undefined)
      return { ok: true }
    }
    case "menu_close": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      await sendToFrame(m, { t: "close", nonce: msg.nonce }).catch(() => undefined)
      return { ok: true }
    }
    case "prompt_data": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      if (m.kind !== "prompt") throw new Error("refused")
      return { offer: pendingOffer(m.tabId) }
    }
    case "prompt_answer": {
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      const m = menuFor(msg.nonce, sender)
      if (m.kind !== "prompt") throw new Error("refused")
      const offer = pendingOffer(m.tabId)
      offers.delete(m.tabId)
      if (!offer?.offer_id) return { done: true }
      try {
        if (msg.save) {
          const saved = await link.call<{ kind: string }>("save", { offer_id: offer.offer_id })
          return { done: true, saved: saved.kind }
        }
        await link.call("dismiss", { offer_id: offer.offer_id })
        return { done: true }
      } catch (e) {
        return { error: words(e) }
      } finally {
        void sendToFrame(m, { t: "close", nonce: msg.nonce }).catch(() => undefined)
      }
    }

    // --- the popup ----------------------------------------------------------
    case "status":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return linkState()
    case "pair":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return link.pair()
    case "search":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return link.call("search", { query: String(msg.query ?? "").slice(0, 200) })
    case "copy":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return link.call("copy", { item_id: msg.item_id })
    case "site": {
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      const tab = await activeTab(msg.tab_id)
      const url = tab?.url ?? ""
      if (!/^https?:/.test(url)) return { url, matches: null }
      return { url, tab_id: tab?.id, matches: await matchesFor(url) }
    }
    case "fill_tab": {
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      const never = await fixes.neverWhy((await activeTab(msg.tab_id))?.url ?? "")
      if (never) return { refuse: never }
      const m = await targetLogin(msg.tab_id)
      if (!m) return { refuse: "There's no sign-in field on this page." }
      const data = await menuData(m)
      if (!data.logins.some((c) => c.item_id === msg.item_id)) return { refuse: "That login isn't for this page." }
      try {
        return await pick(m, msg.item_id, "login", null, false, null)
      } catch (e) {
        return { refuse: words(e) }
      }
    }
    case "change_link":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return bridges.openChangeLink(link, String(msg.item_id ?? ""))
    case "fix_fields": {
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      const tab = await activeTab(msg.tab_id)
      return fixes.fields(tab)
    }
    case "fix_set": {
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      const tab = await activeTab(msg.tab_id)
      return fixes.set(tab, msg.fix)
    }
    case "alias_options":
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      return link.call("alias_options")
    case "alias_fill": {
      if (!isExtensionPage(sender, "popup.html")) throw new Error("refused")
      const tab = await activeTab(msg.tab_id)
      return bridges.aliasFill(link, tab, String(msg.item_id ?? ""))
    }

    // --- passkeys (docs/plan/19-passkeys.md) --------------------------------
    case "pk_begin":
      if (!isContentScript(sender)) throw new Error("refused")
      if (msg.op !== "create" && msg.op !== "get") throw new Error("refused")
      return passkeys.begin(sender, msg.op, String(msg.mediation ?? "optional"), msg.request ?? {})
    case "pk_abort":
      if (!isContentScript(sender)) throw new Error("refused")
      passkeys.abort(sender, String(msg.rid ?? ""))
      return { ok: true }
    case "pk_status":
      if (!isContentScript(sender)) throw new Error("refused")
      return { active: sender.frameId === 0 && (await passkeys.active()) }
    case "pk_signal":
      if (!isContentScript(sender)) throw new Error("refused")
      await passkeys.signal(sender, msg.signal ?? {})
      return { ok: true }
    case "pk_sheet":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      return passkeys.sheet(msg.nonce, sender)
    case "pk_sheet_pick":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      return passkeys.sheetPick(msg.nonce, sender, String(msg.item_id), String(msg.credential_id), msg.confirmed === true, msg.frame_visible)
    case "pk_sheet_create":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      return passkeys.sheetCreate(msg.nonce, sender, msg.frame_visible)
    case "pk_sheet_handoff":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      passkeys.sheetHandoff(msg.nonce, sender)
      return { ok: true }
    case "pk_sheet_cancel":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      passkeys.sheetCancel(msg.nonce, sender)
      return { ok: true }
    case "pk_sheet_retry":
      if (!isExtensionPage(sender, "frame.html")) throw new Error("refused")
      return passkeys.sheetRetry(msg.nonce, sender)
  }
  throw new Error("unknown message")
}

chrome.runtime.onMessage.addListener((msg: ToBackground, sender, sendResponse) => {
  route(msg, sender).then(
    (v) => sendResponse(v),
    (e) => sendResponse({ error: words(e) }),
  )
  return true
})

chrome.tabs.onRemoved.addListener((tabId) => {
  offers.delete(tabId)
  federation.forgetTab(tabId)
  passkeys.forgetTab(tabId)
  for (const [n, m] of menus) if (m.tabId === tabId) menus.delete(n)
})

// --- the fill shortcut (`fill-hotkey`) ----------------------------------------

let cycle: { tabId: number; frameUrl: string; index: number; at: number } | null = null
const CYCLE_MS = 8_000

/** The keyboard shortcut: fills the best login for the focused field, and
 *  the next one on each press within a few seconds. On a code, card or
 *  address field it opens the menu instead, where the second action is. */
export async function onCommand(command: string): Promise<PickAnswer | null> {
  if (command !== "fill-login") return null
  const tab = await activeTab()
  if (!tab?.id) return null
  const focused = (await chrome.tabs.sendMessage(tab.id, { t: "hotkey" } satisfies ToContent).catch(() => null)) as { nonce?: string } | null
  const m = (focused?.nonce && menus.get(focused.nonce)) || (await targetLogin(tab.id))
  if (!m) return null
  if (m.kind !== "login") {
    await sendToFrame(m, { t: "open_menu", nonce: nonceOf(m) }).catch(() => undefined)
    return null
  }
  const data = await menuData(m)
  if (data.logins.length === 0) return { refuse: data.note ?? "No saved login for this page." }
  const again = cycle && cycle.tabId === tab.id && cycle.frameUrl === m.frameUrl && Date.now() - cycle.at < CYCLE_MS
  const index = again && cycle ? (cycle.index + 1) % data.logins.length : 0
  cycle = { tabId: tab.id, frameUrl: m.frameUrl, index, at: Date.now() }
  try {
    return await pick(m, data.logins[index].item_id, "login", null, false, null)
  } catch (e) {
    return { refuse: words(e) }
  }
}

chrome.commands.onCommand.addListener((command) => {
  void onCommand(command)
})

// The shortcut's handler, reachable from the extension's own contexts (the
// end-to-end test drives it, since a headless browser has no keyboard
// shortcuts). Pages cannot reach the background's global scope.
;(globalThis as unknown as { lockdownCommand: typeof onCommand }).lockdownCommand = onCommand

// --- HTTP Basic Auth (`basic-auth-fill`) ---------------------------------------

const answered = new Set<string>()

async function basicAuth(details: chrome.webRequest.OnAuthRequiredDetails): Promise<chrome.webRequest.BlockingResponse> {
  // Proxies, sub-resources and a second challenge for the same request
  // (the credentials were wrong) all go to the browser's own prompt.
  if (details.isProxy || details.type !== "main_frame" || answered.has(details.requestId)) return {}
  answered.add(details.requestId)
  if (answered.size > 500) answered.clear()
  try {
    const answer = await link.call<FillAnswer>("basic_auth", { url: details.url })
    if (answer.verdict !== "fill") return {}
    const user = answer.values.find((v) => v.role === "user")?.value ?? ""
    const pw = answer.values.find((v) => v.role === "pw")?.value ?? ""
    if (!pw) return {}
    return { authCredentials: { username: user, password: pw } }
  } catch {
    return {}
  }
}

const isFirefox = navigator.userAgent.includes("Firefox/")
if (chrome.webRequest?.onAuthRequired) {
  if (isFirefox) {
    // Firefox: a blocking listener may return a promise.
    chrome.webRequest.onAuthRequired.addListener(
      ((details: chrome.webRequest.OnAuthRequiredDetails) => basicAuth(details)) as unknown as Parameters<typeof chrome.webRequest.onAuthRequired.addListener>[0],
      { urls: ["<all_urls>"] },
      ["blocking"],
    )
  } else {
    // Chromium: `asyncBlocking` with a callback (needs webRequestAuthProvider).
    chrome.webRequest.onAuthRequired.addListener(
      ((details: chrome.webRequest.OnAuthRequiredDetails, callback?: (r: chrome.webRequest.BlockingResponse) => void) => {
        void basicAuth(details).then((r) => callback?.(r))
      }) as unknown as Parameters<typeof chrome.webRequest.onAuthRequired.addListener>[0],
      { urls: ["<all_urls>"] },
      ["asyncBlocking"],
    )
  }
}

void matcher.load()
