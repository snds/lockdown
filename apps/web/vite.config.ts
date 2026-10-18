import { createHash } from "node:crypto"
import { existsSync, readFileSync } from "node:fs"
import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import type { Plugin } from "vite"
import { defineConfig } from "vitest/config"

const here = import.meta.dirname
const desktop = path.resolve(here, "../desktop/src")
const wasm = path.resolve(here, "wasm/lockdown_web.wasm")

/** The core module's SHA-384, baked into the worker, which checks it before compiling. */
function wasmHash(): string {
  if (!existsSync(wasm)) return ""
  return createHash("sha384").update(readFileSync(wasm)).digest("base64")
}

/**
 * Subresource Integrity on every script and stylesheet `index.html` names
 * (docs/plan/17-web-vault.md §6): the hash of each file as built, so a
 * file changed after the build (a cache, a proxy, a disk) is refused.
 */
function subresourceIntegrity(): Plugin {
  return {
    name: "lockdown-sri",
    apply: "build",
    enforce: "post",
    transformIndexHtml: {
      order: "post",
      handler(html, ctx) {
        const bundle = ctx.bundle
        if (!bundle) return html
        const base = "/vault/"
        const integrity = (url: string): string | null => {
          if (!url.startsWith(base)) return null
          const file = bundle[url.slice(base.length)]
          if (!file) return null
          const bytes = file.type === "chunk" ? file.code : file.source
          return `sha384-${createHash("sha384").update(bytes).digest("base64")}`
        }
        return html.replace(/<(script|link)\b([^>]*?)\s(src|href)="([^"]+)"([^>]*)>/g, (tag, name: string, before: string, attr: string, url: string, after: string) => {
          const hash = integrity(url)
          if (!hash || tag.includes("integrity=")) return tag
          return `<${name}${before} ${attr}="${url}" integrity="${hash}"${after}>`
        })
      },
    },
  }
}

if (process.env.NODE_ENV === "production" && !existsSync(wasm)) {
  throw new Error("apps/web/wasm/lockdown_web.wasm is missing: run `pnpm build:wasm` first")
}

export default defineConfig({
  base: "/vault/",
  plugins: [react(), tailwindcss(), subresourceIntegrity()],
  define: {
    __LOCKDOWN_WASM_SHA384__: JSON.stringify(wasmHash()),
  },
  resolve: {
    alias: [
      // The desktop's views, reused as they are (`@/…` is their own alias).
      { find: /^@\//, replacement: `${desktop}/` },
      // The desktop's Tauri client is never used here; see src/tauri-stub.ts.
      { find: /^@tauri-apps\/api\/core$/, replacement: path.resolve(here, "src/tauri-stub.ts") },
    ],
    // One copy of each across this app, the desktop's sources and
    // @unicron/ui, resolved from here, or hooks and Base UI's contexts break.
    dedupe: ["react", "react-dom", "@base-ui/react", "@unicron/ui", "@unicron/tokens", "sonner", "next-themes"],
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // Nothing inlined as a data: URL; every asset is a file under /vault/.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  worker: {
    format: "es",
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    css: false,
    server: { deps: { inline: [/@unicron\//] } },
  },
})
