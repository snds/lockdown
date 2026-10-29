import type { ReactNode } from "react"

import type { Handler } from "../host/core"
import type { World } from "../fixture/world"

export type Surface = "desktop" | "web" | "extension" | "server"
export type Theme = "light" | "dark"
export type Density = "comfortable" | "compact"

/**
 * One thing the driver does after the state mounts, in the page itself, so
 * the review build and the capture land on the same screen from the same
 * address. Text matches an element's accessible name (its `aria-label`, or
 * its text), exactly for a string, by pattern for a RegExp.
 */
export type Step =
  | { click: string | RegExp; role?: string; nth?: number; within?: string }
  | { fill: string | RegExp; value: string }
  | { key: string; target?: string }
  | { hold: string | RegExp }
  | { wait: string | RegExp; role?: string }
  | { gone: string | RegExp }
  /** Waits for any visible element matching a CSS selector (a dialog, a menu). */
  | { see: string }
  | { pause: number }
  | { emit: string; payload: unknown }
  | { run: () => void | Promise<void> }

export interface StateDef {
  /** Lower-case and hyphenated; also the review build's address (`#/<id>`). */
  id: string
  surface: Surface
  /** The screen's name, as the app names it. */
  screen: string
  /** Which of its states this is. */
  state: string
  /** The design brief §3.2 row it belongs to, or `null` when the brief has none. */
  brief_row: string | null
  notes: string
  /** The widths captured; a popup or an in-page frame is captured at its natural size instead. */
  widths: number[] | "natural"
  /** Compact density is captured too (the vault list only). */
  compact?: boolean
  /** Changes the fixture before anything mounts. */
  setup?: (world: World) => void
  /** This state's own answers over the fixture's. */
  answers?: (world: World) => Record<string, Handler>
  mount: () => ReactNode
  steps?: Step[]
  /** The element the capture frames when the width is natural. */
  frame?: string
  /**
   * Buttons in an extension in-page frame the capture presses with real
   * (trusted) clicks: the frames ignore synthetic ones by design, so the
   * review build leaves these taps to the reviewer.
   */
  trusted?: string[]
  /**
   * An extension state captured with the clock held still, so an in-page
   * frame stays in its 400 ms arming wait (design review R17) instead of
   * arming before the picture is taken. The review build arms as usual.
   */
  hold_clock?: boolean
}

/** What the capture and the start screen read: everything but the code. */
export interface StateMeta {
  id: string
  surface: Surface
  screen: string
  state: string
  brief_row: string | null
  notes: string
  widths: number[] | "natural"
  compact: boolean
  frame: string | null
  trusted: string[]
  hold_clock: boolean
}

export const WIDTHS = [1280, 900, 390]
