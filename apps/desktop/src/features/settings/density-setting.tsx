import { FieldRow, FilterChip, FilterChipGroup } from "@unicron/ui"

import type { Density } from "@/core/types"

import { useRowSize } from "./setting-rows"

/** Density (brief T0, T6): two short choices, so chips in the row. Picking one saves it. */
export function DensitySetting({ value, onChange, busy }: { value: Density; onChange: (density: Density) => void; busy?: boolean }) {
  return (
    <FieldRow size={useRowSize()} label="Density" description="Compact fits more on the screen.">
      <FilterChipGroup label="Density" value={value} onValueChange={(next) => next && next !== value && !busy && onChange(next as Density)}>
        <FilterChip value="comfortable">Comfortable</FilterChip>
        <FilterChip value="compact">Compact</FilterChip>
      </FilterChipGroup>
    </FieldRow>
  )
}
