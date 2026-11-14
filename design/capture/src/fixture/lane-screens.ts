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
