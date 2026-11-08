import { createContext, useContext } from "react"

import type { UpdateStatus } from "@unicron/ui"

/**
 * Where an update stands, and what Settings → About, the sidebar and the
 * menu show (`updater::UpdateState` and `updater::UpdateStatus`,
 * apps/desktop/src-tauri/src/updater.rs; docs/plan/60-auto-update.md): the
 * update kit's status, typed once in unicron for every app. Plain data and
 * plain words from the shell; nothing in it is secret. Lockdown's shell
 * reports the "main" and "stable" channels.
 */
export type { UpdateState, UpdateStatus } from "@unicron/ui"

/** The window event every change arrives on. */
export const UPDATE_EVENT = "lockdown://update"

/** The updater's five commands and its event (apps/desktop/src-tauri/src/updater/shell.rs). */
export interface UpdateClient {
  status(): Promise<UpdateStatus>
  /** Checks now and starts the download; the outcome arrives through `onStatus`. */
  check(): Promise<UpdateStatus>
  /** Locks the vault, installs the downloaded update, and relaunches. */
  restart(): Promise<void>
  /** The "Check automatically" switch. */
  setAuto(on: boolean): Promise<UpdateStatus>
  /** Locks the vault, then closes and reopens the app (installing a downloaded update on the way). */
  reload(): Promise<void>
  /** Every change; resolves to the unsubscribe. */
  onStatus(handler: (s: UpdateStatus) => void): Promise<() => void>
}

/**
 * No client by default: only the desktop provides one (over its Tauri
 * commands in `app/tauri-updates.ts`), so the web vault compiles Settings
 * unchanged and shows no About section.
 */
const UpdateContext = createContext<UpdateClient | null>(null)

export const UpdateClientProvider = UpdateContext.Provider

export function useUpdateClient(): UpdateClient | null {
  return useContext(UpdateContext)
}
