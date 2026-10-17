import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

// Tauri expects a fixed port and must not clear its own console output.
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
    // One instance each across the app and @unicron/ui, or hooks and Base UI's
    // contexts break across the line.
    dedupe: ["react", "react-dom", "@base-ui/react"],
  },
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
    css: false,
    // The release tarballs land in the pnpm store under a directory named
    // after their URL, with "%2F" in it, which Node's own loader refuses;
    // let Vite transform the design system instead of importing it natively.
    server: { deps: { inline: [/@unicron\//] } },
  },
})
