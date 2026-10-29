import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

import { readTemplates } from "./scripts/rust-templates.ts"

const here = import.meta.dirname
const repo = path.resolve(here, "../..")
const desktop = path.join(repo, "apps/desktop/src")

/** The core's templates, read from the Rust source (scripts/rust-templates.ts). */
function coreTemplates(): Plugin {
  const id = "virtual:lockdown-templates"
  const file = path.join(repo, "crates/vault/src/template.rs")
  return {
    name: "lockdown-core-templates",
    resolveId: (source) => (source === id ? `\0${id}` : null),
    load(source) {
      if (source !== `\0${id}`) return null
      this.addWatchFile(file)
      return `export default ${JSON.stringify(readTemplates(file))}`
    },
  }
}

/**
 * The review build: the desktop's, the web vault's and the extension's own
 * components, imported from their sources, over the fake host in src/host.
 * Only the host's edges are swapped: Tauri's `invoke` and `listen` (every
 * desktop client goes through them) and, at run time, the extension's
 * `chrome.runtime`. Static files with relative paths, so the folder opens
 * from any place it is copied to.
 */
export default defineConfig({
  base: "./",
  plugins: [coreTemplates(), react(), tailwindcss()],
  define: {
    // The web vault's worker module is never loaded here; its type file reads this name.
    __LOCKDOWN_WASM_SHA384__: JSON.stringify(""),
  },
  resolve: {
    alias: [
      // The desktop's views, as they are (`@/…` is their own alias).
      { find: /^@\//, replacement: `${desktop}/` },
      // The two Tauri modules the desktop's clients call: the fake host.
      { find: /^@tauri-apps\/api\/core$/, replacement: path.join(here, "src/host/tauri-core.ts") },
      { find: /^@tauri-apps\/api\/event$/, replacement: path.join(here, "src/host/tauri-event.ts") },
    ],
    // One copy of each across this harness and the three apps' sources, or
    // hooks and Base UI's contexts break.
    dedupe: ["react", "react-dom", "@base-ui/react", "@unicron/ui", "@unicron/tokens", "sonner", "next-themes"],
  },
  server: { fs: { allow: [repo] } },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 8192,
    // Fonts and images as files, never data: URLs.
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
    // Two pages: the review page, and the extension's in-page frame on its
    // own, so the frame loads none of the apps' styles (src/frame-main.ts).
    rollupOptions: { input: { index: path.join(here, "index.html"), frame: path.join(here, "frame.html") } },
  },
})
