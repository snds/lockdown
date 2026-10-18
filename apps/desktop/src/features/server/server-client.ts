import { createContext, useContext } from "react"

/** The server's restore drills as it reports them (`lockdown_app::server_checks::RestoreDrillView`). */
export interface RestoreDrillView {
  /** The server answered; an older one has no such report. */
  reported: boolean
  scheduled: boolean
  last_run_at: number | null
  last_passed: boolean | null
  /** The last successful restore. */
  last_success_at: number | null
  /** That drill also signed a client in to the restored copy and read an item back. */
  client_read_back: boolean
  /** The line to show, in plain words. */
  words: string
}

/** What one log mirror said at the last sync (`lockdown_app::types::MirrorCheck`). */
export type MirrorCheck = {
  url: string
  /** `server` (the account server named it) or `settings` (you added it). */
  named_by: string
  at: number
} & (
  | { state: "agrees"; size: number; checked_at: number }
  | { state: "split"; message: string }
  | { state: "unusable"; message: string }
  | { state: "unreachable"; message: string }
)

export interface MirrorsView {
  /** The mirrors you added. */
  configured: string[]
  checks: MirrorCheck[]
}

/** The server checks, as the UI sees them. One method per command. */
export interface ServerClient {
  restoreDrill(): Promise<RestoreDrillView>
  ktMirrors(): Promise<MirrorsView>
  setKtMirrors(urls: string[]): Promise<MirrorsView>
}

/**
 * No server client by default: each host provides one (the desktop over
 * Tauri commands in `app/tauri-server.ts`, the web vault over its worker),
 * so the web vault compiles these views unchanged. Without one the section
 * isn't shown.
 */
const ServerClientContext = createContext<ServerClient | null>(null)

export const ServerClientProvider = ServerClientContext.Provider

export function useServerClient(): ServerClient | null {
  return useContext(ServerClientContext)
}
