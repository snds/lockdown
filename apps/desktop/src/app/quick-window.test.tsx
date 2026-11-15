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
    // Before the core answers: the window's main and heading, nothing else.
    expect(within(screen.getByRole("main")).getByRole("heading", { level: 1 })).toHaveTextContent("Lockdown quick access")
    // Locked, the focus frame is the window's main (brief X4).
    expect(await screen.findByText("Lockdown is locked.")).toBeInTheDocument()
    const main = screen.getByRole("main")
    expect(within(main).getByText("Lockdown is locked.")).toBeInTheDocument()
    expect(within(main).getByRole("heading", { level: 1 })).toHaveTextContent("Lockdown quick access")
    expect(within(main).getByRole("button", { name: "Unlock" })).toBeInTheDocument()
    expect(screen.getAllByRole("main")).toHaveLength(1)
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1)
  })

  it("locked, it shows the mark, one sentence, UNLOCK as the one primary and NOT NOW, and nothing of the vault", async () => {
    render(<QuickWindow />)
    await screen.findByText("Lockdown is locked.")
    const main = screen.getByRole("main")
    expect(main).toHaveAttribute("data-slot", "app-frame-focus")
    expect(main.querySelector('[data-slot="app-lockup-mark"]')).not.toBeNull()
    const unlock = within(main).getByRole("button", { name: "Unlock" })
    expect(unlock).toHaveFocus()
    expect(within(main).getByRole("button", { name: "Not now" })).toBeInTheDocument()
    expect(within(main).getAllByRole("button")).toHaveLength(2)
    expect(within(main).queryByRole("combobox")).not.toBeInTheDocument()
    expect(within(main).queryByRole("option")).not.toBeInTheDocument()
  })
})
