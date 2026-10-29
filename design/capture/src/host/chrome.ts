// The extension's background, faked: what `chrome.runtime.sendMessage`
// answers the popup and the in-page frames (extension/src/background/index.ts
// routes the same messages to the app over native messaging). Answers come
// from the same fixture as the desktop's, so the popup lists Sam's items.

import { OPEN_HOW } from "../../../../extension/src/shared/protocol"
import type {
  AliasOption,
  LinkState,
  LoginCandidate,
  MenuData,
  PageMatches,
  SaveOffer,
  SearchItem,
  SheetData,
} from "../../../../extension/src/shared/protocol"

import { codesOf, freshWorld, summaryOf, type WItem } from "../fixture/world"

export interface ExtScenario {
  link: LinkState | null
  /** The tab the popup was opened over: its address and what matched it. */
  site?: { url: string; matches: PageMatches | null }
  /** A search that forgave a typo (`exact: false`). */
  closeMatch?: boolean
  aliases?: AliasOption[]
  /** The in-page menu's data, the save prompt's offer, the passkey sheet's. */
  menu?: MenuData
  prompt?: SaveOffer | null
  sheet?: SheetData
}

const ready = (over: Partial<LinkState> = {}): LinkState => ({ link: "ready", message: "", paired: true, locked: false, pairing: null, passkeys: true, ...over })
export const LINK = {
  ready,
  connecting: null,
  noApp: (): LinkState => ({ link: "no_app", message: "Lockdown isn't running on this computer. Open the app, then try again.", paired: false, locked: true, pairing: null, passkeys: false }),
  unpaired: () => ready({ paired: false, locked: true }),
  pairing: () => ready({ paired: false, locked: true, pairing: { code: "482913" } }),
  locked: () => ready({ locked: true }),
}

const host = (url: string) => {
  try {
    return new URL(url).host
  } catch {
    return ""
  }
}

export function candidate(i: WItem, decision: LoginCandidate["decision"] = "exact", asked = false): LoginCandidate {
  return {
    item_id: i.id,
    title: i.title,
    username: i.crown ? "" : (i.fields.user ?? ""),
    saved: i.fields["url.0"] ?? null,
    rule: null,
    decision,
    crown_jewel: i.crown ?? false,
    password: Boolean(i.fields.pw),
    code_field: i.fields["code.0"] ? "code.0" : null,
    edited_at: 0,
    asked,
  }
}

/** The fixture's items, for building scenarios. */
export function items(): WItem[] {
  return freshWorld().items
}

export function byTitle(title: string): WItem {
  const i = items().find((x) => x.title === title)
  if (!i) throw new Error(`fixture: no item called ${title}`)
  return i
}

/** The message handler a scenario stands for. */
export function background(s: ExtScenario): (msg: Record<string, unknown>) => unknown {
  const world = freshWorld()
  const live = world.items.filter((i) => (i.state ?? "live") === "live")
  const search = (query: string): SearchItem[] => {
    const q = query.trim().toLowerCase()
    const hits = q ? live.filter((i) => [i.title, i.fields.user ?? "", i.fields["url.0"] ?? ""].some((t) => t.toLowerCase().includes(q))) : live.filter((i) => i.used !== undefined && i.used < 3)
    return hits.slice(0, 8).map((i) => {
      const r = summaryOf(i)
      return {
        item_id: i.id,
        kind: i.kind,
        title: i.title,
        username: r.username,
        host: host(r.site),
        subtitle: r.subtitle,
        crown_jewel: r.crown_jewel,
        copy: r.copy ? { label: r.copy.label, how: r.copy.how, field_id: r.copy.field_id } : null,
        exact: !(s.closeMatch && i === hits[hits.length - 1]),
      }
    })
  }
  let pairing = false
  return (msg) => {
    switch (msg.t) {
      case "status":
        return s.link ?? new Promise(() => {})
      case "menu_link":
        return pairing ? LINK.pairing() : s.link
      case "pair":
      case "menu_pair":
        pairing = true
        return LINK.pairing()
      case "site":
        return { url: s.site?.url, tab_id: 7, matches: s.site?.matches ?? null }
      case "search":
        return { items: search(String(msg.query ?? "")) }
      case "copy": {
        const i = world.items.find((x) => x.id === msg.item_id)
        return { label: summaryOf(i!).copy?.label ?? "password", clears_in_secs: 30 }
      }
      case "code_show": {
        const i = world.items.find((x) => x.id === msg.item_id)!
        const c = codesOf([i])[0]
        return { code: c?.current ?? "", seconds_left: c?.seconds_left ?? 30, title: i.title }
      }
      case "fill_tab":
        return { done: true }
      case "change_link":
        return { url: "https://secure.bluefin-bank.example/.well-known/change-password", kind: "well_known", checked: true, note: null }
      case "fix_fields":
        return {
          host: host(s.site?.url ?? ""),
          fields: [
            { id: "login_id", name: "login_id", autocomplete: "", type: "text", label: "Customer number", role: "username" },
            { id: "pin", name: "pin", autocomplete: "off", type: "password", label: "PIN", role: "password" },
            { id: "otp", name: "otp", autocomplete: "one-time-code", type: "text", label: "Code from your card reader", role: null },
          ],
          view: { host: host(s.site?.url ?? ""), never: false, never_source: null, fields: [], note: null, yours: 0 },
        }
      case "fix_set":
        return { logins: 1 }
      case "alias_options":
        return { services: s.aliases ?? [] }
      case "alias_fill":
        return { address: "quiet.otter.7741@relay.rivera.example", filled: true }
      case "menu_data":
        return s.menu ?? { error: "This menu isn't open any more." }
      case "prompt_data":
        return { offer: s.prompt ?? null }
      case "pk_sheet":
      case "pk_sheet_retry":
        return s.sheet ?? { error: "This request isn't open any more." }
      case "menu_pick":
        return { confirm: "Fill the card ending 1111 into this page?" }
      case "pk_sheet_pick":
        return { confirm: "Sign in to pixelforge.example as sam_r?" }
      case "open_app":
        // A running app brings its window forward; with none, this stand-in
        // can't start one, so the popup says how (docs/plan/37-cli-popup.md §2).
        return s.link?.link === "ready" ? { opened: "shown" } : { error: OPEN_HOW }
      default:
        return {}
    }
  }
}

/** Puts the fake background behind `chrome.runtime` in this window. */
export function installChrome(handler: (msg: Record<string, unknown>) => unknown) {
  const w = window as unknown as { chrome?: Record<string, unknown> }
  const runtime = {
    id: "dbdblnolpiehhlabpmhobmkbjipmebpe",
    sendMessage: (msg: Record<string, unknown>) => Promise.resolve(handler(msg)),
    getURL: (p: string) => p,
  }
  w.chrome = { ...(w.chrome ?? {}), runtime }
}
