// The steps a state takes after it mounts, run in the page: find an
// element by its accessible name, click it, type into it, wait for text.
// Pointer events are dispatched in full (down, up, click) because Base UI's
// menus and the hold-to-reveal control listen for the pointer, not the click.

import { emit } from "../host/tauri-event"
import type { Step } from "./types"

const TIMEOUT_MS = 6000
const CLICKABLE = 'button, a[href], [role="button"], [role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"], [role="tab"], [role="option"], [role="switch"], [role="checkbox"], [role="radio"], [role="link"], summary, input[type="checkbox"], input[type="radio"], label'

/** The elements that carry a role, by their tags too. */
function roleSelector(role: string): string {
  const tags: Record<string, string> = {
    heading: "h1, h2, h3, h4, h5, h6",
    button: "button",
    link: "a[href]",
    textbox: "input, textarea",
    checkbox: 'input[type="checkbox"]',
    dialog: "dialog",
    list: "ul, ol",
    listitem: "li",
  }
  return [`[role="${role}"]`, tags[role]].filter(Boolean).join(", ")
}

const squash = (s: string) => s.replace(/\s+/g, " ").trim()

/** An element's name as a person hears it: `aria-label`, its label, or its text. */
function nameOf(el: Element): string {
  const aria = el.getAttribute("aria-label")
  if (aria) return squash(aria)
  const by = el.getAttribute("aria-labelledby")
  if (by) {
    const text = by
      .split(/\s+/)
      .map((id) => document.getElementById(id)?.textContent ?? "")
      .join(" ")
    if (text.trim()) return squash(text)
  }
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) {
    const label = el.id ? document.querySelector(`label[for="${CSS.escape(el.id)}"]`) : el.closest("label")
    if (label?.textContent?.trim()) return squash(label.textContent)
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return squash(el.placeholder ?? "")
  }
  return squash(textOf(el))
}

/** The text a screen reader would read: hidden parts left out, a labelled part by its label. */
function textOf(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? ""
  if (!(node instanceof Element)) return ""
  if (node.getAttribute("aria-hidden") === "true") return ""
  let out = ""
  for (const child of Array.from(node.childNodes)) {
    const label = child instanceof Element ? child.getAttribute("aria-label") : null
    out += " " + (label && child instanceof Element && child.getAttribute("aria-hidden") !== "true" ? label : textOf(child))
  }
  return out
}

function visible(el: Element): boolean {
  const r = el.getBoundingClientRect()
  if (r.width === 0 && r.height === 0) return false
  const s = getComputedStyle(el)
  return s.visibility !== "hidden" && s.display !== "none"
}

const matches = (name: string, want: string | RegExp) => (typeof want === "string" ? name === want : want.test(name))

function find(want: string | RegExp, selector: string, nth = 0, within?: string): HTMLElement | null {
  const root: ParentNode = (within ? document.querySelector(within) : null) ?? document
  const all = Array.from(root.querySelectorAll<HTMLElement>(selector)).filter((el) => visible(el) && matches(nameOf(el), want))
  return all[nth] ?? null
}

async function until<T>(what: string, probe: () => T | null): Promise<T> {
  const start = performance.now()
  for (;;) {
    const found = probe()
    if (found) return found
    if (performance.now() - start > TIMEOUT_MS) throw new Error(`waited ${TIMEOUT_MS / 1000} s for ${what}`)
    await new Promise((r) => setTimeout(r, 50))
  }
}

function pointer(el: HTMLElement, type: string) {
  const r = el.getBoundingClientRect()
  const init = { bubbles: true, cancelable: true, composed: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, button: 0, buttons: type.endsWith("down") ? 1 : 0, pointerId: 1, pointerType: "mouse", isPrimary: true }
  el.dispatchEvent(type.startsWith("pointer") ? new PointerEvent(type, init) : new MouseEvent(type, init))
}

function press(el: HTMLElement) {
  el.scrollIntoView({ block: "nearest" })
  pointer(el, "pointerdown")
  pointer(el, "mousedown")
  el.focus({ preventScroll: true })
  pointer(el, "pointerup")
  pointer(el, "mouseup")
  el.click()
}

/** Types into a React-controlled field: the native setter, then an input event. */
function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.focus()
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
  Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, value)
  el.dispatchEvent(new Event("input", { bubbles: true }))
  el.dispatchEvent(new Event("change", { bubbles: true }))
}

function key(combo: string, target?: string) {
  const parts = combo.split("+")
  const k = parts.pop()!
  const el = (target ? document.querySelector(target) : document.activeElement) ?? document.body
  const init = { key: k, code: k.length === 1 ? `Key${k.toUpperCase()}` : k, bubbles: true, cancelable: true, metaKey: parts.includes("Meta"), ctrlKey: parts.includes("Control"), shiftKey: parts.includes("Shift"), altKey: parts.includes("Alt") }
  el.dispatchEvent(new KeyboardEvent("keydown", init))
  el.dispatchEvent(new KeyboardEvent("keyup", init))
}

export async function runSteps(steps: Step[]) {
  for (const s of steps) {
    if ("click" in s) {
      const el = await until(`"${String(s.click)}"`, () => find(s.click, s.role ? roleSelector(s.role) : CLICKABLE, s.nth, s.within))
      press(el)
    } else if ("fill" in s) {
      const el = await until(`a field "${String(s.fill)}"`, () => find(s.fill, "input, textarea") as HTMLInputElement | null)
      type(el, s.value)
    } else if ("key" in s) {
      key(s.key, s.target)
    } else if ("hold" in s) {
      const el = await until(`"${String(s.hold)}"`, () => find(s.hold, CLICKABLE))
      pointer(el, "pointerdown")
      pointer(el, "mousedown")
    } else if ("wait" in s) {
      await until(`"${String(s.wait)}" to show`, () => find(s.wait, s.role ? roleSelector(s.role) : "h1, h2, h3, h4, p, span, div, li, td, th, label, button, code, pre, [role]"))
    } else if ("see" in s) {
      await until(`${s.see} to show`, () => Array.from(document.querySelectorAll(s.see)).find(visible) ?? null)
    } else if ("gone" in s) {
      await until(`"${String(s.gone)}" to go`, () => (find(s.gone, "h1, h2, h3, p, span, div, button, [role]") ? null : true))
    } else if ("pause" in s) {
      await new Promise((r) => setTimeout(r, s.pause))
    } else if ("emit" in s) {
      await emit(s.emit, s.payload)
    } else {
      await s.run()
    }
    // Let React and the menus settle between steps.
    await new Promise((r) => setTimeout(r, 120))
  }
}
