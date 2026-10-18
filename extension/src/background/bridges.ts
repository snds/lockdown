// The background's part of bridges (docs/plan/23-bridges.md): fill fixes
// for a page, the federated sign-in map's observations, the security
// center's checked change-password link, and aliases for a sign-up form.
// It holds no vault data: it asks the app, and checks what it opens.

import type { ToContent } from "../shared/messages"
import type { AliasOption, ChangeLink, FederationHints, FillFix, PageField, SiteFixView } from "../shared/protocol"
import { DesktopError, type Link } from "./link"

const FIX_MS = 30_000
const HINTS_MS = 10 * 60_000

function hostOf(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return ""
  }
}

function webPage(url: string | undefined): url is string {
  return /^https?:\/\//.test(url ?? "")
}

/** The fixes for each page's host, asked of the app at most every few
 *  seconds: a page's fields ask before every menu. */
export class Fixes {
  private cache = new Map<string, { view: SiteFixView | null; at: number }>()

  constructor(private link: Link) {}

  /** The fixes that apply on a frame, or `null` when the app can't say
   *  (not paired, locked): the page then fills as it would without them. */
  async forFrame(frame: string): Promise<SiteFixView | null> {
    const host = hostOf(frame)
    if (!host || !webPage(frame)) return null
    const hit = this.cache.get(host)
    if (hit && Date.now() - hit.at < FIX_MS) return hit.view
    let view: SiteFixView | null = null
    try {
      view = await this.link.call<SiteFixView>("site_fixes", { frame })
    } catch {
      view = null
    }
    if (this.cache.size > 200) this.cache.clear()
    // No answer (not paired, locked) is asked again sooner.
    this.cache.set(host, { view, at: view ? Date.now() : Date.now() - FIX_MS + 5_000 })
    return view
  }

  /** Why nothing fills on `frame`, when a fix says never. */
  async neverWhy(frame: string): Promise<string | null> {
    const v = await this.forFrame(frame)
    if (!v?.never) return null
    return v.never_source === "community"
      ? `Lockdown doesn't fill on ${v.host}: its sign-in breaks when it is filled.${v.note ? ` ${v.note}` : ""}`
      : `You told Lockdown never to fill on ${v.host}.`
  }

  /** The page's fields and what applies now, for the popup. */
  async fields(tab: chrome.tabs.Tab | undefined): Promise<{ host: string; fields: PageField[]; view: SiteFixView | null } | { error: string }> {
    if (!tab?.id || !webPage(tab.url)) return { error: "Fixes are for web pages." }
    const listed = (await chrome.tabs.sendMessage(tab.id, { t: "list_fields" } satisfies ToContent, { frameId: 0 }).catch(() => null)) as { fields?: PageField[] } | null
    this.cache.delete(hostOf(tab.url))
    const view = await this.forFrame(tab.url)
    return { host: hostOf(tab.url), fields: listed?.fields ?? [], view }
  }

  /** Keeps a fix on the site's logins and tells the tab's frames. */
  async set(tab: chrome.tabs.Tab | undefined, fix: FillFix): Promise<{ logins: number } | { error: string }> {
    if (!tab?.id || !webPage(tab.url)) return { error: "Fixes are for web pages." }
    const clean: FillFix = {
      never: fix?.never === true,
      fields: (Array.isArray(fix?.fields) ? fix.fields : []).slice(0, 20).map((f) => ({
        match: { id: f?.match?.id ?? null, name: f?.match?.name ?? null, autocomplete: f?.match?.autocomplete ?? null },
        role: f?.role,
      })),
    }
    try {
      const r = await this.link.call<{ logins: number }>("fix_set", { frame: tab.url, fix: clean })
      this.cache.delete(hostOf(tab.url))
      void chrome.tabs.sendMessage(tab.id, { t: "fixes_changed" } satisfies ToContent).catch(() => undefined)
      return r
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) }
    }
  }
}

/** The federated sign-in map's eyes: which site each tab was on, the
 *  provider sign-in pages that load after it, and "Sign in with …"
 *  clicks. It only reports; the app decides (crates/app/src/bridges/
 *  federation.rs). */
export class Federation {
  private hints: { data: FederationHints; at: number } | null = null
  /** The last ordinary web page each tab's top frame loaded. */
  private lastSite = new Map<number, string>()

  constructor(private link: Link) {}

  private async providers(): Promise<FederationHints["providers"]> {
    if (this.hints && Date.now() - this.hints.at < HINTS_MS) return this.hints.data.providers
    try {
      const data = await this.link.call<FederationHints>("federation_hints")
      this.hints = { data, at: Date.now() }
      return data.providers
    } catch {
      // Not paired or locked: ask again in a few seconds, not on every page.
      this.hints = { data: { providers: [] }, at: Date.now() - HINTS_MS + 5_000 }
      return []
    }
  }

  /** The provider whose sign-in page `url` is, by the signed list. */
  private async providerPage(url: string): Promise<string | null> {
    let u: URL
    try {
      u = new URL(url)
    } catch {
      return null
    }
    if (u.protocol !== "https:") return null
    for (const p of await this.providers()) {
      if (p.pages.some(([host, path]) => u.hostname === host && u.pathname.includes(path))) return p.id
    }
    return null
  }

  /** A tab's top frame loaded `url` (the browser's word for it, not the page's). */
  async topLoaded(tab: chrome.tabs.Tab, url: string) {
    if (!tab.id || !webPage(url)) return
    const provider = await this.providerPage(url)
    if (!provider) {
      this.lastSite.set(tab.id, url)
      if (this.lastSite.size > 500) this.lastSite.clear()
      return
    }
    // The site the person came from: this tab's, or that of the tab that
    // opened this sign-in window.
    const page = this.lastSite.get(tab.id) ?? (tab.openerTabId !== undefined ? this.lastSite.get(tab.openerTabId) : undefined)
    if (!page) return
    await this.link.call("federation_seen", { provider, page, auth_url: url })
  }

  /** A trusted click on a button that says "Sign in with <name>". */
  async button(tab: chrome.tabs.Tab | undefined, name: string) {
    const page = tab?.url
    if (!webPage(page) || !name) return
    const want = name.trim().toLowerCase()
    const p = (await this.providers()).find((x) => x.name.toLowerCase() === want)
    if (!p) return
    await this.link.call("federation_seen", { provider: p.id, page }).catch(() => undefined)
  }

  forgetTab(tabId: number) {
    this.lastSite.delete(tabId)
  }
}

/** Opens the security center's change-password link for a login: the
 *  site's well-known page when the app checked it is there, a curated one
 *  from the signed data, or the site's front page. */
export async function openChangeLink(link: Link, itemId: string): Promise<{ kind: ChangeLink["kind"]; checked: boolean; note: string | null; host: string } | { error: string }> {
  try {
    const l = await link.call<ChangeLink>("change_link", { item_id: itemId })
    if (!webPage(l.url)) return { error: "Lockdown opens web pages only." }
    await chrome.tabs.create({ url: l.url })
    return { kind: l.kind, checked: l.checked, note: l.note ?? null, host: hostOf(l.url) }
  } catch (e) {
    return { error: e instanceof DesktopError || e instanceof Error ? e.message : String(e) }
  }
}

/** Makes an alias for the tab's site and puts it in the page's email field. */
export async function aliasFill(link: Link, tab: chrome.tabs.Tab | undefined, itemId: string): Promise<{ address: string; service: string; filled: boolean } | { error: string }> {
  if (!tab?.id || !webPage(tab.url)) return { error: "Aliases are for web pages." }
  try {
    const opts = await link.call<{ services: AliasOption[] }>("alias_options")
    if (!opts.services.some((s) => s.item_id === itemId)) return { error: "That alias service isn't set up in Lockdown." }
    const made = await link.call<{ address: string; service: string }>("alias_create", { item_id: itemId, frame: tab.url })
    const r = (await chrome.tabs.sendMessage(tab.id, { t: "fill_email", address: made.address } satisfies ToContent, { frameId: 0 }).catch(() => null)) as { filled?: boolean } | null
    return { address: made.address, service: made.service, filled: r?.filled === true }
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) }
  }
}
