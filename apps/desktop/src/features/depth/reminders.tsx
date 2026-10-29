import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  Badge,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemContent,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"
import { dateLocal } from "@/lib/format"
import { KitDialog } from "@/features/account/kit-dialog"

import { PASSWORD_DAYS, promptFor, useDepthClient, type DrillCheck, type RemindersView } from "./depth-client"

/**
 * The reminders around recovery, asked once each time they come due
 * (docs/plan/22-security-depth.md §7): the first run's kit test
 * (skipped only by pressing "I understand an untested kit can fail", D251),
 * the account password
 * every 14 days where unlocking never asks for it, and the yearly drill.
 */
export function RemindersPrompt({ onNotice, onChanged }: { onNotice: (text: string) => void; onChanged: () => void }) {
  const depth = useDepthClient()
  const [reminders, setReminders] = useState<RemindersView | null>(null)
  const [kit, setKit] = useState(false)
  const [dismissed, setDismissed] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!depth) return
    depth
      .reminders()
      .then(setReminders)
      .catch(() => setReminders(null))
  }, [depth])

  if (!depth) return null
  const which = dismissed ? null : promptFor(reminders)

  const act = async (f: () => Promise<RemindersView>) => {
    setBusy(true)
    setError(null)
    try {
      setReminders(await f())
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const checkPassword = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const password = String(new FormData(form).get("password") ?? "")
    await act(() => depth.passwordCheck(password))
    form.reset()
  }

  return (
    <>
      <Dialog open={which !== null && !kit} onOpenChange={(o) => !o && setDismissed(true)}>
        <DialogContent>
          {which === "password" ? (
            <form onSubmit={checkPassword} className="flex flex-col gap-4">
              <DialogHeader>
                <DialogTitle>Your account password, once more</DialogTitle>
                <DialogDescription>
                  This device opens without it, so it's easy to forget. A new device will need it. Type it here to keep it fresh; it's checked
                  without leaving this device.
                </DialogDescription>
              </DialogHeader>
              <Field>
                <FieldLabel htmlFor="reminder-password">Account password</FieldLabel>
                <Input id="reminder-password" name="password" type="password" required autoComplete="current-password" />
              </Field>
              {error && <FieldError role="alert">{error}</FieldError>}
              <LabelledActions>
                <LabelledAction kind="cancel" type="button" onClick={() => void act(() => depth.passwordSnooze())} disabled={busy}>
                  Tomorrow
                </LabelledAction>
                <LabelledAction kind="confirm" type="submit" disabled={busy}>
                  Check
                </LabelledAction>
              </LabelledActions>
            </form>
          ) : (
            <>
              <DialogHeader>
                <DialogTitle>{which === "kit-test" ? "Test your recovery kit" : "Time for the recovery drill"}</DialogTitle>
                <DialogDescription>
                  {which === "kit-test"
                    ? "Type the Secret Key and recovery code from the kit back in. It takes a minute and proves the page you kept gets you back in. Skip it, and Lockdown asks again in a week."
                    : "Once a year, type the kit back in, and check that your emergency contacts' keys still hold."}
                </DialogDescription>
              </DialogHeader>
              {error && <FieldError role="alert">{error}</FieldError>}
              <LabelledActions>
                <LabelledAction
                  kind="cancel"
                  onClick={() => void act(() => (which === "kit-test" ? depth.kitTestSkip(true) : depth.drillSnooze()))}
                  disabled={busy}
                >
                  {which === "kit-test" ? "I understand an untested kit can fail" : "Next week"}
                </LabelledAction>
                <LabelledAction kind="confirm" onClick={() => setKit(true)} disabled={busy}>
                  Test it now
                </LabelledAction>
              </LabelledActions>
            </>
          )}
        </DialogContent>
      </Dialog>
      <KitDialog
        kind={kit ? "kit-verify" : null}
        hasAccount
        onClose={() => setKit(false)}
        onDone={(lines) => {
          setKit(false)
          if (lines.length) onNotice(lines.join(" "))
          void depth.reminders().then(setReminders)
          onChanged()
        }}
      />
    </>
  )
}

/**
 * The drill's checks and the password reminder's interval, on the
 * Recovery screen: the kit, the Secret Key kept here, each emergency
 * contact's keys, and the crown jewels they would get.
 */
export function DrillSection() {
  const depth = useDepthClient()
  const [checks, setChecks] = useState<DrillCheck[] | null>(null)
  const [reminders, setReminders] = useState<RemindersView | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    if (!depth) return
    depth
      .drillChecks()
      .then(setChecks)
      .catch((e) => setError(describeError(e)))
    depth
      .reminders()
      .then(setReminders)
      .catch(() => undefined)
  }, [depth])
  useEffect(load, [load])

  if (!depth) return null
  const drilled = reminders?.drill.last_drilled_at
  return (
    <section aria-labelledby="drill-title" className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <h2 id="drill-title" className="text-sm font-medium">
          What the drill checks
        </h2>
        {drilled ? <Badge variant="secondary">last passed {dateLocal(drilled)}</Badge> : null}
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <ItemGroup role="list" aria-label="Drill checks">
        {checks?.map((c, i) => (
          <Item key={`${c.what}-${i}`} role="listitem" size="sm" variant="outline" data-row="">
            <ItemContent>
              <ItemTitle>
                <Icon name={c.ok ? "check_circle" : "error"} aria-label={c.ok ? "fine" : "needs you"} />
                {c.words}
              </ItemTitle>
            </ItemContent>
          </Item>
        ))}
      </ItemGroup>
      {reminders?.password.applies && (
        <Field>
          <FieldLabel htmlFor="password-reminder-days">Ask for the account password every</FieldLabel>
          <NativeSelect
            id="password-reminder-days"
            value={String(reminders.password.every_days)}
            onChange={(e) =>
              void depth
                .setPasswordReminderDays(Number(e.target.value))
                .then(setReminders)
                .catch((err) => setError(describeError(err)))
            }
          >
            {PASSWORD_DAYS.map((d) => (
              <NativeSelectOption key={d} value={String(d)}>
                {d} days
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <FieldDescription>Only here, where unlocking doesn't ask for it.</FieldDescription>
        </Field>
      )}
    </section>
  )
}
