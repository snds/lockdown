import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

import { LIVE_EVENT, type LiveClient, type LiveNotice } from "@/features/sessions/live-client"

/**
 * The desktop's live client: the session commands and the core's live
 * notices as Tauri window events (apps/desktop/src-tauri/src/live.rs).
 * Only the desktop's `App.tsx` imports this; feature code reaches it
 * through `LiveClientProvider`.
 */
export const tauriLiveClient: LiveClient = {
  sessionList: () => invoke("session_list"),
  sessionEnd: (id) => invoke("session_end", { id }),
  sessionEndOthers: () => invoke("session_end_others"),
  onLive: async (handler) => listen<LiveNotice>(LIVE_EVENT, (e) => handler(e.payload)),
}
