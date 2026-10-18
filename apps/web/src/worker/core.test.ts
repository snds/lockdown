// @vitest-environment node
import { existsSync, readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { Core, CoreFailure, type HttpMeta } from "./core"

/**
 * The real core module (apps/web/wasm, from `pnpm build:wasm`) on this
 * worker shim, in Node: the WASI file system, the clock, the command
 * table and the HTTP import, with no browser and no server. CI builds the
 * module before this suite runs.
 */

const wasm = path.resolve(import.meta.dirname, "../../wasm/lockdown_web.wasm")
const built = existsSync(wasm)

async function start(send = (_meta: HttpMeta, _body: Uint8Array) => ({ status: 0, body: new TextEncoder().encode("offline") })) {
  const module = await WebAssembly.compile(readFileSync(wasm))
  const log: string[] = []
  const core = await Core.start({ module, deviceName: "Lockdown web vault · test", send, log: (l) => log.push(l) })
  return { core, log }
}

function refusal(f: () => unknown): CoreFailure {
  try {
    f()
  } catch (e) {
    if (e instanceof CoreFailure) return e
    throw e
  }
  throw new Error("expected a refusal")
}

describe.runIf(built)("the core module on the worker's WASI", () => {
  it("starts signed out and locked, with the desktop's lock defaults", async () => {
    const { core } = await start()
    expect(core.call("web_state")).toEqual({ kind: "json", value: { signed_in: false, locked: true, server: null, login: null } })
    const lock = core.call("lock_state")
    expect(lock.kind).toBe("json")
    expect(lock.value).toMatchObject({ locked: true, idle_timeout_secs: 600, hidden_timeout_secs: 300 })
    expect(core.call("tick")).toEqual({ kind: "json", value: true })
  })

  it("answers a vault call while locked with the app's own error", async () => {
    const { core } = await start()
    const e = refusal(() => core.call("list"))
    expect(e.error.code).toBe("locked")
    const unknown = refusal(() => core.call("export_save"))
    expect(unknown.error.message).toContain("no command 'export_save'")
  })

  it("sends its requests through the import, to the URL it was given", async () => {
    const seen: HttpMeta[] = []
    const { core } = await start((meta) => {
      seen.push(meta)
      return { status: 0, body: new TextEncoder().encode("no network in this test") }
    })
    const e = refusal(() => core.call("web_sign_in", { server: "http://127.0.0.1:9", login: "me@example.com", password: "a long password" }))
    expect(e.error.message).toContain("no network in this test")
    expect(seen[0]).toMatchObject({ method: "POST", url: "http://127.0.0.1:9/v1/account/login/start", contentType: "application/cbor" })
    expect(seen[0]!.authorization).toBeNull()
  })

  it("clears the tab's files between sign-ins but keeps one pending device key", async () => {
    const { core } = await start()
    // A file the core writes itself (a temporary file, then a rename).
    core.call("set_settings", { settings: { ...(core.call("get_settings").value as object), density: "compact" } })
    expect(core.fs.readFile("/lockdown/data/settings.cbor")).not.toBeNull()
    core.fs.writeFile("/lockdown/identity/personas/p.cbor", new Uint8Array([1, 2, 3]))
    core.fs.writeFile("/lockdown/data/sync.cbor", new Uint8Array([4, 5, 6]))
    core.fs.writeFile("/lockdown/data/device-key.cbor", new Uint8Array([7, 8, 9]))
    refusal(() => core.call("web_sign_in", { server: "http://127.0.0.1:9", login: "me@example.com", password: "a long password" }))
    // The worker's ticks between two attempts leave the pending key alone.
    core.call("tick")
    core.call("lock_state")
    expect(core.fs.readFile("/lockdown/identity/personas/p.cbor")).toBeNull()
    expect(core.fs.readFile("/lockdown/data/sync.cbor")).toBeNull()
    expect(core.fs.readFile("/lockdown/data/settings.cbor")).toBeNull()
    expect(core.fs.readFile("/lockdown/data/device-key.cbor")).toEqual(new Uint8Array([7, 8, 9]))
  })

  it("rejects a malformed field change before anything is written", async () => {
    const { core } = await start()
    const e = refusal(() => core.call("add_item", { kind: "login", fields: [{ field_id: "pw", action: "copy" }] }))
    expect(e.error.message).toMatch(/isn't an action|locked/)
  })
})
