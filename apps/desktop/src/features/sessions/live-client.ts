import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

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

export const tauriLiveClient: LiveClient = {
  sessionList: () => invoke("session_list"),
  sessionEnd: (id) => invoke("session_end", { id }),
  sessionEndOthers: () => invoke("session_end_others"),
  onLive: async (handler) => listen<LiveNotice>(LIVE_EVENT, (e) => handler(e.payload)),
}

const LiveClientContext = createContext<LiveClient>(tauriLiveClient)

/** The app's live client; tests provide a fake one. */
export const LiveClientProvider = LiveClientContext.Provider

export function useLiveClient(): LiveClient {
  return useContext(LiveClientContext)
}
