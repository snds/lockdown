import { invoke } from "@tauri-apps/api/core"

import type { GateInput } from "@/core/types"
import type { HygieneClient } from "@/features/hygiene/hygiene-client"
import type { QuickClient } from "@/features/quick/quick-client"
import type { Frame, ScanClient } from "@/features/scan/scan-client"

const g = (gate?: GateInput) => (gate ? { gate } : {})

/**
 * The desktop's quick-access, scan and hygiene clients: one Tauri command
 * per method (apps/desktop/src-tauri/src/{quick,scan,hygiene}.rs). Only the
 * app's roots import this; feature code reaches it through the providers.
 */
export const tauriQuickClient: QuickClient = {
  search: (query, limit) => invoke("quick_search", { query, limit }),
  quickCode: async (itemId, fieldId, gate) => new Uint8Array(await invoke<ArrayBuffer>("quick_code", { itemId, fieldId, ...g(gate) })),
  quickFill: (itemId) => invoke("quick_fill", { itemId }),
  canFill: true,
  hide: () => invoke("quick_hide"),
  openMain: (itemId) => invoke("quick_open_main", { itemId }),
  hotkeyStatus: () => invoke("quick_hotkey_status"),
  setHotkey: (on) => invoke("quick_hotkey_set", { on }),
}

/** A frame crosses as raw bytes, its size and source in the headers. */
const frameHeaders = (f: Frame, source: string) => ({
  headers: { "x-lockdown-width": String(f.width), "x-lockdown-height": String(f.height), "x-lockdown-source": source },
})

export const tauriScanClient: ScanClient = {
  frame: (f, source) => invoke("scan_frame", f.luma, frameHeaders(f, source)),
  text: (text) => invoke("scan_text", { text }),
  approve: (activationId, itemId, gate) => invoke("activation_approve", { activationId, itemId, ...g(gate) }),
  cancel: (activationId) => invoke("activation_cancel", { activationId }),
  contactFrame: (f) => invoke("qr_contact_frame", f.luma, frameHeaders(f, "camera")),
}

export const tauriHygieneClient: HygieneClient = {
  protect: (on) => invoke("capture_protect", { on }),
  memory: () => invoke("memory_report"),
}
