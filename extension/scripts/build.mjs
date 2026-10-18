// Builds the extension for both browser families from one source:
//   dist/chromium/  Chrome, Edge, Brave, Chromium (MV3 service worker)
//   dist/firefox/   Firefox (MV3 event page)
// The code is the same; the manifests differ in the background key, the
// permission Basic Auth needs, the fixed id (`key` on Chromium,
// `browser_specific_settings.gecko.id` on Firefox). Needs `pkg/` from
// `pnpm run build:wasm`.
import { build } from "esbuild"
import { build as viteBuild } from "vite"
import { cp, mkdir, readFile, rm, writeFile, access } from "node:fs/promises"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const here = join(dirname(fileURLToPath(import.meta.url)), "..")
const dist = join(here, "dist")

try {
  await access(join(here, "pkg", "lockdown_wasm_bg.wasm"))
} catch {
  console.error("build: pkg/ is missing; run `pnpm run build:wasm` first")
  process.exit(2)
}

const base = JSON.parse(await readFile(join(here, "manifest.base.json"), "utf8"))

/** The manifest for one family. */
export function manifestFor(family) {
  const { chromium, firefox, ...common } = base
  const own = family === "chromium" ? chromium : firefox
  const { extra_permissions = [], ...rest } = own
  return { ...common, ...rest, permissions: [...common.permissions, ...extra_permissions] }
}

const entries = [
  { in: "src/background/index.ts", out: "background.js", format: "esm" },
  { in: "src/content/index.ts", out: "content.js", format: "iife" },
  { in: "src/content/passkeys.ts", out: "passkeys.js", format: "iife" },
  { in: "src/main/webauthn.ts", out: "webauthn.js", format: "iife" },
  { in: "src/pages/frame.ts", out: "frame.js", format: "iife" },
]

await rm(dist, { recursive: true, force: true })
for (const family of ["chromium", "firefox"]) {
  const out = join(dist, family)
  await mkdir(out, { recursive: true })
  for (const e of entries) {
    await build({
      entryPoints: [join(here, e.in)],
      outfile: join(out, e.out),
      bundle: true,
      format: e.format,
      target: family === "chromium" ? "chrome121" : "firefox128",
      platform: "browser",
      legalComments: "none",
      logLevel: "warning",
    })
  }
  await cp(join(here, "static"), out, { recursive: true })
  // The popup: React on unicron, built by Vite (vite.popup.config.ts).
  await viteBuild({ configFile: join(here, "vite.popup.config.ts"), root: join(here, "src/popup"), build: { outDir: out } })
  await cp(join(here, "pkg", "lockdown_wasm_bg.wasm"), join(out, "lockdown_wasm_bg.wasm"))
  await writeFile(join(out, "manifest.json"), JSON.stringify(manifestFor(family), null, 2) + "\n")
}
console.log("built dist/chromium and dist/firefox")
