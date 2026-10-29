import { createContext, useContext } from "react"

/** One device holding the stronger signatures back, as the Devices screen names it. */
export interface PqDevice {
  id: string
  name: string
}

/**
 * The post-quantum status line (`lockdown_app::pq_status::PqStatusView`;
 * schema: crates/app/schema/pq-status.schema.json). The words come from
 * the core, so the desktop, the web vault and `lockdown security` say the
 * same thing.
 */
export interface PqStatus {
  state: "on" | "waiting" | "unknown"
  /** Why it is `unknown`; `null` otherwise. */
  reason: "no_account" | "unreached" | "silent" | "not_published" | "managed_elsewhere" | null
  /** "Quantum-safe signatures: on", "…: waiting for …", "…: can't check right now". */
  headline: string
  /** One short sentence under it. */
  explanation: string
  /** Devices that don't read the account's post-quantum key yet. */
  without_level_1: PqDevice[]
  /** Devices that read it but not post-quantum vault heads yet (never also in `without_level_1`). */
  without_level_2: PqDevice[]
}

/** The one call behind the line (docs/plan/30-pq-status-line.md). */
export interface PqStatusClient {
  status(): Promise<PqStatus>
}

/**
 * No client by default: each host provides one (the desktop over its Tauri
 * command in `app/tauri-pq-status.ts`, the web vault over its worker), so
 * the web vault compiles the line unchanged. Without one the line isn't
 * shown.
 */
const PqStatusContext = createContext<PqStatusClient | null>(null)

export const PqStatusClientProvider = PqStatusContext.Provider

export function usePqStatusClient(): PqStatusClient | null {
  return useContext(PqStatusContext)
}
