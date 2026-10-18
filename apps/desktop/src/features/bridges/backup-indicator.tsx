import { useEffect, useState } from "react"

import { Button, Icon } from "@unicron/ui"

import { useClient } from "@/core/client"
import type { BackupStatus } from "@/core/types"

import { backupLine } from "./bridges-model"

/**
 * The last-backup line in the sidebar (docs/plan/23-bridges.md §10): when
 * the last backup ran, or that none did, outside Settings, so a backup that
 * stopped is seen. Opens Settings, where backups are set. Says nothing
 * where the host has no backups (the web vault refuses the call).
 */
export function BackupIndicator({ onOpen }: { onOpen: () => void }) {
  const client = useClient()
  const [status, setStatus] = useState<BackupStatus | null>(null)
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    let live = true
    const load = () =>
      client
        .backupStatus()
        .then((s) => live && setStatus(s))
        .catch(() => live && setStatus(null))
    void load()
    const t = window.setInterval(() => {
      setNow(Math.floor(Date.now() / 1000))
      void load()
    }, 60_000)
    return () => {
      live = false
      window.clearInterval(t)
    }
  }, [client])
  if (!status) return null
  const { text, late } = backupLine(status, now)
  return (
    <Button variant="ghost" size="sm" onClick={onOpen} aria-label={`${text}. Open backup settings`}>
      <span className={late ? "text-attention-text" : "text-muted-foreground"}>
        <Icon name={late ? "warning" : "backup"} />
      </span>
      <span className={late ? "text-attention-text" : "text-muted-foreground"}>{text}</span>
    </Button>
  )
}
