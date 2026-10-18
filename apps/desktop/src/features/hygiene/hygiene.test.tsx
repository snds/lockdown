import { act, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"

import { HygieneClientProvider, useCaptureProtect, type HygieneClient } from "./hygiene-client"
import { BLUR_WIPE_MS, useSecretWipe, wipeSecretInputs } from "./secret-wipe"

function Form({ onWiped }: { onWiped: (n: number) => void }) {
  const [typed, setTyped] = useState("")
  useSecretWipe(onWiped)
  return (
    <form>
      <input aria-label="Password" type="password" value={typed} onChange={(e) => setTyped(e.target.value)} />
      <input aria-label="Uncontrolled" type="password" defaultValue="" />
      <input aria-label="Title" type="text" defaultValue="" />
      <output aria-label="State">{typed}</output>
    </form>
  )
}

function setHidden(hidden: boolean) {
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => (hidden ? "hidden" : "visible") })
  document.dispatchEvent(new Event("visibilitychange"))
}

describe("typed secrets leave the page (docs/plan/26-desktop-finish.md §7)", () => {
  afterEach(() => setHidden(false))

  it("when the window hides: what is drawn and what React holds", async () => {
    const onWiped = vi.fn()
    render(<Form onWiped={onWiped} />)
    const user = userEvent.setup()
    await user.type(screen.getByLabelText("Password"), "hunter2")
    await user.type(screen.getByLabelText("Uncontrolled"), "s3cret")
    await user.type(screen.getByLabelText("Title"), "keep me")
    expect(screen.getByLabelText("State")).toHaveTextContent("hunter2")
    act(() => setHidden(true))
    expect(screen.getByLabelText("Password")).toHaveValue("")
    expect(screen.getByLabelText("Uncontrolled")).toHaveValue("")
    expect(screen.getByLabelText("State")).toHaveTextContent("")
    expect(screen.getByLabelText("Title")).toHaveValue("keep me")
    expect(onWiped).toHaveBeenCalledWith(2)
    expect(wipeSecretInputs()).toBe(0)
  })

  it("after a minute out of focus, not at once", async () => {
    vi.useFakeTimers()
    try {
      const onWiped = vi.fn()
      render(<Form onWiped={onWiped} />)
      const input = screen.getByLabelText("Password") as HTMLInputElement
      act(() => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, "typed")
        input.dispatchEvent(new Event("input", { bubbles: true }))
      })
      act(() => window.dispatchEvent(new Event("blur")))
      act(() => vi.advanceTimersByTime(BLUR_WIPE_MS - 1000))
      expect(input).toHaveValue("typed")
      // Back in focus: the clock stops.
      act(() => window.dispatchEvent(new Event("focus")))
      act(() => vi.advanceTimersByTime(5000))
      expect(input).toHaveValue("typed")
      act(() => window.dispatchEvent(new Event("blur")))
      act(() => vi.advanceTimersByTime(BLUR_WIPE_MS))
      expect(input).toHaveValue("")
      expect(onWiped).toHaveBeenCalledWith(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe("screen capture while a reveal is drawn (§6)", () => {
  it("holds the window out of captures while open and lets go after", () => {
    const protect = vi.fn<HygieneClient["protect"]>(() => Promise.resolve(true))
    function Reveal({ open }: { open: boolean }) {
      useCaptureProtect(open)
      return null
    }
    const { rerender, unmount } = render(
      <HygieneClientProvider value={{ protect, memory: () => Promise.resolve(null) }}>
        <Reveal open={false} />
      </HygieneClientProvider>,
    )
    expect(protect).not.toHaveBeenCalled()
    rerender(
      <HygieneClientProvider value={{ protect, memory: () => Promise.resolve(null) }}>
        <Reveal open />
      </HygieneClientProvider>,
    )
    expect(protect).toHaveBeenLastCalledWith(true)
    unmount()
    expect(protect).toHaveBeenLastCalledWith(false)
  })
})
