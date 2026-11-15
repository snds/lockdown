import type { NavMarkProp } from "@unicron/ui"

import type { UpdateStatus } from "./update-client"
import { useUpdates } from "./use-updates"

/**
 * A ready update, on the way into Settings (cross-app decisions §3, brief
 * X2): a square mark on the Settings nav item, its rail item and the
 * phone's profile button, whose words join each one's name. "Restart to
 * update" itself lives in Settings → About (`UpdateStatusRow`); there is no
 * restart row in the sidebar, no toast, no strip and no status-line button
 * for an update (D755). An update not taken installs when Lockdown quits.
 */
export function updateMark(status: UpdateStatus | null): NavMarkProp | undefined {
  if (status?.state !== "ready") return undefined
  return { square: true, label: status.offered ? `update ${status.offered} ready` : "update ready" }
}

/** The update mark for the way into Settings, kept current from the updater; none where the host has no updater. */
export function useUpdateMark(): NavMarkProp | undefined {
  return updateMark(useUpdates())
}
