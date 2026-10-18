import { describe, expect, it, vi } from "vitest"

import type { Link } from "./link"
import { webHygieneClient, webQuickClient, webScanClient, webSharingClient } from "./web-finish-client"

function fakeLink(answers: Record<string, unknown> = {}) {
  const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = []
  const link: Link = {
    call: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args })
      return (answers[cmd] ?? null) as never
    }),
    callBytes: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args })
      return new TextEncoder().encode("123456")
    }),
  }
  return { link, calls }
}

describe("docs/plan/26-desktop-finish.md over the worker", () => {
  it("asks the core's index and shows a code in place, never filling another tab", async () => {
    const { link, calls } = fakeLink()
    const quick = webQuickClient(link)
    await quick.search("git", 20)
    const code = await quick.quickCode("c1", "code.0", { password: "pw" })
    expect(new TextDecoder().decode(code)).toBe("123456")
    expect(calls).toEqual([
      { cmd: "quick_search", args: { query: "git", limit: 20 } },
      { cmd: "quick_code", args: { itemId: "c1", fieldId: "code.0", gate: { password: "pw" } } },
    ])
    expect(quick.canFill).toBe(false)
    await expect(quick.quickFill("l1")).rejects.toMatchObject({ code: "refused" })
  })

  it("sends grey frames to the core and opens only an https page it approved", async () => {
    const started = { url: "https://www.youtube.com/activate?user_code=WDJB-MJHT", host: "www.youtube.com", service: "YouTube", code: "WDJB-MJHT", code_in_url: true }
    const { link, calls } = fakeLink({ activation_approve: started })
    const open = vi.fn()
    const scan = webScanClient(link, open)
    await scan.frame({ width: 2, height: 1, luma: new Uint8Array([0, 255]) }, "camera")
    await scan.text("pasted")
    expect(await scan.approve("act-1", "l1")).toEqual(started)
    await scan.contactFrame({ width: 1, height: 1, luma: new Uint8Array([9]) })
    expect(calls.map((c) => c.cmd)).toEqual(["scan_frame", "scan_text", "activation_approve", "qr_contact_frame"])
    expect(calls[0]?.args).toEqual({ width: 2, height: 1, luma: [0, 255], source: "camera" })
    expect(open).toHaveBeenCalledWith(started.url)
  })

  it("looks people up and compares through the core; sharing stays the desktop's", async () => {
    const { link, calls } = fakeLink()
    const sharing = webSharingClient(link)
    await sharing.lookup("bob@example.com")
    await sharing.verify("acc-bob", "lockdown:contact:v1:acc-bob:00")
    await sharing.forget("acc-bob")
    expect(calls.map((c) => c.cmd)).toEqual(["contact_lookup", "contact_verify", "contact_forget"])
    await expect(sharing.shareVault("v", "bob", "view")).rejects.toMatchObject({ code: "refused" })
    expect(await webHygieneClient(link).protect(true)).toBe(false)
  })
})
