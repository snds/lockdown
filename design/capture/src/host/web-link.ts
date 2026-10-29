// The web vault's worker link, faked: the page's `Link` (apps/web/src/core/link.ts)
// answered by the same fake core the desktop uses, so the web vault's own
// clients (web-client.ts, web-depth-client.ts, …) run unchanged above it.

import type { Link } from "../../../../apps/web/src/core/link"
import type { ClipboardLike } from "../../../../apps/web/src/core/clipboard"

import { call, type Args } from "./core"

const enc = new TextEncoder()

export const fakeLink: Link = {
  call: <T,>(cmd: string, args?: Record<string, unknown>) => call<T>(cmd, (args ?? {}) as Args),
  callBytes: async (cmd: string, args?: Record<string, unknown>) => {
    const v = await call<unknown>(cmd, (args ?? {}) as Args)
    return v instanceof Uint8Array ? v : enc.encode(String(v ?? ""))
  },
  onLive: () => () => {},
}

/** The clipboard the web vault writes to: kept in the page, never the system's. */
export function fakeClipboard(): ClipboardLike {
  let text = ""
  return {
    writeText: (t) => {
      text = t
      return Promise.resolve()
    },
    readText: () => Promise.resolve(text),
  }
}

/** WebAuthn that never answers: a key page waiting on the person's key. */
export const waitingCredentials = {
  get: () => new Promise<Credential | null>(() => {}),
  create: () => new Promise<Credential | null>(() => {}),
  preventSilentAccess: () => Promise.resolve(),
  store: () => Promise.resolve(),
} as unknown as CredentialsContainer
