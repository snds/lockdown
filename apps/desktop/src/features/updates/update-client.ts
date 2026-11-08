import { createContext, useContext } from "react"

/** Where an update stands (`updater::UpdateState`, apps/desktop/src-tauri/src/updater.rs). */
export type UpdateState = "idle" | "checking" | "up_to_date" | "downloading" | "ready" | "available" | "failed"

/**
 * What Settings → About, the sidebar and the menu show
 * (`updater::UpdateStatus`; docs/plan/60-auto-update.md). Plain data and
 * plain words from the shell; nothing in it is secret.
 */
export interface UpdateStatus {
  /** The running version, `0.1.0-main.20261107093000`. */
  version: string
  channel: "main" | "stable"
  /** Whether this build checks for updates at all. */
  enabled: boolean
  /** Why it doesn't, in one line. */
  off: string | null
  /** The "Check automatically" switch. */
  auto_check: boolean
  state: UpdateState
  /** 0 to 99 while downloading. */
  percent: number | null
  /** The newer version (downloading, ready, available). */
  offered: string | null
  /** What went wrong, in plain words (failed). */
  error: string | null
  /** The release page (available). */
  link: string | null
  /** Why the app points there instead of updating itself (available). */
  link_why: string | null
  /** The last check was asked for (the button or the menu). */
  asked: boolean
}

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
