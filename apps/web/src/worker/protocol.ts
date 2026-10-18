import type { CoreError } from "./core"
import type { LiveNotice } from "./live"

/** A call from the page to the worker. */
export interface Request {
  id: number
  cmd: string
  args: Record<string, unknown>
}

/** The worker's answer to one call, or one of its own notices (`id: 0`). */
export type Response =
  | { id: number; ok: true; kind: "json"; value: unknown }
  | { id: number; ok: true; kind: "bytes"; value: Uint8Array }
  | { id: number; ok: false; error: CoreError }
  | { id: 0; event: "ready" }
  | { id: 0; event: "failed"; message: string }
  | { id: 0; event: "live"; notice: LiveNotice }
