import { useState, type ReactNode } from "react"

import {
  Button,
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  Icon,
  IconButton,
  Input,
  NativeSelect,
  NativeSelectOption,
  Switch,
  Textarea,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import type { Control } from "@/core/types"

import { isSecretControl, type EditorRow } from "./editor-model"

interface FieldControlProps {
  row: EditorRow
  onChange: (patch: Partial<EditorRow>) => void
  /** Drops a field the person was adding, before it was ever saved. */
  onDiscard?: () => void
}

const INPUT_TYPE: Partial<Record<Control, string>> = {
  email: "email",
  phone: "tel",
  date: "date",
  month: "month",
}

/** Words for the generator, when a field has one: the template's, or the strong default for a secret of your own. */
function generatorWords(row: EditorRow): string | null {
  if (row.template?.generator) return row.template.generator.words
  if (!row.template && row.control === "secret" && row.editable) return "20 characters: letters, digits and symbols"
  return null
}

/**
 * One field in the editor, drawn from its template's control (D110). A
 * secret is unicron's SecretField in edit mode; on an item that already
 * holds it the field starts empty and is kept as it is unless the person
 * types a new one, generates one (the core makes it on save; this window
 * never sees it) or removes it. Every other control is a unicron
 * primitive with no overrides.
 */
export function FieldControl({ row, onChange, onDiscard }: FieldControlProps) {
  const id = `field-${row.key.replace(/[^a-z0-9]+/gi, "-")}`
  const required = row.template?.required ?? false
  const hint = row.template?.hint || undefined
  const words = generatorWords(row)
  const discard = onDiscard && <IconButton label={`Drop ${row.label}`} icon="close" onClick={onDiscard} />

  if (!row.editable) {
    return (
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>{row.label}</FieldLabel>
          <FieldDescription>{row.removed ? "Removed when you save." : "Made by a passkey provider; kept as it is."}</FieldDescription>
        </FieldContent>
        <Button type="button" variant="ghost" size="sm" onClick={() => onChange({ removed: !row.removed })}>
          {row.removed ? "Undo" : "Remove"}
        </Button>
      </Field>
    )
  }

  if (isSecretControl(row.control)) {
    if (row.secret === "generate" || row.secret === "remove") {
      return (
        <Field orientation="horizontal">
          <FieldContent>
            <FieldLabel>{row.label}</FieldLabel>
            <FieldDescription>
              {row.secret === "generate" ? `A new one is made when you save: ${words}. You can copy it from the item afterwards.` : "Removed when you save."}
            </FieldDescription>
          </FieldContent>
          <Button type="button" variant="ghost" size="sm" onClick={() => onChange({ secret: row.exists ? "keep" : "type", text: "" })}>
            Undo
          </Button>
        </Field>
      )
    }
    const kept = row.exists && row.text === ""
    const description = kept ? "Left empty, it stays as it is." : hint
    const actions = (
      <>
        {words && <IconButton label={`Generate ${row.label.toLowerCase()}`} tooltip={`Generate: ${words}`} icon="password" onClick={() => onChange({ secret: "generate", text: "" })} />}
        {row.exists && !required && <IconButton label={`Remove ${row.label.toLowerCase()}`} icon="delete" onClick={() => onChange({ secret: "remove", text: "" })} />}
        {discard}
      </>
    )
    if (row.control === "secret-multiline") return <MaskedText id={id} row={row} description={description} onChange={onChange} actions={actions} />
    return (
      <SecretField
        mode="edit"
        id={id}
        label={required && !row.exists ? `${row.label} (needed)` : row.label}
        value={row.text}
        onChange={(text) => onChange({ text, secret: "type" })}
        description={description}
      >
        {actions}
      </SecretField>
    )
  }

  if (row.control === "boolean") {
    return (
      <Field orientation="horizontal">
        <Switch id={id} checked={row.flag} onCheckedChange={(flag) => onChange({ flag })} />
        <FieldContent>
          <FieldLabel htmlFor={id}>{row.label}</FieldLabel>
          {hint && <FieldDescription>{hint}</FieldDescription>}
        </FieldContent>
        {discard}
      </Field>
    )
  }

  const label = discard ? (
    <div className="flex items-center justify-between gap-2">
      <FieldLabel htmlFor={id}>{row.label}</FieldLabel>
      {discard}
    </div>
  ) : (
    <FieldLabel htmlFor={id}>{row.label}</FieldLabel>
  )
  const removable = row.exists && !required ? "Left empty, it is taken off the item." : undefined
  return (
    <Field>
      {label}
      {row.control === "multiline" ? (
        <Textarea id={id} value={row.text} onChange={(e) => onChange({ text: e.target.value })} required={required} rows={row.fieldId === "body" ? 10 : 3} />
      ) : row.control === "select" ? (
        <NativeSelect id={id} value={row.text} onChange={(e) => onChange({ text: e.target.value })} required={required}>
          <NativeSelectOption value="">{required ? "Choose one" : "Not set"}</NativeSelectOption>
          {(row.template?.options ?? []).map((o) => (
            <NativeSelectOption key={o} value={o}>
              {o}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      ) : (
        <Input
          id={id}
          type={INPUT_TYPE[row.control] ?? "text"}
          inputMode={row.control === "number" ? "numeric" : row.control === "url" ? "url" : undefined}
          pattern={row.control === "number" ? "[0-9]*" : undefined}
          placeholder={row.control === "url" ? "https://" : undefined}
          value={row.text}
          onChange={(e) => onChange({ text: e.target.value })}
          required={required}
          autoComplete="off"
        />
      )}
      {(hint || removable) && <FieldDescription>{hint ?? removable}</FieldDescription>}
    </Field>
  )
}

/**
 * A secret over several lines (a private key, a recovery phrase): a
 * textarea drawn masked (`data-masked`, app-extension.css), because a
 * one-line password input would drop the line breaks a key needs.
 */
function MaskedText({
  id,
  row,
  description,
  onChange,
  actions,
}: {
  id: string
  row: EditorRow
  description?: string
  onChange: (patch: Partial<EditorRow>) => void
  actions: ReactNode
}) {
  const [shown, setShown] = useState(false)
  const required = row.template?.required ?? false
  return (
    <Field>
      <div className="flex items-center justify-between gap-2">
        <FieldLabel htmlFor={id}>{required && !row.exists ? `${row.label} (needed)` : row.label}</FieldLabel>
        <span className="flex items-center">
          <IconButton label={shown ? `Hide ${row.label}` : `Show ${row.label}`} icon={shown ? "visibility_off" : "visibility"} aria-pressed={shown} onClick={() => setShown((s) => !s)} />
          {actions}
        </span>
      </div>
      <Textarea
        id={id}
        data-masked={shown ? undefined : ""}
        value={row.text}
        onChange={(e) => onChange({ text: e.target.value, secret: "type" })}
        required={required && !row.exists}
        rows={4}
        spellCheck={false}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
      />
      {description && <FieldDescription>{description}</FieldDescription>}
      {row.template?.crown_jewel && (
        <FieldDescription>
          <Icon name="lock" data-icon="inline-start" />
          Kept as a crown jewel: showing, copying or changing it always asks it's you first.
        </FieldDescription>
      )}
    </Field>
  )
}
