import { createContext, useContext } from "react"

/** One place the account is signed in (docs/plan/20-self-host-server.md §6). */
export interface SessionView {
  id: string
  device_id: string
  /** The device's name from the identity's records, when it is there. */
  device_name: string | null
  platform: string | null
  /** Where it was last used from, as the server saw it. */
  address: string | null
  /** The client's name for itself, `lockdown/0.1.0 (macos)`. */
  client: string | null
  created_at: number
  last_seen: number
  expires_at: number
  /** This device's own session. */
  current: boolean
}

/** What the server's live channel did (`lockdown_app::live::LiveNotice`). */
export type LiveNotice =
  | { kind: "connected" }
  | { kind: "disconnected"; message: string }
  | { kind: "synced"; pulled: number; lines: string[] }
  | { kind: "sync_failed"; message: string }
  | { kind: "alerts" }
  | { kind: "sessions" }
  | { kind: "session_ended" }

/** The window event every notice arrives as (apps/desktop/src-tauri/src/live.rs). */
export const LIVE_EVENT = "lockdown://live"

/** Sessions and the live channel, as the UI sees them. One method per Tauri command. */
export interface LiveClient {
  sessionList(): Promise<SessionView[]>
  /** Ends one session on the server; its token stops at once. */
  sessionEnd(id: string): Promise<void>
  /** Ends every session but this device's; resolves to how many. */
  sessionEndOthers(): Promise<number>
  /** Calls `handler` for every notice until the returned function is called. */
  onLive(handler: (notice: LiveNotice) => void): Promise<() => void>
}

/**
 * No live client by default: a host that has one provides it (the desktop
 * fills it with Tauri's commands and window events in `app/tauri-live.ts`;
 * the web vault has none yet). Without one the Signed in section isn't
 * shown and `useLive` does nothing. Feature code never imports a host's
 * API directly, so the web vault compiles these views unchanged (D183).
 */
const LiveClientContext = createContext<LiveClient | null>(null)

/** The host's live client; tests provide a fake one. */
export const LiveClientProvider = LiveClientContext.Provider

export function useLiveClient(): LiveClient | null {
  return useContext(LiveClientContext)
}
