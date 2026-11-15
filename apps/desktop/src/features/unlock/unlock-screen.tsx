import { useEffect, useId, useRef, useState, type FormEvent } from "react"

import { Banner, Button, EmptyState, FloatButton, Skeleton, useAppFrame } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { LOCKED_WORDS } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { LockReason, Probe, Status } from "@/core/types"
import { isMac } from "@/features/palette/palette-model"
import type { ShownPage } from "@/features/status/once-page"

import { ClosedFrame, ClosedHeading, lockedLine } from "./closed-frame"
import { FirstRun } from "./first-run"
import { useFirstRunClient, type QuickUnlock } from "./first-run-client"
import { ForgotPassword } from "./forgot-password"
import { decodePage, useCapsLock } from "./master-password"
import { Migration } from "./migration"

interface UnlockScreenProps {
  /** Why the vault is shown locked, when a timer did it. */
  reason?: LockReason | null
  onOpened: (status: Status) => void
  /**
   * The gate locked the vault after three wrong passwords (brief X5): the
   * sentence says so instead of the usual one. Additive; the shell passes it
   * when the gate's lock was the reason.
   */
  triesLocked?: boolean
}

/** "This Mac" or "This PC", as Settings names the device (cross-app decisions §5). */
const thisDevice = () => (isMac() ? "This Mac" : "This PC")

/**
 * The first screen and the lock screen (docs/plan/39-first-run.md §1.9,
 * §1.10; brief F1; Mobile / 13, Desktop / 10). What it offers depends on
 * what is on this computer (the core's `probe`):
 *
 * - nothing at all → the first run;
 * - keys sealed under a master password → **Unlock**: the reason and the
 *   name in the overline, "Lockdown is locked.", the master password, and
 *   the way back in with the kit; a first run that stopped after the account
 *   was made resumes at the kit;
 * - keys from before the master password → they open the vault one last
 *   time, then the migration asks for one (Q2: required);
 * - keys but no identity (a join that stopped part way) → screen J again;
 * - an identity this computer is not enrolled in → a plain explanation.
 *
 * It sits in the focus frame alone: no shell, no wash behind it (Q15).
 * Never a server word on this screen. Every error comes from the core in
 * plain words and is shown as is.
 */
export function UnlockScreen({ reason, onOpened, triesLocked = false }: UnlockScreenProps) {
  const client = useClient()
  const frc = useFirstRunClient()
  const [probe, setProbe] = useState<Probe | null>(null)
  const [probeError, setProbeError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
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
  }, [client, attempt])

  const enrolled = probe !== null && probe.identity_present && probe.device_key_present
  const legacy = enrolled && !probe.master_password
  // An install from before the master password opens once with its key,
  // then gets one before the list draws.
  useEffect(() => {
    if (!legacy || migrating || attachError || !frc) return
    client.unlock().then(setMigrating, (e) => setAttachError(describeError(e)))
  }, [legacy, migrating, attachError, frc, client])

  if (probeError) {
    return (
      <CantOpen
        title="Lockdown can't start."
        words={probeError}
        onTryAgain={() => {
          setProbeError(null)
          setProbe(null)
          setAttempt((n) => n + 1)
        }}
      />
    )
  }
  if (!probe) return <Starting />
  if (resume) return <FirstRun onOpened={onOpened} start="kit" kit={resume} />
  if (!probe.identity_present && !probe.device_key_present) return <FirstRun onOpened={onOpened} />
  // Keys but no identity: a join that stopped after its keys were sealed
  // (a refused Secret Key or kit keeps them, D522). Joining again with the
  // same password carries on as the same device.
  if (!probe.identity_present && frc) return <FirstRun onOpened={onOpened} start="join" />
  if (!enrolled) {
    return (
      <ClosedFrame>
        <ClosedHeading title="This device isn't enrolled yet.">
          An ecosystem identity already exists at <code className="type-code-inline text-text-body">{probe.root}</code>, but it doesn't list this device. Enrol it from another
          ecosystem app on this computer, or recover with the kit on a device with nothing on it. A second identity is never created next to the first.
        </ClosedHeading>
      </ClosedFrame>
    )
  }
  if (legacy) {
    if (!frc) return <AttachForm reason={reason} onOpened={onOpened} />
    if (migrating) return <Migration status={migrating} onDone={onOpened} />
    if (attachError) return <CantOpen title="Lockdown can't open your vault." words={attachError} onTryAgain={() => setAttachError(null)} />
    return <Starting />
  }
  return <PasswordUnlock probe={probe} reason={reason} triesLocked={triesLocked} onOpened={onOpened} onResume={(page) => setResume(page)} />
}

/** The core has not answered yet: the lockup and one skeleton bar (brief F1, Starting). */
function Starting() {
  return (
    <ClosedFrame>
      <h1 className="sr-only">Lockdown is starting</h1>
      <Skeleton variant="row" aria-hidden />
    </ClosedFrame>
  )
}

/** Lockdown can't start or can't open the vault: the core's sentence, and Try again (cross-app decisions §9). */
function CantOpen({ title, words, onTryAgain }: { title: string; words: string; onTryAgain: () => void }) {
  return (
    <ClosedFrame>
      <EmptyState tone="critical" role="alert" overline={thisDevice()} title={<h1>{title}</h1>} body={words} actionLabel="Try again" onAction={onTryAgain} />
    </ClosedFrame>
  )
}

/** How many wrong tries before the kit link says so. */
const TRIES_BEFORE_HINT = 3

/** The lock screen proper: the reason and the name, the master password, Unlock. */
function PasswordUnlock({
  probe,
  reason,
  triesLocked,
  onOpened,
  onResume,
}: {
  probe: Probe
  reason?: LockReason | null
  triesLocked: boolean
  onOpened: (s: Status) => void
  onResume: (kit: ShownPage) => void
}) {
  const client = useClient()
  const frc = useFirstRunClient()
  const { phone } = useAppFrame()
  const formId = useId()
  const field = useRef<HTMLInputElement & HTMLTextAreaElement>(null)
  const [password, setPassword] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [wrong, setWrong] = useState(0)
  const [forgot, setForgot] = useState(false)
  const [quick, setQuick] = useState<QuickUnlock | null>(null)
  const [idleSecs, setIdleSecs] = useState<number | null>(null)
  const [caps, onKey] = useCapsLock()
  useEffect(() => {
    frc?.quickUnlock().then(setQuick, () => setQuick(null))
  }, [frc])
  // The idle timer that fired, when the core reports it ("LOCKED // IDLE 05:00"); no running time is made up.
  useEffect(() => {
    if (reason !== "idle") return
    let live = true
    client
      .lockState()
      .then((s) => live && setIdleSecs(s.idle_timeout_secs))
      .catch(() => {})
    return () => {
      live = false
    }
  }, [client, reason])
  useEffect(() => {
    if (!forgot) field.current?.focus()
  }, [forgot])
  if (!frc) return null
  if (forgot) return <ForgotPassword name={probe.name ?? null} onBack={() => setForgot(false)} onOpened={onOpened} />

  const unlock = async (e: FormEvent) => {
    e.preventDefault()
    if (busy) return
    if (password === "") {
      setError("Type your master password.")
      field.current?.focus()
      return
    }
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
      field.current?.focus()
    } finally {
      setBusy(false)
    }
  }
  // Touch ID or Windows Hello: offered only where this build has it. No command opens the vault with it yet
  // (it waits for the signed app, doc 39 R4), so it is never the primary: it shows disabled, with the core's reason.
  const quickName = quick?.kind === "windows_hello" ? "Unlock with Windows Hello" : "Unlock with Touch ID"
  const quickShown = quick !== null && quick.kind !== "none" && quick.available
  const sentence = triesLocked ? LOCKED_WORDS : `Your vault is encrypted on this ${isMac() ? "Mac" : "PC"}. Your master password opens it.`
  return (
    <ClosedFrame
      end={
        phone ? (
          <FloatButton kind="primary" size="l" icon="unlocked" label="Unlock" type="submit" form={formId} aria-busy={busy || undefined} disabled={busy} />
        ) : undefined
      }
    >
      <ClosedHeading overline={lockedLine(reason, idleSecs, probe.name)} title="Lockdown is locked.">
        {sentence}
      </ClosedHeading>
      <form id={formId} onSubmit={unlock} aria-label="Unlock" className="flex flex-col gap-6">
        <div className="flex flex-col gap-3" onKeyDown={onKey} onKeyUp={onKey}>
          <SecretField
            mode="edit"
            label="Master password"
            name="password"
            mono={false}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            inputRef={field}
            error={error ?? undefined}
          />
          {caps ? <Banner tone="info" size="compact" title="Caps Lock is on." /> : null}
        </div>
        {(!phone || quickShown) && (
          <div className="flex flex-wrap items-start gap-4">
            {!phone && (
              <Button type="submit" loading={busy}>
                Unlock
              </Button>
            )}
            {quickShown && (
              <Button type="button" variant="secondary" disabled disabledReason={quick.why ?? "Use your master password on this computer for now."}>
                {quickName}
              </Button>
            )}
          </div>
        )}
        {/* A text button hangs its word on the column's edge, as the field's label does. */}
        <div className="-ms-4">
          <Button type="button" variant="ghost" wrap onClick={() => setForgot(true)}>
            {wrong >= TRIES_BEFORE_HINT ? "Still not it? Use your recovery kit" : "Forgot your password? Use your recovery kit"}
          </Button>
        </div>
      </form>
    </ClosedFrame>
  )
}

/**
 * A host with no first-run client (none today on the desktop; kept for a
 * shell that hasn't one): the device's key opens the vault, as before D461.
 */
function AttachForm({ reason, onOpened }: { reason?: LockReason | null; onOpened: (s: Status) => void }) {
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
    <ClosedFrame>
      <ClosedHeading overline={lockedLine(reason, null, null)} title="Lockdown is locked.">
        This device opens it with its own key.
      </ClosedHeading>
      <form onSubmit={unlock} aria-label="Unlock" className="flex flex-col gap-3">
        <div>
          <Button type="submit" loading={busy} autoFocus>
            Unlock
          </Button>
        </div>
        {error && (
          <p role="alert" className="type-caption text-critical-text">
            {error}
          </p>
        )}
      </form>
    </ClosedFrame>
  )
}
