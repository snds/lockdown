import { Button, Icon } from "@unicron/ui"

import { describeError } from "@/core/client"

import { useUpdateClient } from "./update-client"
import { useUpdates } from "./use-updates"

/**
 * The sidebar's one quiet action once an update is downloaded and checked
 * (docs/plan/60-auto-update.md §6). Nothing shows otherwise; an update not
 * taken installs when Lockdown quits.
 */
export function RestartRow({ onError }: { onError: (text: string) => void }) {
  const updates = useUpdateClient()
  const status = useUpdates()
  if (!updates || status?.state !== "ready") return null
  return (
    <Button
      variant="ghost"
      size="sm"
      className="w-full justify-start"
      aria-label={`Restart to update to version ${status.offered ?? ""}. Lockdown locks first.`}
      onClick={() => updates.restart().catch((e) => onError(describeError(e)))}
    >
      <Icon name="restart_alt" data-icon="inline-start" />
      <span className="flex-1 truncate text-left">Restart to update</span>
    </Button>
  )
}
