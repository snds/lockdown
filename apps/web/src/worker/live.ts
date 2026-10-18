/**
 * The live channel, held by the web vault's worker
 * (docs/plan/27-web-server-finish.md §2; the server's side is
 * bins/server/src/live.rs, the desktop's crates/app/src/live.rs).
 *
 * While the vault is open and the account has a session, the worker keeps
 * `GET /v1/events` open with `fetch` and reads its server-sent events. An
 * event says only that something changed: on `sync` the worker gathers a
 * burst and runs one sync in the core, with every manifest and
 * key-transparency check a sync has, and the page redraws; `alerts` and
 * `sessions` tell the page to read those again; `session_ended` makes the
 * core forget the token. Nothing the channel carries is trusted: a forged
 * event costs one needless sync.
 *
 * It closes within a second of a lock or a sign-out (the worker shows it
 * the core's answer after every call and every tick), and reopens with a
 * growing pause (2 s to a minute) after a drop. The token stays in the
 * worker: the page never asks for it (`worker_…` calls are refused from
 * the page) and the channel goes only to the page's own origin.
 */

/** What the channel did, for the page: `lockdown_app::live::LiveNotice`. */
export type LiveNotice =
  | { kind: "connected" }
  | { kind: "disconnected"; message: string }
  | { kind: "synced"; pulled: number; lines: string[] }
  | { kind: "sync_failed"; message: string }
  | { kind: "alerts" }
  | { kind: "sessions" }
  | { kind: "session_ended" }

/** Where the channel opens, from the core (`worker_live_target`). */
export interface LiveTarget {
  url: string
  token: string
}

export interface LiveDeps {
  /** The page's origin: the only one the channel may reach. */
  origin: string
  /** The core's target now, run in the worker's call queue. */
  target(): Promise<LiveTarget | null>
  /** One quiet sync in the core, run in the queue. Rejects with the core's error. */
  sync(): Promise<{ pulled: number; lines: string[] }>
  /** The server ended this token's session. */
  ended(token: string): Promise<void>
  notify(notice: LiveNotice): void
  fetch: (url: string, init: RequestInit) => Promise<Response>
}

export interface LiveTiming {
  /** How often it looks whether the vault opened, when it isn't. */
  idleMs: number
  /** How long a burst of events is gathered into one sync. */
  gatherMs: number
  /** No byte for this long (three keep-alives missed) and the stream is dropped. */
  readTimeoutMs: number
  firstPauseMs: number
  maxPauseMs: number
}

export const LIVE_TIMING: LiveTiming = { idleMs: 3000, gatherMs: 250, readTimeoutMs: 65_000, firstPauseMs: 2000, maxPauseMs: 60_000 }

const KINDS = new Set(["sync", "alerts", "sessions", "session_ended"])

/** One event's kind from its `data:` line, or `null` for anything else. */
export function eventKind(data: string): string | null {
  try {
    const v = JSON.parse(data) as { v?: unknown; kind?: unknown }
    return typeof v.kind === "string" && KINDS.has(v.kind) ? v.kind : null
  } catch {
    return null
  }
}

function errorWords(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message)
  return String(e)
}

export class LiveChannel {
  private stopped = false
  private current: LiveTarget | null = null
  private abort: AbortController | null = null
  /** A token the server said was ended: never knock with it again. */
  private endedToken: string | null = null
  private wake: (() => void) | null = null
  private syncDue = false
  private gathering = false
  private running: Promise<void> | null = null
  private readonly t: LiveTiming

  private readonly deps: LiveDeps

  constructor(deps: LiveDeps, timing: Partial<LiveTiming> = {}) {
    this.deps = deps
    this.t = { ...LIVE_TIMING, ...timing }
  }

  /** Starts the loop (once). */
  start(): Promise<void> {
    this.running ??= this.loop()
    return this.running
  }

  /** Stops for good (tests; a worker that stopped after a trap). */
  stop() {
    this.stopped = true
    this.abort?.abort()
    this.wake?.()
  }

  /** Whether a stream is open now. */
  get open(): boolean {
    return this.current !== null
  }

  /**
   * The core's target after a call or a tick: a lock or a sign-out (no
   * target, or another token) closes the stream at once; an unlock wakes
   * the loop instead of waiting for its next look.
   */
  observe(target: LiveTarget | null) {
    if (this.current && (!target || target.token !== this.current.token)) {
      this.current = null
      this.abort?.abort()
    } else if (!this.current && target) {
      this.wake?.()
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => {
      let timer: ReturnType<typeof setTimeout> | undefined = undefined
      const done = () => {
        clearTimeout(timer)
        if (this.wake === done) this.wake = null
        resolve()
      }
      timer = setTimeout(done, ms)
      this.wake = done
    })
  }

  private async loop() {
    let pause = this.t.firstPauseMs
    while (!this.stopped) {
      const target = await this.deps.target().catch(() => null)
      if (!target || target.token === this.endedToken || !this.sameOrigin(target.url)) {
        await this.sleep(this.t.idleMs)
        continue
      }
      const why = await this.stream(target)
      this.current = null
      this.abort = null
      if (why === "ended" || this.stopped) continue
      if (why === "closed") {
        // Closed by a lock or a sign-out: not a drop, no pause.
        continue
      }
      if (why.connected) pause = this.t.firstPauseMs
      this.deps.notify({ kind: "disconnected", message: why.message })
      await this.sleep(pause)
      pause = Math.min(pause * 2, this.t.maxPauseMs)
    }
  }

  private sameOrigin(url: string): boolean {
    try {
      return new URL(url).origin === this.deps.origin
    } catch {
      return false
    }
  }

  /** One stream, until it drops (`{message}`), is closed here, or the session ended. */
  private async stream(target: LiveTarget): Promise<"closed" | "ended" | { message: string; connected: boolean }> {
    const abort = new AbortController()
    this.abort = abort
    this.current = target
    let res: Response
    try {
      res = await this.deps.fetch(target.url, {
        method: "GET",
        headers: { Authorization: `Bearer ${target.token}`, Accept: "text/event-stream" },
        credentials: "omit",
        cache: "no-store",
        redirect: "error",
        signal: abort.signal,
      })
    } catch (e) {
      if (abort.signal.aborted) return "closed"
      return { message: `couldn't open the live channel: ${errorWords(e)}`, connected: false }
    }
    if (res.status === 401) {
      this.endedToken = target.token
      void res.body?.cancel().catch(() => undefined)
      return "ended"
    }
    if (!res.ok || !res.body) {
      void res.body?.cancel().catch(() => undefined)
      return { message: `the server refused the live channel (HTTP ${res.status})`, connected: false }
    }
    this.deps.notify({ kind: "connected" })
    const reader = res.body.getReader()
    // An abort (a lock, a sign-out, the read timeout) ends the read in hand.
    abort.signal.addEventListener("abort", () => void reader.cancel().catch(() => undefined), { once: true })
    const decoder = new TextDecoder()
    let buffered = ""
    let data: string[] = []
    let timer: ReturnType<typeof setTimeout> | null = null
    const arm = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => abort.abort(), this.t.readTimeoutMs)
    }
    arm()
    try {
      for (;;) {
        let chunk: ReadableStreamReadResult<Uint8Array>
        let failed: unknown = null
        try {
          chunk = await reader.read()
        } catch (e) {
          failed = e
          chunk = { done: true, value: undefined }
        }
        if (abort.signal.aborted) {
          if (this.current !== target) return "closed"
          return { message: "the live channel went quiet", connected: true }
        }
        if (failed) return { message: `the live channel dropped: ${errorWords(failed)}`, connected: true }
        if (chunk.done) return { message: "the server closed the live channel", connected: true }
        arm()
        buffered += decoder.decode(chunk.value, { stream: true })
        // Lines end in \n (or \r\n); a blank line ends one event.
        let nl: number
        while ((nl = buffered.indexOf("\n")) >= 0) {
          const line = buffered.slice(0, nl).replace(/\r$/, "")
          buffered = buffered.slice(nl + 1)
          if (buffered.length > 64 * 1024) buffered = ""
          if (line === "") {
            const kind = data.length ? eventKind(data.join("\n")) : null
            data = []
            if (kind && (await this.dispatch(kind, target)) === "ended") return "ended"
          } else if (line.startsWith("data:")) {
            if (data.length < 16) data.push(line.slice(5).trimStart())
          }
          // `event:` names and `:` comments (keep-alives) carry nothing else.
        }
      }
    } finally {
      if (timer) clearTimeout(timer)
      void reader.cancel().catch(() => undefined)
    }
  }

  private async dispatch(kind: string, target: LiveTarget): Promise<"ended" | void> {
    switch (kind) {
      case "sync":
        this.syncDue = true
        if (!this.gathering) void this.gatherThenSync()
        return
      case "alerts":
        this.deps.notify({ kind: "alerts" })
        return
      case "sessions":
        this.deps.notify({ kind: "sessions" })
        return
      case "session_ended":
        this.endedToken = target.token
        await this.deps.ended(target.token).catch(() => undefined)
        this.deps.notify({ kind: "session_ended" })
        return "ended"
    }
  }

  /** Folds a burst of `sync` events into one sync. */
  private async gatherThenSync() {
    this.gathering = true
    try {
      while (this.syncDue && !this.stopped) {
        await new Promise((r) => setTimeout(r, this.t.gatherMs))
        this.syncDue = false
        try {
          const out = await this.deps.sync()
          this.deps.notify({ kind: "synced", pulled: out.pulled, lines: out.lines })
        } catch (e) {
          const code = e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : ""
          // Locked meanwhile: the next unlock's sync covers it.
          if (code !== "locked" && code !== "stopped") this.deps.notify({ kind: "sync_failed", message: errorWords(e) })
        }
      }
    } finally {
      this.gathering = false
    }
  }
}
