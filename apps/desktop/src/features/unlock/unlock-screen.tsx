import { useEffect, useState, type FormEvent } from "react"

import { Button, Field, FieldError, FieldLabel, Input, Spinner } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { LockReason, Probe, Status } from "@/core/types"
import type { ShownPage } from "@/features/status/once-page"

import { FirstRun } from "./first-run"
import { useFirstRunClient, type QuickUnlock } from "./first-run-client"
import { ForgotPassword } from "./forgot-password"
import { decodePage, useCapsLock } from "./master-password"
import { Migration } from "./migration"
import { WelcomeActions, WelcomeColumn, WelcomeGroup, WelcomeScreen } from "./welcome"

interface UnlockScreenProps {
  /** Why the vault is shown locked, when a timer did it. */
  reason?: LockReason | null
  onOpened: (status: Status) => void
}

const REASONS: Record<LockReason, string> = {
  requested: "Locked.",
  idle: "Locked after a while without you.",
  hidden: "Locked while the window was out of view.",
  sleep: "Locked while this computer slept.",
}

/**
 * The first screen and the lock screen (docs/plan/39-first-run.md §1.9,
 * §1.10). What it offers depends on what is on this computer (the core's
 * `probe`):
 *
 * - nothing at all → the first run;
 * - keys sealed under a master password → **Unlock** with the name, the
 *   lock reason and the password, and "Forgot your password?"; a first run
 *   that stopped after the account was made resumes at the kit;
 * - keys from before the master password → they open the vault one last
 *   time, then the migration asks for one (Q2: required);
 * - an identity this computer is not enrolled in → a plain explanation.
 *
 * Never a server word on this screen. Every error comes from the core in
 * plain words and is shown as is.
 */
export function UnlockScreen({ reason, onOpened }: UnlockScreenProps) {
  const client = useClient()
  const frc = useFirstRunClient()
  const [probe, setProbe] = useState<Probe | null>(null)
  const [probeError, setProbeError] = useState<string | null>(null)
  const [migrating, setMigrating] = useState<Status | null>(null)
  const [attachError, setAttachError] = useState<string | null>(null)
  const [resume, setResume] = useState<ShownPage | null>(null)

  useEffect(() => {
    let live = true
    client
      .probe()
      .then((p) => live && setProbe(p))
      .catch((e) => live && setProbeError(describeError(e)))
    return () => {
      live = false
    }
  }, [client])

  const enrolled = probe !== null && probe.identity_present && probe.device_key_present
  const legacy = enrolled && !probe.master_password
  // An install from before the master password opens once with its key,
  // then gets one before the list draws.
  useEffect(() => {
    if (!legacy || migrating || !frc) return
    client.unlock().then(setMigrating, (e) => setAttachError(describeError(e)))
  }, [legacy, migrating, frc, client])

  if (probeError) {
    return (
      <WelcomeColumn locked>
        <WelcomeScreen title="Lockdown can't start" sentence={<span role="alert">{probeError}</span>} />
      </WelcomeColumn>
    )
  }
  if (!probe) {
    return (
      <WelcomeColumn locked>
        <Spinner />
      </WelcomeColumn>
    )
  }
  if (resume) return <FirstRun onOpened={onOpened} start="kit" kit={resume} />
  if (!probe.identity_present && !probe.device_key_present) return <FirstRun onOpened={onOpened} />
  if (!enrolled) {
    return (
      <WelcomeColumn locked>
        <WelcomeScreen
          title="This device isn't enrolled yet"
          sentence={
            <>
              An ecosystem identity already exists at <code className="font-mono">{probe.root}</code>, but it doesn't list this device. Enrol it from
              another ecosystem app on this computer, or recover with the kit on a device with nothing on it. A second identity is never created next
              to the first.
            </>
          }
        />
      </WelcomeColumn>
    )
  }
  if (legacy) {
    if (!frc) return <AttachForm probe={probe} reason={reason} onOpened={onOpened} />
    if (migrating) return <Migration status={migrating} onDone={onOpened} />
    return (
      <WelcomeColumn locked>
        {attachError ? <WelcomeScreen title="Lockdown can't open your vault" sentence={<span role="alert">{attachError}</span>} /> : <Spinner />}
      </WelcomeColumn>
    )
  }
  return (
    <PasswordUnlock
      probe={probe}
      reason={reason}
      onOpened={onOpened}
      onResume={(page) => setResume(page)}
    />
  )
}

/** How many wrong tries before the kit link says so. */
const TRIES_BEFORE_HINT = 3

/** The lock screen proper: the name, the reason, the master password. */
function PasswordUnlock({
  probe,
  reason,
  onOpened,
  onResume,
}: {
  probe: Probe
  reason?: LockReason | null
  onOpened: (s: Status) => void
  onResume: (kit: ShownPage) => void
}) {
  const client = useClient()
  const frc = useFirstRunClient()
  const [password, setPassword] = useState("")
  const [shown, setShown] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [wrong, setWrong] = useState(0)
  const [forgot, setForgot] = useState(false)
  const [quick, setQuick] = useState<QuickUnlock | null>(null)
  const [caps, onKey] = useCapsLock()
  useEffect(() => {
    frc?.quickUnlock().then(setQuick, () => setQuick(null))
  }, [frc])
  if (!frc) return null
  if (forgot) return <ForgotPassword name={probe.name ?? null} onBack={() => setForgot(false)} onOpened={onOpened} />

  const unlock = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const status = await frc.unlockWithPassword(password)
      // A first run that stopped before the kit was confirmed saved makes a
      // new kit now, with the password in hand (§1.2), and shows it.
      if (probe.stage === "account_made") {
        const page = await client.kitCreate({ replace: true, password })
        setPassword("")
        onResume(decodePage(page))
        return
      }
      setPassword("")
      onOpened(status)
    } catch (err) {
      setWrong((n) => n + 1)
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const name = probe.name ?? "Lockdown"
  return (
    <WelcomeColumn locked>
      <WelcomeScreen mark="unlock" centered title={name} sentence={reason ? REASONS[reason] : "Your vault is locked."} />
      <form onSubmit={unlock} className="flex flex-col gap-(--c-welcome-gap)">
        <WelcomeGroup>
          <Field data-invalid={error ? true : undefined}>
            <FieldLabel htmlFor="unlock-password">Master password</FieldLabel>
            <div className="flex gap-2">
              <div className="flex-1">
                <Input
                  id="unlock-password"
                  type={shown ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={onKey}
                  onKeyUp={onKey}
                  autoComplete="current-password"
                  autoFocus
                  readOnly={busy}
                  aria-invalid={error ? true : undefined}
                />
              </div>
              <Button type="button" variant="outline" aria-pressed={shown} onClick={() => setShown((s) => !s)}>
                {shown ? "Hide" : "Show"}
              </Button>
            </div>
            {caps && <p className="text-sm text-muted-foreground">Caps Lock is on.</p>}
            {error && <FieldError role="alert">{error}</FieldError>}
          </Field>
        </WelcomeGroup>
        <WelcomeActions>
          <Button size="lg" type="submit" disabled={busy || password === ""}>
            {busy ? <Spinner data-icon="inline-start" /> : null}
            {busy ? "Unlocking…" : "Unlock"}
          </Button>
          {quick?.available && (
            <Button size="lg" type="button" variant="outline" disabled>
              {quick.kind === "windows_hello" ? "Unlock with Windows Hello" : "Unlock with Touch ID"}
            </Button>
          )}
        </WelcomeActions>
        <div className="text-center">
          <Button type="button" variant="link" size="inline" onClick={() => setForgot(true)}>
            {wrong >= TRIES_BEFORE_HINT ? "Still not it? Use your recovery kit" : "Forgot your password? Use your recovery kit"}
          </Button>
        </div>
      </form>
    </WelcomeColumn>
  )
}

/**
 * A host with no first-run client (none today on the desktop; kept for a
 * shell that hasn't one): the device's key opens the vault, as before D461.
 */
function AttachForm({ probe, reason, onOpened }: { probe: Probe; reason?: LockReason | null; onOpened: (s: Status) => void }) {
  const client = useClient()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const unlock = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onOpened(await client.unlock())
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <WelcomeColumn locked>
      <WelcomeScreen mark="unlock" centered title="Lockdown" sentence={`${reason ? REASONS[reason] : "Your vault is locked."} This device (${probe.device_name}) opens it with its own key.`} />
      <form onSubmit={unlock} className="flex flex-col gap-(--c-welcome-gap)">
        {error && <FieldError role="alert">{error}</FieldError>}
        <WelcomeActions>
          <Button size="lg" type="submit" disabled={busy} autoFocus>
            {busy ? <Spinner data-icon="inline-start" /> : null}
            Unlock
          </Button>
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}
