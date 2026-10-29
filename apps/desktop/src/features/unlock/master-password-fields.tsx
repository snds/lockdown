import { useState } from "react"

import { Button, Field, FieldDescription, FieldError, FieldLabel, Input } from "@unicron/ui"
import type { SecretStrength } from "@unicron/ui/security/secret-field"


import { useCapsLock, type MasterPasswordValues } from "./master-password"
import { WelcomeGroup } from "./welcome"

const STRENGTH_WORDS: Record<SecretStrength, string> = { weak: "Weak", fair: "Fair", strong: "Strong" }
const FILLED: Record<SecretStrength, number> = { weak: 1, fair: 2, strong: 3 }
const TONE: Record<SecretStrength, string> = { weak: "bg-destructive", fair: "bg-attention", strong: "bg-secure" }

/** The strength meter: three segments and the word, from the core's estimate. */
export function StrengthMeter({ strength }: { strength: SecretStrength | undefined }) {
  if (!strength) return null
  return (
    <div className="flex items-center gap-3" data-strength={strength}>
      <div className="flex flex-1 gap-1" aria-hidden="true">
        {[1, 2, 3].map((n) => (
          <span key={n} className={`h-1.5 flex-1 rounded-full ${n <= FILLED[strength] ? TONE[strength] : "bg-muted"}`} />
        ))}
      </div>
      <span className="text-sm" role="status">
        Strength: {STRENGTH_WORDS[strength]}
      </span>
    </div>
  )
}

interface FieldsProps {
  idPrefix: string
  values: MasterPasswordValues
  onChange: (v: MasterPasswordValues) => void
  errors: Partial<Record<"name" | "password" | "again", string>>
  /** The name field (absent when only a password is asked). */
  withName: boolean
  readOnly: boolean
  /** The strength word, from `useMasterStrength`. */
  strength: SecretStrength | undefined
  passwordLabel?: string
  passwordHelp?: string
}

/**
 * Your name, the master password with its meter and Caps Lock line, and
 * the confirm field (doc 39 §1.9 screen 1; reused by the migration and a
 * forgotten password).
 */
export function MasterPasswordFields({
  idPrefix,
  values,
  onChange,
  errors,
  withName,
  readOnly,
  strength,
  passwordLabel = "Master password",
  passwordHelp = "A long phrase beats a clever word: four or more ordinary words that don't belong together are hard to guess.",
}: FieldsProps) {
  const [shown, setShown] = useState(false)
  const [caps, onKey] = useCapsLock()
  const set = (k: keyof MasterPasswordValues) => (e: React.ChangeEvent<HTMLInputElement>) => onChange({ ...values, [k]: e.target.value })
  return (
    <>
      {withName && (
        <WelcomeGroup>
          <Field data-invalid={errors.name ? true : undefined}>
            <FieldLabel htmlFor={`${idPrefix}-name`}>Your name</FieldLabel>
            <Input
              id={`${idPrefix}-name`}
              value={values.name}
              onChange={set("name")}
              readOnly={readOnly}
              autoComplete="name"
              aria-invalid={errors.name ? true : undefined}
            />
            <FieldDescription>Shown when Lockdown is locked. It stays on this computer.</FieldDescription>
            {errors.name && <FieldError role="alert">{errors.name}</FieldError>}
          </Field>
        </WelcomeGroup>
      )}
      <WelcomeGroup>
        <Field data-invalid={errors.password ? true : undefined}>
          <FieldLabel htmlFor={`${idPrefix}-password`}>{passwordLabel}</FieldLabel>
          <div className="flex gap-2">
            <div className="flex-1">
              <Input
                id={`${idPrefix}-password`}
                type={shown ? "text" : "password"}
                value={values.password}
                onChange={set("password")}
                onKeyDown={onKey}
                onKeyUp={onKey}
                readOnly={readOnly}
                autoComplete="new-password"
                aria-invalid={errors.password ? true : undefined}
              />
            </div>
            <Button type="button" variant="outline" aria-pressed={shown} onClick={() => setShown((s) => !s)}>
              {shown ? "Hide" : "Show"}
            </Button>
          </div>
          <StrengthMeter strength={strength} />
          {caps && <FieldDescription>Caps Lock is on.</FieldDescription>}
          <FieldDescription>{passwordHelp}</FieldDescription>
          {errors.password && <FieldError role="alert">{errors.password}</FieldError>}
        </Field>
      </WelcomeGroup>
      <WelcomeGroup>
        <Field data-invalid={errors.again ? true : undefined}>
          <FieldLabel htmlFor={`${idPrefix}-again`}>Type it again</FieldLabel>
          <Input
            id={`${idPrefix}-again`}
            type={shown ? "text" : "password"}
            value={values.again}
            onChange={set("again")}
            readOnly={readOnly}
            autoComplete="new-password"
            aria-invalid={errors.again ? true : undefined}
          />
          {errors.again && <FieldError role="alert">{errors.again}</FieldError>}
        </Field>
      </WelcomeGroup>
    </>
  )
}
