import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type { GateInput } from "@/core/types"

/** One passkey as the management view shows it (crates/app/src/passkeys.rs `PasskeyView`). No secret. */
export interface PasskeyView {
  credential_id: string
  rp_id: string
  user_name: string
  user_display_name: string
  /** "Lockdown", "Bitwarden", "Another provider (imported)". */
  provider: string
  /** The provider ID (AAGUID), when the passkey carries one. */
  aaguid: string | null
  algorithm: string
  /** BE: the passkey may sync. */
  backup_eligible: boolean
  /** BS: it does. */
  backed_up: boolean
  created: number | null
  /** On this computer only. */
  last_used: number | null
  hidden: { reason: string; at: number } | null
  /** Deleting it leaves the account with no way in that Lockdown holds. */
  only_sign_in: boolean
}

/** A passkey request from a browser waiting for the person (src-tauri/src/browser/passkeys.rs). */
export interface PasskeyApproval {
  request_id: string
  browser: string
  text: string
  expires_in_secs: number
}

/**
 * The passkey commands (src-tauri/src/passkey_commands.rs), apart from the
 * main client so this slice stays in its own folder. Nothing here carries
 * a secret: a passkey's key never leaves the core.
 */
export interface PasskeyClient {
  list(itemId: string): Promise<PasskeyView[]>
  /** A crown jewel's delete goes through the `Use` gate. */
  remove(itemId: string, credentialId: string, gate?: GateInput): Promise<number>
  /** Undoes a site's Signal: the passkey is offered again. */
  restore(itemId: string, credentialId: string): Promise<number>
  pending(): Promise<PasskeyApproval[]>
  /** An approval goes through the `Approve` gate (the account password here). */
  answer(requestId: string, approve: boolean, gate?: GateInput): Promise<void>
}

export const tauriPasskeyClient: PasskeyClient = {
  list: (itemId) => invoke("passkeys", { itemId }),
  remove: (itemId, credentialId, gate) => invoke("passkey_delete", { itemId, credentialId, gate }),
  restore: (itemId, credentialId) => invoke("passkey_restore", { itemId, credentialId }),
  pending: () => invoke("browser_passkey_pending"),
  answer: (requestId, approve, gate) => invoke("browser_passkey_answer", { requestId, approve, gate }),
}

const PasskeyClientContext = createContext<PasskeyClient>(tauriPasskeyClient)

export const PasskeyClientProvider = PasskeyClientContext.Provider

export function usePasskeyClient(): PasskeyClient {
  return useContext(PasskeyClientContext)
}
