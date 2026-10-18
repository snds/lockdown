// The extension's own copy of the matcher: the same Rust as the desktop's,
// built to wasm (docs/plan/09-extension-boundary.md). The desktop decides
// every fill; the extension runs the same decision on every candidate the
// desktop offers and the same fill check before it asks for a value, so a
// fill both sides do not agree on never happens, and a refused one never
// even reaches the app.

import init, { decide, fill_check, matcher_data_status, set_matcher_data_floor } from "../../pkg/lockdown_wasm.js"

import type { Decision, FillWhat } from "../shared/protocol"

let ready: Promise<void> | null = null

const FLOOR_KEY = "matcherDataFloor"

/** Loads the wasm once, then hands the matcher the newest data version this
 *  profile has seen (D64) and remembers the version in force. */
export function load(): Promise<void> {
  if (!ready) {
    ready = (async () => {
      await init({ module_or_path: chrome.runtime.getURL("lockdown_wasm_bg.wasm") })
      try {
        const stored = await chrome.storage.local.get(FLOOR_KEY)
        const floor = typeof stored[FLOOR_KEY] === "number" ? stored[FLOOR_KEY] : 0
        set_matcher_data_floor(floor)
        const status = matcher_data_status() as { newest_loaded: number }
        if (status.newest_loaded > floor) await chrome.storage.local.set({ [FLOOR_KEY]: status.newest_loaded })
      } catch {
        // Storage can be unavailable in a private window; the built-in
        // data is in force either way.
      }
    })()
  }
  return ready
}

/** How a saved address relates to the frame. */
export async function decision(saved: string, current: string, rule: string | null): Promise<Decision> {
  await load()
  try {
    const out = decide({ saved, current, rule: rule ?? "exact_host" }) as { decision: Decision }
    return out.decision
  } catch {
    return "no_match"
  }
}

export interface Verdict {
  verdict: "fill" | "confirm" | "refuse"
  reason?: string
}

/** The fill gate, run here before the desktop runs it again. */
export async function check(req: {
  top: string
  frame: string
  field_visible: boolean
  user_confirmed: boolean
  kind: FillWhat | "passkey"
  decision: Decision
}): Promise<Verdict> {
  await load()
  try {
    return fill_check(req) as Verdict
  } catch (e) {
    return { verdict: "refuse", reason: e instanceof Error ? e.message : String(e) }
  }
}

export function isMatch(d: Decision): boolean {
  return d === "exact" || d === "base_domain" || d === "equivalent"
}
