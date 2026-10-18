import { useEffect, useState } from "react"

import { Button, Field, FieldDescription, FieldLabel, Icon, Switch } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"

import { useBridges, type ClockView } from "./bridges-client"

/** A check older than this runs again when the codes screen opens. */
const RECHECK_SECS = 3600

/**
 * The clock line on the codes screen (docs/plan/23-bridges.md §8): when
 * this computer's clock is off from the account's server, says by how
 * much and whether codes are corrected, with the way to the clock
 * settings. Nothing shows while the clocks agree or there is no account.
 */
export function ClockBanner() {
  const bridges = useBridges()
  const client = useClient()
  const [view, setView] = useState<ClockView | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  useEffect(() => {
    if (!bridges) return
    let live = true
    void (async () => {
      try {
        const known = await bridges.clockStatus()
        const now = Math.floor(Date.now() / 1000)
        const fresh = known && now - known.checked_at < RECHECK_SECS ? known : await bridges.clockCheck()
        if (live) setView(fresh)
      } catch (e) {
        // No account (nothing to compare with) is not news here.
        const words = describeError(e)
        if (live && !/no account/i.test(words)) setProblem(words)
      }
    })()
    return () => {
      live = false
    }
  }, [bridges])

  if (!bridges || (!view && !problem) || view?.verdict === "in_step") return null
  return (
    <section role="status" aria-label="Clock" className="flex flex-wrap items-center gap-2 rounded-md border p-3 text-sm">
      <span className={view?.verdict === "too_far" ? "text-destructive" : "text-attention-text"}>
        <Icon name="schedule" />
      </span>
      <span className="flex-1">{view?.words ?? `The clock check didn't finish: ${problem}`}</span>
      {view && (
        <Button
          size="sm"
          variant="outline"
          onClick={() =>
            void client.openClockSettings().then((opened) => {
              if (!opened) setProblem("Set the date and time in your system's settings.")
            })
          }
        >
          Clock settings
        </Button>
      )}
    </section>
  )
}

/** Settings → Codes: whether codes follow the server's time. */
export function ClockSetting() {
  const bridges = useBridges()
  const { settings, update } = useSettings()
  const [error, setError] = useState<string | null>(null)
  if (!bridges) return null
  return (
    <section aria-labelledby="clock-setting" className="flex flex-col gap-2">
      <h2 id="clock-setting" className="text-base font-semibold">
        One-time codes
      </h2>
      <Field orientation="horizontal">
        <FieldLabel htmlFor="clock-correct">Correct for a wrong clock</FieldLabel>
        <Switch
          id="clock-correct"
          checked={settings.code_clock_correction ?? true}
          onCheckedChange={(on) => void update({ code_clock_correction: on }).catch((e) => setError(describeError(e)))}
        />
      </Field>
      <FieldDescription>
        When this computer's clock is off by up to five minutes from your account's server, codes are worked out from the server's time so sites
        accept them. Further off than that, Lockdown asks you to set the clock instead.
      </FieldDescription>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </section>
  )
}
