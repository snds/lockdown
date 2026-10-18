// Putting values the core approved into the fields next to the one the
// person is in. Values arrive from the background only after the app's
// `fill` verdict, are written once and are not kept.

import type { FillValue, FillWhat } from "../shared/protocol"
import { cardFields, codeBoxes, identityFields, loginPair } from "./detect"

type Fieldish = HTMLInputElement | HTMLSelectElement

// The prototype setters, taken once from this isolated world, so a page
// that redefines `value` on its own elements cannot intercept the write
// through a setter of its own, and frameworks that track the native
// setter (React) see the change.
const inputSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set
const selectSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set

export function write(el: Fieldish, value: string): boolean {
  if (el instanceof HTMLSelectElement) {
    const want = value.trim().toLowerCase()
    const opt = Array.from(el.options).find((o) => o.value.toLowerCase() === want || o.text.trim().toLowerCase() === want)
    if (!opt) return false
    selectSetter?.call(el, opt.value)
  } else {
    el.focus({ preventScroll: true })
    inputSetter?.call(el, value)
  }
  el.dispatchEvent(new Event("input", { bubbles: true, composed: true }))
  el.dispatchEvent(new Event("change", { bubbles: true }))
  return true
}

function get(values: FillValue[], role: string): string | undefined {
  return values.find((v) => v.role === role)?.value
}

/** `2030-08`, `08/2030`, `8/30` → month and four-digit year. */
export function expiry(text: string): { month: string; year: string } | null {
  const t = text.trim()
  let m = /^(\d{4})-(\d{1,2})/.exec(t)
  if (m) return { month: m[2].padStart(2, "0"), year: m[1] }
  m = /^(\d{1,2})\s*[/-]\s*(\d{2}|\d{4})$/.exec(t)
  if (m) return { month: m[1].padStart(2, "0"), year: m[2].length === 2 ? `20${m[2]}` : m[2] }
  return null
}

function fillExpiry(fields: ReturnType<typeof cardFields>, text: string): number {
  const e = expiry(text)
  if (!e) return 0
  let n = 0
  if (fields.exp) {
    const max = fields.exp instanceof HTMLInputElement ? fields.exp.maxLength : -1
    const placeholder = (fields.exp.getAttribute("placeholder") ?? "").toLowerCase()
    const long = max >= 7 || placeholder.includes("yyyy")
    if (write(fields.exp, `${e.month}/${long ? e.year : e.year.slice(2)}`)) n++
  }
  if (fields.expMonth && (write(fields.expMonth, e.month) || write(fields.expMonth, String(Number(e.month))))) n++
  if (fields.expYear) {
    const yearLen = fields.expYear instanceof HTMLInputElement && fields.expYear.maxLength === 2 ? 2 : 4
    if (write(fields.expYear, yearLen === 2 ? e.year.slice(2) : e.year) || write(fields.expYear, e.year.slice(2))) n++
  }
  return n
}

/** Fills around `el`; answers how many fields took a value. */
export function apply(el: HTMLInputElement, what: FillWhat, values: FillValue[]): number {
  let n = 0
  switch (what) {
    case "login": {
      const { user, pass } = loginPair(el)
      const u = get(values, "user")
      const p = get(values, "pw")
      if (user && u !== undefined && write(user, u)) n++
      if (pass && p !== undefined && write(pass, p)) n++
      break
    }
    case "code": {
      const code = get(values, "code") ?? ""
      const boxes = codeBoxes(el)
      if (boxes.length > 1 && boxes.length === code.length) {
        boxes.forEach((b, i) => write(b, code[i]) && n++)
      } else if (write(el, code)) n++
      break
    }
    case "card": {
      const f = cardFields(el)
      const number = get(values, "card.number")
      const holder = get(values, "card.holder")
      const cvc = get(values, "card.cvc")
      const exp = get(values, "card.expiry")
      if (f.number && number && write(f.number, number.replace(/\s+/g, ""))) n++
      if (f.holder && holder && write(f.holder, holder)) n++
      if (f.cvc && cvc && write(f.cvc, cvc)) n++
      if (exp) n += fillExpiry(f, exp)
      break
    }
    case "identity": {
      const f = identityFields(el)
      const given = get(values, "name.given") ?? ""
      const family = get(values, "name.family") ?? ""
      for (const [role, field] of f) {
        const v = role === "name.full" ? [given, get(values, "name.middle") ?? "", family].filter(Boolean).join(" ") : get(values, role)
        if (v && write(field, v)) n++
      }
      break
    }
  }
  el.focus({ preventScroll: true })
  return n
}
