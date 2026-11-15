import { useEffect, useState, type ReactElement } from "react"

import { AdaptivePopover, Badge, Button, Field, FieldLabel, NativeSelect, NativeSelectOption, TextField, ToggleRow } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { GeneratorPreview, Recipe } from "@/core/types"

const SEPARATORS: Array<[string, string]> = [
  ["-", "Hyphens"],
  [" ", "Spaces"],
  [".", "Periods"],
  ["_", "Underscores"],
]

/** The core's strength word, as the Badge and the helper line write it. */
export const STRENGTH_WORDS: Record<GeneratorPreview["strength"], string> = { weak: "Weak", fair: "Fair", strong: "Strong" }
const STRENGTH_TONE = { weak: "critical", fair: "warning", strong: "success" } as const

/**
 * What a recipe makes, in the core's words (D230, D236), asked after a
 * short pause so a stepper held down asks once. Never a value: the core
 * makes that on save (D112). `null` while it works it out.
 */
export function useGeneratorPreview(recipe: Recipe, url?: string, rule?: string): { preview: GeneratorPreview | null; error: string | null } {
  const client = useClient()
  const [preview, setPreview] = useState<GeneratorPreview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const key = JSON.stringify(recipe)
  useEffect(() => {
    let live = true
    const timer = window.setTimeout(() => {
      client
        .generatorPreview(JSON.parse(key) as Recipe, url || undefined, rule || undefined)
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
  }, [client, key, url, rule])
  return { preview, error }
}

/** A site's own rules, when they apply instead of the options. */
function siteWords(p: GeneratorPreview): string | null {
  if (!p.site) return null
  return p.site.source === "saved" ? "This login's own rules apply." : `${p.site.host}'s published rules apply.`
}

interface GeneratorOptionsProps {
  id: string
  recipe: Recipe
  onChange: (recipe: Recipe) => void
  /** The login's first website and its own rules, so a site's published rules can be named. */
  url?: string
  rule?: string
  /** Goes back to the template's own rule (USUAL RULE). */
  onUsualRule?: () => void
}

/**
 * The generator's options (brief E3, D230): characters (length, digits,
 * symbols, no look-alikes, characters to leave out) or a passphrase (words
 * from EFF's long list, the separator, capitals, a digit). Each change
 * applies as it is made. Under them, the core's preview: the options in
 * words, the bits they carry and the strength as a Badge, and a site's own
 * rules when those will apply instead (D236). No value is ever made or
 * shown here.
 */
export function GeneratorOptions({ id, recipe, onChange, url, rule, onUsualRule }: GeneratorOptionsProps) {
  const { preview, error } = useGeneratorPreview(recipe, url, rule)
  const set = (p: Partial<Recipe>) => onChange({ ...recipe, ...p } as Recipe)

  return (
    <div role="group" aria-label="Generator options" className="flex flex-col gap-4">
      {/* Characters or words. A select, so the web vault's end-to-end steps keep their one control (doc 89 §2). */}
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
          <NativeSelectOption value="characters">Characters</NativeSelectOption>
          <NativeSelectOption value="words">Words</NativeSelectOption>
        </NativeSelect>
      </Field>
      {recipe.style === "characters" ? (
        <>
          <TextField id={`${id}-length`} label="Length" type="number" min={8} max={128} value={recipe.length} onValueChange={(n) => n !== null && set({ length: n })} decrementLabel="Fewer characters" incrementLabel="More characters" />
          <div>
            <ToggleRow id={`${id}-digits`} label="Digits" checked={recipe.digits} onCheckedChange={(digits) => set({ digits })} />
            <ToggleRow id={`${id}-symbols`} label="Symbols" checked={recipe.symbols} onCheckedChange={(symbols) => set({ symbols })} />
            <ToggleRow
              id={`${id}-unambiguous`}
              label="No look-alikes"
              description="Leaves out 0 O 1 l I 5 S 2 Z 8 B, for typing on a TV."
              checked={recipe.unambiguous ?? false}
              onCheckedChange={(unambiguous) => set({ unambiguous })}
              rule="both"
            />
          </div>
          <TextField id={`${id}-avoid`} label="Leave out" value={recipe.avoid ?? ""} onChange={(e) => set({ avoid: e.target.value })} autoComplete="off" placeholder="Characters the site won't take" />
        </>
      ) : (
        <>
          <TextField id={`${id}-words`} label="Words" type="number" min={3} max={20} value={recipe.words} onValueChange={(n) => n !== null && set({ words: n })} decrementLabel="Fewer words" incrementLabel="More words" />
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
          <div>
            <ToggleRow id={`${id}-capitalize`} label="A capital on each word" checked={recipe.capitalize ?? false} onCheckedChange={(capitalize) => set({ capitalize })} />
            <ToggleRow id={`${id}-number`} label="A digit on one word" checked={recipe.number ?? false} onCheckedChange={(number) => set({ number })} rule="both" />
          </div>
        </>
      )}
      <div className="flex items-baseline gap-3">
        <p role="status" aria-live="polite" className="type-caption text-text-secondary min-w-0 flex-1">
          {error ?? (preview ? `${preview.words} // about ${Math.round(preview.bits)} bits${siteWords(preview) ? `. ${siteWords(preview)}` : ""}` : "Working it out…")}
          {/* The word once, as the Badge; said with the line. */}
          {preview && !error && (
            <>
              {" "}
              <Badge tone={STRENGTH_TONE[preview.strength]}>{STRENGTH_WORDS[preview.strength]}</Badge>
            </>
          )}
        </p>
      </div>
      {onUsualRule && (
        <div>
          <Button type="button" variant="ghost" size="sm" onClick={onUsualRule}>
            Usual rule
          </Button>
        </div>
      )}
    </div>
  )
}

interface GeneratorPickerProps extends GeneratorOptionsProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** What the popover stands by: the secret's own row. */
  trigger: ReactElement<Record<string, unknown>>
  /** The secret's name: "Password options". */
  label: string
}

/**
 * The generator options as a picker (brief E3, 3.5): a popover with a
 * PopoverHeader on a desktop, a sheet on a phone. No DONE: each change
 * applies as made, and Escape, a press outside or the sheet's close button
 * closes it, so the editor's own primary stays the only solid button.
 */
export function GeneratorPicker({ open, onOpenChange, trigger, label, ...options }: GeneratorPickerProps) {
  return (
    <AdaptivePopover open={open} onOpenChange={onOpenChange} trigger={trigger} overline="Generator" title={`${label} options`} align="start" side="bottom">
      <div className="w-90 max-w-full">
        <GeneratorOptions {...options} />
      </div>
    </AdaptivePopover>
  )
}
