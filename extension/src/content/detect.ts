// Which fields on a page are what: sign-in, one-time code, payment card,
// identity and address. The page is hostile: this reads attributes and
// types to choose a menu, never trusts them to decide a fill (the core
// decides), and fills only the fields it found next to the one the person
// is in.

import type { FillWhat } from "../shared/protocol"

const TEXTLIKE = new Set(["text", "email", "tel", "number", "search", "url", ""])

export function isInput(el: unknown): el is HTMLInputElement {
  return el instanceof HTMLInputElement
}

function attrs(el: HTMLInputElement | HTMLSelectElement): string {
  const label = el.labels?.[0]?.textContent ?? ""
  return [el.name, el.id, el.getAttribute("autocomplete"), el.getAttribute("placeholder"), el.getAttribute("aria-label"), label]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
}

function auto(el: Element): string {
  return (el.getAttribute("autocomplete") ?? "").toLowerCase().split(/\s+/).pop() ?? ""
}

function usable(el: HTMLInputElement): boolean {
  return !el.disabled && !el.readOnly && el.type !== "hidden"
}

const CODE_WORDS = /(one[-_ ]?time|otp|totp|2fa|mfa|two[-_ ]?factor|verification|security[-_ ]?code|auth(entication)?[-_ ]?code|passcode)/
const CARD = {
  number: /(cc-number|card[-_ ]?num|cardnumber|ccnum|credit[-_ ]?card|card[-_ ]?no\b)/,
  holder: /(cc-name|name[-_ ]?on[-_ ]?card|card[-_ ]?holder|cardholder|holder[-_ ]?name)/,
  exp: /(cc-exp\b|expiry|expiration|exp[-_ ]?date|mm\s*\/\s*yy)/,
  expMonth: /(cc-exp-month|exp[-_ ]?month|expiry[-_ ]?month)/,
  expYear: /(cc-exp-year|exp[-_ ]?year|expiry[-_ ]?year)/,
  cvc: /(cc-csc|cvc|cvv|csc|security[-_ ]?code|card[-_ ]?code)/,
}

/** The kind of fill a field asks for, or `null` for an ordinary field. */
export function classify(el: HTMLInputElement): FillWhat | null {
  if (!usable(el)) return null
  const a = auto(el)
  const words = attrs(el)
  if (el.type === "password") return a === "one-time-code" ? "code" : "login"
  if (!TEXTLIKE.has(el.type)) return null
  if (a === "one-time-code" || (CODE_WORDS.test(words) && !/card|cc-/.test(words))) return "code"
  if (codeBoxes(el).length > 1) return "code"
  if (a.startsWith("cc-") || CARD.number.test(words) || CARD.cvc.test(words)) return "card"
  // `autocomplete="username webauthn"`: a sign-in field a passkey can
  // answer (conditional mediation), with or without a password beside it.
  const tokens = (el.getAttribute("autocomplete") ?? "").toLowerCase().split(/\s+/)
  if (a === "username" || tokens.includes("webauthn") || (a === "email" && passwordNear(el))) return "login"
  if (["given-name", "family-name", "additional-name", "name", "email", "tel", "street-address", "address-line1", "address-level1", "address-level2", "postal-code", "country", "country-name"].includes(a))
    return "identity"
  if (passwordNear(el) && /(user|login|email|account|identifier)/.test(words)) return "login"
  return null
}

function scope(el: Element): ParentNode {
  return el.closest("form") ?? document
}

function inputsIn(root: ParentNode): HTMLInputElement[] {
  return Array.from(root.querySelectorAll("input")).filter(usable)
}

function passwordNear(el: HTMLInputElement): boolean {
  return inputsIn(scope(el)).some((i) => i.type === "password")
}

/** The username field that goes with `password`: the last text-like input
 *  before it in the same form, preferring one marked `username` or `email`. */
export function usernameFor(password: HTMLInputElement): HTMLInputElement | null {
  const all = inputsIn(scope(password))
  const before = all.slice(0, all.indexOf(password)).filter((i) => TEXTLIKE.has(i.type) && i.type !== "number")
  return before.find((i) => auto(i) === "username") ?? before.reverse().find((i) => i.type === "email" || /(user|login|email|account)/.test(attrs(i))) ?? before[0] ?? null
}

/** The sign-in pair around the focused field. */
export function loginPair(el: HTMLInputElement): { user: HTMLInputElement | null; pass: HTMLInputElement | null } {
  if (el.type === "password") return { user: usernameFor(el), pass: el }
  const pass = inputsIn(scope(el)).find((i) => i.type === "password") ?? null
  return { user: el, pass }
}

/** The first sign-in field on the page, for the popup and the shortcut. */
export function firstLoginField(): HTMLInputElement | null {
  const inputs = inputsIn(document)
  const pass = inputs.find((i) => i.type === "password")
  if (pass) return usernameFor(pass) ?? pass
  return inputs.find((i) => classify(i) === "login") ?? null
}

/** Split one-time-code boxes: four or more one-character inputs side by
 *  side in the same container, `el` among them. */
export function codeBoxes(el: HTMLInputElement): HTMLInputElement[] {
  if (el.maxLength !== 1) return [el]
  const parent = el.parentElement?.parentElement ?? el.parentElement
  if (!parent) return [el]
  const boxes = Array.from(parent.querySelectorAll("input")).filter((i) => usable(i) && i.maxLength === 1 && TEXTLIKE.has(i.type))
  return boxes.length >= 4 && boxes.length <= 8 ? boxes : [el]
}

type Fieldish = HTMLInputElement | HTMLSelectElement

function fieldsIn(root: ParentNode): Fieldish[] {
  return Array.from(root.querySelectorAll<Fieldish>("input, select")).filter((f) => !f.disabled && !(f instanceof HTMLInputElement && (f.readOnly || f.type === "hidden")))
}

/** The card fields in the form around `el`, by role. */
export function cardFields(el: Element): Partial<Record<"number" | "holder" | "exp" | "expMonth" | "expYear" | "cvc", Fieldish>> {
  const out: Partial<Record<"number" | "holder" | "exp" | "expMonth" | "expYear" | "cvc", Fieldish>> = {}
  for (const f of fieldsIn(scope(el))) {
    const a = auto(f)
    const w = attrs(f)
    const role =
      a === "cc-number" || (!a.startsWith("cc-") && CARD.number.test(w))
        ? "number"
        : a === "cc-name" || (!a.startsWith("cc-") && CARD.holder.test(w))
          ? "holder"
          : a === "cc-exp-month" || CARD.expMonth.test(w)
            ? "expMonth"
            : a === "cc-exp-year" || CARD.expYear.test(w)
              ? "expYear"
              : a === "cc-exp" || CARD.exp.test(w)
                ? "exp"
                : a === "cc-csc" || CARD.cvc.test(w)
                  ? "cvc"
                  : null
    if (role && !out[role]) out[role] = f
  }
  return out
}

const IDENTITY: [string, RegExp, string[]][] = [
  ["name.given", /(first[-_ ]?name|given[-_ ]?name|fname)/, ["given-name"]],
  ["name.middle", /(middle[-_ ]?name)/, ["additional-name"]],
  ["name.family", /(last[-_ ]?name|family[-_ ]?name|surname|lname)/, ["family-name"]],
  ["name.full", /(full[-_ ]?name|^name$)/, ["name"]],
  ["identity.email", /(e-?mail)/, ["email"]],
  ["address.tel", /(phone|tel\b|mobile)/, ["tel", "tel-national"]],
  ["address.street", /(street|address[-_ ]?(line)?[-_ ]?1|address$)/, ["street-address", "address-line1"]],
  ["address.city", /(city|town|locality)/, ["address-level2"]],
  ["address.territory", /(state|province|region|county)/, ["address-level1"]],
  ["address.postal_code", /(zip|postal|postcode)/, ["postal-code"]],
  ["address.country", /(country)/, ["country", "country-name"]],
]

/** The identity and address fields in the form around `el`, by role. */
export function identityFields(el: Element): Map<string, Fieldish> {
  const out = new Map<string, Fieldish>()
  for (const f of fieldsIn(scope(el))) {
    if (f instanceof HTMLInputElement && f.type === "password") continue
    const a = auto(f)
    const w = attrs(f)
    for (const [role, words, autos] of IDENTITY) {
      if (out.has(role)) continue
      const generic = !a || a === "on" || a === "off"
      if (autos.includes(a) || (generic && words.test(w))) {
        out.set(role, f)
        break
      }
    }
  }
  return out
}

/** The new password and the username around it, when a form that holds a
 *  password is submitted: `null` when there is nothing to offer. */
export function capture(form: ParentNode): { username: string; password: string } | null {
  const passwords = inputsIn(form).filter((i) => i.type === "password" && i.value)
  if (passwords.length === 0) return null
  const chosen =
    passwords.find((p) => auto(p) === "new-password") ?? (passwords.length >= 2 ? passwords[passwords.length - 1] : passwords[0])
  const user = usernameFor(passwords[0])
  return { username: user?.value.trim() ?? "", password: chosen.value }
}
