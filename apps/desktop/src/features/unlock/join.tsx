import { useEffect, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Input,
  Spinner,
} from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { Status } from "@/core/types"
import { needsEmailApproval, needsSecurityKey } from "@/features/depth/depth-client"
import { JoinWithDevice, SecondFactorWait } from "@/features/depth/join-device"
import { ServerPicker } from "@/features/server/server-picker"

import { foundWords, hostOf, useFirstRunClient, withCandidate, type Candidate } from "./first-run-client"
import { WelcomeActions, WelcomeColumn, WelcomeGroup, WelcomeScreen } from "./welcome"

type Way = "device" | "signin" | "kit"

const WAYS: Array<{ way: Way; label: string; help?: string }> = [
  { way: "device", label: "Approve from another device", help: "Show a code on a device that's signed in, then match six digits." },
  { way: "signin", label: "Sign in with your login and password" },
  { way: "kit", label: "Use your recovery kit", help: "For when you've lost every device." },
]

/** How long the login must stop changing before its domain is asked. */
const DOMAIN_PAUSE_MS = 600

/**
 * Screen J, "I already use Lockdown" (docs/plan/39-first-run.md §1.9):
 * approve this computer from another device, sign in to a server, or use
 * the recovery kit. The servers discovery found are listed with the best
 * one chosen; nothing is sent to one before the person confirms its key.
 * After a sign-in the typed password becomes this computer's master
 * password (§1.3).
 */
export function JoinScreen({ onBack, onOpened }: { onBack: () => void; onOpened: (s: Status) => void }) {
  const client = useClient()
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
  const [waiting, setWaiting] = useState<{ server: string; email: string | null } | null>(null)

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
      .then((l) => l && merge(l), () => {})
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
      if (way === "signin") await client.accountLogin({ server: c.address, login: login.trim(), password, secretKey: secretKey.trim() || undefined })
      else await client.recover({ server: c.address, login: login.trim(), password, secretKey: secretKey.trim(), recoveryCode: recoveryCode.trim() })
      await client.openVault(true)
      await frc.setMasterPassword({ password })
      setPassword("")
      setSecretKey("")
      setRecoveryCode("")
      onOpened(await client.status())
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
            void client
              .openVault(true)
              .then(() => frc?.setMasterPassword({ password }))
              .then(() => client.status())
              .then(onOpened, (e) => setError(describeError(e)))
          }}
        />
      </WelcomeColumn>
    )

  return (
    <WelcomeColumn>
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
      <fieldset className="flex flex-col gap-2">
        <legend className="sr-only">How to add this computer</legend>
        {WAYS.map((w) => (
          <label key={w.way} htmlFor={`join-way-${w.way}`} data-row="" className="flex cursor-pointer items-start gap-3 rounded-lg border px-3 has-checked:border-ring has-checked:bg-surface-selected">
            <input id={`join-way-${w.way}`} type="radio" name="join-way" value={w.way} checked={way === w.way} onChange={() => setWay(w.way)} className="mt-1 size-4 accent-primary" />
            <span className="flex flex-col">
              <span className="font-medium">{w.label}</span>
              {w.help && <span className="text-sm text-muted-foreground">{w.help}</span>}
            </span>
          </label>
        ))}
      </fieldset>

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
      {lookingAt && <p className="text-sm text-muted-foreground">Looking for a server at {lookingAt}…</p>}

      {way === "device" ? (
        <JoinWithDevice server={chosen ?? undefined} onOpened={onOpened} />
      ) : (
        <form onSubmit={submit} className="flex flex-col gap-(--c-welcome-gap)">
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="join-login">Login</FieldLabel>
              <Input id="join-login" value={login} onChange={(e) => setLogin(e.target.value)} autoComplete="username" required />
              <FieldDescription>Often an email address.</FieldDescription>
            </Field>
          </WelcomeGroup>
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="join-password">Master password</FieldLabel>
              <Input id="join-password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="current-password" required />
              <FieldDescription>The one you use on your other devices.</FieldDescription>
            </Field>
          </WelcomeGroup>
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="join-secret-key">Secret Key</FieldLabel>
              <Input id="join-secret-key" type="password" value={secretKey} onChange={(e) => setSecretKey(e.target.value)} autoComplete="off" required={way === "kit"} />
              <FieldDescription>On your recovery kit page; it starts with A1-.</FieldDescription>
            </Field>
          </WelcomeGroup>
          {way === "kit" && (
            <WelcomeGroup>
              <Field>
                <FieldLabel htmlFor="join-recovery-code">Recovery code</FieldLabel>
                <Input id="join-recovery-code" type="password" value={recoveryCode} onChange={(e) => setRecoveryCode(e.target.value)} autoComplete="off" required />
              </Field>
            </WelcomeGroup>
          )}
          {error && <FieldError role="alert">{error}</FieldError>}
          <WelcomeActions>
            <Button size="lg" type="submit" disabled={busy}>
              {busy ? <Spinner data-icon="inline-start" /> : null}
              Continue
            </Button>
            <Button size="lg" type="button" variant="outline" onClick={onBack} disabled={busy}>
              Back
            </Button>
          </WelcomeActions>
        </form>
      )}
      {way === "device" && (
        <WelcomeActions>
          <Button size="lg" type="button" variant="outline" onClick={onBack}>
            Back
          </Button>
        </WelcomeActions>
      )}

      <Dialog open={confirming !== null} onOpenChange={(o) => !o && setConfirming(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Connect to {confirming ? hostOf(confirming.address) : ""}?</DialogTitle>
            <DialogDescription>Only continue if you recognize this server. Lockdown checks this key every time it syncs.</DialogDescription>
          </DialogHeader>
          {confirming && <ServerFacts candidate={confirming} />}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(null)}>
              Cancel
            </Button>
            <Button onClick={() => void connect()}>Connect</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </WelcomeColumn>
  )
}

/** Address, where it was found, and the server key's six groups of five digits. */
export function ServerFacts({ candidate }: { candidate: Candidate }) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
      <dt className="text-muted-foreground">Address</dt>
      <dd className="break-all">{candidate.address}</dd>
      <dt className="text-muted-foreground">Found</dt>
      <dd>{candidate.sources.map(foundWords).join(" · ")}</dd>
      <dt className="text-muted-foreground">Server key</dt>
      <dd className="font-mono">{candidate.fingerprint}</dd>
      {candidate.policy_key && (
        <>
          <dt className="sr-only">Policy</dt>
          <dd className="col-span-2">Matches the key your organization set.</dd>
        </>
      )}
    </dl>
  )
}
