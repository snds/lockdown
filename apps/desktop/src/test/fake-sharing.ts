import type { SharingClient } from "@/features/sharing/sharing-client"

/** A sharing client whose every method refuses unless the test gives it one. */
export function fakeSharing(overrides: Partial<SharingClient>): SharingClient {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as SharingClient, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}
