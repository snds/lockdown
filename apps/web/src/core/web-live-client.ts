import type { LiveClient } from "@/features/sessions/live-client"

import type { Link } from "./link"

/**
 * The desktop's `LiveClient` over the web vault's worker
 * (docs/plan/27-web-server-finish.md §2): the session list's three calls
 * under the desktop's command names, and the notices of the live channel
 * the worker holds itself. The token never reaches this side; only the
 * notices do (plain words and counts).
 */
export function webLiveClient(link: Link): LiveClient {
  return {
    sessionList: () => link.call("session_list"),
    sessionEnd: (id) => link.call("session_end", { id }),
    sessionEndOthers: () => link.call("session_end_others"),
    onLive: (handler) => Promise.resolve(link.onLive ? link.onLive(handler) : () => {}),
  }
}
