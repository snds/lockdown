import { render, screen, waitFor } from "@testing-library/react"

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
})
