// @vitest-environment node
import { describe, expect, it } from "vitest"

import { eventKind, LiveChannel, type LiveDeps, type LiveNotice, type LiveTarget } from "./live"

const ORIGIN = "https://vault.example"
const TARGET: LiveTarget = { url: `${ORIGIN}/v1/events`, token: "t1" }
const FAST = { idleMs: 10, gatherMs: 20, readTimeoutMs: 400, firstPauseMs: 10, maxPauseMs: 40 }

/** A server-sent-events body the test writes into. */
class Stream {
  private controller!: ReadableStreamDefaultController<Uint8Array>
  readonly body = new ReadableStream<Uint8Array>({ start: (c) => void (this.controller = c) })
  write(text: string) {
    this.controller.enqueue(new TextEncoder().encode(text))
  }
  event(kind: string) {
    this.write(`event: lockdown\ndata: {"v":1,"kind":"${kind}"}\n\n`)
  }
  end() {
    this.controller.close()
  }
}

interface Harness {
  deps: LiveDeps
  notices: LiveNotice[]
  syncs: number
  ended: string[]
  requests: { url: string; init: RequestInit }[]
  target: LiveTarget | null
  streams: Stream[]
  /** The status the next connection answers with. */
  status: number
}

function harness(): Harness {
  const h: Harness = {
    notices: [],
    syncs: 0,
    ended: [],
    requests: [],
    target: TARGET,
    streams: [],
    status: 200,
    deps: null as unknown as LiveDeps,
  }
  h.deps = {
    origin: ORIGIN,
    target: async () => h.target,
    sync: async () => {
      h.syncs++
      return { pulled: 1, lines: ["pulled 1 item"] }
    },
    ended: async (token) => void h.ended.push(token),
    notify: (n) => void h.notices.push(n),
    fetch: async (url, init) => {
      h.requests.push({ url, init })
      if (h.status !== 200) return new Response(null, { status: h.status })
      const s = new Stream()
      h.streams.push(s)
      init.signal?.addEventListener("abort", () => {
        try {
          s.body.cancel().catch(() => undefined)
        } catch {
          // already closed
        }
      })
      return new Response(s.body, { status: 200, headers: { "content-type": "text/event-stream" } })
    },
  }
  return h
}

async function until(what: () => boolean, ms = 2000) {
  const deadline = Date.now() + ms
  while (!what()) {
    if (Date.now() > deadline) throw new Error("timed out")
    await new Promise((r) => setTimeout(r, 5))
  }
}

describe("the web vault's live channel", () => {
  it("opens with the token as a bearer, to this origin only, and folds a burst of events into one sync", async () => {
    const h = harness()
    const live = new LiveChannel(h.deps, FAST)
    void live.start()
    await until(() => h.streams.length === 1)
    const req = h.requests[0]!
    expect(req.url).toBe(TARGET.url)
    expect((req.init.headers as Record<string, string>).Authorization).toBe("Bearer t1")
    expect(req.init.credentials).toBe("omit")
    await until(() => h.notices.some((n) => n.kind === "connected"))
    const s = h.streams[0]!
    s.write(": keep-alive\n\n")
    s.event("sync")
    s.event("sync")
    s.event("sync")
    await until(() => h.notices.some((n) => n.kind === "synced"))
    await new Promise((r) => setTimeout(r, 60))
    expect(h.syncs).toBe(1)
    s.event("alerts")
    s.event("sessions")
    await until(() => h.notices.some((n) => n.kind === "sessions"))
    expect(h.notices.map((n) => n.kind)).toEqual(["connected", "synced", "alerts", "sessions"])
    live.stop()
  })

  it("closes at once on a lock and opens again on the next unlock", async () => {
    const h = harness()
    const live = new LiveChannel(h.deps, FAST)
    void live.start()
    await until(() => live.open && h.streams.length === 1)
    h.target = null
    live.observe(null)
    expect(live.open).toBe(false)
    await new Promise((r) => setTimeout(r, 50))
    expect(h.streams.length).toBe(1)
    // A lock is not a drop: no "disconnected" and no pause.
    expect(h.notices.some((n) => n.kind === "disconnected")).toBe(false)
    h.target = { ...TARGET, token: "t2" }
    live.observe(h.target)
    await until(() => h.streams.length === 2)
    expect((h.requests[1]!.init.headers as Record<string, string>).Authorization).toBe("Bearer t2")
    live.stop()
  })

  it("reconnects after a drop with a growing pause, and says so", async () => {
    const h = harness()
    const live = new LiveChannel(h.deps, FAST)
    void live.start()
    await until(() => h.streams.length === 1)
    h.streams[0]!.end()
    await until(() => h.notices.some((n) => n.kind === "disconnected"))
    await until(() => h.streams.length === 2)
    // A stream that goes quiet past the read timeout is dropped too.
    await until(() => h.streams.length === 3, 3000)
    expect(h.notices.filter((n) => n.kind === "disconnected").length).toBeGreaterThanOrEqual(2)
    live.stop()
  })

  it("stops knocking with a token the server ended", async () => {
    const h = harness()
    const live = new LiveChannel(h.deps, FAST)
    void live.start()
    await until(() => h.streams.length === 1)
    h.streams[0]!.event("session_ended")
    await until(() => h.notices.some((n) => n.kind === "session_ended"))
    expect(h.ended).toEqual(["t1"])
    await new Promise((r) => setTimeout(r, 80))
    expect(h.requests.length).toBe(1)
    // And a 401 on opening is the same answer.
    h.target = { ...TARGET, token: "t3" }
    h.status = 401
    await until(() => h.requests.length === 2)
    await new Promise((r) => setTimeout(r, 80))
    expect(h.requests.length).toBe(2)
    live.stop()
  })

  it("never opens a channel to another origin, and reads nothing it doesn't know", async () => {
    const h = harness()
    h.target = { url: "https://elsewhere.example/v1/events", token: "t1" }
    const live = new LiveChannel(h.deps, FAST)
    void live.start()
    await new Promise((r) => setTimeout(r, 60))
    expect(h.requests.length).toBe(0)
    live.stop()
    expect(eventKind('{"v":1,"kind":"sync"}')).toBe("sync")
    expect(eventKind('{"v":1,"kind":"surprise"}')).toBeNull()
    expect(eventKind("not json")).toBeNull()
  })
})
