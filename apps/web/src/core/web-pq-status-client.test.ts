import { describe, expect, it, vi } from "vitest"

import type { PqStatus } from "@/features/security/pq-status-client"

import type { Link } from "./link"
import { webPqStatusClient } from "./web-pq-status-client"

describe("the post-quantum line over the worker", () => {
  it("asks the core under the desktop's command name, with nothing else", async () => {
    const answer: PqStatus = {
      state: "waiting",
      reason: null,
      headline: "Quantum-safe signatures: waiting for Old phone",
      explanation: "This device needs the latest Lockdown. Once it's updated and opened, the stronger signatures switch on by themselves.",
      without_level_1: [{ id: "d1", name: "Old phone" }],
      without_level_2: [],
    }
    const call = vi.fn(async () => answer as never)
    const link: Link = { call, callBytes: vi.fn() }
    expect(await webPqStatusClient(link).status()).toEqual(answer)
    expect(call).toHaveBeenCalledWith("pq_status")
    expect(link.callBytes).not.toHaveBeenCalled()
  })
})
