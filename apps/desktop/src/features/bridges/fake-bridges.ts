import type { BridgesClient } from "./bridges-client"

/** A bridges client whose every method refuses unless the test gives it one. */
export function fakeBridges(overrides: Partial<BridgesClient>): BridgesClient {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as BridgesClient, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}
