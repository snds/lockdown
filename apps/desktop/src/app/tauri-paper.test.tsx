import { invoke } from "@tauri-apps/api/core"

import { kitBody, tauriPaperClient } from "./tauri-paper"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const mocked = vi.mocked(invoke)

const PAGE = "LOCKDOWN RECOVERY KIT\nSecret Key:     A1-7GH2K\n"

describe("the desktop's paper client (docs/plan/38-mac-fixes.md)", () => {
  beforeEach(() => mocked.mockReset())

  it("prints through the shell's print_page, never window.print, and says the panel is still open", async () => {
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    mocked.mockResolvedValueOnce(undefined)
    expect(await tauriPaperClient.print()).toBe("open")
    expect(mocked).toHaveBeenCalledWith("print_page")
    expect(print).not.toHaveBeenCalled()
    print.mockRestore()
  })

  it("sends the kit page as raw bytes in kit_create's shape, and wipes them once sent", async () => {
    let sent: Uint8Array | null = null
    let copy: Uint8Array | null = null
    mocked.mockImplementationOnce(async (_cmd, body) => {
      sent = body as Uint8Array
      copy = Uint8Array.from(sent)
      return true
    })
    expect(await tauriPaperClient.saveKitPdf!(["a kit was made"], PAGE)).toBe("saved")
    expect(mocked.mock.calls[0]![0]).toBe("kit_save_pdf")
    const decoded = new TextDecoder().decode(copy!)
    expect(decoded).toBe(`["a kit was made"]\u0000${PAGE}`)
    // The webview's copy of the page is zeros once the shell has it.
    expect(Array.from(sent!).every((b) => b === 0)).toBe(true)
  })

  it("reports a cancelled save dialog, and wipes the bytes on a refusal too", async () => {
    mocked.mockResolvedValueOnce(false)
    expect(await tauriPaperClient.saveKitPdf!([], PAGE)).toBe("cancelled")
    let sent: Uint8Array | null = null
    mocked.mockImplementationOnce(async (_cmd, body) => {
      sent = body as Uint8Array
      throw { code: "refused", message: "that isn't a recovery kit page" }
    })
    await expect(tauriPaperClient.saveKitPdf!([], "hello")).rejects.toMatchObject({ code: "refused" })
    expect(Array.from(sent!).every((b) => b === 0)).toBe(true)
  })

  it("builds the body as the shell splits it: the JSON notes, a NUL, the page", () => {
    const body = kitBody(["one", "two"], "page")
    const nul = body.indexOf(0)
    expect(JSON.parse(new TextDecoder().decode(body.subarray(0, nul)))).toEqual(["one", "two"])
    expect(new TextDecoder().decode(body.subarray(nul + 1))).toBe("page")
  })
})
