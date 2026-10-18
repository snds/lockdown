// The toolbar popup's build (docs/plan/26-desktop-finish.md §9): React and
// @unicron/ui with Tailwind v4, as apps/desktop builds them, into
// dist/<family>/popup.html and dist/<family>/popup/. scripts/build.mjs runs
// it once per browser family; nothing here reaches the in-page frame.
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

export default defineConfig({
  root: "src/popup",
  base: "./",
  plugins: [react(), tailwindcss()],
  resolve: { dedupe: ["react", "react-dom", "@base-ui/react"] },
  logLevel: "warn",
  build: {
    emptyOutDir: false,
    assetsDir: "popup",
    // One module, no runtime chunk loading: the extension's CSP is `script-src 'self'`.
    modulePreload: false,
    rollupOptions: { input: "src/popup/popup.html" },
    chunkSizeWarningLimit: 2048,
  },
})
