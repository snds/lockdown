import { useState } from "react"

import { AutoCheckSwitch, FieldRow, FieldRowAction, SectionHeader, UpdateStatusRow, useAppFrame, type UpdateWords } from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { Status } from "@/core/types"

import { useUpdateClient } from "./update-client"
import { useUpdates } from "./use-updates"

/** Lockdown's words where they differ from unicron's: the switch names what it checks. */
const WORDS: Partial<UpdateWords> = { autoCheck: "Check for updates automatically" }

/** A server address as a row shows it: its host. */
function hostOf(address: string): string {
  try {
    return new URL(address).host
  } catch {
    return address
  }
}

/**
 * Settings → About (brief T10, cross-app decisions §3; docs/plan/60-auto-update.md §6).
 * THIS APP: unicron's update rows (`UpdateStatusRow`, `AutoCheckSwitch`):
 * the version and channel, one status line, "Check for updates", "Restart
 * to update" once an update is ready, "Reload app" otherwise, the release
 * page when this copy can't replace itself, and the switch for the
 * automatic check. A ready update shows only here and as a mark on the way
 * into Settings: no toast, strip or sidebar row (D755).
 * SERVER: the account's server and the way to Settings → Server. The
 * server updates itself from its own admin page (doc 61); the app has no
 * reading of its version, so the row names where to look.
 * Not shown where the host has no updater (the web vault).
 */
export function AboutSection({ status }: { status?: Status } = {}) {
  const updates = useUpdateClient()
  const client = useClient()
  const updateStatus = useUpdates()
  const { go } = usePlaces()
  const phone = useAppFrame().phone
  const [error, setError] = useState<string | null>(null)
  if (!updates || !updateStatus) return null

  const run = (p: Promise<unknown>) => {
    setError(null)
    p.catch((e) => setError(describeError(e)))
  }
  const server = status?.account && !status.account.local ? status.account.server : null

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col">
        <SectionHeader level={3} title="This app" />
        <UpdateStatusRow
          status={updateStatus}
          appName="Lockdown"
          words={WORDS}
          onCheck={() => run(updates.check())}
          onRestart={() => run(updates.restart())}
          onReload={() => run(updates.reload())}
          // The web view won't open a window itself; the shell opens the release page.
          onOpenLink={(link) => run(client.openSite(link))}
          restartNote="Restarting locks Lockdown. Unlock it with your master password as usual."
          error={error}
        />
        <AutoCheckSwitch status={updateStatus} words={WORDS} onCheckedChange={(on) => run(updates.setAuto(on))} />
      </div>
      {server && (
        <div className="flex flex-col">
          <SectionHeader level={3} title="Server" />
          <FieldRow
            size={phone ? "stacked" : "default"}
            label="Server"
            description="Your server checks for its own updates. Its operator updates it from its admin page."
            action={<FieldRowAction onClick={() => go("settings-server")}>Open</FieldRowAction>}
          >
            {hostOf(server)}
          </FieldRow>
        </div>
      )}
    </div>
  )
}
