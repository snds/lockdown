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
export const webOpen: Step[] = [{ fill: /password/i, value: "correct horse battery staple" }, { click: "Unlock", role: "button" }, VAULT_OPEN]

/** The first run's create path to the kit (doc 39 §1.9 screens 0–2). */
export const create: Step[] = [
  { click: "Create a new vault" },
  { fill: "Your name", value: "Sam Rivera" },
  { fill: "Master password", value: MASTER_PASSWORD },
  { fill: "Type it again", value: MASTER_PASSWORD },
  { click: "Continue", role: "button" },
]
/** The kit put away. */
export const kitSaved: Step[] = [{ click: "I saved, printed or wrote down the kit" }, { click: "Continue", role: "button" }]

/** Settings, drawn, on one of its sections (doc 64: Settings is a list of sections; each state opens the one it shows). */
export const settingsAt = (section: SettingsPlace): Step[] => [...open, ...go(section)]
type SettingsPlace = Extract<PlaceId, `settings-${string}`>
/** Settings on Appearance, where Density is. */
export const settings: Step[] = [...settingsAt("settings-appearance"), { wait: "Density" }]
/** Settings is taller than a capture: bring Backups (its last section) into the picture. */
export const toBackups: Step = { run: () => document.getElementById("backup-crown-jewels")?.scrollIntoView({ block: "start" }) }
