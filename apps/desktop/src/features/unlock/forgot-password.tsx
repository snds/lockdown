import { useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldError, FieldLabel, Input, Spinner } from "@unicron/ui"

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"

import { useFirstRunClient } from "./first-run-client"
import { EMPTY_VALUES, checkValues, useMasterStrength, type MasterPasswordValues } from "./master-password"
import { MasterPasswordFields } from "./master-password-fields"
import { WelcomeActions, WelcomeColumn, WelcomeGroup, WelcomeScreen } from "./welcome"

/**
 * "Forgot your password? Use your recovery kit" (docs/plan/39-first-run.md
 * §1.10): K1 takes the kit's two lines, K2 a new master password. The core
 * checks the kit when it saves; a kit that doesn't open this vault comes
 * back to K1 with its words.
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
  if (!frc) return null

  const save = async (e: FormEvent) => {
    e.preventDefault()
    const found = checkValues(values, false, strength)
    setErrors(found)
    if (Object.keys(found).length) return
    setBusy(true)
    setError(null)
    try {
      const status = await frc.resetWithKit({ secretKey: secretKey.trim(), recoveryCode: recoveryCode.trim(), password: values.password })
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

  if (step === "k1")
    return (
      <WelcomeColumn locked>
        <WelcomeScreen title="Use your recovery kit" sentence="Type the Secret Key and recovery code from your kit to choose a new master password." />
        <form
          onSubmit={(e) => {
            e.preventDefault()
            setError(null)
            setStep("k2")
          }}
          className="flex flex-col gap-(--c-welcome-gap)"
        >
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="k1-secret-key">Secret Key</FieldLabel>
              <Input id="k1-secret-key" value={secretKey} onChange={(e) => setSecretKey(e.target.value)} autoComplete="off" />
              <FieldDescription>It starts with A1-.</FieldDescription>
            </Field>
          </WelcomeGroup>
          <WelcomeGroup>
            <Field>
              <FieldLabel htmlFor="k1-recovery-code">Recovery code</FieldLabel>
              <Input id="k1-recovery-code" value={recoveryCode} onChange={(e) => setRecoveryCode(e.target.value)} autoComplete="off" />
            </Field>
          </WelcomeGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          <WelcomeActions>
            <Button size="lg" type="submit" disabled={!secretKey.trim() || !recoveryCode.trim()}>
              Check the kit
            </Button>
            <Button size="lg" type="button" variant="outline" onClick={onBack}>
              Back
            </Button>
          </WelcomeActions>
        </form>
      </WelcomeColumn>
    )

  return (
    <WelcomeColumn locked>
      <WelcomeScreen title="Choose a new master password" sentence="Your old password stops working once you save the new one." />
      <form onSubmit={save} className="flex flex-col gap-(--c-welcome-gap)" noValidate>
        <MasterPasswordFields idPrefix="k2" values={values} onChange={setValues} errors={errors} withName={false} readOnly={busy} strength={strength} />
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <WelcomeActions>
          <Button size="lg" type="submit" disabled={busy}>
            {busy ? <Spinner data-icon="inline-start" /> : null}
            {busy ? "Saving…" : "Save new password"}
          </Button>
          <Button size="lg" type="button" variant="outline" onClick={() => setStep("k1")} disabled={busy}>
            Back
          </Button>
        </WelcomeActions>
      </form>
    </WelcomeColumn>
  )
}
