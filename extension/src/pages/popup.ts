// The toolbar popup: pairing with the app, the logins for the page in
// front, a search over item summaries, and copy through the desktop's
// clipboard path (the value never comes to the browser). It shows
// summaries only: title, username, site, kind (D113 for crown jewels).

import type { LinkState, PageMatches, PickAnswer, SearchItem } from "../shared/protocol"

const root = document.getElementById("app") as HTMLElement
const params = new URLSearchParams(location.search)
const tabParam = params.has("tab") ? Number(params.get("tab")) : undefined

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag)
  if (cls) e.className = cls
  if (text !== undefined) e.textContent = text
  return e
}

function send<T>(msg: Record<string, unknown>): Promise<T> {
  return chrome.runtime.sendMessage(msg) as Promise<T>
}

const live = el("p", "live")
live.setAttribute("role", "status")
live.setAttribute("aria-live", "polite")

function say(text: string) {
  live.textContent = text
}

let lookalike: string[] = []
let site: { url?: string; tab_id?: number; matches: PageMatches | null } = { matches: null }

async function copy(item: { item_id: string; title: string }, button: HTMLButtonElement) {
  if (lookalike.length > 0 && button.dataset.confirm !== "1") {
    // The paste path's explicit confirm: this page imitates a saved site.
    button.dataset.confirm = "1"
    button.textContent = "Copy anyway"
    say(`This page imitates ${lookalike.join(", ")}. Copy only if you mean to paste somewhere else.`)
    return
  }
  const r = await send<{ label?: string; clears_in_secs?: number; error?: string }>({ t: "copy", item_id: item.item_id })
  if (r?.error) say(r.error)
  else say(`Copied the ${r.label} for ${item.title}. Lockdown clears it in ${r.clears_in_secs} seconds.`)
}

function itemRow(item: SearchItem): HTMLElement {
  const li = el("li", "row")
  const text = el("div", "text")
  text.append(el("span", "title", item.title || "Untitled"), el("span", "sub", item.crown_jewel ? `${item.kind} · protected` : [item.username, item.host || item.subtitle].filter(Boolean).join(" · ") || item.kind))
  li.append(text)
  if (item.copy) {
    const b = el("button", "secondary", `Copy ${item.copy.label}`)
    b.addEventListener("click", () => void copy(item, b))
    li.append(b)
  }
  return li
}

async function renderSite(section: HTMLElement) {
  site = await send<typeof site & { error?: string }>({ t: "site", tab_id: tabParam })
  section.replaceChildren()
  const m = site.matches
  if (!m) return
  lookalike = m.lookalike_of
  if (m.lookalike_of.length > 0) {
    const warn = el("p", "warn", `Stop: ${m.host} isn't ${m.lookalike_of.join(", ")}. It only looks like a site you have a login for. Lockdown won't fill here.`)
    warn.setAttribute("role", "alert")
    section.append(warn)
  }
  if (m.logins.length === 0) return
  section.append(el("h2", undefined, `For ${m.host}`))
  const ul = el("ul", "rows")
  for (const c of m.logins) {
    const li = el("li", "row")
    const text = el("div", "text")
    text.append(el("span", "title", c.title || m.host), el("span", "sub", c.crown_jewel ? "protected" : c.username))
    const fill = el("button", "primary", "Fill")
    fill.addEventListener("click", async () => {
      if (site.tab_id === undefined) return
      const r = await send<PickAnswer & { error?: string }>({ t: "fill_tab", tab_id: site.tab_id, item_id: c.item_id })
      say(r?.done ? `Filled ${c.title}.` : (r?.confirm ?? r?.refuse ?? r?.error ?? "Lockdown didn't fill."))
    })
    const change = el("button", "link", "Change password")
    change.addEventListener("click", () => void send({ t: "change_password", origin: site.url }))
    li.append(text, fill, change)
    ul.append(li)
  }
  section.append(ul)
}

function renderMain() {
  root.replaceChildren()
  const siteSection = el("section", "site")
  const search = el("input", "search")
  search.type = "search"
  search.placeholder = "Search Lockdown"
  search.setAttribute("aria-label", "Search Lockdown")
  const results = el("ul", "rows")
  results.setAttribute("aria-label", "Results")
  let timer: ReturnType<typeof setTimeout> | undefined
  let latest = 0
  const run = async () => {
    const mine = ++latest
    const r = await send<{ items?: SearchItem[]; error?: string }>({ t: "search", query: search.value })
    // An older search that answers late never replaces a newer one.
    if (mine !== latest) return
    results.replaceChildren(...(r?.items ?? []).map(itemRow))
    if (r?.error) say(r.error)
    else if (search.value && (r?.items ?? []).length === 0) say("Nothing matches.")
  }
  search.addEventListener("input", () => {
    clearTimeout(timer)
    timer = setTimeout(() => void run(), 120)
  })
  root.append(siteSection, search, results, live)
  void renderSite(siteSection)
  void run()
  search.focus()
}

let poll: ReturnType<typeof setInterval> | undefined

function renderLink(s: LinkState) {
  root.replaceChildren()
  const h = el("h1", undefined, "Lockdown")
  root.append(h)
  if (s.link !== "ready") {
    root.append(el("p", "msg", s.message || "Lockdown isn't reachable."))
    const retry = el("button", "primary", "Try again")
    retry.addEventListener("click", () => void start())
    root.append(retry)
    return
  }
  if (s.pairing) {
    root.append(
      el("p", "msg", "Check that Lockdown on this computer shows this code, then approve it there."),
      el("p", "code", `${s.pairing.code.slice(0, 3)} ${s.pairing.code.slice(3)}`),
    )
    root.lastElementChild?.setAttribute("aria-label", `Pairing code ${s.pairing.code.split("").join(" ")}`)
    return
  }
  if (!s.paired) {
    root.append(el("p", "msg", s.message || "Pair this browser with the Lockdown app on this computer to fill and save sign-ins."))
    const pair = el("button", "primary", "Pair with Lockdown")
    pair.addEventListener("click", async () => {
      pair.disabled = true
      const next = await send<LinkState & { error?: string }>({ t: "pair" })
      if (next?.error) {
        say(next.error)
        pair.disabled = false
        return
      }
      renderLink(next)
      watch()
    })
    root.append(pair, live)
    return
  }
  root.append(el("p", "msg", "Lockdown is locked. Unlock it in the app to fill and search."))
}

function watch() {
  clearInterval(poll)
  poll = setInterval(async () => {
    const s = await send<LinkState>({ t: "status" })
    if (s.pairing) return
    clearInterval(poll)
    void show(s)
  }, 1000)
}

async function show(s: LinkState) {
  if (s.link === "ready" && s.paired && !s.locked) renderMain()
  else {
    renderLink(s)
    if (s.link === "ready" && (s.pairing || s.locked)) watch()
  }
}

async function start() {
  root.replaceChildren(el("p", "msg", "Connecting to Lockdown…"))
  void show(await send<LinkState>({ t: "status" }))
}

void start()
