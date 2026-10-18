// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { HttpMeta } from "./core"
import { isMirrorRead } from "./http"

const get = (authorization: string | null = null): HttpMeta => ({ method: "GET", url: "", contentType: "application/cbor", authorization })

describe("the worker's one cross-origin read", () => {
  it("is a mirror's public head: GET, no token, the mirror path only", () => {
    expect(isMirrorRead(new URL("https://mirror.example/v1/kt/mirror"), get())).toBe(true)
    expect(isMirrorRead(new URL("http://127.0.0.1:9000/v1/kt/mirror"), get())).toBe(true)
    // Anything else stays on the page's own origin.
    expect(isMirrorRead(new URL("https://mirror.example/v1/kt/mirror"), get("Bearer t"))).toBe(false)
    expect(isMirrorRead(new URL("https://mirror.example/v1/vaults"), get())).toBe(false)
    expect(isMirrorRead(new URL("https://mirror.example/v1/kt/mirror?x=1"), get())).toBe(false)
    expect(isMirrorRead(new URL("http://mirror.example/v1/kt/mirror"), get())).toBe(false)
    expect(isMirrorRead(new URL("https://u:p@mirror.example/v1/kt/mirror"), get())).toBe(false)
    expect(isMirrorRead(new URL("https://mirror.example/v1/kt/mirror"), { ...get(), method: "POST" })).toBe(false)
  })
})
