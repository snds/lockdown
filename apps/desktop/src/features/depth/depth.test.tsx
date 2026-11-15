import { render, screen } from "@testing-library/react"
import { isLinkQr, linkCode, phraseDigits, promptFor, type RemindersView } from "./depth-client"
import { QrCode } from "./qr-code"

const reminders = (over: { password?: Partial<RemindersView["password"]>; drill?: Partial<RemindersView["drill"]> }): RemindersView => ({
  password: { applies: true, due: false, last_at: 1, every_days: 14, next_at: 2, ...over.password },
  drill: { kit: "drilled", last_drilled_at: 1, next_at: 2, due: false, kit_test_pending: false, ...over.drill },
})

describe("the security-depth helpers", () => {
  it("reads codes and phrases the way the core does", () => {
    expect(linkCode("7kq2m 9xh4p")).toBe("7KQ2M-9XH4P")
    expect(linkCode("OIL23-45678")).toBe("01123-45678")
    expect(linkCode("7KQ2M")).toBeNull()
    expect(linkCode("UUUUU-UUUUU")).toBeNull()
    expect(phraseDigits("123 456-789")).toBe("123456789")
    expect(phraseDigits("12345678")).toBeNull()
    expect(isLinkQr(" lockdown:link:v1?c=X")).toBe(true)
    expect(isLinkQr("https://example.com")).toBe(false)
  })

  it("asks the kit test first, then the password, then the drill", () => {
    expect(promptFor(null)).toBeNull()
    expect(promptFor(reminders({}))).toBeNull()
    expect(promptFor(reminders({ drill: { kit_test_pending: true, due: true }, password: { due: true } }))).toBe("kit-test")
    expect(promptFor(reminders({ drill: { due: true }, password: { due: true } }))).toBe("password")
    expect(promptFor(reminders({ drill: { due: true }, password: { due: true, applies: false } }))).toBe("drill")
    expect(promptFor(reminders({ drill: { due: true, kit: "none" } }))).toBeNull()
  })

  it("draws a QR code from the core's rows, dark modules only", () => {
    const { container } = render(<QrCode rows={["101", "010", "101"]} label="code" />)
    const d = container.querySelector("path")?.getAttribute("d") ?? ""
    expect(d.match(/M/g)).toHaveLength(5)
    expect(screen.getByRole("img", { name: "code" })).toBeInTheDocument()
  })
})
