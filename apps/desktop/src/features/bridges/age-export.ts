import type { ExportChoice } from "@/features/export/export-model"

/** The export locked to public keys (docs/plan/23-bridges.md §11): the Lockdown backup sealed with age. */
export const AGE_CHOICE: ExportChoice = {
  id: "lockdown-age",
  format: "lockdown",
  sealed: true,
  label: "A Lockdown backup, locked to a public key (age)",
  detail:
    "Everything a Lockdown backup holds, opened only with a private key you keep elsewhere: for a backup machine, a colleague or an offline key. Any age tool opens it too.",
}

/** The export choices with the age one second, where the host can make it. */
export function choicesWithAge(choices: readonly ExportChoice[], available: boolean): ExportChoice[] {
  if (!available) return [...choices]
  return [choices[0], AGE_CHOICE, ...choices.slice(1)].filter((c): c is ExportChoice => c !== undefined)
}
