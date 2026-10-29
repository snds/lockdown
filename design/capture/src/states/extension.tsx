// The extension's states: the toolbar popup (extension/src/popup/popup.tsx,
// unchanged, over the fake background) and the in-page frames
// (extension/src/pages/frame.ts, unchanged) on a plain test page.

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
      <div className="min-h-full bg-muted p-4">
        <div data-capture-frame="" className="inline-block overflow-hidden rounded-lg border bg-background shadow-lg">
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
  p({ id: "extension-popup-connecting", screen: "Popup", state: "Connecting", brief_row: "Popup", notes: "The background hasn't answered yet.", mount: popup("popup-connecting"), steps: [{ pause: 300 }] }),
  p({ id: "extension-popup-no-app", screen: "Popup", state: "App not reachable", brief_row: "Popup", notes: "Lockdown isn't running on this computer.", mount: popup("popup-no-app"), steps: [{ wait: "Try again", role: "button" }] }),
  p({ id: "extension-popup-unpaired", screen: "Popup", state: "Not paired", brief_row: "Popup", notes: "Pair this browser with the app.", mount: popup("popup-unpaired"), steps: [{ wait: "Pair with Lockdown", role: "button" }] }),
  p({ id: "extension-popup-pairing", screen: "Popup", state: "Pairing code", brief_row: "Popup", notes: "The code to compare with the app's dialog.", mount: popup("popup-pairing"), steps: [{ pause: 400 }] }),
  p({ id: "extension-popup-locked", screen: "Popup", state: "Locked", brief_row: "Popup", notes: "Unlock in the app to fill and search.", mount: popup("popup-locked"), steps: [{ pause: 400 }] }),
  p({ id: "extension-popup-results", screen: "Popup", state: "Unlocked with results", brief_row: "Popup", notes: "The page's login (from quick access) and recent items.", mount: popup("popup-results"), steps: [{ wait: "Fill", role: "button" }, { pause: 300 }] }),
  p({ id: "extension-popup-search", screen: "Popup", state: "Search with a close match", brief_row: "Popup", notes: "A search; the last result was found by forgiving a typo.", mount: popup("popup-close-match"), steps: [{ fill: "Search Lockdown", value: "bluefin" }, { pause: 500 }] }),
  p({ id: "extension-popup-code", screen: "Popup", state: "Code shown in place", brief_row: "Popup", notes: "A one-time code shown in the popup for 30 seconds.", mount: popup("popup-results"), steps: [{ fill: "Search Lockdown", value: "kestrel" }, { click: /^Show code for Kestrel/ }, { pause: 300 }] }),
  p({ id: "extension-popup-lookalike", screen: "Popup", state: "Lookalike stop", brief_row: "Popup", notes: "This page imitates a saved site: no fill, and a copy asks first.", mount: popup("popup-lookalike"), steps: [{ wait: /^Stop:/ }, { pause: 300 }] }),
  p({ id: "extension-popup-lookalike-copy", screen: "Popup", state: "Lookalike: Copy anyway", brief_row: "Popup", notes: "Copying on a lookalike page asks for a second press.", mount: popup("popup-lookalike"), steps: [{ fill: "Search Lockdown", value: "bluefin bank" }, { wait: /^Bluefin Bank alerts/ }, { click: "Copy password" }, { wait: "Copy anyway", role: "button" }] }),
  p({ id: "extension-popup-fix", screen: "Popup → Fix this page", state: "Name the fields", brief_row: "Popup → Fix this page", notes: "Name a stubborn page's fields, or never fill there.", mount: popup("popup-results"), steps: [{ click: "Fix this page" }, { wait: "Save fix", role: "button" }] }),
  p({ id: "extension-popup-hide-email", screen: "Popup → Hide my email", state: "Alias services", brief_row: "Popup → Hide my email", notes: "Make an alias into the page's email field.", mount: popup("popup-hide-email"), steps: [{ wait: "Hide my email" }] }),

  // --- in-page frames ------------------------------------------------------------------------
  p({ id: "extension-menu-logins", screen: "Fill menu (in page)", state: "Logins", brief_row: "Fill menu (in page)", notes: "The page's logins under the focused field; a related-site row marked.", mount: page("menu", "menu-logins"), steps: [frameDrawn] }),
  p({ id: "extension-menu-protected", screen: "Fill menu (in page)", state: "Protected login", brief_row: "Fill menu (in page)", notes: "A crown jewel by title only: Protected.", mount: page("menu", "menu-protected"), steps: [frameDrawn] }),
  p({ id: "extension-menu-passkeys", screen: "Fill menu (in page)", state: "Passkeys first", brief_row: "Fill menu (in page)", notes: "A page asking for a passkey: its passkeys with the logins, and another device.", mount: page("menu", "menu-passkeys"), steps: [frameDrawn] }),
  p({ id: "extension-menu-code", screen: "Fill menu (in page)", state: "A code field", brief_row: "Fill menu (in page)", notes: "A one-time code field.", mount: page("menu", "menu-code"), steps: [frameDrawn] }),
  p({ id: "extension-menu-card", screen: "Fill menu (in page)", state: "Cards", brief_row: "Fill menu (in page)", notes: "Cards on a checkout page; a pick asks a second time before it fills.", mount: page("menu", "menu-card"), steps: [frameDrawn] }),
  p({ id: "extension-menu-card-confirm", screen: "Fill menu (in page)", state: "Card: second press", brief_row: "Fill menu (in page)", notes: "A card fills only after a second deliberate press (brief §6 rule 3). In the review build, tap the card.", mount: page("menu", "menu-card"), steps: [frameDrawn], trusted: ["Bluefin Visa Visa ending 1111"] }),
  p({ id: "extension-menu-identity", screen: "Fill menu (in page)", state: "Identity", brief_row: "Fill menu (in page)", notes: "An address form.", mount: page("menu", "menu-identity"), steps: [frameDrawn] }),
  p({ id: "extension-menu-suggest", screen: "Fill menu (in page)", state: "Suggest a password", brief_row: "Fill menu (in page)", notes: "A new-password field: a strong password by the site's rules.", mount: page("menu", "menu-suggest"), steps: [frameDrawn] }),
  p({ id: "extension-menu-lookalike", screen: "Fill menu (in page)", state: "Lookalike stop", brief_row: "Fill menu (in page)", notes: "The page imitates a saved site: Lockdown won't fill here.", mount: page("menu", "menu-lookalike"), steps: [frameDrawn] }),
  p({ id: "extension-menu-locked", screen: "Fill menu (in page)", state: "Locked", brief_row: "Fill menu (in page)", notes: "Unlock in the app to fill.", mount: page("menu", "menu-locked"), steps: [frameDrawn] }),
  p({ id: "extension-menu-unpaired", screen: "Fill menu (in page)", state: "Unpaired", brief_row: "Fill menu (in page)", notes: "Pair from the menu.", mount: page("menu", "menu-unpaired"), steps: [frameDrawn] }),
  p({ id: "extension-menu-pairing", screen: "Fill menu (in page)", state: "Pairing code", brief_row: "Fill menu (in page)", notes: "Pairing from the menu: the code to compare in the app. In the review build, tap Pair this browser.", mount: page("menu", "menu-unpaired"), steps: [frameDrawn], trusted: ["Pair this browser"] }),
  p({ id: "extension-menu-no-app", screen: "Fill menu (in page)", state: "App not reachable", brief_row: "Fill menu (in page)", notes: "The app isn't running.", mount: page("menu", "menu-no-app"), steps: [frameDrawn] }),
  p({ id: "extension-menu-covered", screen: "Fill menu (in page)", state: "Covered", brief_row: "Fill menu (in page)", notes: "Something on the page overlaps the menu: its buttons stay disabled (IntersectionObserver v2).", mount: page("menu", "menu-covered", true), steps: [frameDrawn] }),
  p({ id: "extension-prompt-save", screen: "Save / update prompt (in page)", state: "Save", brief_row: "Save / update prompt (in page)", notes: "Save a new login.", mount: page("prompt", "prompt-save"), steps: [frameDrawn] }),
  p({ id: "extension-prompt-update", screen: "Save / update prompt (in page)", state: "Update", brief_row: "Save / update prompt (in page)", notes: "Update a changed password.", mount: page("prompt", "prompt-update"), steps: [frameDrawn] }),
  p({ id: "extension-sheet-get", screen: "Passkey sheet (in page)", state: "Pick a passkey", brief_row: "Passkey sheet (in page)", notes: "Sign in with a passkey: pick, then Sign in.", mount: page("passkey", "sheet-get"), steps: [frameDrawn] }),
  p({ id: "extension-sheet-confirm", screen: "Passkey sheet (in page)", state: "Sign in?", brief_row: "Passkey sheet (in page)", notes: "After a pick, the second press. In the review build, tap a passkey.", mount: page("passkey", "sheet-get"), steps: [frameDrawn], trusted: ["sam_r"] }),
  p({ id: "extension-sheet-create", screen: "Passkey sheet (in page)", state: "Save a passkey", brief_row: "Passkey sheet (in page)", notes: "Create a passkey into a new login.", mount: page("passkey", "sheet-create"), steps: [frameDrawn] }),
  p({ id: "extension-sheet-held", screen: "Passkey sheet (in page)", state: "Already held", brief_row: "Passkey sheet (in page)", notes: "Lockdown already holds a passkey for this account.", mount: page("passkey", "sheet-create-held"), steps: [frameDrawn] }),
  p({ id: "extension-sheet-locked", screen: "Passkey sheet (in page)", state: "Locked", brief_row: "Passkey sheet (in page)", notes: "Unlock on this computer to use passkeys here.", mount: page("passkey", "sheet-locked"), steps: [frameDrawn] }),
  // --- docs/plan/37-cli-popup.md §2 (design review R16, R17) -----------------------------
  p({ id: "extension-popup-locked-opened", screen: "Popup", state: "Locked: Open Lockdown", brief_row: "Popup", notes: "Open Lockdown brought the app's window forward; unlock there.", mount: popup("popup-locked"), steps: [{ click: "Open Lockdown", role: "button" }, { wait: /^Lockdown is open on this computer/ }] }),
  p({ id: "extension-popup-no-app-how", screen: "Popup", state: "App not reachable: how to open it", brief_row: "Popup", notes: "No app the browser could start: the popup says how to open Lockdown by hand.", mount: popup("popup-no-app"), steps: [{ click: "Open Lockdown", role: "button" }, { wait: /^Lockdown couldn't open the app/ }] }),
  p({ id: "extension-menu-arming", screen: "Fill menu (in page)", state: "Arming", brief_row: "Fill menu (in page)", notes: "In its first 400 ms a choice keeps its colours and shows an underline that fills (drawn whole under reduced motion, as captured); it works once the line is full. The review build arms as usual.", mount: page("menu", "menu-logins"), steps: [frameDrawn], hold_clock: true }),
  p({ id: "extension-prompt-arming", screen: "Save / update prompt (in page)", state: "Arming", brief_row: "Save / update prompt (in page)", notes: "Save in its first 400 ms: full colour with the arming underline, not a disabled look. The review build arms as usual.", mount: page("prompt", "prompt-save"), steps: [frameDrawn], hold_clock: true }),
]
