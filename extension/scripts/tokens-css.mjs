// Writes static/tokens.css: unicron's colour roles as a plain stylesheet the
// in-page frames (menu, save prompt, passkey sheet) can load without
// Tailwind (design review R4; brief §11.1, "a plain-CSS token sheet").
//
//   node scripts/tokens-css.mjs           write static/tokens.css
//   node scripts/tokens-css.mjs --check   fail if static/tokens.css is stale
//
// The source is @unicron/tokens' own built sheet (dist/tokens.css), at the
// version the popup and the desktop use (the override in pnpm-workspace.yaml).
// That sheet is meant for Tailwind: the ground is a `.dark` class and it ends
// in a Tailwind-only `@theme` block. A frame has neither, so this keeps only
// the colour roles the frames draw with and everything those roles stand on,
// and moves the dark values under `prefers-color-scheme: dark`, which is how
// a frame learns the browser's scheme. Values are never changed or invented:
// every declaration is unicron's, copied as written.
//
// From unicron 1.0 a role is a chain (docs/plan/62-unicron-1-0.md §6):
//
//   --popover            the name static/ui.css aliases (the shadcn contract)
//   → --surface-raised   the canonical role, named as in Figma
//   → --g-theme-stone-raised / --g-theme-graphite-raised
//                        the app's theme slot for each ground
//   → a hex value        under [data-theme="lockdown"], set on <html> in
//                        static/frame.html
//
// So the sheet carries, in this order: the lockdown theme's slots (stone and
// graphite), the shared primitives the chain names (the critical colours),
// the roles on stone, and the same roles on graphite under the media query.
// A slot is written only under [data-theme="lockdown"]: a frame without the
// attribute would draw unicron's default app, which is not Lockdown.

import { readFileSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const here = join(dirname(fileURLToPath(import.meta.url)), "..")
const out = join(here, "static", "tokens.css")
const require = createRequire(import.meta.url)

/** The app theme the frames are drawn in (`data-theme` on static/frame.html). */
export const THEME = "lockdown"

/** The roles the frames' sheet (static/ui.css) aliases. */
export const ROLES = [
  "background",
  "foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "destructive",
  "border",
  "input",
  "ring",
]

/** unicron's comment over each block of its sheet, which names the block whatever its selector is. */
const HEADS = {
  primitives: "Tier 1 · Global primitives",
  theme: `Tier 1 · App theme: ${THEME}`,
  light: "Tier 2 · Semantic roles (--*) · light",
  dark: "Tier 2 · Semantic roles (--*) · dark",
}

/** The declarations of the block that follows the comment starting with `head`, in order. */
function block(css, head, version) {
  const at = css.indexOf(`/* ── ${head}`)
  if (at < 0) throw new Error(`@unicron/tokens ${version} has no block headed "${head}"`)
  const open = css.indexOf("{", at)
  const close = css.indexOf("}", open)
  const decls = new Map()
  for (const d of css.slice(open + 1, close).matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) decls.set(d[1], d[2].trim())
  return { selector: css.slice(css.indexOf("*/", at) + 2, open).trim(), decls }
}

/** Every custom property a value names. */
const named = (value) => Array.from(value.matchAll(/var\((--[a-z0-9-]+)\)/g), (m) => m[1])

export function render() {
  const pkgFile = require.resolve("@unicron/tokens/package.json")
  const { version } = JSON.parse(readFileSync(pkgFile, "utf8"))
  const css = readFileSync(join(dirname(pkgFile), "dist", "tokens.css"), "utf8")
  const primitives = block(css, HEADS.primitives, version)
  const theme = block(css, HEADS.theme, version)
  const light = block(css, HEADS.light, version)
  const dark = block(css, HEADS.dark, version)
  if (theme.selector !== `[data-theme="${THEME}"]`) throw new Error(`@unicron/tokens ${version}: the ${THEME} theme is under ${theme.selector}`)

  // Follow each role down its chain, on both grounds: a role may name roles
  // (--popover → --surface-raised), and a role names primitives (--g-*).
  const roles = new Set()
  const prims = new Set()
  const follow = (name) => {
    if (name.startsWith("--g-")) {
      prims.add(name)
      return
    }
    if (roles.has(name)) return
    const values = [light.decls.get(name), dark.decls.get(name)].filter((v) => v !== undefined)
    if (!light.decls.has(name)) throw new Error(`@unicron/tokens ${version} has no ${name}`)
    roles.add(name)
    for (const v of values) for (const n of named(v)) follow(n)
  }
  for (const r of ROLES) follow(`--${r}`)

  // Each primitive is the app theme's slot or a shared one; none may be missing.
  const slots = [...theme.decls.keys()].filter((g) => prims.has(g))
  const shared = [...primitives.decls.keys()].filter((g) => prims.has(g) && !theme.decls.has(g))
  for (const g of prims) if (!theme.decls.has(g) && !primitives.decls.has(g)) throw new Error(`@unicron/tokens ${version} has no ${g}`)

  // In unicron's own order, so a tokens update reads as a small diff.
  const lightRoles = [...light.decls.keys()].filter((r) => roles.has(r))
  const darkRoles = [...dark.decls.keys()].filter((r) => roles.has(r))

  const line = (decls) => (name) => `  ${name}: ${decls.get(name)};`
  return [
    `/* unicron's colour roles for the extension's in-page frames, as plain CSS.`,
    `   Generated by scripts/tokens-css.mjs from @unicron/tokens ${version}`,
    `   (dist/tokens.css): the roles static/ui.css aliases, the canonical roles`,
    `   they point at, and the ${THEME} theme's slots those stand on, values as`,
    `   unicron declares them; graphite under prefers-color-scheme: dark.`,
    `   Do not edit; run \`node scripts/tokens-css.mjs\` after a tokens update.`,
    `   The frames move to unicron's own plain sheet (@unicron/tokens/plain.css)`,
    `   when they are redesigned (doc 62 §6). */`,
    `[data-theme="${THEME}"] {`,
    ...slots.map(line(theme.decls)),
    `}`,
    `:root {`,
    ...shared.map(line(primitives.decls)),
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
