import { useEffect, useState } from "react"

import { useClient } from "@/core/client"

/** How often an open vault asks how backups went. */
export const BACKUP_LOOK_MS = 60_000

/**
 * Whether backups are on and the last one failed, in the core's words, read
 * at unlock and every minute while the vault is open. It feeds the critical
 * Banner at the top of Items and of Settings → Backups (brief X2 table 4,
 * D754), which replaced the sidebar's backup line (D274): a stopped backup
 * is still seen, and "Backed up 3 hours ago" lives in Settings. Null where
 * all is well, and where the host has no backups (the web vault refuses
 * the call).
 */
export function useBackupTrouble(open: boolean): string | null {
  const client = useClient()
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (!open) {
      setError(null)
      return
    }
    let live = true
    const look = () =>
      client
        .backupStatus()
        .then((s) => live && setError(s.enabled && s.last_error ? s.last_error : null))
        .catch(() => live && setError(null))
    void look()
    const t = window.setInterval(() => void look(), BACKUP_LOOK_MS)
    return () => {
      live = false
      window.clearInterval(t)
    }
  }, [client, open])
  return error
}
