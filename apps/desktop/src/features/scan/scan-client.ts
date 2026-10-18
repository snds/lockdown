import { createContext, useContext } from "react"

import type { GateInput } from "@/core/types"

/** Where a code came from: only the camera passes the proximity gate. */
export type ScanSource = "camera" | "image"

/** A login that could sign in on the activation page. */
export interface ActivationAccount {
  item_id: string
  title: string
  username: string
}

/** What the core made of a scan or a paste (lockdown_app::scan::ScanOutcome). */
export interface ScanOutcome {
  found: boolean
  kind: "device_code" | "passkey_qr" | "steam_qr" | "unlisted" | "other" | "none"
  decision: "approve" | "refused" | "unsupported"
  reason?: string
  service?: string
  host?: string
  code?: string
  code_in_url: boolean
  activation_id?: string
  accounts: ActivationAccount[]
}

/** An approved activation: the page that opened. */
export interface ActivationStarted {
  url: string
  host: string
  service: string
  code: string
  code_in_url: boolean
}

/** One grey frame: `luma` is `width × height` bytes, row by row. */
export interface Frame {
  width: number
  height: number
  luma: Uint8Array
}

/**
 * Approving a TV or console sign-in (docs/plan/26-desktop-finish.md §3):
 * frames go to the core, which finds the QR code, runs the proximity gate
 * and the signed allow-list; the yes goes through the `Approve` gate.
 */
export interface ScanClient {
  frame(frame: Frame, source: ScanSource): Promise<ScanOutcome>
  text(text: string): Promise<ScanOutcome>
  approve(activationId: string, itemId: string | null, gate?: GateInput): Promise<ActivationStarted>
  cancel(activationId: string): Promise<void>
  /** A contact's QR code (`lockdown:contact:v1:…`) in a frame, or `null`. */
  contactFrame(frame: Frame): Promise<string | null>
}

const refuse = () => Promise.reject({ code: "refused", message: "Scanning isn't available here" })

export const noScanClient: ScanClient = {
  frame: refuse,
  text: refuse,
  approve: refuse,
  cancel: () => Promise.resolve(),
  contactFrame: () => Promise.resolve(null),
}

const ScanClientContext = createContext<ScanClient>(noScanClient)

export const ScanClientProvider = ScanClientContext.Provider

export function useScanClient(): ScanClient {
  return useContext(ScanClientContext)
}
