import { useState } from "react"

import { AutoCheckSwitch, UpdateStatusRow, type UpdateWords } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"

import { useUpdateClient } from "./update-client"
import { useUpdates } from "./use-updates"

/** Lockdown's words where they differ from unicron's: the switch names what it checks. */
const WORDS: Partial<UpdateWords> = { autoCheck: "Check for updates automatically" }

/**
 * Settings → About (docs/plan/60-auto-update.md §6): unicron's update rows
 * (`UpdateStatusRow`, `AutoCheckSwitch`) in the About card. They show the
 * version and channel, one status line, "Check for updates" (a check now,
 * and the download if there is one), "Restart to update" once an update is
 * ready, "Reload app" otherwise, the release page when this copy can't
 * replace itself, and the switch for the automatic check. Not shown where
 * the host has no updater (the web vault).
 */
export function AboutSection() {
  const updates = useUpdateClient()
  const client = useClient()
  const status = useUpdates()
  const [error, setError] = useState<string | null>(null)
  if (!updates || !status) return null

  const run = (p: Promise<unknown>) => {
    setError(null)
    p.catch((e) => setError(describeError(e)))
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4" aria-labelledby="about-title">
      <h2 id="about-title" className="font-medium">
        About
      </h2>
      <UpdateStatusRow
        status={status}
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
      <AutoCheckSwitch status={status} words={WORDS} onCheckedChange={(on) => run(updates.setAuto(on))} />
    </section>
  )
}
