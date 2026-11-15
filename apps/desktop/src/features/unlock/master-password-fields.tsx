import { Banner, TextField } from "@unicron/ui"
import { SecretField, type SecretStrength } from "@unicron/ui/security/secret-field"

import { useCapsLock, type MasterPasswordValues } from "./master-password"

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
 * Your name, the master password with its strength rule and Caps Lock line,
 * and the confirm field (brief F2; doc 39 §1.9 screen 1; reused by the
 * migration and a forgotten password).
 *
 * The password is a `SecretField` in edit mode: its own Show, and the
 * 3-segment strength rule with the core's word (`strength`, from the core's
 * estimate, never the field's). A field's error is the core's sentence, or
 * the screen's own check, under that field.
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
  const [caps, onKey] = useCapsLock()
  return (
    <div className="flex flex-col gap-6">
      {withName && (
        <TextField
          id={`${idPrefix}-name`}
          label="Your name"
          value={values.name}
          onChange={(e) => onChange({ ...values, name: e.target.value })}
          readOnly={readOnly}
          autoComplete="name"
          helper="Shown when Lockdown is locked. It stays on this computer."
          error={errors.name}
        />
      )}
      <div className="flex flex-col gap-3" onKeyDown={onKey} onKeyUp={onKey}>
        <SecretField
          mode="edit"
          id={`${idPrefix}-password`}
          label={passwordLabel}
          name="password"
          mono={false}
          value={values.password}
          onChange={(password) => onChange({ ...values, password })}
          disabled={readOnly}
          strength={values.password ? strength : undefined}
          description={passwordHelp}
          error={errors.password}
        />
        {caps ? <Banner tone="info" size="compact" title="Caps Lock is on." /> : null}
      </div>
      <SecretField
        mode="edit"
        id={`${idPrefix}-again`}
        label="Type it again"
        name="again"
        mono={false}
        value={values.again}
        onChange={(again) => onChange({ ...values, again })}
        disabled={readOnly}
        error={errors.again}
      />
    </div>
  )
}
