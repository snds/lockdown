import { invoke } from "@tauri-apps/api/core"

import type { PqStatusClient } from "@/features/security/pq-status-client"

/**
 * The post-quantum status line over its Tauri command
 * (apps/desktop/src-tauri/src/pq_status.rs). Only `App.tsx` imports this;
 * feature code reaches it through `PqStatusClientProvider`.
 */
export const tauriPqStatusClient: PqStatusClient = {
  status: () => invoke("pq_status"),
}
