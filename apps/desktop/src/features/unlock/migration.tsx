import { useState, type FormEvent } from "react"

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
import type { OncePage, Status } from "@/core/types"
import type { ShownPage } from "@/features/status/once-page"

import { KitScreen } from "./first-run"
import { decodePage } from "./master-password"
import { useFirstRunClient } from "./first-run-client"
import { EMPTY_VALUES, checkValues, useMasterStrength, type MasterPasswordValues } from "./master-password"
import { MasterPasswordFields } from "./master-password-fields"
import { WelcomeActions, WelcomeColumn, WelcomeGroup, WelcomeScreen } from "./welcome"

const M1_LEARN_MORE = [
  "Until now your vault opened with a key kept in a file on this computer, so anyone who copied that file could open it. Now that key is locked with your master password.",
  "Lockdown turns your master password into the key that opens your vault, here on this computer. The password itself is never stored and never sent anywhere.",
  "That's why nobody, Lockdown included, can tell you what it was or reset it. If you forget it, your recovery kit is the only way back in.",
  "Length matters most. A phrase of four or more ordinary words is easier to remember, and much harder to guess, than a short word with symbols.",
]

/**
 * Existing installs (docs/plan/39-first-run.md §1.6; Q2: required, with
 * no "Not now"). The vault opened with the device's key one last time;
 * before the list draws, it gets a master password.
 *
 * - M1, a "This device only" install: a name and a master password.
 * - M2, when that install already has a kit: the kit's Secret Key and
 *   recovery code, or "I don't have it: make a new kit".
 * - M3, an install with an account on a server: that password, checked
 *   there once.
 *
 * Nothing in the vault changes; a new kit's page is shown before the vault.
 */
export function Migration({ status, onDone }: { status: Status; onDone: (s: Status) => void }) {
  const client = useClient()
  const frc = useFirstRunClient()
  const server = status.account && !status.account.local ? status.account.server : null
  const hasKit = status.recovery !== "none"
  const [screen, setScreen] = useState<"m1" | "m2" | "m3" | "kit">(server ? "m3" : "m1")
  const [values, setValues] = useState<MasterPasswordValues>(EMPTY_VALUES)
  const [kit, setKit] = useState<ShownPage | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [errors, setErrors] = useState<ReturnType<typeof checkValues>>({})
  const [secretKey, setSecretKey] = useState("")
  const [recoveryCode, setRecoveryCode] = useState("")
  const [askNewKit, setAskNewKit] = useState(false)
  const [offline, setOffline] = useState(false)
  const strength = useMasterStrength(values, true)
  if (!frc) return null

  const finish = async (page: OncePage) => {
    setValues(EMPTY_VALUES)
    setSecretKey("")
    setRecoveryCode("")
    if (page.page.length) {
      setKit(decodePage(page))
      setScreen("kit")
    } else onDone(await client.status())
  }
  const run = async (input: Parameters<typeof frc.setMasterPassword>[0]) => {
    setBusy(true)
    setError(null)
    try {
      await finish(await frc.setMasterPassword(input))
    } catch (err) {
      const words = describeError(err)
      if (server && /reach|network|connect/i.test(words)) setOffline(true)
      setError(words)
    } finally {
      setBusy(false)
    }
  }
  const m1 = (e: FormEvent) => {
    e.preventDefault()
    const found = checkValues(values, true, strength)
    setErrors(found)
    if (Object.keys(found).length) return
    if (hasKit) setScreen("m2")
    else void run({ name: values.name.trim(), password: values.password })
  }

  if (screen === "kit")
    return (
      <KitScreen
        page={kit}
        onContinue={() => {
          setKit(null)
          void frc.stage("done").catch(() => {})
          void client.status().then(onDone)
        }}
      />
    )

  if (screen === "m3")
    return (
      <WelcomeColumn>
        <WelcomeScreen
          title="Unlock with your password from now on"
          sentence="Type your password once and Lockdown will use it to lock this vault on this computer; everything in it stays as it is."
          learnMore={{ title: "Your master password", paragraphs: M1_LEARN_MORE.slice(0, 3) }}
        />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void run({ password: values.password })
          }}
          className="flex flex-col gap-(--c-welcome-gap)"
        >
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="m3-password">Master password</FieldLabel>
              <Input id="m3-password" type="password" value={values.password} onChange={(e) => setValues({ ...values, password: e.target.value })} readOnly={busy} autoComplete="current-password" />
              <FieldDescription>The password you sign in to {server?.replace(/^https?:\/\//, "")} with.</FieldDescription>
            </Field>
          </WelcomeGroup>
          {error && <FieldError role="alert">{offline ? `Lockdown needs to reach ${server?.replace(/^https?:\/\//, "")} once to check your password. You can keep using your vault for now; it asks again next time.` : error}</FieldError>}
          <WelcomeActions>
            <Button size="lg" type="submit" disabled={busy || values.password === ""}>
              {busy ? <Spinner data-icon="inline-start" /> : null}
              {busy ? "Locking your vault…" : "Continue"}
            </Button>
            {offline && (
              <Button size="lg" type="button" variant="outline" onClick={() => void client.status().then(onDone)}>
                Continue for now
              </Button>
            )}
          </WelcomeActions>
        </form>
      </WelcomeColumn>
    )

  if (screen === "m2")
    return (
      <WelcomeColumn>
        <WelcomeScreen
          title="Type your recovery kit"
          sentence="Your vault already has a recovery kit; its Secret Key also protects your master password, and the kit will open this vault if you forget it."
        />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void run({ name: values.name.trim(), password: values.password, secretKey: secretKey.trim(), recoveryCode: recoveryCode.trim() })
          }}
          className="flex flex-col gap-(--c-welcome-gap)"
        >
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="m2-secret-key">Secret Key</FieldLabel>
              <Input id="m2-secret-key" value={secretKey} onChange={(e) => setSecretKey(e.target.value)} readOnly={busy} autoComplete="off" />
              <FieldDescription>On your recovery kit page; it starts with A1-.</FieldDescription>
            </Field>
          </WelcomeGroup>
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="m2-recovery-code">Recovery code</FieldLabel>
              <Input id="m2-recovery-code" value={recoveryCode} onChange={(e) => setRecoveryCode(e.target.value)} readOnly={busy} autoComplete="off" />
            </Field>
          </WelcomeGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <WelcomeActions>
            <Button size="lg" type="submit" disabled={busy || !secretKey.trim() || !recoveryCode.trim()}>
              {busy ? <Spinner data-icon="inline-start" /> : null}
              {busy ? "Locking your vault…" : "Continue"}
            </Button>
            <Button size="lg" type="button" variant="outline" onClick={() => setScreen("m1")} disabled={busy}>
              Back
            </Button>
          </WelcomeActions>
          <div className="text-center">
            <Button type="button" variant="link" size="inline" onClick={() => setAskNewKit(true)} disabled={busy}>
              I don't have it: make a new kit
            </Button>
          </div>
        </form>
        <Dialog open={askNewKit} onOpenChange={setAskNewKit}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Make a new kit?</DialogTitle>
              <DialogDescription>Your old kit page stops working.</DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="outline" onClick={() => setAskNewKit(false)}>
                Cancel
              </Button>
              <Button
                onClick={() => {
                  setAskNewKit(false)
                  void run({ name: values.name.trim(), password: values.password, newKit: true })
                }}
              >
                Make a new kit
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </WelcomeColumn>
    )

  return (
    <WelcomeColumn>
      <WelcomeScreen
        title="Lock your vault with a master password"
        sentence="From now on Lockdown asks for a master password to open your vault; everything in it stays as it is."
        learnMore={{ title: "Your master password", paragraphs: M1_LEARN_MORE }}
      />
      <form onSubmit={m1} className="flex flex-col gap-(--c-welcome-gap)" noValidate>
        <MasterPasswordFields idPrefix="m1" values={values} onChange={setValues} errors={errors} withName readOnly={busy} strength={strength} />
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <WelcomeActions>
          <Button size="lg" type="submit" disabled={busy}>
            {busy ? <Spinner data-icon="inline-start" /> : null}
            {busy ? "Locking your vault…" : "Continue"}
          </Button>
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}
