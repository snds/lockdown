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

import { MATCH_RULES } from "@/features/organization/org-model"

import { isSecretControl, type EditorRow } from "./editor-model"
import { useStrength } from "./use-strength"

interface FieldControlProps {
  row: EditorRow
  onChange: (patch: Partial<EditorRow>) => void
  /** Drops a field the person was adding, before it was ever saved. */
  onDiscard?: () => void
  /** The form's plain words (title, username, website), which make a typed password weaker when it holds them. */
  context?: readonly string[]
}

const NO_CONTEXT: readonly string[] = []

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
 * never sees it) or removes it. A secret over several lines (a private
 * key, a recovery phrase) is the same field with `multiline` (@unicron/ui
 * 0.8.0), masked until Show; a required one carries native `required`, so
 * the form's own check stops an empty save. A password the person types
 * gets the strength word from the core's estimate (`useStrength`). Every
 * other control is a unicron primitive with no overrides.
 */
export function FieldControl({ row, onChange, onDiscard, context = NO_CONTEXT }: FieldControlProps) {
  const id = `field-${row.key.replace(/[^a-z0-9]+/gi, "-")}`
  const required = row.template?.required ?? false
  const hint = row.template?.hint || undefined
  const words = generatorWords(row)
  // The meter is for passwords, the one-line secrets the person picks (those the generator is offered
  // for), and only for what they type here; never a card number, a PIN or a stored value (D116).
  const strength = useStrength(row.text, context, row.editable && row.control === "secret" && row.secret === "type" && words !== null)
  const discard = onDiscard && <IconButton label={`Drop ${row.label}`} icon="close" onClick={onDiscard} />

  if (row.managed) {
    return (
      <Field orientation="horizontal">
        <FieldContent>
          <FieldLabel>{row.label}</FieldLabel>
          <FieldDescription>Made by a site through a passkey provider; kept as it is. Delete it from the item's passkeys, which says first if it is your only way in.</FieldDescription>
        </FieldContent>
      </Field>
    )
  }

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
    const crownJewel = row.template?.crown_jewel ?? false
    return (
      <SecretField
        mode="edit"
        id={id}
        name={row.fieldId ?? row.key}
        label={row.label}
        // Native and announced: the form won't submit without it, and the core refuses a missing one too.
        required={required && !row.exists}
        // A private key or a recovery phrase keeps its line breaks, masked until Show.
        multiline={row.control === "secret-multiline"}
        value={row.text}
        onChange={(text) => onChange({ text, secret: "type" })}
        strength={strength}
        description={
          crownJewel ? (
            <>
              {description && <>{description} </>}
              <Icon name="lock" data-icon="inline-start" />
              Kept as a crown jewel: showing, copying or changing it always asks it's you first.
            </>
          ) : (
            description
          )
        }
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
      {row.control === "url" && row.matchRule !== undefined && (
        <NativeSelect id={`${id}-rule`} aria-label={`Where ${row.label.toLowerCase()} is offered`} value={row.matchRule} onChange={(e) => onChange({ matchRule: e.target.value })}>
          {MATCH_RULES.map(([rule, words]) => (
            <NativeSelectOption key={rule} value={rule}>
              {words}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      )}
      {(hint || removable) && <FieldDescription>{hint ?? removable}</FieldDescription>}
    </Field>
  )
}
