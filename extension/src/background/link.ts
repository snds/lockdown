// The background's one link to the Lockdown app: a native-messaging port to
// `lockdown-native-host`, which relays to the desktop's socket. The first
// thing on the port is the host's own event: `ready` with a fresh
// challenge, or why there is no app to talk to. Then requests go out with
// an id and answers come back with the same id, one at a time.

import type { LinkState } from "../shared/protocol"
import * as pairkey from "./pairkey"

export const HOST_NAME = "design.snds.lockdown"

/** How long an answer may take (a fill waiting on Touch ID is the longest). */
const ANSWER_MS = 190_000
// The host hashes the app's executable before it relays (process identity,
// D155); a large development build takes seconds to hash.
const READY_MS = 30_000

export class DesktopError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
  }
}

interface Waiting {
  resolve(v: unknown): void
  reject(e: unknown): void
  timer: ReturnType<typeof setTimeout>
}

interface Answer {
  id: number
  ok: boolean
  result?: unknown
  error?: { code: string; message: string }
}

const WORDS: Record<string, string> = {
  no_app: "Lockdown isn't running on this computer. Open it and unlock.",
  refused: "The Lockdown app refused this browser.",
  untrusted_app: "The program on Lockdown's socket isn't the Lockdown app.",
  closed: "The Lockdown app closed the connection.",
  unsupported: "The Lockdown app can't talk to the browser on this system yet.",
}

/** "Chrome on Linux", "Firefox on macOS": what the app's pairing dialog shows. */
export function browserName(): string {
  const ua = navigator.userAgent
  const browser = /Firefox\//.test(ua)
    ? "Firefox"
    : /Edg\//.test(ua)
      ? "Edge"
      : "brave" in navigator
        ? "Brave"
        : /Chrome\//.test(ua)
          ? "Chrome"
          : "A browser"
  const os = /Windows/.test(ua) ? "Windows" : /Mac OS X|Macintosh/.test(ua) ? "macOS" : /Linux|X11/.test(ua) ? "Linux" : "this computer"
  return `${browser} on ${os}`
}

export class Link {
  private port: chrome.runtime.Port | null = null
  private connecting: Promise<void> | null = null
  private waiting = new Map<number, Waiting>()
  private nextId = 1
  private challenge: string | null = null
  private authed = false
  private state: LinkState = { link: "connecting", message: "", paired: false, locked: true, pairing: null, passkeys: false }
  private pairPoll: ReturnType<typeof setInterval> | null = null

  /** Opens the port (once) and waits for the host's `ready`. */
  private connect(): Promise<void> {
    if (this.port && this.challenge) return Promise.resolve()
    if (this.connecting) return this.connecting
    this.connecting = new Promise<void>((resolve, reject) => {
      let settled = false
      const settle = (err?: DesktopError) => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        this.connecting = null
        if (err) reject(err)
        else resolve()
      }
      const timer = setTimeout(() => {
        this.drop("no_app", WORDS.no_app)
        settle(new DesktopError("no_app", WORDS.no_app))
      }, READY_MS)
      let port: chrome.runtime.Port
      try {
        port = chrome.runtime.connectNative(HOST_NAME)
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e)
        this.drop("error", message)
        settle(new DesktopError("error", message))
        return
      }
      this.port = port
      port.onMessage.addListener((msg: Record<string, unknown>) => {
        if (typeof msg.event === "string") {
          if (msg.event === "ready" && typeof msg.challenge === "string") {
            this.challenge = msg.challenge
            this.authed = false
            this.state = { ...this.state, link: "ready", message: "" }
            settle()
          } else {
            const message = typeof msg.message === "string" ? msg.message : (WORDS[msg.event] ?? "The Lockdown app isn't reachable.")
            this.drop(msg.event, message)
            settle(new DesktopError(msg.event, message))
          }
          return
        }
        const answer = msg as unknown as Answer
        const w = this.waiting.get(answer.id)
        if (!w) return
        this.waiting.delete(answer.id)
        clearTimeout(w.timer)
        if (answer.ok) w.resolve(answer.result)
        else w.reject(new DesktopError(answer.error?.code ?? "error", answer.error?.message ?? "the app refused"))
      })
      port.onDisconnect.addListener(() => {
        const why = chrome.runtime.lastError?.message
        if (this.port === port) {
          const link = this.state.link === "ready" || this.state.link === "connecting" ? "no_app" : this.state.link
          this.drop(link, this.state.link === "ready" || this.state.link === "connecting" ? (why && !/exited/.test(why) ? why : WORDS.no_app) : this.state.message)
        }
        settle(new DesktopError(this.state.link, this.state.message || WORDS.no_app))
      })
    })
    return this.connecting
  }

  private drop(link: string, message: string) {
    const port = this.port
    this.port = null
    this.challenge = null
    this.authed = false
    this.state = { ...this.state, link, message, paired: false }
    for (const [, w] of this.waiting) {
      clearTimeout(w.timer)
      w.reject(new DesktopError(link, message))
    }
    this.waiting.clear()
    try {
      port?.disconnect()
    } catch {
      // already gone
    }
  }

  /** Sends one request on the open port. */
  private send<T>(op: string, args: Record<string, unknown> = {}): Promise<T> {
    const port = this.port
    if (!port) return Promise.reject(new DesktopError("no_app", WORDS.no_app))
    const id = this.nextId++
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id)
        reject(new DesktopError("timeout", "the Lockdown app didn't answer in time"))
      }, ANSWER_MS)
      this.waiting.set(id, { resolve: resolve as (v: unknown) => void, reject, timer })
      port.postMessage({ ...args, id, op })
    })
  }

  /** Connects and, when this profile holds a pairing, proves it. */
  private async ensure(): Promise<void> {
    await this.connect()
    if (this.authed || !this.challenge) return
    const pairing = await pairkey.load()
    if (!pairing) return
    try {
      await this.send("auth", { client_id: pairing.clientId, proof: await pairkey.proof(pairing, this.challenge) })
      this.authed = true
      this.state = { ...this.state, paired: true }
    } catch (e) {
      if (e instanceof DesktopError && e.code === "not_paired" && !this.state.pairing) await pairkey.forget()
      this.authed = false
      this.state = { ...this.state, paired: false }
    }
  }

  /** A request that needs the pairing. */
  async call<T>(op: string, args: Record<string, unknown> = {}): Promise<T> {
    await this.ensure()
    if (!this.authed) throw new DesktopError("not_paired", "Pair this browser with Lockdown first: open the Lockdown button in the toolbar.")
    try {
      return await this.send<T>(op, args)
    } catch (e) {
      if (e instanceof DesktopError && e.code === "not_paired") {
        this.authed = false
        this.state = { ...this.state, paired: false }
      }
      if (e instanceof DesktopError && e.code === "locked") this.state = { ...this.state, locked: true }
      throw e
    }
  }

  /** Where the link stands, asking the app when it is reachable. */
  async status(): Promise<LinkState> {
    try {
      await this.ensure()
      const s = await this.send<{ locked: boolean; paired: boolean; passkeys?: boolean }>("status")
      this.state = { ...this.state, link: "ready", message: "", locked: s.locked, paired: s.paired, passkeys: s.passkeys === true }
    } catch (e) {
      if (e instanceof DesktopError && this.state.link === "ready") this.state = { ...this.state, message: e.message }
    }
    return { ...this.state }
  }

  /** Starts pairing: a fresh key, `pair_begin`, and a poll until the
   *  person answers in the app. Returns the state with the code to compare. */
  async pair(): Promise<LinkState> {
    await this.connect()
    await pairkey.forget()
    const { pairing, keyHex } = await pairkey.create()
    const begun = await this.send<{ code: string; request_id: string }>("pair_begin", {
      client_id: pairing.clientId,
      client_key: keyHex,
      name: browserName(),
    })
    this.state = { ...this.state, pairing: { code: begun.code }, message: "" }
    if (this.pairPoll) clearInterval(this.pairPoll)
    const startedAt = Date.now()
    this.pairPoll = setInterval(() => {
      void (async () => {
        try {
          const s = await this.send<{ state: string }>("pair_status", { client_id: pairing.clientId })
          if (s.state === "pending" && Date.now() - startedAt < 125_000) return
          if (this.pairPoll) clearInterval(this.pairPoll)
          this.pairPoll = null
          this.state = { ...this.state, pairing: null }
          if (s.state === "paired") {
            await this.ensure()
          } else {
            await pairkey.forget()
            this.state = { ...this.state, message: s.state === "refused" ? "Pairing was declined in Lockdown." : "The pairing request expired. Try again." }
          }
        } catch {
          if (this.pairPoll) clearInterval(this.pairPoll)
          this.pairPoll = null
          this.state = { ...this.state, pairing: null }
        }
      })()
    }, 1000)
    return { ...this.state }
  }
}
