import { DEVTOOLS_STATES } from "../states/devtools"
import { EDGES_STATES } from "../states/edges"
import { EXTENSION_STATES } from "../states/extension"
import { SCREENS_STATES } from "../states/screens"
import { SERVER_STATES } from "../states/server"
import { SHELL_STATES } from "../states/shell"
import { VAULT_STATES } from "../states/vault"
import type { Frame } from "./prefs"
import type { Density, StateDef, StateMeta, Theme } from "./types"

/**
 * Every state, one file per owner so lanes that work at once never edit the
 * same file: the shell (foundation), then the screens, vault and edges lanes.
 * An id never changes: before and after pictures pair up by it.
 */
export const STATES: StateDef[] = [...SHELL_STATES, ...SCREENS_STATES, ...DEVTOOLS_STATES, ...SERVER_STATES, ...VAULT_STATES, ...EDGES_STATES, ...EXTENSION_STATES]

const byId = new Map(STATES.map((s) => [s.id, s]))
if (byId.size !== STATES.length) {
  const seen = new Set<string>()
  const twice = STATES.filter((s) => (seen.has(s.id) ? true : (seen.add(s.id), false))).map((s) => s.id)
  throw new Error(`state ids must be unique: ${twice.join(", ")}`)
}

export function stateById(id: string): StateDef | undefined {
  return byId.get(id)
}

export function statesMeta(): StateMeta[] {
  return STATES.map((s) => ({
    id: s.id,
    surface: s.surface,
    screen: s.screen,
    state: s.state,
    brief_row: s.brief_row,
    notes: s.notes,
    widths: s.widths,
    compact: s.compact ?? false,
    frame: s.frame ?? null,
    trusted: s.trusted ?? [],
    hold_clock: s.hold_clock ?? false,
    live_clock: s.live_clock ?? false,
    built: s.built ?? null,
  }))
}

/**
 * One image's id: the state's, then its width (or `natural`), theme and,
 * when compact, the density. The review build opens any of them:
 * `#/desktop-vault-filled--w390-dark` lands on that state, dark, at 390.
 * A fixed window (`pnpm shots`) adds its height: `--w390x844-dark`.
 */
export function imageId(state: string, width: number | "natural", theme: Theme, density: Density, height?: number): string {
  return `${state}--${width === "natural" ? "natural" : `w${width}${height ? `x${height}` : ""}`}-${theme}${density === "compact" ? "-compact" : ""}`
}

export interface Address {
  id: string | null
  theme: Theme | null
  frame: Frame | null
  density: Density | null
}

export function parseAddress(hash: string): Address {
  const raw = decodeURIComponent(hash.replace(/^#\/?/, ""))
  if (!raw) return { id: null, theme: null, frame: null, density: null }
  const [id, suffix] = raw.split("--")
  const m = suffix ? /^(?:w(\d+)(?:x\d+)?|natural)-(light|dark)(-compact)?$/.exec(suffix) : null
  const width = m?.[1] ? Number(m[1]) : null
  return {
    id: id ?? null,
    theme: (m?.[2] as Theme | undefined) ?? null,
    frame: width === 1280 ? "1280" : width === 900 ? "900" : width ? "phone" : null,
    density: m?.[3] ? "compact" : null,
  }
}
