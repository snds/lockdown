// The developer tools (docs/plan/40-devtools.md), as a development build
// draws them: the first run's way into the demo, the Developer menu (the
// OS draws the real one; this is its list, drawn in its place), the reset's
// confirm dialog, and the demo open with its bar, with one item of each
// kind from the default pack.

import App from "@/App"
import { MARKER } from "@/devtools/marker"
import { SAMPLE_PLAN } from "@/devtools/fake-devtools"

import { demoItems, DEMO_PACK } from "../fixture/demo-pack"
import { uid } from "../fixture/ids"
import type { World } from "../fixture/world"
import type { StateDef, Step } from "../harness/types"
import { MASTER_PASSWORD } from "../fixture/commands"
import { WIDTHS } from "../harness/types"
import { DeveloperMenu } from "./developer-menu"

const app = () => <App />
const DIALOG: Step = { see: '[role="dialog"]' }
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
const starts = (s: string) => new RegExp(`^${esc(s)}(\\s|$)`)
const open: Step[] = [{ fill: "Master password", value: MASTER_PASSWORD }, { click: "Unlock", role: "button" }, { wait: /^All items/ }]

/** The demo's world: the pack's items in place of the fixture's. */
function demoWorld(w: World) {
  w.items = demoItems().map((s) => ({ ...s, id: uid("item", s.n), version: 1 }))
  w.settings = { ...w.settings, breach_check: false, change_password_check: false }
}

const demoOpen = () => ({
  "plugin:devtools|status": () => ({ marker: MARKER, reset: true, demo: { open: true, came_from: "first_run", pack: DEMO_PACK.name, items: 158 } }),
  "plugin:devtools|demo_leave": () => "first_run",
})

/** One item of each kind, by its title in the pack. */
const KINDS: [string, string][] = [
  ["login", "Example Mail"],
  ["passkey", "Example Docs passkey"],
  ["code", "Example VPN"],
  ["wi-fi", "Home Wi-Fi"],
  ["ssh-key", "Build server key"],
  ["api-credential", "Example API token"],
  ["document", "Lease agreement"],
  ["identity", "Passport (expires soon)"],
  ["card", "Expired Mastercard"],
  ["bank-account", "Example Savings"],
  ["note", "Welcome to the Lockdown demo"],
  ["crypto-wallet", "Test wallet (BIP-39 test phrase)"],
  ["server", "Example mail account (IMAP)"],
  ["database", "Orders database"],
  ["software-licence", "Example Editor licence"],
  ["membership", "Example Gym"],
  ["custom", "Locker combination"],
]

export const DEVTOOLS_STATES: StateDef[] = [
  {
    id: "desktop-devtools-first-run",
    surface: "desktop",
    screen: "First run",
    state: "With the demo",
    brief_row: "First run: create",
    notes: "A development build: Explore the demo under the three doors (docs/plan/40-devtools.md §3). Not in a release.",
    widths: WIDTHS,
    setup: (w) => {
      w.device = { identity: false, deviceKey: false }
    },
    mount: app,
    steps: [{ wait: "Explore the demo", role: "button" }],
  },
  {
    id: "desktop-devtools-menu",
    surface: "desktop",
    screen: "Developer menu",
    state: "Items",
    brief_row: null,
    notes: "The Developer menu's items in order (crates/devtools/src/menu.rs). The OS draws the real menu: in the macOS menu bar, and in the main window's menu on Windows and Linux.",
    widths: "natural",
    frame: '[aria-label="Developer menu (as the OS draws it)"]',
    mount: () => <DeveloperMenu />,
  },
  {
    id: "desktop-devtools-reset",
    surface: "desktop",
    screen: "Developer reset",
    state: "Confirm",
    brief_row: null,
    notes: "Developer → Reset Lockdown…: what is erased and what stays, the identity's verdict, and the word typed.",
    widths: WIDTHS,
    answers: () => ({ "plugin:devtools|reset_plan": () => SAMPLE_PLAN, "plugin:devtools|reset_run": () => undefined }),
    mount: app,
    steps: [...open, { emit: "devtools://menu", payload: "reset" }, DIALOG, { fill: "Type reset to confirm", value: "reset" }],
  },
  {
    id: "desktop-devtools-reset-restarting",
    surface: "desktop",
    screen: "Developer reset",
    state: "Restarting",
    brief_row: null,
    notes: "Erased; the app relaunches into the first run. Opened over the lock screen: the reset works locked.",
    widths: WIDTHS,
    answers: () => ({ "plugin:devtools|reset_plan": () => SAMPLE_PLAN, "plugin:devtools|reset_run": () => undefined }),
    mount: app,
    steps: [{ wait: "Unlock", role: "button" }, { pause: 300 }, { emit: "devtools://menu", payload: "reset" }, DIALOG, { fill: "Type reset to confirm", value: "reset" }, { click: "Erase and restart" }, { wait: /Lockdown is restarting/ }],
  },
  {
    id: "desktop-devtools-demo",
    surface: "desktop",
    screen: "Demo",
    state: "Open",
    brief_row: null,
    notes: "The demo open: its bar on every screen, the default pack's 158 items.",
    widths: WIDTHS,
    setup: demoWorld,
    answers: demoOpen,
    mount: app,
    steps: [...open, { wait: "Demo", role: "region" }],
  },
  ...KINDS.map(
    ([kind, title]): StateDef => ({
      id: `desktop-devtools-demo-${kind}`,
      surface: "desktop",
      screen: "Demo",
      state: `A ${kind}`,
      brief_row: null,
      notes: `The demo's ${kind}: "${title}", from the default pack.`,
      widths: WIDTHS,
      setup: demoWorld,
      answers: demoOpen,
      mount: app,
      steps: [...open, { wait: "Demo", role: "region" }, { click: starts(title), within: "main" }, { wait: title, role: "heading" }],
    }),
  ),
]
