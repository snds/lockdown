// Writes static/tokens.css: unicron's plain-CSS sheet (@unicron/tokens/plain.css,
// contract 5.1b #54) cut down to what the in-page frames (menu, save prompt,
// passkey sheet) draw with, plus Geist and Geist Mono as files from the
// extension's own origin (brief B2, docs/plan/86-redesign-extension-frames.md).
//
//   node scripts/tokens-css.mjs           write static/tokens.css
//   node scripts/tokens-css.mjs --check   fail if static/tokens.css is stale
//
// The source is the installed @unicron/tokens (the version the popup and the
// desktop use). Its plain sheet carries every role of every app on both
// grounds, and picks the dark ground by a class on the root (the dark class,
// or `system` for the browser's scheme). A frame takes no class from anyone and
// needs a small sheet, so this keeps only the roles static/ui.css names and
// everything those roles stand on, and writes the graphite values under
// `prefers-color-scheme: dark`, which is how a frame learns the browser's
// scheme. Values are never changed or invented: every declaration is
// unicron's, copied as written. If a future plain sheet lacks a block, the
// script stops with the block's name; it never falls back to a guess.
//
// A role is a chain:
//
//   --surface-raised     the role static/ui.css names
//   → --g-theme-stone-raised / --g-theme-graphite-raised
//                        the app's theme slot for each ground
//   → a hex value        under [data-theme="lockdown"], set on <html> in
//                        static/frame.html; Lockdown declares these itself
//                        (src/popup/lockdown-theme.css: Unicron 1.1 ships no
//                        app's theme)
//
// So the sheet carries, in this order: the fonts, the lockdown theme's slots
// (stone and graphite), the shared primitives the chain names, the type and
// font roles, the roles on stone, and the same roles on graphite under the
// media query. A slot is written only under [data-theme="lockdown"]: a frame
// without the attribute would draw unicron's default app, which is not
// Lockdown.

import { readFileSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const here = join(dirname(fileURLToPath(import.meta.url)), "..")
const out = join(here, "static", "tokens.css")
const require = createRequire(import.meta.url)

/** The app theme the frames are drawn in (`data-theme` on static/frame.html). */
export const THEME = "lockdown"

/** The roles static/ui.css names. */
export const ROLES = [
  "surface-raised",
  "line-rule",
  "text-body",
  "text-label",
  "primary-fill",
  "primary-on-fill",
  "critical-text",
  "critical-soft",
  "state-hover",
  "state-pressed",
  "ring",
  "radius",
  "stroke-rule",
  "stroke-emphasis",
]

/** The type roles static/ui.css names; each is a family, a weight, a size and a tracking. */
export const TYPE = ["ui-emphasis", "caption", "meta", "overline", "code-display", "brand-wordmark"]

/** The font files the frames load, each family's Latin and Latin Extended subsets, upright. */
export const FONTS = [
  { pkg: "@fontsource-variable/geist", file: "geist-latin-wght-normal.woff2" },
  { pkg: "@fontsource-variable/geist", file: "geist-latin-ext-wght-normal.woff2" },
  { pkg: "@fontsource-variable/geist-mono", file: "geist-mono-latin-wght-normal.woff2" },
  { pkg: "@fontsource-variable/geist-mono", file: "geist-mono-latin-ext-wght-normal.woff2" },
]

/** Where the font packages are: beside @unicron/ui, whose stylesheet imports them for the popup. */
export function fontDir(pkg) {
  const ui = createRequire(require.resolve("@unicron/ui/package.json"))
  return dirname(ui.resolve(pkg))
}

/** unicron's comment over each block of its plain sheet, which names the block whatever its selector is. */
const HEADS = {
  primitives: "Tier 1 · Global primitives",
  type: "Type roles",
  font: "Font roles",
  light: "Tier 2 · Semantic roles (--*) · light",
  dark: "Tier 2 · Semantic roles (--*) · dark",
}

/** The declarations of the block that follows the comment starting with `head`, in order. */
function block(css, head, version) {
  const at = css.indexOf(`/* ── ${head}`)
  if (at < 0) throw new Error(`@unicron/tokens ${version} (plain.css) has no block headed "${head}"`)
  const open = css.indexOf("{", at)
  const close = css.indexOf("}", open)
  const decls = new Map()
  for (const d of css.slice(open + 1, close).matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) decls.set(d[1], d[2].trim())
  return { selector: css.slice(css.indexOf("*/", at) + 2, open).trim(), decls }
}

/** Lockdown's own theme layer, the one block the popup imports too. */
export const THEME_FILE = join(here, "src", "popup", `${THEME}-theme.css`)

/** The declarations of the app's theme block, from its own layer. */
function themeBlock() {
  const css = readFileSync(THEME_FILE, "utf8")
  const sel = `[data-theme="${THEME}"]`
  const at = css.indexOf(`${sel} {`)
  if (at < 0) throw new Error(`${THEME_FILE} has no ${sel} block`)
  const open = css.indexOf("{", at)
  const close = css.indexOf("}", open)
  const decls = new Map()
  for (const d of css.slice(open + 1, close).matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) decls.set(d[1], d[2].trim())
  return { selector: sel, decls }
}

/** Every custom property a value names. */
const named = (value) => Array.from(value.matchAll(/var\((--[a-z0-9-]+)\)/g), (m) => m[1])

/** The @font-face rules for FONTS, as the font package writes them, pointed at fonts/ in the extension. */
function fontFaces() {
  const rules = []
  for (const { pkg, file } of FONTS) {
    const dir = fontDir(pkg)
    const css = readFileSync(join(dir, "wght.css"), "utf8")
    const at = css.indexOf(`/* ${file.replace(/\.woff2$/, "")} */`)
    if (at < 0) throw new Error(`${pkg} has no @font-face for ${file}`)
    const open = css.indexOf("{", at)
    const close = css.indexOf("}", open)
    const body = css
      .slice(open + 1, close)
      .trim()
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => l.replace(`url(./files/${file})`, `url("fonts/${file}")`))
    if (!body.some((l) => l.includes(`url("fonts/${file}")`))) throw new Error(`${pkg}'s @font-face for ${file} names another file`)
    const { version } = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"))
    rules.push(`/* ${pkg} ${version}: ${file} */`, `@font-face {`, ...body.map((l) => `  ${l}`), `}`)
  }
  return rules
}

export function render() {
  const pkgFile = require.resolve("@unicron/tokens/package.json")
  const { version } = JSON.parse(readFileSync(pkgFile, "utf8"))
  const css = readFileSync(join(dirname(pkgFile), "dist", "plain.css"), "utf8")
  const primitives = block(css, HEADS.primitives, version)
  const theme = themeBlock()
  const type = block(css, HEADS.type, version)
  const font = block(css, HEADS.font, version)
  const light = block(css, HEADS.light, version)
  const dark = block(css, HEADS.dark, version)

  // Follow each role down its chain, on both grounds: a role may name roles
  // (--ring → --text-body), and a role names primitives (--g-*).
  const roles = new Set()
  const types = new Set()
  const prims = new Set()
  const follow = (name) => {
    if (name.startsWith("--g-")) {
      prims.add(name)
      return
    }
    if (roles.has(name) || types.has(name)) return
    if (type.decls.has(name) || font.decls.has(name)) {
      types.add(name)
      for (const n of named(type.decls.get(name) ?? font.decls.get(name))) follow(n)
      return
    }
    if (!light.decls.has(name)) throw new Error(`@unicron/tokens ${version} has no ${name}`)
    roles.add(name)
    for (const v of [light.decls.get(name), dark.decls.get(name)]) if (v !== undefined) for (const n of named(v)) follow(n)
  }
  for (const r of ROLES) follow(`--${r}`)
  for (const t of TYPE) for (const part of ["family", "weight", "size", "tracking"]) follow(`--type-${t}-${part}`)
  for (const f of ["sans", "mono"]) follow(`--font-${f}`)

  // Each primitive is the app theme's slot or a shared one; none may be missing.
  const slots = [...theme.decls.keys()].filter((g) => prims.has(g))
  const shared = [...primitives.decls.keys()].filter((g) => prims.has(g) && !theme.decls.has(g))
  for (const g of prims) if (!theme.decls.has(g) && !primitives.decls.has(g)) throw new Error(`neither @unicron/tokens ${version} nor ${THEME}'s theme has ${g}`)

  // In unicron's own order, so a tokens update reads as a small diff.
  const typeRoles = [...type.decls.keys(), ...font.decls.keys()].filter((r) => types.has(r))
  const typeDecls = new Map([...type.decls, ...font.decls])
  const lightRoles = [...light.decls.keys()].filter((r) => roles.has(r))
  const darkRoles = [...dark.decls.keys()].filter((r) => roles.has(r))

  const line = (decls) => (name) => `  ${name}: ${decls.get(name)};`
  return [
    `/* unicron's plain-CSS sheet for the extension's in-page frames.`,
    `   Generated by scripts/tokens-css.mjs from @unicron/tokens ${version}`,
    `   (dist/plain.css): the roles and type roles static/ui.css names, values`,
    `   as unicron declares them, and the slots they stand on from ${THEME}'s own`,
    `   theme (src/popup/${THEME}-theme.css);`,
    `   stone at the root, graphite under prefers-color-scheme: dark. The fonts`,
    `   are files in the extension (scripts/build.mjs copies them to fonts/).`,
    `   Do not edit; run \`node scripts/tokens-css.mjs\` after a tokens update. */`,
    ...fontFaces(),
    `[data-theme="${THEME}"] {`,
    ...slots.map(line(theme.decls)),
    `}`,
    `:root {`,
    ...shared.map(line(primitives.decls)),
    `}`,
    `:root {`,
    ...typeRoles.map(line(typeDecls)),
    `}`,
    `:root {`,
    ...lightRoles.map(line(light.decls)),
    `}`,
    `@media (prefers-color-scheme: dark) {`,
    `  :root {`,
    ...darkRoles.map((r) => `  ${line(dark.decls)(r)}`),
    `  }`,
    `}`,
    ``,
  ].join("\n")
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const text = render()
  if (process.argv.includes("--check")) {
    let have = ""
    try {
      have = readFileSync(out, "utf8")
    } catch {
      // Missing reads as stale.
    }
    if (have !== text) {
      console.error("tokens-css: static/tokens.css is stale; run `node scripts/tokens-css.mjs`")
      process.exit(1)
    }
    console.log("tokens-css: static/tokens.css matches @unicron/tokens")
  } else {
    writeFileSync(out, text)
    console.log("tokens-css: wrote static/tokens.css")
  }
}
