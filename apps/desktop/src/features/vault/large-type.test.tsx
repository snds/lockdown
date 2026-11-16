import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { CodeLargeType, LARGE_TYPE_LIFETIME_S, codeLargeTitle } from "./large-type"

describe("a code in large type (Lockdown 28)", () => {
  afterEach(() => vi.useRealTimers())

  it("counts down the live code's seconds in its title", () => {
    expect(codeLargeTitle("Northwind · sam", 100, 112)).toBe("Code · Northwind · sam // 12 S left")
    expect(codeLargeTitle("Northwind · sam", 100, 190)).toBe("Code · Northwind · sam // 01:30 left")
    // A counter-based code has no clock.
    expect(codeLargeTitle("Bank", 100, null)).toBe("Code · Bank")
  })

  it("follows the code as it turns over, and still closes two minutes after it opened", () => {
    vi.useFakeTimers()
    const onOpenChange = vi.fn()
    const { rerender } = render(<CodeLargeType open onOpenChange={onOpenChange} current="123456" named="Northwind" now={100} expiresAt={130} />)
    act(() => {
      vi.advanceTimersByTime(60_000)
    })
    // A new code: the view reads it again, but the two minutes run from opening.
    rerender(<CodeLargeType open onOpenChange={onOpenChange} current="654321" named="Northwind" now={160} expiresAt={190} />)
    act(() => {
      vi.advanceTimersByTime(LARGE_TYPE_LIFETIME_S * 1000 - 60_000 - 1)
    })
    expect(onOpenChange).not.toHaveBeenCalledWith(false)
    act(() => {
      vi.advanceTimersByTime(2)
    })
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })

  it("keeps one view across a turnover: Captions and focus stay where the person put them, and the new code shows", async () => {
    const user = userEvent.setup()
    const { rerender } = render(<CodeLargeType open onOpenChange={() => {}} current="123456" named="Northwind" now={100} expiresAt={130} />)
    const captions = await screen.findByRole("switch", { name: "Captions" })
    expect(captions).toBeChecked()
    await user.click(captions)
    expect(captions).not.toBeChecked()
    const smaller = screen.getByRole("button", { name: "Smaller" })
    smaller.focus()
    const canvas = () => document.querySelector('[data-slot="large-type-view-canvas"]')?.getAttribute("aria-label") ?? ""
    await waitFor(() => expect(canvas()).toMatch(/3, pause, 4/))
    rerender(<CodeLargeType open onOpenChange={() => {}} current="654321" named="Northwind" now={130} expiresAt={160} />)
    await waitFor(() => expect(canvas()).toMatch(/4, pause, 3/))
    // The same switch, still off; focus did not jump back to Close.
    expect(screen.getByRole("switch", { name: "Captions" })).toBe(captions)
    expect(captions).not.toBeChecked()
    expect(smaller).toHaveFocus()
  })

  it("once hidden, a new code does not bring the value back", async () => {
    const { rerender } = render(<CodeLargeType open onOpenChange={() => {}} current="123456" named="Northwind" now={100} expiresAt={130} />)
    await waitFor(() => expect(document.querySelector('[data-slot="large-type-view"]')?.getAttribute("data-status")).toBe("shown"))
    act(() => {
      window.dispatchEvent(new Event("blur"))
    })
    await waitFor(() => expect(document.querySelector('[data-slot="large-type-view"]')?.getAttribute("data-status")).toBe("expired"))
    rerender(<CodeLargeType open onOpenChange={() => {}} current="654321" named="Northwind" now={130} expiresAt={160} />)
    await act(async () => {})
    expect(document.querySelector('[data-slot="large-type-view"]')?.getAttribute("data-status")).toBe("expired")
    expect(screen.getByRole("button", { name: "Show again" })).toBeInTheDocument()
  })
})
