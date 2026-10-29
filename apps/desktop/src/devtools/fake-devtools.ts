import type { Status } from "@/core/types"

import type { DemoOpened, DevtoolsClient, MenuAction, ResetPlan } from "./devtools-client"
import { MARKER } from "./marker"

/** A reset plan as the shell answers it, for tests and the review harness. */
export const SAMPLE_PLAN: ResetPlan = {
  erase: [
    { what: "Lockdown's data: the vault, this device's key, settings, browser pairings, the endpoint file and the demo", path: "/home/sam/.local/share/snds/lockdown" },
    { what: "The app's settings folder", path: "/home/sam/.config/design.snds.lockdown" },
    { what: "The app's cache", path: "/home/sam/.cache/design.snds.lockdown" },
    { what: "The shared identity (only Lockdown uses it)", path: "/home/sam/.local/share/snds/identity" },
  ],
  keystore: { service: "design.snds.lockdown", entries: ["session:sam@example.com@https://vault.example.com"] },
  identity: {
    root: "/home/sam/.local/share/snds/identity",
    erase: true,
    why: "Only Lockdown uses it, so it is erased: the next launch starts from nothing.",
    apps: ["lockdown"],
  },
  keeps: [
    "Your account on a server, if you made one: a reset deletes nothing there. Sign in again to get its vaults back.",
    "Exports and backups saved in folders you chose.",
    "The browser connection's registration with each browser, which Lockdown writes again when it starts. Each browser has to be paired again.",
    "Anything belonging to Mailbird, Synapse or another app that shares your identity.",
  ],
  refused: [],
  web_storage: true,
}

/** A fake shell: records calls, and lets a test send menu picks. */
export function fakeDevtools(status: Status, overrides: Partial<DevtoolsClient> = {}) {
  const menu = new Set<(a: MenuAction) => void>()
  let open = false
  const opened = (): DemoOpened => {
    open = true
    return { status, came_from: "first_run", loaded: { pack: "Lockdown QA pack", items: 158, by_kind: {}, notes: [] } }
  }
  const client: DevtoolsClient = {
    status: () => Promise.resolve({ marker: MARKER, reset: true, demo: open ? { open, came_from: "first_run", pack: "Lockdown QA pack", items: 158 } : { open, items: 0 } }),
    resetPlan: () => Promise.resolve(SAMPLE_PLAN),
    resetRun: () => Promise.resolve(),
    demoOpen: () => Promise.resolve(opened()),
    demoReset: () => Promise.resolve(opened()),
    demoLeave: () => {
      open = false
      return Promise.resolve("first_run")
    },
    demoLoadPack: () => Promise.resolve(opened()),
    onMenu: (on) => {
      menu.add(on)
      return () => menu.delete(on)
    },
    ...overrides,
  }
  return { client, pick: (a: MenuAction) => menu.forEach((on) => on(a)) }
}
