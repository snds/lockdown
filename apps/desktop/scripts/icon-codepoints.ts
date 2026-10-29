// The Material Symbols codepoints Lockdown draws (docs/plan/38-mac-fixes.md §2).
//
// unicron's Icon draws an icon as its ligature: the icon's name as text in
// an SVG <text>, shaped into one glyph by the font's `liga` feature. Apple's
// WebKit (WKWebView, so the Mac app) does not shape ligatures in SVG text,
// so every icon drew as blank letters. The patched Icon (patches/, the
// request to unicron in doc 38) draws the glyph by its private-use
// codepoint instead, when it knows it. This module works those codepoints
// out at build time from the font that ships: it reads the WOFF2 (Brotli,
// Node's own zlib), takes the ligatures from GSUB and the codepoints from
// cmap, and keeps the names that appear as string literals in the sources
// given, so the bundle carries the icons Lockdown uses and not all 4,000.
//
// `iconCodepoints()` is the Vite plugin each app's config adds: it serves
// `virtual:icon-codepoints`, a `{ name: codepoint }` object that the app's
// entry hands to `setIconCodepoints`.

import { readFileSync, readdirSync, statSync } from "node:fs"
import { createRequire } from "node:module"
import path from "node:path"
import { brotliDecompressSync } from "node:zlib"

/** WOFF2's known-table tags, by the index its directory uses (WOFF2 §5.1). */
const KNOWN_TAGS = (
  "cmap head hhea hmtx maxp name OS/2 post cvt  fpgm glyf loca prep CFF  VORG EBDT EBLC gasp hdmx kern LTSH PCLT VDMX vhea vmtx BASE GDEF GPOS GSUB EBSC JSTF MATH CBDT CBLC COLR CPAL SVG  sbix acnt avar bdat bloc bsln cvar fdsc feat fmtx fvar gvar hsty just lcar mort morx opbd prop trak Zapf Silf Glat Gloc Feat Sill"
)
  .match(/.{4}\s?/g)!
  .map((t) => t.slice(0, 4))

class Reader {
  b: Uint8Array
  at: number
  constructor(b: Uint8Array) {
    this.b = b
    this.at = 0
  }
  u8() {
    return this.b[this.at++]!
  }
  u16() {
    const v = (this.b[this.at]! << 8) | this.b[this.at + 1]!
    this.at += 2
    return v
  }
  u32() {
    const v = ((this.b[this.at]! << 24) >>> 0) + ((this.b[this.at + 1]! << 16) | (this.b[this.at + 2]! << 8) | this.b[this.at + 3]!)
    this.at += 4
    return v
  }
  base128() {
    let v = 0
    for (let i = 0; i < 5; i++) {
      const byte = this.u8()
      v = v * 128 + (byte & 0x7f)
      if (!(byte & 0x80)) return v
    }
    throw new Error("a WOFF2 number runs past five bytes")
  }
}

const u16 = (b: Uint8Array, at: number) => (b[at]! << 8) | b[at + 1]!
const u32 = (b: Uint8Array, at: number) => ((b[at]! << 24) >>> 0) + ((b[at + 1]! << 16) | (b[at + 2]! << 8) | b[at + 3]!)

/** The cmap and GSUB tables of a WOFF2 font (neither is ever transformed). */
export function woff2Tables(file: Uint8Array): Map<string, Uint8Array> {
  const r = new Reader(file)
  if (r.u32() !== 0x774f4632) throw new Error("not a WOFF2 file")
  r.u32() // flavor
  r.u32() // length
  const numTables = r.u16()
  r.u16()
  r.u32() // totalSfntSize
  const compressed = r.u32()
  r.at = 48
  const dir: Array<{ tag: string; length: number }> = []
  for (let i = 0; i < numTables; i++) {
    const flags = r.u8()
    const tag = (flags & 0x3f) === 63 ? String.fromCharCode(r.u8(), r.u8(), r.u8(), r.u8()) : KNOWN_TAGS[flags & 0x3f]!
    const orig = r.base128()
    const version = (flags >> 6) & 3
    const transformed = tag === "glyf" || tag === "loca" ? version === 0 : version !== 0
    const length = transformed ? r.base128() : orig
    dir.push({ tag, length })
  }
  const data = new Uint8Array(brotliDecompressSync(file.subarray(r.at, r.at + compressed)))
  const out = new Map<string, Uint8Array>()
  let at = 0
  for (const t of dir) {
    out.set(t.tag, data.subarray(at, at + t.length))
    at += t.length
  }
  return out
}

/** Every codepoint the cmap maps, by glyph, and every glyph by codepoint (formats 4 and 12). */
export function readCmap(cmap: Uint8Array): { glyphOf: Map<number, number>; codepointsOf: Map<number, number[]> } {
  const glyphOf = new Map<number, number>()
  const n = u16(cmap, 2)
  const subtables = Array.from({ length: n }, (_, i) => ({ platform: u16(cmap, 4 + i * 8), encoding: u16(cmap, 6 + i * 8), offset: u32(cmap, 8 + i * 8) }))
  const pick = subtables.find((s) => s.platform === 3 && s.encoding === 10) ?? subtables.find((s) => s.platform === 3 && s.encoding === 1) ?? subtables.find((s) => s.platform === 0)
  if (!pick) throw new Error("the font has no Unicode cmap")
  const t = cmap.subarray(pick.offset)
  const format = u16(t, 0)
  if (format === 12) {
    const groups = u32(t, 12)
    for (let g = 0; g < groups; g++) {
      const start = u32(t, 16 + g * 12)
      const end = u32(t, 20 + g * 12)
      const glyph = u32(t, 24 + g * 12)
      for (let c = start; c <= end; c++) glyphOf.set(c, glyph + (c - start))
    }
  } else if (format === 4) {
    const segs = u16(t, 6) / 2
    const ends = 14
    const starts = ends + segs * 2 + 2
    const deltas = starts + segs * 2
    const ranges = deltas + segs * 2
    for (let s = 0; s < segs; s++) {
      const end = u16(t, ends + s * 2)
      const start = u16(t, starts + s * 2)
      const delta = u16(t, deltas + s * 2)
      const range = u16(t, ranges + s * 2)
      for (let c = start; c <= end && c !== 0xffff; c++) {
        let g: number
        if (range === 0) g = (c + delta) & 0xffff
        else {
          g = u16(t, ranges + s * 2 + range + (c - start) * 2)
          if (g !== 0) g = (g + delta) & 0xffff
        }
        if (g !== 0) glyphOf.set(c, g)
      }
    }
  } else throw new Error(`cmap format ${format} isn't read here`)
  const codepointsOf = new Map<number, number[]>()
  for (const [c, g] of glyphOf) {
    const list = codepointsOf.get(g) ?? []
    list.push(c)
    codepointsOf.set(g, list)
  }
  return { glyphOf, codepointsOf }
}

function coverage(t: Uint8Array, at: number): number[] {
  const format = u16(t, at)
  const out: number[] = []
  if (format === 1) {
    const n = u16(t, at + 2)
    for (let i = 0; i < n; i++) out.push(u16(t, at + 4 + i * 2))
  } else if (format === 2) {
    const n = u16(t, at + 2)
    for (let i = 0; i < n; i++) {
      const start = u16(t, at + 4 + i * 6)
      const end = u16(t, at + 6 + i * 6)
      const index = u16(t, at + 8 + i * 6)
      for (let g = start; g <= end; g++) out[index + (g - start)] = g
    }
  } else throw new Error(`coverage format ${format} isn't read here`)
  return out
}

/** Every ligature in GSUB (lookup type 4, through type 7 extensions too): its component glyphs, and the glyph it makes. */
export function readLigatures(gsub: Uint8Array): Array<{ components: number[]; glyph: number }> {
  const out: Array<{ components: number[]; glyph: number }> = []
  const lookups = u16(gsub, 8)
  const count = u16(gsub, lookups)
  const ligSubst = (at: number) => {
    if (u16(gsub, at) !== 1) return
    const firsts = coverage(gsub, at + u16(gsub, at + 2))
    const sets = u16(gsub, at + 4)
    for (let s = 0; s < sets; s++) {
      const set = at + u16(gsub, at + 6 + s * 2)
      const ligs = u16(gsub, set)
      for (let l = 0; l < ligs; l++) {
        const lig = set + u16(gsub, set + 2 + l * 2)
        const glyph = u16(gsub, lig)
        const n = u16(gsub, lig + 2)
        const components = [firsts[s]!]
        for (let c = 1; c < n; c++) components.push(u16(gsub, lig + 2 + c * 2))
        out.push({ components, glyph })
      }
    }
  }
  for (let i = 0; i < count; i++) {
    const lookup = lookups + u16(gsub, lookups + 2 + i * 2)
    const type = u16(gsub, lookup)
    const subtables = u16(gsub, lookup + 4)
    for (let s = 0; s < subtables; s++) {
      const sub = lookup + u16(gsub, lookup + 6 + s * 2)
      if (type === 4) ligSubst(sub)
      else if (type === 7 && u16(gsub, sub + 2) === 4) ligSubst(sub + u32(gsub, sub + 4))
    }
  }
  return out
}

const isPrivateUse = (c: number) => (c >= 0xe000 && c <= 0xf8ff) || c >= 0xf0000

/** Every icon name the font has a ligature for, with the private-use codepoint of the glyph it makes. */
export function fontCodepoints(file: Uint8Array): Map<string, number> {
  const tables = woff2Tables(file)
  const cmap = tables.get("cmap")
  const gsub = tables.get("GSUB")
  if (!cmap || !gsub) throw new Error("the font has no cmap or GSUB")
  const { codepointsOf } = readCmap(cmap)
  const charOf = new Map<number, string>()
  for (const [g, cps] of codepointsOf) {
    // The font maps both cases of a letter to one glyph; names are lower case.
    const ascii = cps.find((c) => /[a-z0-9_]/.test(String.fromCharCode(c))) ?? cps.find((c) => c < 0x80)
    if (ascii !== undefined) charOf.set(g, String.fromCharCode(ascii))
  }
  const out = new Map<string, number>()
  for (const { components, glyph } of readLigatures(gsub)) {
    const chars = components.map((g) => charOf.get(g))
    if (chars.some((c) => c === undefined)) continue
    const cp = codepointsOf.get(glyph)?.filter(isPrivateUse).sort((a, b) => a - b)[0]
    if (cp === undefined) continue
    const name = chars.join("")
    // The first ligature for a name wins, as the shaper would pick it.
    if (!out.has(name)) out.set(name, cp)
  }
  return out
}

/** The Material Symbols Rounded font @unicron/ui ships, as installed under `root`. */
export function shippedFont(root: string): string {
  const fromRoot = createRequire(path.join(root, "package.json"))
  const ui = fromRoot.resolve("@unicron/ui/package.json")
  const fromUi = createRequire(ui)
  return path.join(path.dirname(fromUi.resolve("material-symbols/package.json")), "material-symbols-rounded.woff2")
}

/** Every string literal in the sources under `dirs` that names an icon. */
export function usedNames(dirs: string[], known: Map<string, number>): string[] {
  const names = new Set<string>()
  const visit = (p: string) => {
    const st = statSync(p)
    if (st.isDirectory()) {
      for (const e of readdirSync(p)) if (e !== "node_modules" && !e.startsWith(".")) visit(path.join(p, e))
    } else if (/\.(tsx?|jsx?|mjs)$/.test(p) && !/\.test\.tsx?$/.test(p)) {
      for (const m of readFileSync(p, "utf8").matchAll(/["'`]([a-z0-9_]{2,})["'`]/g)) if (known.has(m[1]!)) names.add(m[1]!)
    }
  }
  for (const d of dirs) visit(d)
  return [...names].sort()
}

/** `{ name: codepoint }` for the icons the sources under `dirs` name, from the font installed under `root`. */
export function codepointsFor(root: string, dirs: string[]): Record<string, number> {
  const all = fontCodepoints(readFileSync(shippedFont(root)))
  return Object.fromEntries(usedNames(dirs, all).map((n) => [n, all.get(n)!]))
}

/** The Vite plugin: `virtual:icon-codepoints`, built once per build from the font and `dirs`. */
export function iconCodepoints(opts: { root: string; dirs: string[] }) {
  const id = "virtual:icon-codepoints"
  const ui = path.dirname(createRequire(path.join(opts.root, "package.json")).resolve("@unicron/ui/package.json"))
  // unicron's own components name icons too (the dialog's close, a select's check).
  const dirs = [...opts.dirs, path.join(ui, "dist")]
  return {
    name: "lockdown-icon-codepoints",
    resolveId: (source: string) => (source === id ? `\0${id}` : null),
    load(source: string) {
      if (source !== `\0${id}`) return null
      return `export default ${JSON.stringify(codepointsFor(opts.root, dirs))}`
    },
  }
}
