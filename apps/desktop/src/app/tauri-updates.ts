import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

import { UPDATE_EVENT, type UpdateClient, type UpdateStatus } from "@/features/updates/update-client"

/**
 * The updater over its Tauri commands (apps/desktop/src-tauri/src/updater/shell.rs).
 * Only `App.tsx` imports this; feature code reaches it through `UpdateClientProvider`.
 */
export const tauriUpdateClient: UpdateClient = {
  status: () => invoke("update_status"),
  check: () => invoke("update_check"),
  restart: () => invoke("update_restart"),
  setAuto: (on) => invoke("update_auto_set", { on }),
  reload: () => invoke("app_reload"),
  onStatus: async (handler) => listen<UpdateStatus>(UPDATE_EVENT, (e) => handler(e.payload)),
}
