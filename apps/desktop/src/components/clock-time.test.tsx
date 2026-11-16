import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { TooltipProvider } from "@unicron/ui"

import { ageWords } from "@/lib/format"

import { ClockTime } from "./clock-time"

describe("ClockTime (the cross-app clock-time rule, D1040)", () => {
  it("takes keyboard focus, so its tip (how long ago) opens from the keyboard as from a pointer", async () => {
    const now = Date.now() / 1000
    const secs = now - 3 * 3600
    render(
      <TooltipProvider delay={0}>
        <ClockTime secs={secs} nowSecs={now} prefix="Backed up" />
      </TooltipProvider>,
    )
    const user = userEvent.setup()
    await user.tab()
    const time = screen.getByText(/^Backed up /)
    expect(time).toHaveFocus()
    expect(time).toHaveAttribute("aria-description", ageWords(secs, now))
  })
})
