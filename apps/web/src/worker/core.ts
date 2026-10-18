import { MemoryFs, Wasi } from "./wasi"

/**
 * The core module, instantiated: `lockdown-web` (crates/web), which is
 * `lockdown-app` built for `wasm32-wasip1`. See crates/web/src/abi.rs for
 * the exports and imports this side implements.
 */

/** One HTTP request the core asks for. */
export interface HttpMeta {
  method: "GET" | "PUT" | "POST" | "HEAD"
  url: string
  contentType: string
  authorization: string | null
}

/** The answer: a status and a body, or `status: 0` and why there was none. */
export interface HttpAnswer {
  status: number
  body: Uint8Array
}

/** Makes one request, synchronously (the core waits for it). */
export type HttpSend = (meta: HttpMeta, body: Uint8Array) => HttpAnswer

/** An error the core answered with: lockdown-app's `AppError`. */
export interface CoreError {
  code: string
  message: string
  detail?: string
}

export type CoreReply = { kind: "json"; value: unknown } | { kind: "bytes"; value: Uint8Array }

interface Exports {
  memory: WebAssembly.Memory
  _initialize?: () => void
  ld_init: () => void
  ld_alloc: (len: number) => number
  ld_free: (ptr: number, len: number) => void
  ld_call: (ptr: number, len: number) => bigint
}

export class CoreFailure extends Error {
  readonly error: CoreError
  constructor(error: CoreError) {
    super(error.message)
    this.error = error
  }
}

/** Overwrites a buffer with zeros. */
export function wipe(bytes: Uint8Array) {
  bytes.fill(0)
}

export interface CoreOptions {
  /** The compiled module. */
  module: WebAssembly.Module
  /** The device's name in the identity's device list. */
  deviceName: string
  send: HttpSend
  log?: (line: string) => void
}

/** The running core: one per worker, so one per tab. */
export class Core {
  private held: Uint8Array | null = null
  private readonly exports: Exports
  /** The tab's files (the identity root and Lockdown's data directory). */
  readonly fs: MemoryFs

  private constructor(exports: Exports, fs: MemoryFs) {
    this.exports = exports
    this.fs = fs
  }

  static async start(options: CoreOptions): Promise<Core> {
    const fs = new MemoryFs()
    const wasi = new Wasi({
      fs,
      env: { LOCKDOWN_DEVICE_NAME: options.deviceName },
      log: options.log ?? ((line) => console.warn(`[core] ${line}`)),
    })
    let core: Core | null = null
    const lockdown = {
      http_send: (ptr: number, len: number): number => {
        if (!core) return 0
        return core.httpSend(ptr, len, options.send)
      },
      http_take: (ptr: number) => {
        if (!core?.held) return
        new Uint8Array(core.exports.memory.buffer, ptr, core.held.length).set(core.held)
        wipe(core.held)
        core.held = null
      },
    }
    const instance = await WebAssembly.instantiate(options.module, {
      wasi_snapshot_preview1: wasi.imports(),
      lockdown,
    })
    const exports = instance.exports as unknown as Exports
    wasi.bind(exports.memory)
    core = new Core(exports, fs)
    exports._initialize?.()
    exports.ld_init()
    return core
  }

  private httpSend(ptr: number, len: number, send: HttpSend): number {
    const frame = new Uint8Array(this.exports.memory.buffer, ptr, len).slice()
    const view = new DataView(frame.buffer)
    const metaLen = view.getUint32(0, true)
    const meta = JSON.parse(new TextDecoder().decode(frame.subarray(4, 4 + metaLen))) as HttpMeta
    const body = frame.subarray(4 + metaLen)
    let answer: HttpAnswer
    try {
      answer = send(meta, body)
    } catch (e) {
      answer = { status: 0, body: new TextEncoder().encode(e instanceof Error ? e.message : String(e)) }
    }
    wipe(frame)
    const held = new Uint8Array(2 + answer.body.length)
    new DataView(held.buffer).setUint16(0, answer.status, true)
    held.set(answer.body, 2)
    this.held = held
    return held.length
  }

  /** One command (crates/web/src/commands.rs). Throws `CoreFailure` on the core's refusals. */
  call(cmd: string, args: Record<string, unknown> = {}): CoreReply {
    const e = this.exports
    const request = new TextEncoder().encode(JSON.stringify({ cmd, args }))
    const ptr = e.ld_alloc(request.length)
    new Uint8Array(e.memory.buffer, ptr, request.length).set(request)
    wipe(request)
    const packed = e.ld_call(ptr, request.length)
    e.ld_free(ptr, request.length)
    const outPtr = Number(packed >> 32n)
    const outLen = Number(packed & 0xffff_ffffn)
    const out = new Uint8Array(e.memory.buffer, outPtr, outLen).slice()
    e.ld_free(outPtr, outLen)
    const tag = out[0]
    const payload = out.subarray(1)
    if (tag === 1) {
      const bytes = payload.slice()
      wipe(out)
      return { kind: "bytes", value: bytes }
    }
    const value: unknown = JSON.parse(new TextDecoder().decode(payload))
    wipe(out)
    if (tag === 2) throw new CoreFailure(value as CoreError)
    return { kind: "json", value }
  }
}
