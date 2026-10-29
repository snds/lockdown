import { invoke } from "@tauri-apps/api/core"

import { splitOncePage } from "@/core/secret-bytes"
import type { FirstRunClient } from "@/features/unlock/first-run-client"

const bytes = async (cmd: string, args?: Record<string, unknown>) => {
  const body = await invoke<ArrayBuffer | Uint8Array | number[]>(cmd, args)
  if (body instanceof Uint8Array) return body
  if (body instanceof ArrayBuffer) return new Uint8Array(body)
  return Uint8Array.from(body)
}

/**
 * The desktop's first run over Tauri commands
 * (apps/desktop/src-tauri/src/first_run.rs). Only `App.tsx` imports this;
 * feature code reaches it through `FirstRunClientProvider`.
 */
export const tauriFirstRunClient: FirstRunClient = {
  create: async (name, password) => splitOncePage(await bytes("first_run_create", { name, password })),
  unlockWithPassword: (password) => invoke("unlock_with_password", { password }),
  setMasterPassword: async (input) => splitOncePage(await bytes("set_master_password", { input })),
  resetWithKit: (input) => invoke("password_reset_with_kit", input),
  stage: (stage) => invoke("onboarding_stage", { stage }),
  setName: (name) => invoke("set_name", { name }),
  checkMasterPassword: (password, name) => invoke("check_master_password", { password, name: name ?? null }),
  quickUnlock: () => invoke("quick_unlock_status"),
  candidates: (network) => invoke("discovery_candidates", { network }),
  check: (address) => invoke("discovery_check", { address }),
  domain: (login) => invoke("discovery_domain", { login }),
  confirm: (address) => invoke("discovery_confirm", { address }),
  forget: (address) => invoke("discovery_forget", { address }),
  moveToServer: (input) => invoke("move_to_server", input),
}
