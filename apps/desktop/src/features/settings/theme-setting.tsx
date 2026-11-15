import { FieldRow, FilterChip, FilterChipGroup, Tooltip, TooltipContent, TooltipTrigger } from "@unicron/ui"

/** The ground the app is drawn on, in the words Settings uses: follow the device, or hold one. */
export type ThemeChoice = "system" | "stone" | "graphite"

/** Each choice, its word, and what its tip says: "light" and "dark" appear only there. */
const CHOICES: ReadonlyArray<readonly [ThemeChoice, string, string]> = [
  ["system", "System", "Follows your device's light or dark setting."],
  ["stone", "Stone", "Light"],
  ["graphite", "Graphite", "Dark"],
]

/**
 * Settings → Appearance → Theme (docs/plan/62-unicron-1-0.md §3): one row,
 * three chips, one of them on. Picking one changes the whole app at once
 * and says nothing more; there is nothing to save or to fail, because the
 * choice is kept on this device by whoever holds `value` (next-themes on
 * the desktop, apps/web/src/theme.ts in the web vault), never in the
 * vault's settings. Each chip carries its tip as its description too, so
 * the words "light" and "dark" also reach a screen reader.
 */
export function ThemeSetting({ value, onChange }: { value: ThemeChoice; onChange: (choice: ThemeChoice) => void }) {
  return (
    <FieldRow label="Theme">
      <FilterChipGroup label="Theme" value={value} onValueChange={(next) => onChange(next as ThemeChoice)}>
        {CHOICES.map(([choice, name, tip]) => (
          <Tooltip key={choice}>
            {/* The tip hangs on a plain box around the chip, not on the chip: as a tooltip's own
                trigger the chip takes the trigger's `data-slot`, and the group then no longer
                finds it for the arrow keys (a unicron gap, doc 62). Focus and the pointer still
                reach the box through the chip. */}
            <TooltipTrigger render={<span className="inline-flex" />}>
              <FilterChip value={choice} aria-description={tip}>
                {name}
              </FilterChip>
            </TooltipTrigger>
            <TooltipContent>{tip}</TooltipContent>
          </Tooltip>
        ))}
      </FilterChipGroup>
    </FieldRow>
  )
}
