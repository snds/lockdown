// What `@tauri-apps/api/core` is in the harness (vite.config.ts aliases it):
// every desktop client's `invoke` lands in the fake core. Byte answers come
// back as `Uint8Array`, which the clients read as they read an ArrayBuffer.

import { call, type Args } from "./core"

export function invoke<T>(cmd: string, args?: unknown, options?: unknown): Promise<T> {
  void options
  // A raw body (an attachment, a camera frame) arrives as bytes, not as named arguments.
  const named = args && typeof args === "object" && !(args instanceof Uint8Array) && !Array.isArray(args) ? (args as Args) : { body: args }
  return call<T>(cmd, named)
}
