import { describe, expect, it, vi } from "vitest"

import type { Link } from "./link"
import { KIT_FILE_NAME, browserDownload, webPaperClient, type Download } from "./web-paper-client"

const PAGE = "LOCKDOWN RECOVERY KIT\nSecret Key:     A1-7GH2K\n"

describe("the web vault's paper (docs/plan/38-mac-fixes.md)", () => {
  it("prints with the browser's own dialog, which is over when it returns", async () => {
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    const link: Link = { call: vi.fn(), callBytes: vi.fn() }
    expect(await webPaperClient(link).print()).toBe("done")
    expect(print).toHaveBeenCalledTimes(1)
    expect(link.call).not.toHaveBeenCalled()
    print.mockRestore()
  })

  it("asks the core for the kit's PDF and downloads exactly those bytes, then wipes them", async () => {
    const pdf = new TextEncoder().encode("%PDF-1.7 the kit")
    const callBytes = vi.fn(async () => pdf)
    const link: Link = { call: vi.fn(), callBytes }
    let handed: number[] = []
    const download = vi.fn<Download>((bytes) => {
      handed = Array.from(bytes)
    })
    expect(await webPaperClient(link, download).saveKitPdf!(["a kit was made"], PAGE)).toBe("saved")
    expect(callBytes).toHaveBeenCalledWith("kit_pdf", { lines: ["a kit was made"], page: PAGE })
    expect(download).toHaveBeenCalledWith(pdf, KIT_FILE_NAME)
    expect(new TextDecoder().decode(Uint8Array.from(handed))).toBe("%PDF-1.7 the kit")
    expect(Array.from(pdf).every((b) => b === 0)).toBe(true)
    expect(KIT_FILE_NAME).toBe("Lockdown Recovery Kit.pdf")
  })

  it("downloads nothing when the core refuses the page", async () => {
    const link: Link = { call: vi.fn(), callBytes: vi.fn(async () => Promise.reject({ code: "locked", message: "locked" })) }
    const download = vi.fn<Download>()
    await expect(webPaperClient(link, download).saveKitPdf!([], PAGE)).rejects.toMatchObject({ code: "locked" })
    expect(download).not.toHaveBeenCalled()
  })

  it("hands the browser a link to a PDF named as the desktop names it, then revokes it", () => {
    const types: string[] = []
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {})
    const create = vi.spyOn(URL, "createObjectURL").mockImplementation((b) => {
      types.push((b as Blob).type)
      return "blob:kit"
    })
    const appended: HTMLAnchorElement[] = []
    const observer = new MutationObserver((records) => records.forEach((r) => r.addedNodes.forEach((n) => n instanceof HTMLAnchorElement && appended.push(n))))
    observer.observe(document.body, { childList: true })
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {})
    vi.useFakeTimers()
    try {
      browserDownload(new Uint8Array([1, 2, 3]), KIT_FILE_NAME)
      observer.takeRecords().forEach((r) => r.addedNodes.forEach((n) => n instanceof HTMLAnchorElement && appended.push(n)))
      const clicked = appended[0]!
      expect(click).toHaveBeenCalledTimes(1)
      expect(types).toEqual(["application/pdf"])
      expect(clicked.download).toBe(KIT_FILE_NAME)
      expect(clicked.getAttribute("href")).toBe("blob:kit")
      expect(document.body.contains(clicked)).toBe(false)
      vi.runAllTimers()
      expect(revoke).toHaveBeenCalledWith("blob:kit")
    } finally {
      observer.disconnect()
      vi.useRealTimers()
      click.mockRestore()
      create.mockRestore()
      revoke.mockRestore()
    }
  })
})
