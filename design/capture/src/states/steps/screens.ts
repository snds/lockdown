// Steps the screens lane owns: unlock, the first run, Settings. Every state
// that needs the unlocked app starts with `open` (or `webOpen`), so a change
// to the unlock screen is fixed here and nowhere else.

import { MASTER_PASSWORD } from "../../fixture/commands"
import type { Step } from "../../harness/types"
import { VAULT_OPEN, go, type PlaceId } from "./shell"

/** The desktop's unlock screen, answered. */
export const unlock: Step[] = [{ fill: "Master password", value: MASTER_PASSWORD }, { click: "Unlock", role: "button" }]
/** Opens the unlocked desktop app. */
export const open: Step[] = [...unlock, VAULT_OPEN]
/** Opens the unlocked web vault: a tab that is signed in asks for the password only. */
export const webOpen: Step[] = [{ fill: "Master password", value: "correct horse battery staple" }, { click: "Unlock", role: "button" }, VAULT_OPEN]

/** The first run's create path to the kit (brief F2; doc 39 §1.9 screens 0–2). */
export const create: Step[] = [
  { click: "New vault", role: "button" },
  { fill: "Your name", value: "Sam Rivera" },
  { fill: "Master password", value: MASTER_PASSWORD },
  { fill: "Type it again", value: MASTER_PASSWORD },
  { click: "Create my vault", role: "button" },
]
/** The kit put away. */
export const kitSaved: Step[] = [{ click: "I saved, printed or wrote down the kit" }, { click: "Continue", role: "button" }]
/** Quick unlock passed by (the fixture's computer has Touch ID that doesn't work yet). */
export const quickSkipped: Step[] = [{ wait: "Unlock with Touch ID?", role: "heading" }, { click: "Not now", role: "button" }]

/** Settings, drawn, on one of its sections (doc 64: Settings is a list of sections; each state opens the one it shows). */
export const settingsAt = (section: SettingsPlace): Step[] => [...open, ...go(section)]
type SettingsPlace = Extract<PlaceId, `settings-${string}`>
/** Settings on Appearance, where Density is. */
export const settings: Step[] = [...settingsAt("settings-appearance"), { wait: "Density" }]
/** Settings is taller than a capture: bring Backups' crown-jewel row into the picture. */
export const toBackups: Step = { run: () => document.getElementById("backup-crown-jewels")?.scrollIntoView({ block: "center" }) }

/**
 * The import's CHOOSE FILE, as the window's file dialog would answer it: a
 * CSV export of invented logins put on the wizard's file input. The fixture's
 * core reads none of it; its preview is the fixture's.
 */
export const pickImportFile: Step[] = [
  { wait: "Choose file…", role: "button" },
  {
    run: () => {
      const input = document.getElementById("import-file") as HTMLInputElement | null
      if (!input) throw new Error("no import file input")
      const files = new DataTransfer()
      files.items.add(new File(["title,url,username,password\n"], "logins-export.csv", { type: "text/csv" }))
      input.files = files.files
      input.dispatchEvent(new Event("change", { bubbles: true }))
    },
  },
  { wait: "logins-export.csv" },
]

/** Waits, up to 6 s, until the page's text holds `words`, in view or not (a row below the fold). */
export const inPage = (words: RegExp): Step => ({
  run: async () => {
    for (let i = 0; i < 60 && !words.test(document.body.textContent ?? ""); i++) await new Promise((r) => setTimeout(r, 100))
    if (!words.test(document.body.textContent ?? "")) throw new Error(`waited 6 s for ${words} in the page`)
  },
})
/** Settings has drawn its rows and its numbers, at any width: This Mac's rows and its sessions line, or (medium) the section list. */
export const settingsDrawn: Step[] = [
  inPage(/Auto-lock|Import and export/),
  {
    run: async () => {
      const text = () => document.body.textContent ?? ""
      for (let i = 0; i < 60 && /Other sessions/.test(text()) && !/\d+ others|None/.test(text()); i++) await new Promise((r) => setTimeout(r, 100))
    },
  },
  { pause: 300 },
]
