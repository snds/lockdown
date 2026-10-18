import { useEffect, useState } from "react"

import type { Client } from "@/core/client"
import { useClient } from "@/core/client"
import type { Playbook } from "@/core/types"

let cached: { client: Client; playbooks: Promise<Playbook[]> } | null = null

/**
 * The core's playbooks (static), fetched once per client and shared by the
 * Security screen and an item's own security panel. `null` until they arrive.
 */
export function usePlaybooks(): Record<string, Playbook> | null {
  const client = useClient()
  const [byId, setById] = useState<Record<string, Playbook> | null>(null)
  useEffect(() => {
    let live = true
    if (!cached || cached.client !== client) {
      const playbooks = client.securityPlaybooks()
      cached = { client, playbooks }
      playbooks.catch(() => {
        if (cached?.playbooks === playbooks) cached = null
      })
    }
    cached.playbooks
      .then((list) => live && setById(Object.fromEntries(list.map((p) => [p.id, p]))))
      .catch(() => live && setById({}))
    return () => {
      live = false
    }
  }, [client])
  return byId
}
