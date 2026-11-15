// The edges lane's own changes to the fixture: what its states
// (states/edges.tsx) set up or answer. Seed, world, commands and story are
// the foundation's; a lane adds here and never edits those.

/** The core's words while a split view pauses sharing (crates/app/src/session/split_pause.rs). */
export const SHARING_PAUSED =
  "Sharing is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Until it clears, Lockdown won't look anyone up or check their keys with a phrase, share a vault or an item, make share or request links, name an emergency contact or open emergency access. Your vault still opens and syncs."
export const PAUSED_REASON =
  "is paused: this server's public log doesn't match a mirror, and a server showing two histories could swap someone's keys. Your vault still opens and syncs; this works again once the split clears."
/** The core's words on a shared vault whose new members wait (crates/app/src/session/split_hold.rs). */
export const HELD = "New members wait until the server's log matches its mirrors again."
/** The core's words once a head from someone unmet stopped the vault (crates/app/src/session/shared_sync.rs). */
export const HELD_AFTER =
  "New members wait until the server's log matches its mirrors again. Changes made after a new member's or a new device's wait too, even from members you already have."
