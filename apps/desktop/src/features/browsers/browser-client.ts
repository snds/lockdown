import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type { GateInput } from "@/core/types"

/** A browser asking to pair, with the code it shows (src-tauri/src/browser/pairing.rs). */
export interface PendingPairing {
  request_id: string
  code: string
  name: string
  family: "chromium" | "firefox"
  extension_id: string
  expires_in_secs: number
}

/** A paired browser. */
export interface PairedBrowser {
  client_id: string
  name: string
  family: "chromium" | "firefox"
  extension_id: string
  paired_at: number
}

export type BrowserId = "chrome" | "chromium" | "edge" | "brave" | "firefox"

/** The endpoint's state and the browsers on this computer. */
export interface EndpointStatus {
  listening: boolean
  problem: string | null
  host_binary: string
  browsers: { browser: BrowserId; name: string; connected: boolean }[]
  /**
   * How the browser host is checked (docs/plan/29-release-polish.md §2):
   * by its code signature on a signed build, by path and SHA-256 on an
   * unsigned development build or on Linux. Absent from an older app.
   */
  peer_check?: { by: "signature" | "path_and_hash" | "refused"; detail: string }
}

/** Where a browser's host manifest was written. */
export interface InstallTarget {
  browser: BrowserId
  manifest: string
  registry_key: string | null
}

/**
 * The browser extension's commands (src-tauri/src/browser_commands.rs),
 * kept apart from the main client so the extension slice stays in its own
 * folder. Nothing here carries a secret: pairing codes, names and paths.
 */
export interface BrowserClient {
  status(): Promise<EndpointStatus>
  pending(): Promise<PendingPairing[]>
  /** An approval goes through the `Approve` gate (D101). */
  answer(requestId: string, approve: boolean, gate?: GateInput): Promise<void>
  paired(): Promise<PairedBrowser[]>
  unpair(clientId: string): Promise<void>
  install(browsers: BrowserId[]): Promise<InstallTarget[]>
}

export const tauriBrowserClient: BrowserClient = {
  status: () => invoke("browser_status"),
  pending: () => invoke("browser_pending"),
  answer: (requestId, approve, gate) => invoke("browser_answer", { requestId, approve, gate }),
  paired: () => invoke("browser_paired"),
  unpair: (clientId) => invoke("browser_unpair", { clientId }),
  install: (browsers) => invoke("browser_install", { browsers }),
}

const BrowserClientContext = createContext<BrowserClient>(tauriBrowserClient)

export const BrowserClientProvider = BrowserClientContext.Provider

export function useBrowserClient(): BrowserClient {
  return useContext(BrowserClientContext)
}

/** "653540" → "653 540", the way the extension shows it. */
export function spaced(code: string): string {
  return `${code.slice(0, 3)} ${code.slice(3)}`
}
