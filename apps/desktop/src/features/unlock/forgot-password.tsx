import { useState, type FormEvent } from "react"

import { Banner, Button } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"

import { useFirstRunClient } from "./first-run-client"
import { EMPTY_VALUES, checkValues, useMasterStrength, type MasterPasswordValues } from "./master-password"
import { MasterPasswordFields } from "./master-password-fields"
import { WelcomeActions, WelcomeColumn, WelcomeScreen, useFormFill } from "./welcome"

/**
 * "Forgot your password? Use your recovery kit" (docs/plan/39-first-run.md
 * §1.10; brief F4): K1 takes the kit's two lines, K2 a new master password,
 * each in the focus frame with no folio. The core checks the kit when it
 * saves; a kit that doesn't open this vault comes back to K1 with its
 * words, as a compact critical Banner. Save new password keeps its label
 * while it works.
 */
export function ForgotPassword({ name, onBack, onOpened }: { name: string | null; onBack: () => void; onOpened: (s: Status) => void }) {
  const frc = useFirstRunClient()
  const [step, setStep] = useState<"k1" | "k2">("k1")
  const [secretKey, setSecretKey] = useState("")
  const [recoveryCode, setRecoveryCode] = useState("")
  const [values, setValues] = useState<MasterPasswordValues>(EMPTY_VALUES)
  const [errors, setErrors] = useState<ReturnType<typeof checkValues>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const strength = useMasterStrength({ ...values, name: name ?? "" }, true)
  const fill = useFormFill()
  if (!frc) return null

  const save = async (e: FormEvent) => {
    e.preventDefault()
    const found = checkValues(values, false, strength)
    setErrors(found)
    if (Object.keys(found).length) return
    setBusy(true)
    setError(null)
    try {
      const status = await frc.resetWithKit({
        secretKey: secretKey.trim(),
        recoveryCode: recoveryCode.trim(),
        password: values.password,
      })
      setValues(EMPTY_VALUES)
      setSecretKey("")
      setRecoveryCode("")
      onOpened(status)
    } catch (err) {
      const words = describeError(err)
      if (words.startsWith("That kit")) {
        setValues(EMPTY_VALUES)
        setStep("k1")
        setError(words)
      } else if (words.includes("at least fair")) setErrors({ password: words })
      else setError(words)
    } finally {
      setBusy(false)
    }
  }

  // A kit that didn't open the vault, or a save that failed: a compact critical Banner over the buttons (brief 4.0).
  const failure = error ? <Banner tone="critical" size="compact" title={error} /> : null

  if (step === "k1") {
    const missing = !secretKey.trim() || !recoveryCode.trim()
    return (
      <WelcomeColumn>
        <WelcomeScreen title="Use your recovery kit" sentence="Type the Secret Key and recovery code from your kit to choose a new master password." />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            if (missing) return
            setError(null)
            setStep("k2")
          }}
          className={fill}
          aria-label="Use your recovery kit"
        >
          <div className="flex flex-col gap-6">
            <SecretField
              mode="edit"
              id="k1-secret-key"
              label="Secret Key"
              name="secret-key"
              mono
              value={secretKey}
              onChange={setSecretKey}
              autoComplete="off"
              description="It starts with A1-."
            />
            <SecretField
              mode="edit"
              id="k1-recovery-code"
              label="Recovery code"
              name="recovery-code"
              mono
              value={recoveryCode}
              onChange={setRecoveryCode}
              autoComplete="off"
            />
          </div>
          {failure}
          <WelcomeActions>
            <Button size="lg" block type="submit" disabled={missing} disabledReason={missing ? "Type both lines from your kit first." : undefined}>
              Check kit
            </Button>
            <Button size="lg" block type="button" variant="secondary" onClick={onBack}>
              Back
            </Button>
          </WelcomeActions>
        </form>
      </WelcomeColumn>
    )
  }

  return (
    <WelcomeColumn>
      <WelcomeScreen title="Choose a new master password" sentence="Your old password stops working once you save the new one." />
      <form onSubmit={save} className={fill} noValidate aria-label="Choose a new master password">
        <MasterPasswordFields idPrefix="k2" values={values} onChange={setValues} errors={errors} withName={false} readOnly={busy} strength={strength} />
        {failure}
        <WelcomeActions>
          <Button size="lg" block type="submit" loading={busy}>
            Save new password
          </Button>
          <Button size="lg" block type="button" variant="secondary" onClick={() => setStep("k1")} disabled={busy}>
            Back
          </Button>
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}
