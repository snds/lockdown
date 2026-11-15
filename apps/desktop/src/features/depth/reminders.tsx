import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Badge,
  Button,
  ConfirmDialog,
  Field,
  FieldDescription,
  FieldLabel,
  Icon,
  Item,
  ItemContent,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useNeedsYouSource, type NeedsYouEntry } from "@/app/needs-you"
import { describeError } from "@/core/client"
import { dateLocal } from "@/lib/format"
import { KitDialog } from "@/features/account/kit-dialog"

import { PASSWORD_DAYS, useDepthClient, type DrillCheck, type RemindersView } from "./depth-client"

/** The reminders that are due now, in the X3 table's order (rows 7, 8, 9). */
export function dueReminders(r: RemindersView | null): Array<"kit-test" | "password" | "drill"> {
  if (!r) return []
  const due: Array<"kit-test" | "password" | "drill"> = []
  if (r.drill.kit_test_pending) due.push("kit-test")
  if (r.password.applies && r.password.due) due.push("password")
  if (r.drill.due && r.drill.kit !== "none") due.push("drill")
  return due
}

/** The skip question's words (D251, D383): its one yes is the sentence the person accepts. */
export const SKIP_WORDS = "I understand an untested kit can fail"

/**
 * The reminders around recovery (docs/plan/22-security-depth.md §7), read
 * once after unlock and shown as needs-you rows 7 to 9 (brief X3, Q4), not
 * as a dialog: the block on a desktop, the strip on a phone.
 *
 *   7  RECOVERY KIT // NOT TESTED   TEST IT NOW (the kit test), SKIP (a
 *      question whose one yes is "I understand an untested kit can fail",
 *      D251, D383; it asks again in a week)
 *   8  MASTER PASSWORD // 14 DAYS   TYPE IT (a one-field form, checked on
 *      this device), TOMORROW (D252)
 *   9  RECOVERY DRILL // DUE        RUN DRILL (the kit test), NEXT WEEK (D253)
 *
 * Each leaves when it is answered; the next due one then shows.
 */
export function RemindersPrompt({ onNotice, onChanged }: { onNotice: (text: string) => void; onChanged: () => void }) {
  const depth = useDepthClient()
  const [reminders, setReminders] = useState<RemindersView | null>(null)
  const [kit, setKit] = useState(false)
  const [skipping, setSkipping] = useState(false)
  const [typing, setTyping] = useState(false)
  const [password, setPassword] = useState("")
  // A failed answer, in the core's words, on the reminder it belongs to.
  const [error, setError] = useState<{ which: string; text: string } | null>(null)
  const [busy, setBusy] = useState<string | null>(null)

  useEffect(() => {
    if (!depth) return
    depth
      .reminders()
      .then(setReminders)
      .catch(() => setReminders(null))
  }, [depth])

  const act = async (which: string, f: () => Promise<RemindersView>): Promise<boolean> => {
    setBusy(which)
    setError(null)
    try {
      setReminders(await f())
      return true
    } catch (e) {
      setError({ which, text: describeError(e) })
      return false
    } finally {
      setBusy(null)
    }
  }

  const due = dueReminders(reminders)
  const lineOf = (which: string, line: string) => (error?.which === which ? error.text : line)
  const entry = (which: (typeof due)[number]): NeedsYouEntry | null => {
    if (!depth || !reminders) return null
    if (which === "kit-test")
      return {
        id: "reminder:kit-test",
        priority: 7,
        shows: "every",
        title: "Recovery kit // not tested",
        line: lineOf("kit-test", "Test your recovery kit."),
        action: { label: "Test it now", onClick: () => setKit(true) },
        textAction: { label: "Skip", onClick: () => setSkipping(true), busy: busy === "kit-test" },
      }
    if (which === "password")
      return {
        id: "reminder:password",
        priority: 8,
        shows: "every",
        title: `Master password // ${reminders.password.every_days} days`,
        line: lineOf("password", "Type your master password once more."),
        action: { label: "Type it", onClick: () => setTyping(true) },
        textAction: { label: "Tomorrow", onClick: () => void act("password", () => depth.passwordSnooze()), busy: busy === "password" },
      }
    return {
      id: "reminder:drill",
      priority: 9,
      shows: "every",
      title: "Recovery drill // due",
      line: lineOf("drill", "Time for the recovery drill."),
      action: { label: "Run drill", onClick: () => setKit(true) },
      textAction: { label: "Next week", onClick: () => void act("drill", () => depth.drillSnooze()), busy: busy === "drill" },
    }
  }
  useNeedsYouSource(due.includes("kit-test") ? entry("kit-test") : null)
  useNeedsYouSource(due.includes("password") ? entry("password") : null)
  useNeedsYouSource(due.includes("drill") ? entry("drill") : null)

  if (!depth) return null

  const checkPassword = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!password) return
    const typed = password
    setPassword("")
    if (await act("password-form", () => depth.passwordCheck(typed))) setTyping(false)
  }

  return (
    <>
      {/* SKIP: the question whose one yes is D251's sentence. Focus starts on Cancel. */}
      <ConfirmDialog
        open={skipping}
        onOpenChange={(o) => !o && setSkipping(false)}
        overline="Recovery kit"
        title="Skip the kit test?"
        description="Typing the kit back in takes a minute and proves the page you kept gets you back in. Skip it, and Lockdown asks again in a week."
        confirmLabel={SKIP_WORDS}
        error={error?.which === "kit-skip" ? error.text : undefined}
        onConfirm={async () => {
          if (!(await act("kit-skip", () => depth.kitTestSkip(true)))) throw new Error("not skipped")
          setSkipping(false)
        }}
        onCancel={() => setSkipping(false)}
      />
      {/* TYPE IT: a one-field form. The password is checked without leaving this device. */}
      <AdaptiveDialog
        open={typing}
        onOpenChange={(o) => {
          if (o) return
          setTyping(false)
          setPassword("")
          setError(null)
        }}
        size="form"
        overline="Master password"
        title="Your master password, once more"
        description="This device opens without it, so it's easy to forget. A new device will need it. Type it here to keep it fresh; it's checked without leaving this device."
        footer={
          <>
            <Button type="button" variant="secondary" onClick={() => setTyping(false)}>
              Cancel
            </Button>
            <Button type="submit" form="reminder-password-form" loading={busy === "password-form"}>
              Check
            </Button>
          </>
        }
      >
        <form id="reminder-password-form" onSubmit={checkPassword}>
          <SecretField
            mode="edit"
            id="reminder-password"
            label="Master password"
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            error={error?.which === "password-form" ? error.text : undefined}
          />
        </form>
      </AdaptiveDialog>
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
                <Icon name={c.ok ? "checkmark--outline" : "warning"} aria-label={c.ok ? "fine" : "needs you"} />
                {c.words}
              </ItemTitle>
            </ItemContent>
          </Item>
        ))}
      </ItemGroup>
      {reminders?.password.applies && (
        <Field>
          <FieldLabel htmlFor="password-reminder-days">Ask for the master password every</FieldLabel>
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
