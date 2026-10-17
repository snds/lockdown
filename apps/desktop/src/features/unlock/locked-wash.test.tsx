import { render } from "@testing-library/react"

import { LockedWash } from "./locked-wash"

describe("the locked wash (brief §4)", () => {
  it("renders no titles: nothing to read, nothing for assistive tech", () => {
    const { container } = render(<LockedWash />)
    expect(container.textContent).toBe("")
    expect(container.querySelector("h1, h2, h3, button, a, img, svg")).toBeNull()
    expect(container.querySelector('[data-slot="locked-wash"]')).toHaveAttribute("aria-hidden", "true")
    expect(container.querySelectorAll('[data-slot="skeleton"]').length).toBeGreaterThan(8)
  })
})
