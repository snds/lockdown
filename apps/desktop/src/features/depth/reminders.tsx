import { useCallback, useEffect, useState, type FormEvent } from "react"

import { AdaptiveDialog, Badge, Banner, Button, ConfirmDialog, FieldRow, FilterChip, FilterChipGroup, ListRow, SectionHeader } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useNeedsYouSource, type NeedsYouEntry } from "@/app/needs-you"
import { describeError } from "@/core/client"
import { KitDialog } from "@/features/account/kit-dialog"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"

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
export function RemindersPrompt({ onNotice, onChanged, hasAccount = true }: {
  onNotice: (text: string) => void
  onChanged: () => void
  /** An account to sign in to: the drill's optional password shows only then (a local vault has none). */
  hasAccount?: boolean
}) {
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
        hasAccount={hasAccount}
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

/** What the Recovery section reads about the drill: its checks and the reminders (the last drill's date, the password interval). */
export interface RecoveryFacts {
  checks: DrillCheck[] | null
  reminders: RemindersView | null
  /** A failed read of the checks, in the core's words. */
  error: string | null
  reload: () => void
  setReminders: (r: RemindersView) => void
}

/** Reads the drill's checks and the reminders once, and again on `reload` (after a drill, a new kit). */
export function useRecoveryFacts(): RecoveryFacts {
  const depth = useDepthClient()
  const [checks, setChecks] = useState<DrillCheck[] | null>(null)
  const [reminders, setReminders] = useState<RemindersView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const reload = useCallback(() => {
    if (!depth) return
    depth
      .drillChecks()
      .then((c) => {
        setChecks(c)
        setError(null)
      })
      .catch((e) => setError(describeError(e)))
    depth
      .reminders()
      .then(setReminders)
      .catch(() => undefined)
  }, [depth])
  useEffect(reload, [reload])
  return { checks, reminders, error, reload, setReminders }
}

/**
 * WHAT THE DRILL CHECKS NN (brief T3): one row per check the core lists
 * (the kit, the Secret Key kept here, each emergency contact's keys, the
 * crown jewels they would get), its words and a Badge OK or CHECK. A read
 * that failed after one that worked keeps the rows under a critical Banner
 * with TRY AGAIN; with nothing to show, the Banner stands alone.
 */
export function DrillSection({ facts }: { facts: RecoveryFacts }) {
  const level = useGroupLevel()
  const { checks, error, reload } = facts
  if (!checks && !error) return null
  return (
    <section aria-label="What the drill checks" className="flex flex-col">
      <SectionHeader level={level} title="What the drill checks" count={checks ? checks.length : undefined} />
      {error && (
        <Banner
          tone="critical"
          size="compact"
          title={error}
          action={
            <Button variant="secondary" size="sm" onClick={reload}>
              Try again
            </Button>
          }
        />
      )}
      {checks && (
        <ul role="list" aria-label="Drill checks" className="flex flex-col">
          {checks.map((c, i) => (
            <ListRow
              key={`${c.what}-${i}`}
              render={<li />}
              size="compact"
              title={c.words}
              badge={<Badge tone={c.ok ? "neutral" : "warning"}>{c.ok ? "OK" : "Check"}</Badge>}
            />
          ))}
        </ul>
      )}
    </section>
  )
}

/**
 * MASTER PASSWORD REMINDER (brief T3): how often Lockdown asks for the
 * master password, only where unlocking does not ask for it. Three short
 * choices, so chips in the row (cross-app decisions §5); picking one saves
 * it at once, silently, and a refusal keeps the old one with one critical
 * line under the row.
 */
export function PasswordReminderSetting({ facts }: { facts: RecoveryFacts }) {
  const depth = useDepthClient()
  const size = useRowSize()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const password = facts.reminders?.password
  if (!depth || !password?.applies) return null
  const pick = async (days: number) => {
    setBusy(true)
    setError(null)
    try {
      facts.setReminders(await depth.setPasswordReminderDays(days))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="flex flex-col">
      <FieldRow size={size} label="Master password reminder" description="Only here, where unlocking doesn't ask for it.">
        <FilterChipGroup
          label="Ask for the master password every"
          value={String(password.every_days)}
          onValueChange={(next) => next && next !== String(password.every_days) && !busy && void pick(Number(next))}
        >
          {PASSWORD_DAYS.map((d) => (
            <FilterChip key={d} value={String(d)}>
              {`${d} days`}
            </FilterChip>
          ))}
        </FilterChipGroup>
      </FieldRow>
      <SaveError text={error} />
    </div>
  )
}
