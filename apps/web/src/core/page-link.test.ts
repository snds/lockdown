import { describe, expect, it } from "vitest"

import { linkFromLocation } from "./page-link"

describe("the link a page was opened for", () => {
  const origin = "https://vault.example"
  const id = "0f6c3a52-7d2e-4f0e-9c1b-2d7f5e8a9b10"

  it("rebuilds a share link and a request link with their fragments", () => {
    expect(linkFromLocation(origin, `/vault/s/${id}`, "#abc_DEF-1")).toEqual({ kind: "share", link: `${origin}/s/${id}#abc_DEF-1` })
    expect(linkFromLocation(origin, `/vault/r/${id}`, "#xyz")).toEqual({ kind: "request", link: `${origin}/r/${id}#xyz` })
  })

  it("is no link without a key, or anywhere else", () => {
    expect(linkFromLocation(origin, `/vault/s/${id}`, "")).toBeNull()
    expect(linkFromLocation(origin, `/vault/s/${id}`, "#")).toBeNull()
    expect(linkFromLocation(origin, "/vault/", "#abc")).toBeNull()
    expect(linkFromLocation(origin, "/vault/s/<script>", "#abc")).toBeNull()
    expect(linkFromLocation(origin, `/vault/x/${id}`, "#abc")).toBeNull()
  })
})
