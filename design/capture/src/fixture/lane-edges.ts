// The edges lane's own changes to the fixture: what its states
// (states/edges.tsx) set up or answer. Seed, world, commands and story are
// the foundation's; a lane adds here and never edits those.

import type { ClockView } from "@/features/bridges/bridges-client"
import type { ScanOutcome } from "@/features/scan/scan-client"

import { uid } from "./ids"
import type { World } from "./world"

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

// --- Codes and the TV sign-in (edges-5, doc 88) ---------------------------------------


/** Codes kept by invented services, enough to pass the search field's ten (brief C1). */
const MORE_CODES = ["Heronsgate Bank", "Pinecrest Mail", "Tidewater DNS", "Larkspur Pay", "Quarry Git"]

/** Adds five code items, so Codes holds twelve and shows its search field. */
export const moreCodes = (w: World) => {
  w.items = [
    ...w.items,
    ...MORE_CODES.map((title, i) => ({
      n: 9500 + i,
      id: uid("item", 9500 + i),
      version: 1,
      kind: "code",
      title,
      vault: "personal" as const,
      edited: 10 + i * 7,
      uses: 3,
      fields: { "code.0": `otpauth://totp/${encodeURIComponent(title)}:sam.rivera?secret=K${"RSXG5CTMVRXEZLU".slice(i)}${"A".repeat(i)}&issuer=${encodeURIComponent(title)}` },
    })),
  ]
}

/** The core's clock check when this computer is 45 seconds behind and codes are corrected. */
export const CLOCK_CORRECTED: ClockView = {
  checked_at: Math.floor(Date.now() / 1000),
  offset_secs: 45,
  verdict: "corrected",
  correcting: true,
  words: "This computer's clock is 45 seconds behind your server's. Codes are worked out from the server's time.",
}

/** What the core reads from the TV's code through the camera: a sign-in it approves, with the logins that fit the page. */
export const TV_SIGN_IN: ScanOutcome = {
  found: true,
  kind: "device_code",
  decision: "approve",
  service: "Meadowlark Streaming",
  host: "watch.meadowlark.example",
  code: "WDJB-MJHT",
  code_in_url: false,
  activation_id: "act-tv-1",
  accounts: [
    { item_id: "meadowlark-1", title: "Meadowlark", username: "sam.rivera@northwind.example" },
    { item_id: "meadowlark-2", title: "Meadowlark (kids)", username: "kids@rivera.example" },
  ],
}

/**
 * A camera for the capture: the webview's `getUserMedia` answers with a canvas's
 * stream, a dark screen with a square of blocks drawn on it (a stand-in, not a
 * code anything could read). The fake core decides what the frames hold.
 */
export const fakeCamera = () => {
  const canvas = document.createElement("canvas")
  canvas.width = 480
  canvas.height = 480
  const draw = () => {
    const ctx = canvas.getContext("2d")
    if (!ctx) return
    ctx.fillStyle = "#2a2d2b"
    ctx.fillRect(0, 0, 480, 480)
    ctx.fillStyle = "#e8e6e1"
    ctx.fillRect(120, 120, 240, 240)
    ctx.fillStyle = "#1b1d1c"
    for (let y = 0; y < 10; y++) for (let x = 0; x < 10; x++) if ((x * 7 + y * 3 + x * y) % 3 === 0) ctx.fillRect(136 + x * 21, 136 + y * 21, 19, 19)
  }
  draw()
  window.setInterval(draw, 200)
  const stream = (canvas as HTMLCanvasElement & { captureStream: (fps?: number) => MediaStream }).captureStream(10)
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { getUserMedia: () => Promise.resolve(stream) },
  })
}
