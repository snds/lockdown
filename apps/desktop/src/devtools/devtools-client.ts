import { createContext, useContext } from "react"

import type { Status } from "@/core/types"

/** One place on disk a reset erases (crates/devtools/src/reset.rs `Place`). */
export interface ResetPlace {
  what: string
  path: string
}

/** Everything a reset would do, worked out by the shell when asked. */
export interface ResetPlan {
  erase: ResetPlace[]
  keystore: { service: string; entries: string[]; problem?: string }
  identity: { root: string; erase: boolean; why: string; apps: string[] }
  keeps: string[]
  refused: ResetPlace[]
  /** The web view's own storage is cleared too. */
  web_storage: boolean
}

export type CameFrom = "first_run" | "vault"

/** The demo as the shell reports it. */
export interface DemoState {
  open: boolean
  came_from?: CameFrom
  pack?: string
  items: number
}

/** What opening (or rebuilding) the demo gives back. */
export interface DemoOpened {
  status: Status
  came_from: CameFrom
  loaded: { pack: string; items: number; by_kind: Record<string, number>; notes: string[] }
}

/** Which developer tools this build has. */
export interface DevtoolsStatus {
  marker: string
  reset: boolean
  /** Absent when the demo isn't built in. */
  demo?: DemoState
}

/** What the Developer menu asks for. */
export type MenuAction = "reset" | "demo_open" | "demo_reset" | "demo_load"

/**
 * The shell's developer commands (crates/devtools/src/shell.rs). Every
 * call is the main window's only; the shell refuses any other.
 */
export interface DevtoolsClient {
  /** `null` when the shell has no developer tools. */
  status(): Promise<DevtoolsStatus | null>
  resetPlan(): Promise<ResetPlan>
  /** Erases and relaunches; `confirm` must be the word `reset`. */
  resetRun(confirm: string): Promise<void>
  demoOpen(): Promise<DemoOpened>
  demoReset(): Promise<DemoOpened>
  demoLeave(): Promise<CameFrom>
  /** `null` when the file dialog was cancelled. */
  demoLoadPack(): Promise<DemoOpened | null>
  /** Calls `on` for each Developer menu pick; returns the unsubscribe. */
  onMenu(on: (action: MenuAction) => void): () => void
}

const Context = createContext<DevtoolsClient | null>(null)

export const DevtoolsClientProvider = Context.Provider

/** The client a test installed, else `fallback` (the shell's). */
export function useDevtoolsClient(fallback: DevtoolsClient): DevtoolsClient {
  return useContext(Context) ?? fallback
}
