import { describe, expect, it, vi } from "vitest"

import { WebClipboard } from "./clipboard"
import type { Link } from "./link"
import { webClient, webSession } from "./web-client"

function fakeLink(bytesFor: Record<string, string> = {}) {
  const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = []
  const link: Link = {
    call: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args })
      return null as never
    }),
    callBytes: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args })
      return new TextEncoder().encode(bytesFor[cmd] ?? "")
    }),
  }
  return { link, calls }
}

const board = () => {
  let text = ""
  return { writeText: async (t: string) => void (text = t), readText: async () => text, text: () => text }
}

describe("the desktop's client over the worker", () => {
  it("keeps the desktop's command names and argument shapes", async () => {
    const { link, calls } = fakeLink()
    const client = webClient(link, new WebClipboard(board()))
    await client.show("item-1")
    await client.editItem({ id: "item-1", title: "T", changes: [] }, { password: "pw" })
    await client.organize("item-1", { folder: "", favourite: true })
    await client.addItemIn("vault-1", { kind: "login", fields: [] })
    expect(calls).toEqual([
      { cmd: "show", args: { id: "item-1" } },
      { cmd: "edit_item", args: { id: "item-1", title: "T", changes: [], gate: { password: "pw" } } },
      { cmd: "organize", args: { id: "item-1", folder: "", favourite: true } },
      { cmd: "add_item", args: { kind: "login", fields: [], vault: "vault-1" } },
    ])
  })

  it("copies through the page's clipboard with the timer, never returning the value", async () => {
    const { link, calls } = fakeLink({ reveal_to_use: "s3cret" })
    const clip = board()
    const client = webClient(link, new WebClipboard(clip))
    const copied = await client.copySecret("item-1", "pw", 20)
    expect(copied).toEqual({ clears_in_secs: 20, copy: 1 })
    expect(JSON.stringify(copied)).not.toContain("s3cret")
    expect(clip.text()).toBe("s3cret")
    expect(calls[0]).toEqual({ cmd: "reveal_to_use", args: { id: "item-1", fieldId: "pw" } })
    expect(await client.clipboardHolds(1)).toBe(true)
  })

  it("refuses what the web vault doesn't do yet in words", async () => {
    const { link } = fakeLink()
    const client = webClient(link, new WebClipboard(board()))
    await expect(client.exportSave({ target: 1, format: "cxf", plainConfirmed: true })).rejects.toMatchObject({
      code: "refused",
      message: expect.stringContaining("isn't in the web vault yet"),
    })
    await expect(client.unlock()).rejects.toMatchObject({ code: "refused" })
    await expect(client.openSite("javascript:alert(1)")).rejects.toMatchObject({ code: "refused" })
  })

  it("reaches vault depth with the desktop's names; the guest card comes as bytes; files from disk refuse", async () => {
    const { link, calls } = fakeLink({ wifi_qr: "grid" })
    const client = webClient(link, new WebClipboard(board()))
    await client.saveUserTemplate({ template: { name: "Gym", fields: [] } })
    await client.generatorPreview({ style: "words", words: 6 }, "https://example.com")
    await client.logAway({ id: "i1", place: "Hotel TV", remindInHours: 24 }, { confirmed: true })
    await client.linkTargets("v1", "i1")
    const grid = await client.wifiQr("w1")
    expect(new TextDecoder().decode(grid)).toBe("grid")
    expect(calls).toEqual([
      { cmd: "save_user_template", args: { vault: null, id: null, template: { name: "Gym", fields: [] } } },
      { cmd: "generator_preview", args: { recipe: { style: "words", words: 6 }, url: "https://example.com", rule: null } },
      { cmd: "log_away", args: { id: "i1", place: "Hotel TV", remindInHours: 24, gate: { confirmed: true } } },
      { cmd: "link_targets", args: { vaultId: "v1", except: "i1" } },
      { cmd: "wifi_qr", args: { id: "w1" } },
    ])
    await expect(client.attachChoose()).rejects.toMatchObject({ code: "refused", message: expect.stringContaining("isn't in the web vault yet") })
    await expect(client.attachmentSave("i1", "a1", 1)).rejects.toMatchObject({ code: "refused" })
  })

  it("runs the web's own lifecycle", async () => {
    const { link, calls } = fakeLink()
    const session = webSession(link)
    await session.signIn({ server: "https://v.example", login: "me", password: "pw", secretKey: "A1-X" })
    await session.unlock("pw")
    await session.signOut()
    expect(calls.map((c) => c.cmd)).toEqual(["web_sign_in", "web_unlock", "web_sign_out"])
  })
})
