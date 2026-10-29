// What `@tauri-apps/api/event` is in the harness: window events on a small
// bus, so a state can raise one (a live-channel notice, an item opened from
// the quick-access window) the way the shell would.

export interface Event<T> {
  event: string
  id: number
  payload: T
}

export type UnlistenFn = () => void

type Listener = (e: Event<unknown>) => void

const listeners = new Map<string, Set<Listener>>()
let next = 1

export function listen<T>(event: string, handler: (e: Event<T>) => void): Promise<UnlistenFn> {
  const set = listeners.get(event) ?? new Set<Listener>()
  listeners.set(event, set)
  const l = handler as Listener
  set.add(l)
  return Promise.resolve(() => {
    set.delete(l)
  })
}

/** Raises `event` with `payload` to everything listening, as the shell does. */
export function emit<T>(event: string, payload: T): Promise<void> {
  for (const l of listeners.get(event) ?? []) l({ event, id: next++, payload })
  return Promise.resolve()
}
