import type { PqStatusClient } from "@/features/security/pq-status-client"

import type { Link } from "./link"

/**
 * The post-quantum status line over the web vault's worker
 * (crates/web/src/commands/server.rs), under the desktop's command name.
 */
export function webPqStatusClient(link: Link): PqStatusClient {
  return {
    status: () => link.call("pq_status"),
  }
}
