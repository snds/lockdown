import type { DepthClient } from "@/features/depth/depth-client"

/** A security-depth client whose every method refuses unless the test gives it one. */
export function fakeDepth(overrides: Partial<DepthClient>): DepthClient {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as DepthClient, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}
