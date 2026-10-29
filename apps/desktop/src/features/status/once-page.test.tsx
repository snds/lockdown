import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PaperClientProvider, type PaperClient } from "@/features/paper/paper-client"

import { OncePage, kitValues } from "./once-page"

const PAGE = `LOCKDOWN RECOVERY KIT
=====================
Keep this page somewhere safe, away from your devices.

Secret Key:     A1-7GH2K-M9PQR-ST3VW-XY4Z5-6789AB
Recovery code:  QRST-2345-VWXY-6789-ABCD-EFGH

Login name:     sean
Server:         https://vault.example.test
`

describe("the kit page through LargeTypeView's print variant", () => {
  it("reads the two values off the page as printed", () => {
    expect(kitValues(PAGE)).toEqual({ secretKey: "A1-7GH2K-M9PQR-ST3VW-XY4Z5-6789AB", recoveryCode: "QRST-2345-VWXY-6789-ABCD-EFGH" })
    expect(kitValues("nothing here")).toEqual({ secretKey: null, recoveryCode: null })
  })

  it("prints the Secret Key and the recovery code in large type, every character captioned", async () => {
    render(<OncePage page={{ title: "Your recovery kit", lines: ["a kit was made"], text: PAGE }} onClose={() => {}} />)
    const dialog = await screen.findByRole("dialog")
    await waitFor(() => expect(dialog.querySelectorAll('[data-slot="large-type-view"][data-variant="print"]')).toHaveLength(2))
    const [key, code] = Array.from(dialog.querySelectorAll('[data-slot="large-type-view"][data-variant="print"]'))
    expect(key).toHaveTextContent("Secret Key")
    expect(code).toHaveTextContent("Recovery code")
    await waitFor(() => expect(key!.querySelectorAll('[data-slot="large-type-view-char"]').length).toBeGreaterThan(0))
    const keyChars = Array.from(key!.querySelectorAll('[data-slot="large-type-view-char"]')).map((c) => c.firstElementChild?.textContent).join("")
    expect(keyChars).toBe("A17GH2KM9PQRST3VWXY4Z56789AB")
    // Captions name the glyphs people confuse on paper.
    expect(key!.querySelectorAll('[data-slot="large-type-view-caption"]').length).toBeGreaterThan(0)
    const codeChars = Array.from(code!.querySelectorAll('[data-slot="large-type-view-char"]')).map((c) => c.firstElementChild?.textContent).join("")
    expect(codeChars).toBe("QRST2345VWXY6789ABCDEFGH")
    // The page as written is still there to print.
    expect(dialog.textContent).toContain("A1-7GH2K-M9PQR-ST3VW-XY4Z5-6789AB")
  })

  it("the page as text takes keyboard focus, since it scrolls sideways on a phone (design review R18)", async () => {
    render(<OncePage page={{ title: "Your recovery kit", lines: [], text: PAGE }} onClose={() => {}} />)
    const text = await screen.findByRole("region", { name: "The page as text" })
    expect(text.tagName).toBe("PRE")
    expect(text).toHaveAttribute("tabindex", "0")
    expect(text).toHaveTextContent("LOCKDOWN RECOVERY KIT")
  })
})

describe("printing and saving the kit page (docs/plan/38-mac-fixes.md)", () => {
  const shown = { title: "Your recovery kit", lines: ["a kit was made"], text: PAGE }

  it("prints through the host, and offers no file where the host can't keep one", async () => {
    const print = vi.fn<PaperClient["print"]>(() => Promise.resolve("done" as const))
    render(
      <PaperClientProvider value={{ print }}>
        <OncePage page={shown} onClose={() => {}} />
      </PaperClientProvider>,
    )
    await userEvent.click(await screen.findByRole("button", { name: "Print" }))
    expect(print).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("button", { name: "Save as PDF…" })).toBeNull()
  })

  it("saves the page shown, its notes and text, and says Lockdown kept no copy", async () => {
    const saveKitPdf = vi.fn<NonNullable<PaperClient["saveKitPdf"]>>(() => Promise.resolve("saved" as const))
    render(
      <PaperClientProvider value={{ print: () => Promise.resolve("done" as const), saveKitPdf }}>
        <OncePage page={shown} onClose={() => {}} />
      </PaperClientProvider>,
    )
    const dialog = await screen.findByRole("dialog")
    // What the file is, before it is made.
    expect(dialog).toHaveTextContent("A saved file holds everything needed to get into your account with your password.")
    expect(dialog).toHaveTextContent("Keep it off this computer: on a USB drive, as a printed copy, or in a safe. Lockdown keeps no copy.")
    await userEvent.click(screen.getByRole("button", { name: "Save as PDF…" }))
    expect(saveKitPdf).toHaveBeenCalledWith(["a kit was made"], PAGE)
    expect(await screen.findByRole("status")).toHaveTextContent("Saved where you chose. Lockdown kept no copy.")
  })

  it("says nothing was saved when the dialog was cancelled, and shows a refusal", async () => {
    const saveKitPdf = vi
      .fn<NonNullable<PaperClient["saveKitPdf"]>>()
      .mockResolvedValueOnce("cancelled")
      .mockRejectedValueOnce({ code: "refused", message: "the kit couldn't be saved there: read-only" })
    render(
      <PaperClientProvider value={{ print: () => Promise.resolve("done" as const), saveKitPdf }}>
        <OncePage page={shown} onClose={() => {}} />
      </PaperClientProvider>,
    )
    await userEvent.click(await screen.findByRole("button", { name: "Save as PDF…" }))
    await waitFor(() => expect(saveKitPdf).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole("status")).toBeNull()
    await userEvent.click(screen.getByRole("button", { name: "Save as PDF…" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("the kit couldn't be saved there: read-only")
  })

  it("keeps the buttons and the file's words off the paper", async () => {
    render(
      <PaperClientProvider value={{ print: () => Promise.resolve("done" as const), saveKitPdf: () => Promise.resolve("saved" as const) }}>
        <OncePage page={shown} onClose={() => {}} />
      </PaperClientProvider>,
    )
    const save = await screen.findByRole("button", { name: "Save as PDF…" })
    expect(save.closest('[data-slot="dialog-footer"]')).not.toBeNull()
    expect(screen.getByText(/A saved file holds everything/)).toHaveAttribute("data-print-hide")
  })
})
