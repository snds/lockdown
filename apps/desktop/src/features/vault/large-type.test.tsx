import { act, render } from "@testing-library/react"

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
})
