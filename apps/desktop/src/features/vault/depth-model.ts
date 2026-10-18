import type { GeneratorView, Recipe, UserFieldView } from "@/core/types"

/** The template's rule as options, the starting point when the person opens them (D230). */
export function recipeOf(g: GeneratorView | null | undefined): Recipe {
  return { style: "characters", length: g?.length ?? 20, digits: true, symbols: g?.symbols ?? true, unambiguous: g?.unambiguous ?? false, avoid: "" }
}

/** `<item id>#<field id>`, the value a linked field stores (D235). */
export function linkText(itemId: string, fieldId: string): string {
  return itemId && fieldId ? `${itemId}#${fieldId}` : ""
}

export function parseLink(text: string): { itemId: string; fieldId: string } {
  const at = text.indexOf("#")
  return at < 0 ? { itemId: "", fieldId: "" } : { itemId: text.slice(0, at), fieldId: text.slice(at + 1) }
}

/** How long the Wi-Fi guest card stays up before it hides itself (D236). */
export const WIFI_CARD_LIFETIME_S = 60

/** The four-module quiet zone a QR code needs around it to scan. */
const QUIET = 4

/**
 * The grid the core sends (its width, then a byte per module, 1 for dark)
 * as one SVG path: a square per dark module, offset by the quiet zone.
 */
export function qrPath(grid: Uint8Array): { size: number; d: string } {
  const w = grid[0] ?? 0
  const parts: string[] = []
  for (let y = 0; y < w; y += 1) {
    for (let x = 0; x < w; x += 1) {
      if (grid[1 + y * w + x] === 1) parts.push(`M${x + QUIET} ${y + QUIET}h1v1h-1z`)
    }
  }
  return { size: w + 2 * QUIET, d: parts.join("") }
}

/** When to be reminded to sign out, in hours (D237). */
export const REMIND_CHOICES: Array<[number, string]> = [
  [4, "In 4 hours"],
  [24, "Tomorrow"],
  [72, "In 3 days"],
  [168, "In a week"],
  [336, "In two weeks"],
]

/** A field's key from its label: lower case, `_` between words, unique among `taken` (D232). */
export function keyFor(label: string, taken: readonly string[]): string {
  const base =
    label
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "")
      .slice(0, 28) || "field"
  let key = base
  for (let n = 2; taken.includes(key); n += 1) key = `${base}_${n}`
  return key
}

/** One field of a template being designed. */
export interface DraftField {
  /** Kept from the saved template, so items made from it keep their fields; new ones get a key on save. */
  key: string | null
  label: string
  kind: string
  required: boolean
}

/** The fields as the core takes them, each with a stable key. */
export function draftToFields(drafts: readonly DraftField[]): UserFieldView[] {
  const taken: string[] = drafts.flatMap((d) => (d.key ? [d.key] : []))
  return drafts.map((d) => {
    const key = d.key ?? keyFor(d.label, taken)
    if (!d.key) taken.push(key)
    return { key, label: d.label.trim(), kind: d.kind, required: d.required }
  })
}
