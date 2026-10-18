/// <reference lib="webworker" />
import wasmUrl from "../../wasm/lockdown_web.wasm?url"

import { loadCarried, saveCarried } from "./carried"
import { Core, CoreFailure, wipe } from "./core"
import { sameOriginSend } from "./http"
import type { Request, Response } from "./protocol"

/**
 * The web vault's dedicated worker: the core runs here, never on the page's
 * thread, so the page holds no key and a synchronous request blocks only
 * this thread. One worker per tab; when the tab goes, the worker and every
 * key in its memory go with it (docs/plan/17-web-vault.md §3).
 *
 * The module is fetched from this origin and its SHA-384 checked against
 * the hash the build baked into this script before it is compiled, the
 * module-level twin of the Subresource Integrity on the page's own tags.
 */

declare const __LOCKDOWN_WASM_SHA384__: string
declare const self: DedicatedWorkerGlobalScope

const TICK_MS = 1000

function post(message: Response, transfer: Transferable[] = []) {
  self.postMessage(message, transfer)
}

function base64(bytes: ArrayBuffer): string {
  let s = ""
  for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b)
  return btoa(s)
}

/** The browser, for the device's name in the identity's device list. */
function browserName(ua: string): string {
  if (/Edg\//.test(ua)) return "Edge"
  if (/Firefox\//.test(ua)) return "Firefox"
  if (/Chrome\//.test(ua) || /Chromium\//.test(ua)) return "Chrome"
  if (/Safari\//.test(ua)) return "Safari"
  return "a browser"
}

async function start(): Promise<Core> {
  const res = await fetch(wasmUrl, { credentials: "omit" })
  if (!res.ok) throw new Error(`couldn't load the vault's core (HTTP ${res.status})`)
  const bytes = await res.arrayBuffer()
  const digest = base64(await crypto.subtle.digest("SHA-384", bytes))
  if (!__LOCKDOWN_WASM_SHA384__ || digest !== __LOCKDOWN_WASM_SHA384__) {
    throw new Error("the vault's core didn't match the one this page was built with; nothing was run")
  }
  const module = await WebAssembly.compile(bytes)
  return Core.start({
    module,
    deviceName: `Lockdown web vault · ${browserName(navigator.userAgent)}`,
    origin: self.location.origin,
    send: sameOriginSend(self.location.origin),
  })
}

let core: Core | null = null
let stopped: string | null = null
/** Calls run one at a time, in order, whatever awaits sit between them. */
let queue: Promise<void> = Promise.resolve()

/** Hands on what this visit carries, for the next one in this browser. */
async function keepCarried(c: Core, login: unknown) {
  if (typeof login !== "string") return
  try {
    const reply = c.call("web_carried")
    if (reply.kind === "bytes") await saveCarried(login, reply.value)
  } catch {
    // Nothing to carry (signed out, locked before the list was read).
  }
}

function signedInLogin(c: Core): unknown {
  try {
    const reply = c.call("web_state")
    return reply.kind === "json" ? (reply.value as { login?: unknown }).login : null
  } catch {
    return null
  }
}

async function handle(req: Request) {
  if (!core) {
    post({ id: req.id, ok: false, error: { code: "stopped", message: stopped ?? "the vault is still starting" } })
    return
  }
  const c = core
  const args = { ...req.args }
  try {
    if (req.cmd === "web_sign_in" && typeof args.login === "string") {
      const carried = await loadCarried(args.login)
      if (carried) args.carried = Array.from(carried)
    }
    if (req.cmd === "lock" || req.cmd === "web_sign_out") {
      await keepCarried(c, signedInLogin(c))
    }
    const reply = c.call(req.cmd, args)
    if (["web_sign_in", "web_sign_in_answer", "web_sign_in_continue", "web_join_finish", "sync"].includes(req.cmd)) {
      await keepCarried(c, signedInLogin(c))
    }
    if (reply.kind === "bytes") post({ id: req.id, ok: true, kind: "bytes", value: reply.value }, [reply.value.buffer])
    else post({ id: req.id, ok: true, kind: "json", value: reply.value })
  } catch (e) {
    if (e instanceof CoreFailure) {
      post({ id: req.id, ok: false, error: e.error })
    } else {
      // A trap inside the core (a panic): the instance can't be trusted to
      // continue, so every key it held is dropped with it.
      stopped = "the vault stopped after an internal error; reload the page and sign in again"
      core?.fs.clear()
      core = null
      post({ id: req.id, ok: false, error: { code: "stopped", message: stopped } })
    }
  } finally {
    for (const v of Object.values(args)) if (v instanceof Uint8Array) wipe(v)
  }
}

self.onmessage = (event: MessageEvent<Request>) => {
  const req = event.data
  queue = queue.then(() => handle(req))
}

start()
  .then((c) => {
    core = c
    post({ id: 0, event: "ready" })
    setInterval(() => {
      queue = queue.then(() => {
        if (!core) return
        try {
          core.call("tick")
        } catch {
          // A tick that fails changes nothing; the next one tries again.
        }
      })
    }, TICK_MS)
  })
  .catch((e: unknown) => {
    stopped = e instanceof Error ? e.message : String(e)
    post({ id: 0, event: "failed", message: stopped })
  })
