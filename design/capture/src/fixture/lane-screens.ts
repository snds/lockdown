// The screens lane's own changes to the fixture: what its states
// (states/screens.tsx) set up before the app mounts. Seed, world, commands
// and story are the foundation's; a lane adds here and never edits those.

import type { World } from "./world"

/** Nothing on this computer yet: the first run. */
export const fresh = (w: World) => {
  w.device = { identity: false, deviceKey: false }
}

/** The web vault before anyone signed in to this tab. */
export const signedOut = (world: World) => {
  world.web = { signed_in: false, locked: true, login: null }
}

/** The core's words for a revoked device (crates/sync/src/renew.rs, `SignInReason::Revoked`). */
export const REVOKED = "This device was revoked from your account. Join it again from a device you still use."

/**
 * The fixture's computer is a Mac (its quick unlock is Touch ID, its device
 * "macos"), so Settings names it "This Mac" (cross-app decisions §5). The
 * capture runs on Linux, whose browser says otherwise; this says Mac before
 * the app mounts.
 */
export const onMac = () => {
  Object.defineProperty(navigator, "platform", { value: "MacIntel", configurable: true })
}

/** The core's own word for the kit's state (`none`, `made`, `drilled`): the story's sentence is not one. */
export const kitState = (status: object, recovery: "none" | "made" | "drilled") => ({ ...status, recovery })

/** The updater's status as the shell reports it, for About and the status line's version. */
export const updateStatus = (over: Record<string, unknown> = {}) => ({
  version: "1.8.0",
  channel: "stable",
  enabled: true,
  off: null,
  auto_check: true,
  state: "idle",
  percent: null,
  offered: null,
  error: null,
  link: null,
  link_why: null,
  asked: false,
  ...over,
})
