import type { CoreError } from "../worker/core"
import type { Request, Response } from "../worker/protocol"

/** What the page needs from the worker: one call in, one answer out. */
export interface Link {
  call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
  callBytes(cmd: string, args?: Record<string, unknown>): Promise<Uint8Array>
}

interface Pending {
  resolve: (value: { kind: "json" | "bytes"; value: unknown }) => void
  reject: (error: CoreError) => void
}

/**
 * The page's side of the worker. Every answer is matched to its call by
 * id; a refusal arrives as lockdown-app's `AppError` shape, so the
 * desktop's `isAppError` and `describeError` read it unchanged.
 */
export class WorkerLink implements Link {
  private next = 1
  private pending = new Map<number, Pending>()
  private readonly worker: Worker
  /** Resolves when the core is running; rejects with why it couldn't start. */
  readonly ready: Promise<void>

  constructor(worker: Worker) {
    this.worker = worker
    let ready: () => void = () => {}
    let failed: (e: CoreError) => void = () => {}
    this.ready = new Promise<void>((res, rej) => {
      ready = res
      failed = rej
    })
    worker.onmessage = (event: MessageEvent<Response>) => {
      const msg = event.data
      if ("event" in msg) {
        if (msg.event === "ready") ready()
        else failed({ code: "stopped", message: msg.message })
        return
      }
      const p = this.pending.get(msg.id)
      if (!p) return
      this.pending.delete(msg.id)
      if (msg.ok) p.resolve({ kind: msg.kind, value: msg.value })
      else p.reject(msg.error)
    }
  }

  /** Waits for the core to be running, then sends; a core that couldn't start rejects every call with why. */
  private async send(cmd: string, args: Record<string, unknown>) {
    await this.ready
    const id = this.next++
    return new Promise<{ kind: "json" | "bytes"; value: unknown }>((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      const req: Request = { id, cmd, args }
      this.worker.postMessage(req)
    })
  }

  async call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
    const reply = await this.send(cmd, args)
    if (reply.kind === "bytes") {
      const bytes = reply.value as Uint8Array
      bytes.fill(0)
      throw { code: "refused", message: `${cmd} answered with bytes where words were expected` } satisfies CoreError
    }
    return reply.value as T
  }

  async callBytes(cmd: string, args: Record<string, unknown> = {}): Promise<Uint8Array> {
    const reply = await this.send(cmd, args)
    if (reply.kind !== "bytes") {
      throw { code: "refused", message: `${cmd} answered with words where bytes were expected` } satisfies CoreError
    }
    return reply.value as Uint8Array
  }
}
