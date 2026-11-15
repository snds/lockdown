import { useCallback, useEffect, useState, type FormEvent } from "react"

import { Banner, Button, ConfirmDialog, FieldRow, FieldRowValue, RadioGroup, RadioRow, TextField } from "@unicron/ui"
import { DigitGroups } from "@unicron/ui/security/digit-groups"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { needsEmailApproval, needsSecurityKey } from "@/features/depth/depth-client"
import { JoinWithDevice, SecondFactorWait, useShownWhen, type JoinStage } from "@/features/depth/join-device"
import { ServerPicker } from "@/features/server/server-picker"

import { foundWords, hostOf, useFirstRunClient, withCandidate, type Candidate } from "./first-run-client"
import { WelcomeActions, WelcomeColumn, WelcomeScreen, stepFolio, useFormFill } from "./welcome"

type Way = "device" | "signin" | "kit"

const WAYS: Array<{ way: Way; label: string; help?: string }> = [
  {
    way: "device",
    label: "Approve from another device",
    help: "Show a code on a device that's signed in, then match six digits.",
  },
  { way: "signin", label: "Sign in with your login and password" },
  {
    way: "kit",
    label: "Use your recovery kit",
    help: "For when you've lost every device.",
  },
]

/** The device way's three steps, for the folio: the code, the six digits, the password. */
const DEVICE_STEPS: Record<JoinStage, number> = {
  code: 1,
  compare: 2,
  finish: 3,
}

/** How long the login must stop changing before its domain is asked. */
const DOMAIN_PAUSE_MS = 600

/**
 * Screen J, "I already use Lockdown" (docs/plan/39-first-run.md §1.9;
 * brief F3): approve this computer from another device, sign in to a
 * server, or use the recovery kit, as three Radio Rows in the focus frame.
 * The servers discovery found are listed with the best one chosen; when it
 * found none, the device way asks for the server in its own field. Nothing
 * is sent to a server before the person confirms its key (a question whose
 * yes is Connect). The typed password becomes this computer's master
 * password (§1.3): the core seals the new device's keys under it in the
 * same step as the sign-in or recovery, so no plain key file is ever
 * written (docs/plan/45-desktop-join-sealed.md), then opens the vault with
 * it. The device way shows a folio of three steps; the other two are one
 * step each and show none.
 */
export function JoinScreen({ onBack, onOpened }: { onBack: () => void; onOpened: (s: Status) => void }) {
  const frc = useFirstRunClient()
  const [way, setWay] = useState<Way>("signin")
  const [candidates, setCandidates] = useState<Candidate[]>([])
  const [chosen, setChosen] = useState<string | null>(null)
  const [looking, setLooking] = useState(frc !== null)
  const [login, setLogin] = useState("")
  const [lookingAt, setLookingAt] = useState<string | null>(null)
  const [password, setPassword] = useState("")
  const [secretKey, setSecretKey] = useState("")
  const [recoveryCode, setRecoveryCode] = useState("")
  const [confirming, setConfirming] = useState<Candidate | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [waiting, setWaiting] = useState<{
    server: string
    email: string | null
  } | null>(null)
  const [stage, setStage] = useState<JoinStage>("code")
  const fill = useFormFill()
  const onStage = useCallback((s: JoinStage) => setStage(s), [])
  const refusal = useShownWhen(error)

  const merge = (list: Candidate[]) => {
    setCandidates((old) => list.reduce(withCandidate, old))
    setChosen((c) => c ?? list.find((x) => x.state === "ready")?.address ?? null)
  }
  const lookAgain = () => {
    if (!frc) return
    setLooking(true)
    frc
      .candidates(true)
      .then(merge, () => {})
      .finally(() => setLooking(false))
  }
  useEffect(() => {
    if (!frc) return
    frc
      .candidates(false)
      .then(merge, () => {})
      .then(() => frc.candidates(true))
      .then(
        (l) => l && merge(l),
        () => {},
      )
      .finally(() => setLooking(false))
  }, [frc])
  // An email-shaped login: its domain may name the server.
  useEffect(() => {
    const domain = login.trim().match(/^[^@\s]+@([^@\s]+\.[^@\s]+)$/)?.[1]
    if (!frc || !domain) return
    const timer = window.setTimeout(() => {
      setLookingAt(domain.toLowerCase())
      frc
        .domain(login.trim())
        .then(merge, () => {})
        .finally(() => setLookingAt(null))
    }, DOMAIN_PAUSE_MS)
    return () => window.clearTimeout(timer)
  }, [frc, login])

  const clearSecrets = () => {
    setPassword("")
    setSecretKey("")
    setRecoveryCode("")
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    const c = candidates.find((x) => x.address === chosen)
    if (!c) {
      setError("Choose a server, or type its address.")
      return
    }
    setConfirming(c)
  }

  const connect = async () => {
    const c = confirming
    if (!c || !frc) return
    setConfirming(null)
    setBusy(true)
    setError(null)
    try {
      // The key is pinned before anything is sent there.
      await frc.confirm(c.address)
      if (way === "signin")
        await frc.accountLoginSealed({
          server: c.address,
          login: login.trim(),
          password,
          secretKey: secretKey.trim() || undefined,
        })
      else
        await frc.recoverSealed({
          server: c.address,
          login: login.trim(),
          password,
          secretKey: secretKey.trim(),
          recoveryCode: recoveryCode.trim(),
        })
      const status = await frc.openJoined(password)
      clearSecrets()
      onOpened(status)
    } catch (err) {
      if (needsSecurityKey(err)) setWaiting({ server: c.address, email: null })
      else if (needsEmailApproval(err)) setWaiting({ server: c.address, email: describeError(err) })
      else setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (waiting)
    return (
      <WelcomeColumn>
        <SecondFactorWait
          server={waiting.server}
          email={waiting.email}
          onCancel={() => setWaiting(null)}
          onDone={() => {
            setWaiting(null)
            if (!frc) return
            void frc.openJoined(password).then(
              (status) => {
                clearSecrets()
                onOpened(status)
              },
              (e) => setError(describeError(e)),
            )
          }}
        />
      </WelcomeColumn>
    )

  // When discovery found nothing, the device way asks for the server in its own field (brief F3).
  const noneFound = candidates.length === 0 && !looking
  // Once a join from another device is under way, the screen holds only it: the digits, then the password.
  const joining = way === "device" && stage !== "code"
  const showPicker = !joining && !(way === "device" && noneFound)
  const folio = way === "device" ? stepFolio([DEVICE_STEPS[stage], 3]) : undefined

  return (
    <WelcomeColumn folio={folio}>
      <WelcomeScreen
        title="Add this computer to your Lockdown"
        sentence="Let a device you already use approve this one, or sign in to your server."
        learnMore={{
          title: "Using Lockdown on more than one device",
          paragraphs: [
            "Your devices stay in step through a Lockdown server. It stores only locked data it can't read, and Lockdown checks its work every time it syncs.",
            "Approving from another device is quickest: no kit to type.",
          ],
        }}
      />
      {!joining && (
        <RadioGroup aria-label="How to add this computer" value={way} onValueChange={(v) => setWay(v as Way)}>
          {WAYS.map((w) => (
            <RadioRow key={w.way} value={w.way} label={w.label} description={w.help} />
          ))}
        </RadioGroup>
      )}

      {showPicker && (
        <div className="flex flex-col gap-3">
          <ServerPicker
            name="join-server"
            candidates={candidates}
            chosen={chosen}
            onChoose={setChosen}
            looking={looking}
            onLookAgain={lookAgain}
            onCheck={async (address) => {
              if (!frc) return
              const c = await frc.check(address)
              merge([c])
              if (c.state === "ready") setChosen(c.address)
              else if (c.reason) throw new Error(c.reason)
            }}
          />
          {lookingAt && (
            <p role="status" className="type-caption text-text-secondary">
              Looking for a server at {lookingAt}…
            </p>
          )}
        </div>
      )}

      {way === "device" ? (
        <div className={fill}>
          <JoinWithDevice server={chosen ?? undefined} onOpened={onOpened} onStage={onStage} />
          {/* While the digits show, Cancel is the way out: it ends the join on both devices. */}
          {stage !== "compare" && (
            <WelcomeActions>
              <div className="-ms-4">
                <Button type="button" variant="ghost" onClick={onBack}>
                  Back
                </Button>
              </div>
            </WelcomeActions>
          )}
        </div>
      ) : (
        <form onSubmit={submit} className={fill} aria-label={way === "signin" ? "Sign in" : "Recover with your kit"} noValidate>
          <div className="flex flex-col gap-6">
            <TextField
              id="join-login"
              label="Login"
              value={login}
              onChange={(e) => setLogin(e.target.value)}
              autoComplete="username"
              required
              helper="Often an email address."
              readOnly={busy}
            />
            <SecretField
              mode="edit"
              id="join-password"
              label="Master password"
              name="password"
              mono={false}
              value={password}
              onChange={setPassword}
              autoComplete="current-password"
              required
              disabled={busy}
              description="The one you use on your other devices."
            />
            <SecretField
              mode="edit"
              id="join-secret-key"
              label="Secret Key"
              name="secret-key"
              mono
              value={secretKey}
              onChange={setSecretKey}
              autoComplete="off"
              required={way === "kit"}
              disabled={busy}
              description="On your recovery kit page. It starts with A1-."
            />
            {way === "kit" && (
              <SecretField
                mode="edit"
                id="join-recovery-code"
                label="Recovery code"
                name="recovery-code"
                mono
                value={recoveryCode}
                onChange={setRecoveryCode}
                autoComplete="off"
                required
                disabled={busy}
              />
            )}
          </div>
          {/* A form that could not go on: a compact critical Banner over its buttons, in the core's words (brief 4.0). */}
          {error ? (
            <div ref={refusal}>
              <Banner tone="critical" size="compact" title={error} />
            </div>
          ) : null}
          <WelcomeActions>
            <Button size="lg" block type="submit" loading={busy}>
              {way === "signin" ? "Sign in" : "Recover"}
            </Button>
            <div className="-ms-4">
              <Button type="button" variant="ghost" onClick={onBack} disabled={busy}>
                Back
              </Button>
            </div>
          </WelcomeActions>
        </form>
      )}

      <ConfirmDialog
        open={confirming !== null}
        onOpenChange={(o) => !o && setConfirming(null)}
        title={`Connect to ${confirming ? hostOf(confirming.address) : ""}?`}
        description="Only continue if you recognize this server. Lockdown checks this key every time it syncs."
        confirmLabel="Connect"
        onConfirm={() => {
          void connect()
        }}
      >
        {confirming && <ServerFacts candidate={confirming} />}
      </ConfirmDialog>
    </WelcomeColumn>
  )
}

/**
 * Address, where it was found, and the server key's six groups of five
 * digits (in `DigitGroups`, so they read group by group), as Field Rows
 * (brief T5's review). Also drawn by Settings → Server's confirm.
 */
export function ServerFacts({ candidate }: { candidate: Candidate }) {
  return (
    <div className="flex flex-col">
      <FieldRow size="stacked" label="Address">
        <FieldRowValue truncate="middle">{candidate.address}</FieldRowValue>
      </FieldRow>
      <FieldRow size="stacked" label="Found">
        <FieldRowValue truncate="wrap">{candidate.sources.map(foundWords).join(" · ")}</FieldRowValue>
      </FieldRow>
      {candidate.fingerprint && (
        <FieldRow size="stacked" label="Server key">
          <DigitGroups value={candidate.fingerprint} size="sm" label="Server key" />
        </FieldRow>
      )}
      {candidate.policy_key && (
        <FieldRow size="stacked" label="Policy">
          <FieldRowValue truncate="wrap">Matches the key your organization set.</FieldRowValue>
        </FieldRow>
      )}
    </div>
  )
}
