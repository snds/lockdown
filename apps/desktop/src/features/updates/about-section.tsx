import { useState } from "react"

import { Button, Field, FieldLabel, Icon, Switch } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"

import { useUpdateClient } from "./update-client"
import { canCheck, channelName, updateLine } from "./update-model"
import { useUpdates } from "./use-updates"

/**
 * Settings → About (docs/plan/60-auto-update.md §6): the version and
 * channel, "Check for updates" (a check now, and the download if there is
 * one), one status line, "Restart to update" once an update is ready, and
 * the switch for the automatic check. Not shown where the host has no
 * updater (the web vault).
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
      <p className="text-sm">
        Lockdown {status.version} · {channelName(status)}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" disabled={!canCheck(status)} onClick={() => run(updates.check())}>
          <Icon name="refresh" data-icon="inline-start" />
          Check for updates
        </Button>
        {status.state === "ready" && (
          <Button size="sm" onClick={() => run(updates.restart())}>
            <Icon name="restart_alt" data-icon="inline-start" />
            Restart to update
          </Button>
        )}
        {status.state === "available" && status.link && (
          <Button size="sm" variant="outline" onClick={() => run(client.openSite(status.link!))}>
            <Icon name="open_in_new" data-icon="inline-start" />
            Get it from GitHub
          </Button>
        )}
      </div>
      <p role="status" className={status.state === "failed" ? "text-sm text-destructive" : "text-sm text-muted-foreground"}>
        {updateLine(status)}
      </p>
      {status.state === "ready" && <p className="text-sm text-muted-foreground">Restarting locks Lockdown. Unlock it with your master password as usual.</p>}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {status.enabled && (
        <Field orientation="horizontal">
          <FieldLabel htmlFor="update-auto">Check for updates automatically</FieldLabel>
          <Switch id="update-auto" checked={status.auto_check} onCheckedChange={(on) => run(updates.setAuto(on))} />
        </Field>
      )}
    </section>
  )
}
