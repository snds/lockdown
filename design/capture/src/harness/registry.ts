import { DESKTOP_STATES } from "../states/desktop"
import { DESKTOP_MORE } from "../states/desktop-more"
import { EXTENSION_STATES } from "../states/extension"
import { SERVER_STATES } from "../states/server"
import { WEB_STATES } from "../states/web"
import type { Frame } from "./prefs"
import type { Density, StateDef, StateMeta, Theme } from "./types"

export const STATES: StateDef[] = [...DESKTOP_STATES, ...DESKTOP_MORE, ...WEB_STATES, ...EXTENSION_STATES, ...SERVER_STATES]

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
  }))
}

/**
 * One image's id: the state's, then its width (or `natural`), theme and,
 * when compact, the density. The review build opens any of them:
 * `#/desktop-vault-filled--w390-dark` lands on that state, dark, at 390.
 */
export function imageId(state: string, width: number | "natural", theme: Theme, density: Density): string {
  return `${state}--${width === "natural" ? "natural" : `w${width}`}-${theme}${density === "compact" ? "-compact" : ""}`
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
  const m = suffix ? /^(?:w(\d+)|natural)-(light|dark)(-compact)?$/.exec(suffix) : null
  const width = m?.[1] ? Number(m[1]) : null
  return {
    id: id ?? null,
    theme: (m?.[2] as Theme | undefined) ?? null,
    frame: width === 1280 ? "1280" : width === 900 ? "900" : width ? "phone" : null,
    density: m?.[3] ? "compact" : null,
  }
}
