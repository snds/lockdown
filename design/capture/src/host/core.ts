// The fake core's dispatch: one table from command name to answer, shared
// by every surface. The desktop reaches it through Tauri's `invoke` (see
// tauri-core.ts), the web vault through its worker link (web-link.ts), the
// extension through `chrome.runtime` (chrome.ts). A command with no answer
// refuses in the app's own error shape and is recorded, so a screen that
// asks for something the fixture lacks says so instead of failing quietly.

/** What a command receives: the arguments the client sent. */
export type Args = Record<string, unknown>

/** One command's answer: a value, a promise, or a thrown `AppError`. */
export type Handler = (args: Args) => unknown

export interface AppErrorShape {
  code: "locked" | "not_set_up" | "refused" | "stopped" | "approval_needed"
  message: string
  detail?: string
}

/** Throws the app's own error shape, as lockdown-app would answer. */
export function fail(code: AppErrorShape["code"], message: string, detail?: string): never {
  throw { code, message, ...(detail ? { detail } : {}) } satisfies AppErrorShape
}

/** A promise that never settles: a screen's loading state, held. */
export function forever<T>(): Promise<T> {
  return new Promise<T>(() => {})
}

const table = new Map<string, Handler>()
const overrides = new Map<string, Handler>()
const missing = new Set<string>()
const seen: string[] = []

/** Sets the base answers (the fixture's). */
export function install(handlers: Record<string, Handler>) {
  table.clear()
  for (const [k, v] of Object.entries(handlers)) table.set(k, v)
}

/** A state's own answers, over the base ones, until the page reloads. */
export function override(handlers: Record<string, Handler>) {
  for (const [k, v] of Object.entries(handlers)) overrides.set(k, v)
}

/** The commands asked for that had no answer, for the capture's report. */
export function missingCommands(): string[] {
  return [...missing].sort()
}

/** Every command asked for, in order (for debugging a state). */
export function commandLog(): readonly string[] {
  return seen
}

/** Runs one command, a tick later, as the real host answers asynchronously. */
export function call<T>(cmd: string, args: Args = {}): Promise<T> {
  seen.push(cmd)
  const h = overrides.get(cmd) ?? table.get(cmd)
  if (!h) {
    missing.add(cmd)
    console.warn(`[harness] the fixture has no answer for ${cmd}`)
    return Promise.reject({ code: "refused", message: `The design harness has no answer for ${cmd}.` } satisfies AppErrorShape)
  }
  return new Promise<T>((resolve, reject) => {
    setTimeout(() => {
      try {
        Promise.resolve(h(args)).then((v) => resolve(v as T), reject)
      } catch (e) {
        reject(e)
      }
    }, 0)
  })
}

/** Another command's base answer, for a state that answers one command as another would. */
export function as(cmd: string): Handler {
  return (args) => {
    const h = table.get(cmd)
    if (!h) throw new Error(`the fixture has no ${cmd}`)
    return h(args)
  }
}
