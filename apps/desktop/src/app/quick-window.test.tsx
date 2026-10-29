import { render, screen, within } from "@testing-library/react"

import { QuickWindow } from "./quick-window"

// The window's host edges: no core answers here, so it reads as locked.
vi.mock("@tauri-apps/api/core", () => ({ invoke: () => Promise.reject({ code: "refused", message: "not in this test" }) }))
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }))

describe("the quick-access window's landmarks (design review R18)", () => {
  beforeAll(() => {
    // The window's theme follows the system (next-themes asks matchMedia, which jsdom lacks).
    window.matchMedia ??= ((query: string) =>
      ({ matches: false, media: query, onchange: null, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, dispatchEvent: () => false }) as MediaQueryList)
  })

  it("draws inside one main, under one page heading named as the window is", async () => {
    render(<QuickWindow />)
    const main = screen.getByRole("main")
    expect(within(main).getByRole("heading", { level: 1 })).toHaveTextContent("Lockdown quick access")
    expect(await within(main).findByText("Lockdown is locked.")).toBeInTheDocument()
    expect(within(main).getByRole("button", { name: "Unlock" })).toBeInTheDocument()
    expect(screen.getAllByRole("main")).toHaveLength(1)
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1)
  })
})
