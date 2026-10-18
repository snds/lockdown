import { useEffect, useState } from "react"

import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel, Input, NativeSelect, NativeSelectOption, Switch } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { GeneratorPreview, Recipe } from "@/core/types"

const SEPARATORS: Array<[string, string]> = [
  ["-", "Hyphens"],
  [" ", "Spaces"],
  [".", "Full stops"],
  ["_", "Underscores"],
]

const STRENGTH_WORDS: Record<GeneratorPreview["strength"], string> = { weak: "Weak", fair: "Fair", strong: "Strong" }

interface GeneratorOptionsProps {
  id: string
  recipe: Recipe
  onChange: (recipe: Recipe) => void
  /** The login's first website and its own rules, so a site's published rules can be named. */
  url?: string
  rule?: string
}

/**
 * The generator's options in the editor (D230): characters (length,
 * digits, symbols, no look-alikes, characters to leave out) or a
 * passphrase (words from EFF's long list, the separator, capitals, a
 * digit). The core makes the value on save, as before (D112); what shows
 * here is the core's preview: the options in words, the bits they carry,
 * the strength word, and a site's own rules when those will apply instead
 * (D236). No value is ever made or shown here.
 */
export function GeneratorOptions({ id, recipe, onChange, url, rule }: GeneratorOptionsProps) {
  const client = useClient()
  const [preview, setPreview] = useState<GeneratorPreview | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    const timer = window.setTimeout(() => {
      client
        .generatorPreview(recipe, url || undefined, rule || undefined)
        .then((p) => {
          if (!live) return
          setPreview(p)
          setError(null)
        })
        .catch((e) => {
          if (!live) return
          setPreview(null)
          setError(describeError(e))
        })
    }, 150)
    return () => {
      live = false
      window.clearTimeout(timer)
    }
  }, [client, recipe, url, rule])

  const set = (p: Partial<Recipe>) => onChange({ ...recipe, ...p } as Recipe)
  const num = (text: string, fallback: number) => {
    const n = Number.parseInt(text, 10)
    return Number.isFinite(n) ? n : fallback
  }

  return (
    <FieldGroup aria-label="Generator options">
      <Field>
        <FieldLabel htmlFor={`${id}-style`}>Make</FieldLabel>
        <NativeSelect
          id={`${id}-style`}
          value={recipe.style}
          onChange={(e) =>
            onChange(
              e.target.value === "words"
                ? { style: "words", words: 6, separator: "-", capitalize: false, number: false }
                : { style: "characters", length: 20, digits: true, symbols: true, unambiguous: false, avoid: "" },
            )
          }
        >
          <NativeSelectOption value="characters">A password of random characters</NativeSelectOption>
          <NativeSelectOption value="words">A passphrase of random words</NativeSelectOption>
        </NativeSelect>
      </Field>
      {recipe.style === "characters" ? (
        <>
          <Field>
            <FieldLabel htmlFor={`${id}-length`}>Characters</FieldLabel>
            <Input id={`${id}-length`} type="number" min={8} max={128} value={recipe.length} onChange={(e) => set({ length: num(e.target.value, recipe.length) })} />
          </Field>
          <Field orientation="horizontal">
            <Switch id={`${id}-digits`} checked={recipe.digits} onCheckedChange={(digits) => set({ digits })} />
            <FieldLabel htmlFor={`${id}-digits`}>Digits</FieldLabel>
          </Field>
          <Field orientation="horizontal">
            <Switch id={`${id}-symbols`} checked={recipe.symbols} onCheckedChange={(symbols) => set({ symbols })} />
            <FieldLabel htmlFor={`${id}-symbols`}>Symbols</FieldLabel>
          </Field>
          <Field orientation="horizontal">
            <Switch id={`${id}-unambiguous`} checked={recipe.unambiguous ?? false} onCheckedChange={(unambiguous) => set({ unambiguous })} />
            <FieldContent>
              <FieldLabel htmlFor={`${id}-unambiguous`}>No look-alikes</FieldLabel>
              <FieldDescription>Leaves out 0 O 1 l I 5 S 2 Z 8 B, for typing on a TV.</FieldDescription>
            </FieldContent>
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-avoid`}>Leave out</FieldLabel>
            <Input id={`${id}-avoid`} value={recipe.avoid ?? ""} onChange={(e) => set({ avoid: e.target.value })} autoComplete="off" placeholder="Characters the site won't take" />
          </Field>
        </>
      ) : (
        <>
          <Field>
            <FieldLabel htmlFor={`${id}-words`}>Words</FieldLabel>
            <Input id={`${id}-words`} type="number" min={3} max={20} value={recipe.words} onChange={(e) => set({ words: num(e.target.value, recipe.words) })} />
          </Field>
          <Field>
            <FieldLabel htmlFor={`${id}-separator`}>Between words</FieldLabel>
            <NativeSelect id={`${id}-separator`} value={recipe.separator ?? "-"} onChange={(e) => set({ separator: e.target.value })}>
              {SEPARATORS.map(([v, words]) => (
                <NativeSelectOption key={v} value={v}>
                  {words}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <Field orientation="horizontal">
            <Switch id={`${id}-capitalize`} checked={recipe.capitalize ?? false} onCheckedChange={(capitalize) => set({ capitalize })} />
            <FieldLabel htmlFor={`${id}-capitalize`}>A capital on each word</FieldLabel>
          </Field>
          <Field orientation="horizontal">
            <Switch id={`${id}-number`} checked={recipe.number ?? false} onCheckedChange={(number) => set({ number })} />
            <FieldLabel htmlFor={`${id}-number`}>A digit on one word</FieldLabel>
          </Field>
        </>
      )}
      <p role="status" aria-live="polite" className="text-sm text-muted-foreground">
        {error ??
          (preview
            ? `${preview.words}: about ${Math.round(preview.bits)} bits, ${STRENGTH_WORDS[preview.strength].toLowerCase()}.${preview.site ? ` ${preview.site.source === "saved" ? "This login's own rules" : `${preview.site.host}'s published rules`} apply.` : ""}`
            : "Working out how strong that is…")}
      </p>
    </FieldGroup>
  )
}
