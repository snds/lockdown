import { useState } from "react"

import { Field, FieldLabel, Icon, IconButton, NativeSelect, NativeSelectOption, TextField, ToggleRow, type SecretFieldTextAction } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import type { Control } from "@/core/types"

import { MATCH_RULES } from "@/features/organization/org-model"

import { isSecretControl, type EditorRow } from "./editor-model"
import { recipeOf } from "./depth-model"
import { GeneratorPicker, STRENGTH_WORDS, useGeneratorPreview } from "./generator-options"
import { LinkPicker } from "./link-picker"
import { useStrength } from "./use-strength"

interface FieldControlProps {
  row: EditorRow
  onChange: (patch: Partial<EditorRow>) => void
  /** Drops a field the person was adding, before it was ever saved. */
  onDiscard?: () => void
  /** The form's plain words (title, username, website), which make a typed password weaker when it holds them. */
  context?: readonly string[]
  /** Where a link may point: the item's vault, and the item itself (which it can't point at). */
  linkScope?: { vaultId: string; itemId?: string }
  /** For a login's password: its first website and its own rules, for the generator's preview (D236). */
  site?: { url?: string; rule?: string }
  /**
   * A web address's match rule: beside it (`inline`, the default), left for
   * More options (`none`), or that rule alone, for the first website whose
   * box is among the essentials (`only`).
   */
  rule?: "inline" | "none" | "only"
  /** Something on the field's own line at its end (Username's NEW ALIAS). */
  trailing?: React.ReactNode
  /** A line under the field in the critical ink: an action on it failed (NEW ALIAS), in the core's words. */
  error?: string | null
}

const NO_CONTEXT: readonly string[] = []

const INPUT_TYPE: Partial<Record<Control, "email" | "tel" | "date" | "month">> = {
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

const lower = (s: string) => s.toLowerCase()

/**
 * One field in the editor, drawn from its template's control (D110; brief
 * E2). A secret is unicron's SecretField in edit mode, in one of its
 * states: typed (masked, with Show, and for a password the core's strength
 * word on the 3-segment rule), kept (an edit: empty, "Left empty, it stays
 * as it is."), made when you save (the template's words and the core's
 * strength; OPTIONS opens the generator as a picker), or removed when you
 * save. The window never sees a generated value (D112). A secret over
 * several lines (a private key, a recovery phrase) is the same field with
 * `multiline`, masked until Show. Every other control is a unicron part
 * with no overrides. The empty-field rule is said once, by the editor.
 */
export function FieldControl({ row, onChange, onDiscard, context = NO_CONTEXT, linkScope, site, rule = "inline", trailing, error }: FieldControlProps) {
  const id = `field-${row.key.replace(/[^a-z0-9]+/gi, "-")}`
  const required = row.template?.required ?? false
  const hint = row.template?.hint || undefined
  const words = generatorWords(row)
  // The meter is for passwords, the one-line secrets the person picks (those the generator is offered
  // for), and only for what they type here; never a card number, a PIN or a stored value (D116).
  const strength = useStrength(row.text, context, row.editable && row.control === "secret" && row.secret === "type" && words !== null)
  const discard = onDiscard && <IconButton label={`Drop ${row.label}`} icon="close" size="icon-sm" onClick={onDiscard} />

  if (row.control === "url" && rule === "only") {
    if (row.matchRule === undefined) return null
    return (
      <Field>
        <FieldLabel htmlFor={`${id}-rule`}>Matches</FieldLabel>
        <NativeSelect id={`${id}-rule`} aria-label={`Where ${lower(row.label)} is offered`} value={row.matchRule} onChange={(e) => onChange({ matchRule: e.target.value })}>
          {MATCH_RULES.map(([value, said]) => (
            <NativeSelectOption key={value} value={value}>
              {said}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>
    )
  }

  if (row.managed || !row.editable) {
    // A passkey's material, or something only kept or removed: the SecretField's settled line.
    return (
      <SecretField
        mode="edit"
        id={id}
        label={row.label}
        value=""
        onChange={() => {}}
        stateText={row.managed ? "Made by a site through a passkey provider; kept as it is." : row.removed ? "Removed when you save." : "Made by a passkey provider; kept as it is."}
        description={row.managed ? "Delete it from the item's passkeys, which says first if it is your only way in." : undefined}
        actions={row.managed ? undefined : [{ label: row.removed ? "Keep" : "Remove", "aria-label": `${row.removed ? "Keep" : "Remove"} ${lower(row.label)}`, onClick: () => onChange({ removed: !row.removed }) }]}
      />
    )
  }

  if (isSecretControl(row.control)) return <SecretControl id={id} row={row} onChange={onChange} words={words} required={required} hint={hint} strength={strength} site={site} discard={discard} />

  if (row.control === "linked") {
    return (
      <div className="flex flex-col gap-1">
        <LinkPicker id={id} label={row.label} value={row.text} onChange={(text) => onChange({ text })} vaultId={linkScope?.vaultId ?? ""} except={linkScope?.itemId} required={required} />
        {discard}
      </div>
    )
  }

  if (row.control === "boolean") {
    return (
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <ToggleRow id={id} label={row.label} description={hint} checked={row.flag} onCheckedChange={(flag) => onChange({ flag })} rule="none" />
        </div>
        {discard}
      </div>
    )
  }

  if (row.control === "select") {
    return (
      <Field>
        <FieldLabel htmlFor={id}>{row.label}</FieldLabel>
        <NativeSelect id={id} value={row.text} onChange={(e) => onChange({ text: e.target.value })} required={required}>
          <NativeSelectOption value="">{required ? "Choose one" : "Not set"}</NativeSelectOption>
          {(row.template?.options ?? []).map((o) => (
            <NativeSelectOption key={o} value={o}>
              {o}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>
    )
  }

  const end = trailing || discard ? (
    <>
      {trailing}
      {discard}
    </>
  ) : undefined
  return (
    <div className="flex flex-col gap-3">
      {row.control === "multiline" ? (
        <TextField id={id} label={row.label} multiline minRows={row.fieldId === "body" ? 10 : 6} value={row.text} onChange={(e) => onChange({ text: e.target.value })} required={required} helper={hint} />
      ) : (
        <TextField
          id={id}
          label={row.label}
          // A website stays plain text (a bare host is fine); the keyboard is the URL one.
          type={INPUT_TYPE[row.control] ?? "text"}
          inputMode={row.control === "number" ? "numeric" : row.control === "url" ? "url" : undefined}
          pattern={row.control === "number" ? "[0-9]*" : undefined}
          placeholder={row.control === "url" ? "https://" : undefined}
          value={row.text}
          onChange={(e) => onChange({ text: e.target.value })}
          required={required}
          autoComplete="off"
          helper={hint}
          error={error ?? undefined}
          trailing={end}
        />
      )}
      {row.control === "url" && rule === "inline" && row.matchRule !== undefined && (
        <Field>
          <FieldLabel htmlFor={`${id}-rule`}>Matches</FieldLabel>
          <NativeSelect id={`${id}-rule`} aria-label={`Where ${lower(row.label)} is offered`} value={row.matchRule} onChange={(e) => onChange({ matchRule: e.target.value })}>
            {MATCH_RULES.map(([value, said]) => (
              <NativeSelectOption key={value} value={value}>
                {said}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
      )}
    </div>
  )
}

interface SecretControlProps {
  id: string
  row: EditorRow
  onChange: (patch: Partial<EditorRow>) => void
  words: string | null
  required: boolean
  hint?: string
  strength: ReturnType<typeof useStrength>
  site?: { url?: string; rule?: string }
  discard?: React.ReactNode
}

/** A secret in the editor: SecretField edit mode in its four states (brief E2's table). */
function SecretControl({ id, row, onChange, words, required, hint, strength, site, discard }: SecretControlProps) {
  const [picking, setPicking] = useState(false)
  const generating = row.secret === "generate"
  const removing = row.secret === "remove"
  const name = lower(row.label)
  const crownJewel = row.template?.crown_jewel ?? false
  const jewelLine = crownJewel ? (
    <>
      <Icon name="locked" size="sm" data-icon="inline-start" />
      Kept as a crown jewel: showing, copying or changing it always asks it's you first.
    </>
  ) : null

  const recipe = row.recipe ?? recipeOf(row.template?.generator)
  if (generating) {
    const actions: SecretFieldTextAction[] = [
      ...(words ? [{ label: "Options", "aria-label": `Options for ${name}`, onClick: () => setPicking(true) }] : []),
      row.exists
        ? { label: "Keep", "aria-label": `Keep ${name}`, onClick: () => onChange({ secret: "keep", text: "", recipe: undefined }) }
        : { label: "Type one instead", onClick: () => onChange({ secret: "type", text: "", recipe: undefined }) },
    ]
    return (
      <div className="relative">
        <SecretField mode="edit" id={id} label={row.label} value="" onChange={() => {}} stateText="Made when you save" actions={actions} description={<GeneratedLine recipe={recipe} custom={row.recipe !== undefined} site={site} words={words} />}>
          {discard}
        </SecretField>
        {words && (
          <GeneratorPicker
            id={`${id}-gen`}
            open={picking}
            onOpenChange={setPicking}
            label={row.label}
            // The popover stands by the field's box: a point at its foot, never focused, so focus goes back to OPTIONS' row.
            trigger={<span aria-hidden="true" className="absolute bottom-0 left-0 size-0" />}
            recipe={recipe}
            onChange={(next) => onChange({ recipe: next })}
            onUsualRule={row.recipe ? () => onChange({ recipe: undefined }) : undefined}
            url={site?.url}
            rule={site?.rule}
          />
        )}
      </div>
    )
  }

  if (removing) {
    return <SecretField mode="edit" id={id} label={row.label} value="" onChange={() => {}} stateText="Removed when you save." actions={[{ label: "Keep", "aria-label": `Keep ${name}`, onClick: () => onChange({ secret: "keep", text: "" }) }]} />
  }

  const kept = row.exists && row.text === ""
  const actions: SecretFieldTextAction[] = [
    ...(words ? [{ label: "Generate", "aria-label": `Generate ${name}`, onClick: () => onChange({ secret: "generate", text: "" }) }] : []),
    ...(row.exists && !required ? [{ label: "Remove", "aria-label": `Remove ${name}`, onClick: () => onChange({ secret: "remove", text: "" }) }] : []),
  ]
  const said = kept ? "Left empty, it stays as it is." : hint
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
      actions={actions.length ? actions : undefined}
      description={
        jewelLine ? (
          <>
            {said && <>{said} </>}
            {jewelLine}
          </>
        ) : (
          said
        )
      }
    >
      {discard}
    </SecretField>
  )
}

/** The helper under a secret made on save: what the options make and the core's strength word (`20 characters: … // STRONG`). */
function GeneratedLine({ recipe, custom, site, words }: { recipe: ReturnType<typeof recipeOf>; custom: boolean; site?: { url?: string; rule?: string }; words: string | null }) {
  const { preview } = useGeneratorPreview(recipe, site?.url, site?.rule)
  // Until the core's preview arrives (or if it can't say), the template's own words.
  if (!preview) return <>{custom || !words ? "Working it out…" : words}</>
  const rules = preview.site ? (preview.site.source === "saved" ? " This login's own rules apply." : ` ${preview.site.host}'s published rules apply.`) : ""
  return (
    <>
      {preview.words} // {STRENGTH_WORDS[preview.strength].toUpperCase()}
      {custom ? " // your options" : ""}
      {rules}
    </>
  )
}
