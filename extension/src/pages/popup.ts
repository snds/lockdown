// The toolbar popup: pairing with the app, the logins for the page in
// front, a search over item summaries, and copy through the desktop's
// clipboard path (the value never comes to the browser). It shows
// summaries only: title, username, site, kind (D113 for crown jewels).

import type { AliasOption, ChangeLink, FieldMatch, FieldRole, FillFix, LinkState, PageField, PageMatches, PickAnswer, SearchItem, SiteFixView } from "../shared/protocol"

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
    change.addEventListener("click", () => void changePassword(c.item_id, c.title || m.host))
    li.append(text, fill, change)
    ul.append(li)
  }
  section.append(ul)
  section.append(fixSection())
}

// --- the security center's checked change-password link (docs/plan/23-bridges.md §7)

type Opened = { kind: ChangeLink["kind"]; checked: boolean; note: string | null; host: string; error?: string }

async function changePassword(itemId: string, title: string) {
  const r = await send<Opened>({ t: "change_link", item_id: itemId })
  if (!r || r.error) {
    say(r?.error ?? "Lockdown couldn't find where to change it.")
    return
  }
  if (r.kind === "well_known") say(`Opened the change-password page for ${title}.`)
  else if (r.kind === "recipe") say(`Opened the page where ${r.host} changes passwords.${r.note ? ` ${r.note}` : ""}`)
  else say(r.checked ? `${r.host} has no change-password page Lockdown could find, so its front page is open.` : `Opened ${r.host}; look for its password settings.`)
}

// --- "Fix this page": per-site fill fixes (§5) -------------------------------

const ROLES: [FieldRole | "", string][] = [
  ["", "Lockdown decides"],
  ["username", "Username or email"],
  ["password", "Password"],
  ["new_password", "New password"],
  ["code", "One-time code"],
  ["ignore", "Not a sign-in field"],
]

function fieldName(f: PageField): string {
  return f.label || f.name || f.id || f.autocomplete
}

/** How a fix names a field: its id and name, or its autocomplete when it has neither. */
function matchFor(f: PageField): FieldMatch {
  return { id: f.id || null, name: f.name || null, autocomplete: f.id || f.name ? null : f.autocomplete || null }
}

function sameMatch(a: FieldMatch, b: FieldMatch): boolean {
  return (a.id ?? null) === (b.id ?? null) && (a.name ?? null) === (b.name ?? null) && (a.autocomplete ?? null) === (b.autocomplete ?? null)
}

function fixSection(): HTMLElement {
  const box = el("details", "fix")
  box.append(el("summary", undefined, "Fix this page"))
  let loaded = false
  box.addEventListener("toggle", () => {
    if (box.open && !loaded) {
      loaded = true
      void renderFix(box)
    }
  })
  return box
}

async function renderFix(box: HTMLElement) {
  const r = await send<{ host: string; fields: PageField[]; view: SiteFixView | null; error?: string }>({ t: "fix_fields", tab_id: site.tab_id })
  const body = el("div", "fix-body")
  box.querySelector(".fix-body")?.remove()
  box.append(body)
  if (!r || r.error) {
    body.append(el("p", "msg", r?.error ?? "Lockdown couldn't read this page."))
    return
  }
  body.append(el("p", "msg", "If Lockdown fills the wrong field here, say which is which. It's kept with your logins for this site and syncs with them."))
  const neverLabel = el("label", "check")
  const never = el("input")
  never.type = "checkbox"
  never.checked = r.view?.never === true && r.view.never_source === "yours"
  neverLabel.append(never, document.createTextNode(` Never fill on ${r.host}`))
  body.append(neverLabel)
  if (r.view?.never_source === "community") body.append(el("p", "msg", `Lockdown's list says not to fill on ${r.host}.${r.view.note ? ` ${r.view.note}` : ""}`))
  const choices: { field: PageField; select: HTMLSelectElement }[] = []
  const list = el("ul", "rows")
  list.setAttribute("aria-label", "Fields on this page")
  for (const f of r.fields) {
    const li = el("li", "row")
    const label = el("label", "text")
    const sel = el("select")
    for (const [value, words] of ROLES) {
      const o = el("option", undefined, words)
      o.value = value
      sel.append(o)
    }
    const mine = r.view?.fields.find((x) => x.source === "yours" && sameMatch(x.match, matchFor(f)))
    sel.value = mine?.role ?? ""
    label.append(el("span", "title", fieldName(f)), el("span", "sub", `${f.type} · now ${ROLES.find(([v]) => v === (f.role ?? ""))?.[1].toLowerCase() ?? "an ordinary field"}`), sel)
    li.append(label)
    list.append(li)
    choices.push({ field: f, select: sel })
  }
  if (r.fields.length) body.append(list)
  else body.append(el("p", "msg", "This page has no fields Lockdown can name."))
  const save = el("button", "primary", "Save fix")
  save.addEventListener("click", async () => {
    const fix: FillFix = {
      never: never.checked,
      fields: choices
        .filter((c) => c.select.value)
        .map((c) => ({
          match: matchFor(c.field),
          role: c.select.value as FieldRole,
        })),
    }
    save.disabled = true
    const done = await send<{ logins?: number; error?: string }>({ t: "fix_set", tab_id: site.tab_id, fix })
    save.disabled = false
    if (!done || done.error) say(done?.error ?? "Lockdown didn't keep the fix.")
    else say(`Kept the fix with ${done.logins} ${done.logins === 1 ? "login" : "logins"} for ${r.host}. It works on the page now.`)
  })
  body.append(save)
}

// --- "Hide my email": an alias for a sign-up form (§5) ------------------------

async function renderAliases(section: HTMLElement) {
  if (!site.url || !/^https?:/.test(site.url)) return
  const r = await send<{ services?: AliasOption[]; error?: string }>({ t: "alias_options" })
  const services = r?.services ?? []
  if (services.length === 0) return
  section.append(el("h2", undefined, "Hide my email"))
  const ul = el("ul", "rows")
  for (const svc of services) {
    const li = el("li", "row")
    const text = el("div", "text")
    text.append(el("span", "title", svc.name), el("span", "sub", svc.domain ? `on ${svc.domain}` : "a new address that forwards to you"))
    const b = el("button", "secondary", "New alias")
    b.setAttribute("aria-label", `New alias from ${svc.name}`)
    b.addEventListener("click", async () => {
      b.disabled = true
      say(`Asking ${svc.name} for a new address…`)
      const made = await send<{ address?: string; filled?: boolean; error?: string }>({ t: "alias_fill", tab_id: site.tab_id, item_id: svc.item_id })
      b.disabled = false
      if (!made || made.error || !made.address) say(made?.error ?? "No alias was made.")
      else say(made.filled ? `Made ${made.address} and put it in the email field.` : `Made ${made.address}. This page has no email field to put it in; copy it from here.`)
    })
    li.append(text, b)
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
  const aliasSection = el("section", "aliases")
  root.append(siteSection, aliasSection, search, results, live)
  void renderSite(siteSection).then(() => renderAliases(aliasSection))
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
