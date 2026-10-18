import { invoke } from "@tauri-apps/api/core"

import type { ServerClient } from "@/features/server/server-client"

/**
 * The desktop's server checks over Tauri commands
 * (apps/desktop/src-tauri/src/server_checks.rs). Only `App.tsx` imports
 * this; feature code reaches it through `ServerClientProvider`.
 */
export const tauriServerClient: ServerClient = {
  restoreDrill: () => invoke("server_restore_drill"),
  ktMirrors: () => invoke("kt_mirrors"),
  setKtMirrors: (urls) => invoke("kt_mirrors_set", { urls }),
}
