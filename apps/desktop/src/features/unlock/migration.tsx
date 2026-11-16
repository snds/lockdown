import { useState, type FormEvent } from "react"

import { Banner, Button, ConfirmDialog } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError, useClient } from "@/core/client"
import type { OncePage, Status } from "@/core/types"
import type { ShownPage } from "@/features/status/once-page"

import { KitScreen } from "./first-run"
import { decodePage } from "./master-password"
import { useFirstRunClient } from "./first-run-client"
import { EMPTY_VALUES, checkValues, useMasterStrength, type MasterPasswordValues } from "./master-password"
import { MasterPasswordFields } from "./master-password-fields"
import { WelcomeActions, WelcomeColumn, WelcomeScreen, useFormFill } from "./welcome"

const M1_LEARN_MORE = [
  "Until now your vault opened with a key kept in a file on this computer, so anyone who copied that file could open it. Now that key is locked with your master password.",
  "Lockdown turns your master password into the key that opens your vault, here on this computer. The password itself is never stored and never sent anywhere.",
  "That's why nobody, Lockdown included, can tell you what it was or reset it. If you forget it, your recovery kit is the only way back in.",
  "Length matters most. A phrase of four or more ordinary words is easier to remember, and much harder to guess, than a short word with symbols.",
]

/**
 * Existing installs (docs/plan/39-first-run.md §1.6; brief F4; Q2:
 * required, with no "Not now"). The vault opened with the device's key one
 * last time; before the list draws, it gets a master password. Each screen
 * is the focus frame with no folio: this happens once, not as a flow.
 *
 * - M1, a "This device only" install: a name and a master password;
 *   Continue when M2 follows, Lock my vault when it doesn't (the yes on the
 *   step that locks names it, Lockdown 33).
 * - M2, when that install already has a kit: the kit's Secret Key and
 *   recovery code, then Lock my vault; or "I don't have it", a question
 *   whose yes is New kit.
 * - M3, an install with an account on a server: that password, checked
 *   there once; Continue for now when the server can't be reached.
 *
 * Working buttons keep their labels. Nothing in the vault changes; a new
 * kit's page is shown before the vault.
 */
export function Migration({ status, onDone }: { status: Status; onDone: (s: Status) => void }) {
  const client = useClient()
  const frc = useFirstRunClient()
  const server = status.account && !status.account.local ? status.account.server : null
  const host = server?.replace(/^https?:\/\//, "")
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
  const fill = useFormFill()
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
    if (hasKit) {
      setError(null)
      setScreen("m2")
    } else void run({ name: values.name.trim(), password: values.password })
  }
  // A form that could not save: a compact critical Banner over its buttons, in the core's words (brief 4.0).
  const failure = error ? <Banner tone="critical" size="compact" title={error} /> : null

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
          learnMore={{
            title: "Your master password",
            paragraphs: M1_LEARN_MORE.slice(0, 3),
          }}
        />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void run({ password: values.password })
          }}
          className={fill}
          aria-label="Unlock with your password from now on"
        >
          <SecretField
            mode="edit"
            id="m3-password"
            label="Master password"
            name="password"
            mono={false}
            value={values.password}
            onChange={(password) => setValues({ ...values, password })}
            disabled={busy}
            autoComplete="current-password"
            description={`The password you sign in to ${host} with.`}
          />
          {error ? (
            <Banner
              tone="critical"
              size="compact"
              title={
                offline ? `Lockdown needs to reach ${host} once to check your password. You can keep using your vault for now; it asks again next time.` : error
              }
            />
          ) : null}
          <WelcomeActions>
            <Button
              size="lg"
              block
              type="submit"
              loading={busy}
              disabled={values.password === ""}
              disabledReason={values.password === "" && !busy ? "Type your password first." : undefined}
            >
              Continue
            </Button>
            {offline && (
              <Button size="lg" block type="button" variant="secondary" onClick={() => void client.status().then(onDone)} disabled={busy}>
                Continue for now
              </Button>
            )}
          </WelcomeActions>
        </form>
      </WelcomeColumn>
    )

  if (screen === "m2") {
    const missing = !secretKey.trim() || !recoveryCode.trim()
    return (
      <WelcomeColumn>
        <WelcomeScreen
          title="Type your recovery kit"
          sentence="Your vault already has a recovery kit; its Secret Key also protects your master password, and the kit will open this vault if you forget it."
        />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            void run({
              name: values.name.trim(),
              password: values.password,
              secretKey: secretKey.trim(),
              recoveryCode: recoveryCode.trim(),
            })
          }}
          className={fill}
          aria-label="Type your recovery kit"
        >
          <div className="flex flex-col gap-6">
            <SecretField
              mode="edit"
              id="m2-secret-key"
              label="Secret Key"
              name="secret-key"
              mono
              value={secretKey}
              onChange={setSecretKey}
              disabled={busy}
              autoComplete="off"
              description="On your recovery kit page. It starts with A1-."
            />
            <SecretField
              mode="edit"
              id="m2-recovery-code"
              label="Recovery code"
              name="recovery-code"
              mono
              value={recoveryCode}
              onChange={setRecoveryCode}
              disabled={busy}
              autoComplete="off"
            />
          </div>
          {failure}
          <WelcomeActions>
            <Button
              size="lg"
              block
              type="submit"
              loading={busy}
              disabled={missing}
              disabledReason={missing && !busy ? "Type both lines from your kit first." : undefined}
            >
              Lock my vault
            </Button>
            <Button size="lg" block type="button" variant="secondary" onClick={() => setScreen("m1")} disabled={busy}>
              Back
            </Button>
            {/* A text button hangs its word on the column's edge, as the title does. */}
            <div className="-ms-4">
              <Button type="button" variant="ghost" onClick={() => setAskNewKit(true)} disabled={busy}>
                I don't have it
              </Button>
            </div>
          </WelcomeActions>
        </form>
        <ConfirmDialog
          open={askNewKit}
          onOpenChange={setAskNewKit}
          title="New recovery kit?"
          description="Your old kit page stops working."
          confirmLabel="New kit"
          onConfirm={() => {
            setAskNewKit(false)
            void run({
              name: values.name.trim(),
              password: values.password,
              newKit: true,
            })
          }}
        />
      </WelcomeColumn>
    )
  }

  return (
    <WelcomeColumn>
      <WelcomeScreen
        title="Lock your vault with a master password"
        sentence="From now on Lockdown asks for a master password to open your vault; everything in it stays as it is."
        learnMore={{ title: "Your master password", paragraphs: M1_LEARN_MORE }}
      />
      <form onSubmit={m1} className={fill} noValidate aria-label="Lock your vault with a master password">
        <MasterPasswordFields idPrefix="m1" values={values} onChange={setValues} errors={errors} withName readOnly={busy} strength={strength} />
        {failure}
        <WelcomeActions>
          <Button size="lg" block type="submit" loading={busy}>
            {hasKit ? "Continue" : "Lock my vault"}
          </Button>
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}
