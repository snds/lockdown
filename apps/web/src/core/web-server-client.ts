import type { ServerClient } from "@/features/server/server-client"

import type { Link } from "./link"

/**
 * The server checks over the web vault's worker
 * (crates/web/src/commands/server.rs), under the desktop's command names.
 * The worker fetches a mirror's head only from the origins the server's
 * policy allows (docs/plan/27-web-server-finish.md §4).
 */
export function webServerClient(link: Link): ServerClient {
  return {
    restoreDrill: () => link.call("server_restore_drill"),
    ktMirrors: () => link.call("kt_mirrors"),
    setKtMirrors: (urls) => link.call("kt_mirrors_set", { urls }),
  }
}
