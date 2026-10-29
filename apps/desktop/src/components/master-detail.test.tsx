import { render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"

import { MasterDetail, PaneHeader } from "./master-detail"

const tree = (detail: ReactNode | null) => (
  <MasterDetail
    header={<PaneHeader title="All items" />}
    title="All items"
    list={<p>rows</p>}
    detail={detail}
    onBack={() => {}}
    emptyIcon="key"
    emptyLabel="Pick an item."
  />
)

describe("one page heading at every width (design review R18)", () => {
  it("with nothing open, the list pane's title is the only h1", () => {
    render(tree(null))
    expect(screen.getAllByRole("heading", { level: 1 }).map((h) => h.textContent)).toEqual(["All items"])
  })

  it("with an item open, the phone bar carries the same title, and each h1 shows at the width the other hides", () => {
    render(tree(<h2>Northwind Mail</h2>))
    const [listTitle, barTitle] = screen.getAllByRole("heading", { level: 1 })
    expect(listTitle).toHaveTextContent("All items")
    expect(barTitle).toHaveTextContent("All items")
    // jsdom draws no layout, so the widths are read from the classes: the
    // list pane (and its h1) is hidden below `md` while an item is open; the
    // bar (and its h1, for a screen reader only) is hidden from `md` up.
    expect(listTitle!.closest("section")).toHaveClass("hidden", "md:flex")
    const bar = barTitle!.parentElement!
    expect(bar).toHaveClass("md:hidden")
    expect(barTitle).toHaveClass("sr-only")
    expect(within(bar).getByRole("button", { name: "Back" })).toBeInTheDocument()
    // The item's own title stays a level below.
    expect(screen.getByRole("heading", { level: 2, name: "Northwind Mail" })).toBeInTheDocument()
  })
})
