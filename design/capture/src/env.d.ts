/// <reference types="vite/client" />

declare module "virtual:lockdown-templates" {
  import type { Templates } from "@/core/types"
  const templates: Templates
  export default templates
}

/** Baked in by the web vault's build; the harness never loads the worker that reads it. */
declare const __LOCKDOWN_WASM_SHA384__: string

/** The desktop's developer tools are drawn here too (docs/plan/40-devtools.md); vite.config.ts sets it. */
declare const __LOCKDOWN_DEVTOOLS__: boolean
