// The extension's states: the toolbar popup (extension/src/popup/popup.tsx,
// over the fake background, brief B1) and the in-page frames
// (extension/src/pages/frame.ts) on a plain test page.

import { TooltipProvider } from "@unicron/ui"

import { Popup } from "../../../../extension/src/popup/popup"

import { background, installChrome } from "../host/chrome"
import { TestPage, type Mode } from "../harness/test-page"
import type { StateDef, Step } from "../harness/types"
import { scenario } from "./extension-scenarios"

/** The popup's colour scheme follows the system, as its own main.tsx does. */
function followScheme() {
  const dark = window.matchMedia("(prefers-color-scheme: dark)")
  document.documentElement.classList.toggle("dark", dark.matches)
}

function popup(name: string) {
  return () => {
    installChrome(background(scenario(name)))
    followScheme()
    return (
      // The browser draws the popup's window edge; here a hairline stands in for it.
      <div className="min-h-full bg-muted p-4">
        <div data-capture-frame="" className="inline-block overflow-hidden bg-surface-ground rule-box">
          <TooltipProvider>
            <Popup />
          </TooltipProvider>
        </div>
      </div>
    )
  }
}

const page = (mode: Mode, name: string, covered = false) => () => <TestPage mode={mode} name={name} covered={covered} />

/** Waits for the frame's own document to draw. */
const frameDrawn: Step = {
  run: async () => {
    const start = performance.now()
    for (;;) {
      const doc = document.querySelector<HTMLIFrameElement>("iframe[name^='ld-extframe:']")?.contentDocument
      if (doc?.querySelector("#app")?.childElementCount) break
      if (performance.now() - start > 6000) throw new Error("the in-page frame drew nothing")
      await new Promise((r) => setTimeout(r, 50))
    }
    // The frame arms its buttons 400 ms after it shows (clickjacking rule).
    await new Promise((r) => setTimeout(r, 1200))
  },
}

type ExtDef = Omit<StateDef, "surface" | "widths" | "frame">
const p = (s: ExtDef): StateDef => ({ surface: "extension", widths: "natural", frame: "[data-capture-frame]", ...s })

export const EXTENSION_STATES: StateDef[] = [
  // --- the popup -----------------------------------------------------------------------
  p({ id: "extension-popup-connecting", built: { area: "extension", screen: "popup-connecting", kind: "undrawn" }, screen: "Popup", state: "Connecting", brief_row: "Popup", notes: "The background hasn't answered yet.", mount: popup("popup-connecting"), steps: [{ pause: 300 }] }),
  p({ id: "extension-popup-no-app", built: { area: "extension", screen: "popup-no-app", kind: "undrawn" }, screen: "Popup", state: "App not reachable", brief_row: "Popup", notes: "Lockdown isn't running on this computer.", mount: popup("popup-no-app"), steps: [{ wait: "Try again", role: "button" }] }),
  p({ id: "extension-popup-unpaired", built: { area: "extension", screen: "popup-unpaired", kind: "undrawn" }, screen: "Popup", state: "Not paired", brief_row: "Popup", notes: "Pair this browser with the app.", mount: popup("popup-unpaired"), steps: [{ wait: "Pair with Lockdown", role: "button" }] }),
  p({ id: "extension-popup-pairing", built: { area: "extension", screen: "popup-pairing", kind: "undrawn" }, screen: "Popup", state: "Pairing code", brief_row: "Popup", notes: "The code to compare with the app's dialog.", mount: popup("popup-pairing"), steps: [{ wait: "Check the code." }, { pause: 400 }] }),
  p({ id: "extension-popup-locked", built: { area: "extension", screen: "popup-locked", kind: "undrawn" }, screen: "Popup", state: "Locked", brief_row: "Popup", notes: "Unlock in the app to fill and search.", mount: popup("popup-locked"), steps: [{ wait: "Open Lockdown", role: "button" }, { pause: 400 }] }),
  p({ id: "extension-popup-results", built: { area: "extension", screen: "popup-results", kind: "undrawn" }, screen: "Popup", state: "Unlocked with results", brief_row: "Popup", notes: "The page's two logins (the first from quick access, the second protected) and recent items.", mount: popup("popup-results"), steps: [{ wait: /^Fill /, role: "button" }, { pause: 300 }] }),
  p({ id: "extension-popup-search", built: { area: "extension", screen: "popup-search", kind: "undrawn" }, screen: "Popup", state: "Search with a close match", brief_row: "Popup", notes: "A search; the last result was found by forgiving a typo.", mount: popup("popup-close-match"), steps: [{ fill: "Search Lockdown", value: "bluefin" }, { pause: 500 }] }),
  p({ id: "extension-popup-code", built: { area: "extension", screen: "popup-code", kind: "undrawn" }, screen: "Popup", state: "Code shown in place", brief_row: "Popup", notes: "A one-time code shown in the popup for 30 seconds.", mount: popup("popup-results"), steps: [{ fill: "Search Lockdown", value: "kestrel" }, { click: /^Show code for Kestrel/ }, { wait: "Copy code", role: "button" }, { pause: 300 }] }),
  p({ id: "extension-popup-lookalike", built: { area: "extension", screen: "popup-lookalike", kind: "undrawn" }, screen: "Popup", state: "Lookalike stop", brief_row: "Popup", notes: "This page imitates a saved site: no fill, and a copy asks first.", mount: popup("popup-lookalike"), steps: [{ wait: /^Stop:/ }, { pause: 300 }] }),
  p({ id: "extension-popup-lookalike-copy", built: { area: "extension", screen: "popup-lookalike-copy", kind: "undrawn" }, screen: "Popup", state: "Lookalike: Copy anyway", brief_row: "Popup", notes: "Copying on a lookalike page asks for a second press.", mount: popup("popup-lookalike"), steps: [{ fill: "Search Lockdown", value: "bluefin bank" }, { wait: /^Bluefin Bank alerts/ }, { click: "Copy password" }, { wait: "Copy anyway", role: "button" }] }),
  p({ id: "extension-popup-fix", built: { area: "extension", screen: "popup-fix", kind: "undrawn" }, screen: "Popup → Fix this page", state: "Name the fields", brief_row: "Popup → Fix this page", notes: "Name a stubborn page's fields, or never fill there.", mount: popup("popup-results"), steps: [{ click: "Fix this page", role: "button" }, { wait: "Save fix", role: "button" }, { pause: 300 }] }),
  p({ id: "extension-popup-hide-email", built: { area: "extension", screen: "popup-hide-email", kind: "undrawn" }, screen: "Popup → Hide my email", state: "Alias services", brief_row: "Popup → Hide my email", notes: "Make an alias into the page's email field.", mount: popup("popup-hide-email"), steps: [{ wait: /^New alias from/, role: "button" }, { pause: 300 }] }),

  // --- in-page frames ------------------------------------------------------------------------
  p({ id: "extension-menu-logins", built: { area: "extension", screen: "menu-logins", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Logins", brief_row: "Fill menu (in page)", notes: "The page's logins under the focused field; a related-site row marked.", mount: page("menu", "menu-logins"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-protected", built: { area: "extension", screen: "menu-protected", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Protected login", brief_row: "Fill menu (in page)", notes: "A crown jewel by title only: Protected.", mount: page("menu", "menu-protected"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-passkeys", built: { area: "extension", screen: "menu-passkeys", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Passkeys first", brief_row: "Fill menu (in page)", notes: "A page asking for a passkey: its passkeys with the logins, and another device.", mount: page("menu", "menu-passkeys"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-code", built: { area: "extension", screen: "menu-code", kind: "undrawn" }, screen: "Fill menu (in page)", state: "A code field", brief_row: "Fill menu (in page)", notes: "A one-time code field.", mount: page("menu", "menu-code"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-card", built: { area: "extension", screen: "menu-card", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Cards", brief_row: "Fill menu (in page)", notes: "Cards on a checkout page; a pick asks a second time before it fills.", mount: page("menu", "menu-card"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-card-confirm", built: { area: "extension", screen: "menu-card-confirm", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Card: second press", brief_row: "Fill menu (in page)", notes: "A card fills only after a second deliberate press (brief §6 rule 3). In the review build, tap the card.", mount: page("menu", "menu-card"), steps: [frameDrawn], trusted: ["Bluefin Visa Visa ending 1111"], live_clock: true }),
  p({ id: "extension-menu-identity", built: { area: "extension", screen: "menu-identity", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Identity", brief_row: "Fill menu (in page)", notes: "An address form.", mount: page("menu", "menu-identity"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-suggest", built: { area: "extension", screen: "menu-suggest", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Suggest a password", brief_row: "Fill menu (in page)", notes: "A new-password field: a strong password by the site's rules.", mount: page("menu", "menu-suggest"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-lookalike", built: { area: "extension", screen: "menu-lookalike", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Lookalike stop", brief_row: "Fill menu (in page)", notes: "The page imitates a saved site: Lockdown won't fill here.", mount: page("menu", "menu-lookalike"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-locked", built: { area: "extension", screen: "menu-locked", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Locked", brief_row: "Fill menu (in page)", notes: "Unlock in the app to fill.", mount: page("menu", "menu-locked"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-unpaired", built: { area: "extension", screen: "menu-unpaired", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Unpaired", brief_row: "Fill menu (in page)", notes: "Pair from the menu.", mount: page("menu", "menu-unpaired"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-pairing", built: { area: "extension", screen: "menu-pairing", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Pairing code", brief_row: "Fill menu (in page)", notes: "Pairing from the menu: the code to compare in the app. In the review build, tap Pair this browser.", mount: page("menu", "menu-unpaired"), steps: [frameDrawn], trusted: ["Pair this browser"], live_clock: true }),
  p({ id: "extension-menu-no-app", built: { area: "extension", screen: "menu-no-app", kind: "undrawn" }, screen: "Fill menu (in page)", state: "App not reachable", brief_row: "Fill menu (in page)", notes: "The app isn't running.", mount: page("menu", "menu-no-app"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-menu-covered", built: { area: "extension", screen: "menu-covered", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Covered", brief_row: "Fill menu (in page)", notes: "Something on the page overlaps the menu: its buttons stay disabled (IntersectionObserver v2).", mount: page("menu", "menu-covered", true), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-prompt-save", built: { area: "extension", screen: "prompt-save", kind: "undrawn" }, screen: "Save / update prompt (in page)", state: "Save", brief_row: "Save / update prompt (in page)", notes: "Save a new login.", mount: page("prompt", "prompt-save"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-prompt-update", built: { area: "extension", screen: "prompt-update", kind: "undrawn" }, screen: "Save / update prompt (in page)", state: "Update", brief_row: "Save / update prompt (in page)", notes: "Update a changed password.", mount: page("prompt", "prompt-update"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-sheet-get", built: { area: "extension", screen: "sheet-get", kind: "undrawn" }, screen: "Passkey sheet (in page)", state: "Pick a passkey", brief_row: "Passkey sheet (in page)", notes: "Sign in with a passkey: pick, then Sign in.", mount: page("passkey", "sheet-get"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-sheet-confirm", built: { area: "extension", screen: "sheet-confirm", kind: "undrawn" }, screen: "Passkey sheet (in page)", state: "Sign in?", brief_row: "Passkey sheet (in page)", notes: "After a pick, the second press. In the review build, tap a passkey.", mount: page("passkey", "sheet-get"), steps: [frameDrawn], trusted: ["sam_r"], live_clock: true }),
  p({ id: "extension-sheet-create", built: { area: "extension", screen: "sheet-create", kind: "undrawn" }, screen: "Passkey sheet (in page)", state: "Save a passkey", brief_row: "Passkey sheet (in page)", notes: "Create a passkey into a new login.", mount: page("passkey", "sheet-create"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-sheet-held", built: { area: "extension", screen: "sheet-held", kind: "undrawn" }, screen: "Passkey sheet (in page)", state: "Already held", brief_row: "Passkey sheet (in page)", notes: "Lockdown already holds a passkey for this account.", mount: page("passkey", "sheet-create-held"), steps: [frameDrawn], live_clock: true }),
  p({ id: "extension-sheet-locked", built: { area: "extension", screen: "sheet-locked", kind: "undrawn" }, screen: "Passkey sheet (in page)", state: "Locked", brief_row: "Passkey sheet (in page)", notes: "Unlock on this computer to use passkeys here.", mount: page("passkey", "sheet-locked"), steps: [frameDrawn], live_clock: true }),
  // --- docs/plan/37-cli-popup.md §2 (design review R16, R17) -----------------------------
  p({ id: "extension-popup-locked-opened", built: { area: "extension", screen: "popup-locked-opened", kind: "undrawn" }, screen: "Popup", state: "Locked: Open Lockdown", brief_row: "Popup", notes: "Open Lockdown brought the app's window forward; unlock there.", mount: popup("popup-locked"), steps: [{ click: "Open Lockdown", role: "button" }, { wait: /^Lockdown is open on this computer/ }] }),
  p({ id: "extension-popup-no-app-how", built: { area: "extension", screen: "popup-no-app-how", kind: "undrawn" }, screen: "Popup", state: "App not reachable: how to open it", brief_row: "Popup", notes: "No app the browser could start: the popup says how to open Lockdown by hand.", mount: popup("popup-no-app"), steps: [{ click: "Open Lockdown", role: "button" }, { wait: /^Lockdown couldn't open the app/ }] }),
  p({ id: "extension-menu-arming", built: { area: "extension", screen: "menu-arming", kind: "undrawn" }, screen: "Fill menu (in page)", state: "Arming", brief_row: "Fill menu (in page)", notes: "In its first 400 ms a choice keeps its colors and a 2 px rule under its label fills from the left (drawn whole under reduced motion, as captured); it works once the rule is full. The review build arms as usual.", mount: page("menu", "menu-logins"), steps: [frameDrawn], hold_clock: true }),
  p({ id: "extension-prompt-arming", built: { area: "extension", screen: "prompt-arming", kind: "undrawn" }, screen: "Save / update prompt (in page)", state: "Arming", brief_row: "Save / update prompt (in page)", notes: "Save and Not now in their first 400 ms: full color with the 2 px arming rule, not a disabled look. The review build arms as usual.", mount: page("prompt", "prompt-save"), steps: [frameDrawn], hold_clock: true }),
]
