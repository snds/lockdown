// The content script, in every frame, in the extension's isolated world.
// It treats the page as hostile: it finds the fields, measures them, shows
// Lockdown's menu in an extension-owned frame inside a closed shadow root
// in the browser's top layer, and writes the values the core approved. It
// never holds vault data beyond the moment of a fill, and the page can
// neither read nor restyle the menu (docs/plan/16-browser-extension.md §6).

import type { MenuKind, ToContent } from "../shared/messages"
import { extensionOrigin } from "../shared/messages"
import type { FillWhat } from "../shared/protocol"
import { capture, classify, emailField, firstLoginField, isInput, pageFields } from "./detect"
import { apply, write } from "./fill"
import { ensureFixes, resetFixes } from "./fixes"
import { makeHost } from "./host"
import { fieldVisibility, hostVisible } from "./visibility"

const ORIGIN = extensionOrigin()

interface Field {
  el: HTMLInputElement | null
  kind: MenuKind
}

interface Shown {
  nonce: string
  host: HTMLElement
  frame: HTMLIFrameElement
  el: HTMLInputElement | null
}

const fields = new Map<string, Field>()
let menu: Shown | null = null
let prompt: Shown | null = null

function nonce(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("")
}

async function register(el: HTMLInputElement | null, kind: MenuKind): Promise<string> {
  const n = nonce()
  fields.set(n, { el, kind })
  await chrome.runtime.sendMessage({ t: "register", nonce: n, kind })
  return n
}

function forget(n: string) {
  fields.delete(n)
  void chrome.runtime.sendMessage({ t: "unregister", nonce: n }).catch(() => undefined)
}

const MENU_W = 320
const MENU_H = 56

/** Puts the menu under the field, or over it when there is more room
 *  there, and never past the edge of this frame: a menu the frame clips is
 *  one IntersectionObserver v2 cannot call visible, so it would never arm. */
function place(host: HTMLElement, el: HTMLInputElement, wanted?: number) {
  const r = el.getBoundingClientRect()
  const want = wanted ?? (Number(host.dataset.want) || MENU_H)
  host.dataset.want = String(want)
  const roomBelow = window.innerHeight - r.bottom - 8
  const roomAbove = r.top - 8
  const above = roomBelow < want && roomAbove > roomBelow
  const h = Math.max(40, Math.min(want, above ? roomAbove : roomBelow))
  host.style.height = `${Math.round(h)}px`
  const top = above ? r.top - h - 4 : r.bottom + 4
  const left = Math.max(4, Math.min(r.left, window.innerWidth - MENU_W - 4))
  host.style.top = `${Math.round(top)}px`
  host.style.left = `${Math.round(left)}px`
}

async function openMenu(el: HTMLInputElement, kind: FillWhat, existing?: string) {
  if (menu && menu.el === el) return
  closeMenu()
  const before = el.value
  const n = existing ?? (await register(el, kind))
  // The person moved on, or started typing, while the menu was being set up.
  if (document.activeElement !== el || el.value !== before) {
    if (!existing) forget(n)
    return
  }
  const { host, frame } = makeHost(chrome.runtime.getURL(`frame.html#menu:${n}`), "Lockdown", MENU_W, MENU_H)
  menu = { nonce: n, host, frame, el }
  place(host, el)
}

function closeMenu() {
  if (!menu) return
  menu.host.remove()
  forget(menu.nonce)
  menu = null
}

async function showPrompt() {
  if (window.top !== window || prompt) return
  const n = await register(null, "prompt")
  const { host, frame } = makeHost(chrome.runtime.getURL(`frame.html#prompt:${n}`), "Save to Lockdown", MENU_W, MENU_H)
  host.style.top = "12px"
  host.style.right = "12px"
  host.style.left = "auto"
  host.style.height = "150px"
  prompt = { nonce: n, host, frame, el: null }
}

function closePrompt() {
  if (!prompt) return
  prompt.host.remove()
  forget(prompt.nonce)
  prompt = null
}

// --- the field the person is in ----------------------------------------------

/** While Lockdown writes a fill, the focus it moves is its own. */
let filling = false

document.addEventListener(
  "focusin",
  (e) => {
    const el = e.target
    if (filling || !isInput(el) || !e.isTrusted) return
    // The site's fill fixes first: they can say what the field is, or
    // that nothing fills here.
    void ensureFixes().then(() => {
      if (document.activeElement !== el) return
      const kind = classify(el)
      if (kind) void openMenu(el, kind)
    })
  },
  true,
)

document.addEventListener(
  "focusout",
  () => {
    setTimeout(() => {
      if (!menu) return
      const active = document.activeElement
      if (active !== menu.host && active !== menu.el) closeMenu()
    }, 200)
  },
  true,
)

document.addEventListener(
  "keydown",
  (e) => {
    if (e.key === "Escape" && menu) closeMenu()
  },
  true,
)

// The person is typing into the field themselves: the menu gets out of the
// way (and off the form's own buttons).
document.addEventListener(
  "input",
  (e) => {
    if (e.isTrusted && menu && e.target === menu.el) closeMenu()
  },
  true,
)

window.addEventListener(
  "scroll",
  () => {
    if (menu?.el) place(menu.host, menu.el)
  },
  true,
)
window.addEventListener("resize", () => {
  if (menu?.el) place(menu.host, menu.el)
})

// The frames say how tall they want to be. Only Lockdown's own frames are
// listened to: the source must be one of them and the origin the
// extension's own, which a page cannot fake.
window.addEventListener("message", (e) => {
  const shown = [menu, prompt].find((s) => s && e.source === s.frame.contentWindow)
  if (!shown || e.origin !== ORIGIN) return
  const data = e.data as { lockdown?: string; px?: number }
  if (data?.lockdown === "height" && typeof data.px === "number") {
    const px = Math.max(40, Math.min(420, Math.round(data.px)))
    if (shown === menu && menu.el) place(menu.host, menu.el, px)
    else shown.host.style.height = `${px}px`
  }
})

// --- save after a sign-in (`autosave`) ---------------------------------------

let lastOffer = ""
let lastOfferAt = 0

function offerFrom(root: ParentNode) {
  const got = capture(root)
  if (!got) return
  const key = `${got.username}\u0000${got.password.length}`
  if (key === lastOffer && Date.now() - lastOfferAt < 3000) return
  lastOffer = key
  lastOfferAt = Date.now()
  void chrome.runtime.sendMessage({ t: "offer_save", username: got.username, password: got.password }).catch(() => undefined)
}

document.addEventListener(
  "submit",
  (e) => {
    if (e.isTrusted && e.target instanceof HTMLFormElement) offerFrom(e.target)
  },
  true,
)
document.addEventListener(
  "click",
  (e) => {
    if (!e.isTrusted) return
    const t = e.target instanceof Element ? e.target.closest("button, input[type=submit]") : null
    if (!t) return
    const form = t.closest("form")
    if (!form && !(t instanceof HTMLButtonElement && /sign|log|save|continue|next|submit/i.test(t.textContent ?? ""))) return
    offerFrom(form ?? document)
  },
  true,
)
document.addEventListener(
  "keydown",
  (e) => {
    if (e.isTrusted && e.key === "Enter" && isInput(e.target) && e.target.type === "password") offerFrom(e.target.closest("form") ?? document)
  },
  true,
)

// --- "Sign in with …" (the federated sign-in map, docs/plan/23-bridges.md §4) ---

/** A button's words: "Sign in with Google", "Continue with Apple". The
 *  page wrote them, so they only ever start a question for the person
 *  (the background matches the name against the signed provider list, and
 *  the app keeps it waiting for their yes). */
const WITH = /^(?:sign in|log in|login|sign up|continue|connect)\s+(?:with|using|via)\s+([\p{L}\p{N} .-]{2,30})$/iu

document.addEventListener(
  "click",
  (e) => {
    if (!e.isTrusted || !(e.target instanceof Element)) return
    const t = e.target.closest("button, a, [role=button], input[type=submit], input[type=button]")
    if (!t) return
    const text = (t instanceof HTMLInputElement ? t.value : (t.getAttribute("aria-label") ?? t.textContent ?? "")).replace(/\s+/g, " ").trim()
    const m = text.length <= 60 ? WITH.exec(text) : null
    if (m) void chrome.runtime.sendMessage({ t: "provider_button", name: m[1].trim() }).catch(() => undefined)
  },
  true,
)

// --- what the background asks ------------------------------------------------

chrome.runtime.onMessage.addListener((msg: ToContent, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id) return false
  switch (msg.t) {
    case "measure": {
      const f = fields.get(msg.nonce)
      if (!f?.el || !f.el.isConnected) {
        sendResponse({ ok: false, visible: false, why: "gone", menu_visible: null })
        return false
      }
      const v = fieldVisibility(f.el, menu?.host)
      const menuVisible = menu && menu.nonce === msg.nonce ? hostVisible(menu.host) : null
      sendResponse({ ok: true, visible: v.visible, why: v.why, menu_visible: menuVisible })
      return false
    }
    case "apply": {
      const f = fields.get(msg.nonce)
      filling = true
      let n = 0
      try {
        n = f?.el && f.el.isConnected ? apply(f.el, msg.what, msg.values) : 0
      } finally {
        filling = false
      }
      for (const v of msg.values) v.value = ""
      sendResponse({ filled: n })
      return false
    }
    case "close":
      if (menu?.nonce === msg.nonce) closeMenu()
      if (prompt?.nonce === msg.nonce) closePrompt()
      sendResponse({ ok: true })
      return false
    case "hotkey": {
      const el = document.activeElement
      if (!document.hasFocus() || !isInput(el)) return false
      void ensureFixes().then(async () => {
        const kind = classify(el)
        sendResponse(kind ? { nonce: await register(el, kind) } : {})
      })
      return true
    }
    case "target_login": {
      if (window.top !== window) return false
      void ensureFixes().then(async () => {
        const el = firstLoginField()
        sendResponse(el ? { nonce: await register(el, "login") } : {})
      })
      return true
    }
    case "fixes_changed":
      resetFixes()
      closeMenu()
      sendResponse({ ok: true })
      return false
    case "list_fields": {
      if (window.top !== window) return false
      void ensureFixes().then(() => sendResponse({ fields: pageFields() }))
      return true
    }
    case "fill_email": {
      if (window.top !== window) return false
      const el = emailField()
      const address = typeof msg.address === "string" ? msg.address : ""
      // An address only: one `@`, no spaces, short.
      if (!el || !/^[^\s@]{1,64}@[^\s@]{1,190}$/.test(address)) {
        sendResponse({ filled: false })
        return false
      }
      filling = true
      try {
        sendResponse({ filled: write(el, address) })
      } finally {
        filling = false
      }
      return false
    }
    case "show_prompt": {
      void showPrompt()
      sendResponse({ ok: true })
      return false
    }
    case "open_menu": {
      const f = fields.get(msg.nonce)
      if (f?.el && f.kind !== "prompt") void openMenu(f.el, f.kind, msg.nonce)
      sendResponse({ ok: true })
      return false
    }
  }
  return false
})

// A sign-in saved on the page before it navigated: the top frame asks
// whether a save prompt is waiting for this tab.
if (window.top === window) {
  void chrome.runtime
    .sendMessage({ t: "top_ready" })
    .then((r: { prompt?: boolean } | undefined) => {
      if (r?.prompt) void showPrompt()
    })
    .catch(() => undefined)
}

