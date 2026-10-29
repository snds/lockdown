import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

import { iconCodepoints } from "./scripts/icon-codepoints.ts"

// Tauri expects a fixed port and must not clear its own console output.
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  // The icons' codepoints from the shipped font (docs/plan/38-mac-fixes.md §2).
  plugins: [react(), tailwindcss(), iconCodepoints({ root: import.meta.dirname, dirs: [path.resolve(import.meta.dirname, "src")] })],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
    // One instance each across the app and @unicron/ui, or hooks and Base UI's
    // contexts break across the line.
    dedupe: ["react", "react-dom", "@base-ui/react"],
  },
  // The developer tools (docs/plan/40-devtools.md): on unless LOCKDOWN_DEVTOOLS=off,
  // which a tagged release sets; off, the bundle carries none of src/devtools.
  define: { __LOCKDOWN_DEVTOOLS__: JSON.stringify(process.env.LOCKDOWN_DEVTOOLS !== "off") },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.tsx"],
    // Only the dialog-actions sheet, which its test reads as text (R1).
    css: { include: [/dialog-actions\.css/] },
    // The release tarballs land in the pnpm store under a directory named
    // after their URL, with "%2F" in it, which Node's own loader refuses;
    // let Vite transform the design system instead of importing it natively.
    server: { deps: { inline: [/@unicron\//] } },
  },
})
