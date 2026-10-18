// Lockdown's menu and save prompt, inside the page but in the extension's
// own origin: the page cannot read this document or style it. Before a
// click counts, the frame must be what the person sees: on Chromium,
// IntersectionObserver v2 (`trackVisibility`) reports whether anything
// covers, fades or transforms this frame; elsewhere the content script's
// measurement of the frame's host stands in (docs/plan/16-browser-extension.md §6).

import type { FillWhat, LoginCandidate, MenuData, PickAnswer, SaveOffer, WalletEntry } from "../shared/protocol"

const [mode, nonce] = location.hash.slice(1).split(":")
const root = document.getElementById("app") as HTMLElement

/** `true` / `false` from IntersectionObserver v2; `null` where the browser
 *  has no such signal. */
let visible: boolean | null = null
let shownAt = Date.now()
/** A click this soon after the menu appeared is not a deliberate one
 *  (a page can open the menu under a pointer that is already moving). */
const SETTLE_MS = 400

try {
  const io = new IntersectionObserver(
    (entries) => {
      const e = entries[entries.length - 1] as IntersectionObserverEntry & { isVisible?: boolean }
      if (typeof e.isVisible === "boolean") {
        if (e.isVisible && visible !== true) shownAt = Date.now()
        visible = e.isVisible
      }
    },
    { threshold: [1.0], trackVisibility: true, delay: 100 } as IntersectionObserverInit,
  )
  io.observe(document.body)
} catch {
  visible = null
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag)
  if (cls) e.className = cls
  if (text !== undefined) e.textContent = text
  return e
}

/** Two heights only, so the page learns no more from the frame's size than
 *  "something to offer" or "a message". */
function size(list: boolean) {
  window.parent.postMessage({ lockdown: "height", px: list ? 232 : mode === "prompt" ? 150 : 64 }, "*")
}

function send<T>(msg: Record<string, unknown>): Promise<T> {
  return chrome.runtime.sendMessage({ ...msg, nonce }) as Promise<T>
}

function close() {
  void send({ t: "menu_close" })
}

function header(host: string): HTMLElement {
  const h = el("div", "head")
  h.append(el("span", "brand", "Lockdown"), el("span", "host", host))
  const x = el("button", "close", "×")
  x.setAttribute("aria-label", "Close")
  x.addEventListener("click", close)
  h.append(x)
  return h
}

function message(host: string, text: string, tone: "plain" | "stop" = "plain") {
  root.replaceChildren(header(host))
  const p = el("p", tone === "stop" ? "msg stop" : "msg", text)
  p.setAttribute("role", tone === "stop" ? "alert" : "status")
  root.append(p)
  size(false)
}

function deliberate(e: MouseEvent | KeyboardEvent): boolean {
  return e.isTrusted && visible !== false && Date.now() - shownAt >= SETTLE_MS
}

/** Buttons stay disabled until the frame has been on screen, uncovered, for
 *  a moment, and go disabled again whenever something covers it. */
function arm() {
  const buttons = Array.from(root.querySelectorAll<HTMLButtonElement>("button:not(.close)"))
  const update = () => {
    const ready = visible !== false && Date.now() - shownAt >= SETTLE_MS
    for (const b of buttons) if (b.dataset.busy !== "1") b.disabled = !ready
    return ready
  }
  if (!update()) {
    const t = setInterval(() => {
      if (!root.contains(buttons[0] ?? null)) clearInterval(t)
      else update()
    }, 100)
  }
}

async function pick(data: MenuData, itemId: string, what: FillWhat, field: string | null, confirmed: boolean) {
  if (visible === false) {
    message(data.host, "Something on this page covers Lockdown's menu, so Lockdown won't fill.", "stop")
    return
  }
  const answer = await send<PickAnswer & { error?: string }>({ t: "menu_pick", item_id: itemId, what, field, confirmed, frame_visible: visible })
  if (answer?.done) return
  if (answer?.confirm) {
    root.replaceChildren(header(data.host))
    const p = el("p", "msg", answer.confirm)
    p.setAttribute("role", "status")
    const row = el("div", "actions")
    const go = el("button", "primary", what === "card" ? "Fill card" : what === "code" ? "Fill code" : "Fill")
    go.addEventListener("click", (e) => {
      if (deliberate(e)) void pick(data, itemId, what, field, true)
    })
    const no = el("button", "secondary", "Cancel")
    no.addEventListener("click", close)
    row.append(go, no)
    root.append(p, row)
    size(true)
    shownAt = Date.now()
    arm()
    go.focus()
    return
  }
  message(data.host, answer?.refuse ?? answer?.error ?? "Lockdown didn't fill.", "stop")
}

function row(title: string, sub: string, onPick: (e: MouseEvent) => void, badge?: string): HTMLElement {
  const b = el("button", "item")
  const t = el("span", "title", title || "Untitled")
  if (badge) t.append(el("span", "badge", badge))
  b.append(t, el("span", "sub", sub))
  b.addEventListener("click", onPick)
  return b
}

function renderMenu(data: MenuData) {
  if (data.link.link !== "ready") return message(data.host, data.link.message || "Lockdown isn't reachable.")
  if (!data.link.paired) return message(data.host, "Pair this browser with Lockdown first: open Lockdown from the toolbar.")
  if (data.link.locked) return message(data.host, "Lockdown is locked. Unlock it to fill.")
  if (data.lookalike_of.length > 0 && data.logins.length === 0) {
    return message(
      data.host,
      `Stop: ${data.host} isn't ${data.lookalike_of.join(", ")}. It looks like a site you have a login for, so Lockdown won't fill here.`,
      "stop",
    )
  }
  const list: HTMLElement[] = []
  const what = data.kind
  if (what === "login" || what === "code") {
    for (const c of data.logins as LoginCandidate[]) {
      const sub = c.crown_jewel ? "Protected" : what === "code" ? `One-time code · ${c.username}` : c.username || c.saved || ""
      list.push(
        row(c.title, sub, (e) => {
          if (deliberate(e)) void pick(data, c.item_id, what, what === "code" ? c.code_field : null, false)
        }, c.decision === "exact" ? undefined : c.decision === "equivalent" ? "related site" : "same site"),
      )
    }
  } else {
    for (const w of data.wallet as WalletEntry[]) {
      list.push(
        row(w.title, w.crown_jewel ? "Protected" : w.subtitle, (e) => {
          if (deliberate(e)) void pick(data, w.item_id, what, null, false)
        }),
      )
    }
  }
  // Nothing to offer on a field that merely looks like a sign-in field (a
  // sign-up form, a new site): the menu gets out of the way.
  if (list.length === 0) return close()
  root.replaceChildren(header(data.host))
  const ul = el("div", "list")
  ul.setAttribute("role", "list")
  ul.append(...list)
  root.append(ul)
  size(true)
  arm()
}

async function renderPrompt() {
  const { offer } = await send<{ offer: SaveOffer | null }>({ t: "prompt_data" })
  if (!offer) return close()
  root.replaceChildren(header(""))
  const q =
    offer.kind === "update" ? `Update the password for ${offer.title} in Lockdown?` : `Save this login for ${offer.title} in Lockdown?`
  const p = el("p", "msg", q)
  if (offer.username) p.append(el("span", "sub block", offer.username))
  const actions = el("div", "actions")
  const yes = el("button", "primary", offer.kind === "update" ? "Update" : "Save")
  const no = el("button", "secondary", "Not now")
  const answer = (save: boolean) => async (e: MouseEvent) => {
    if (!deliberate(e)) return
    yes.dataset.busy = no.dataset.busy = "1"
    yes.disabled = no.disabled = true
    const r = await send<{ done?: boolean; error?: string }>({ t: "prompt_answer", save })
    if (r?.error) message("", r.error, "stop")
  }
  yes.addEventListener("click", answer(true))
  no.addEventListener("click", answer(false))
  actions.append(yes, no)
  root.append(p, actions)
  size(false)
  arm()
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") close()
})

if (!/^[0-9a-f]{32}$/.test(nonce ?? "")) {
  root.textContent = ""
} else if (mode === "prompt") {
  void renderPrompt()
} else {
  void send<MenuData & { error?: string }>({ t: "menu_data" }).then((d) => {
    if (!d || d.error) return message("", d?.error ?? "Lockdown isn't reachable.")
    renderMenu(d)
  })
}
