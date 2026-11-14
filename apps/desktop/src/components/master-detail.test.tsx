import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { AppFrame, AppFrameMain, useAppFrame, type AppFrameLayout } from "@unicron/ui"

import { Embedded, MasterDetail, PaneHeader } from "./master-detail"

/** The shell's part: the way back the frame hands it while a detail is pushed. */
function Back() {
  const frame = useAppFrame()
  return frame.back ? <button onClick={frame.back.onBack}>{frame.back.label}</button> : null
}

const tree = (layout: AppFrameLayout, detail: ReactNode | null, onBack = () => {}) => (
  <AppFrame layout={layout}>
    <Back />
    <AppFrameMain>
      <MasterDetail header={<PaneHeader title="All items" />} title="All items" list={<p>rows</p>} detail={detail} onBack={onBack} emptyIcon="password" emptyLabel="Pick an item." />
    </AppFrameMain>
  </AppFrame>
)

const h1s = () => screen.getAllByRole("heading", { level: 1 }).map((h) => h.textContent)

describe("one page heading at every width (design review R18)", () => {
  it("with nothing open, the list pane's title is the only h1, and the detail says so", () => {
    render(tree("expanded", null))
    expect(h1s()).toEqual(["All items"])
    expect(screen.getByText("Nothing open.")).toBeInTheDocument()
    expect(screen.getByText("Pick an item.")).toBeInTheDocument()
  })

  it("beside the list, an open item adds no second h1", () => {
    render(tree("expanded", <h2>Northwind Mail</h2>))
    expect(h1s()).toEqual(["All items"])
    expect(screen.getByText("rows")).toBeVisible()
    expect(screen.getByRole("heading", { level: 2, name: "Northwind Mail" })).toBeInTheDocument()
    // Nothing is pushed, so there is no way back to offer.
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull()
  })

  it.each(["phone", "medium"] as const)("at %s width an open item is pushed over the list: the list is hidden, the detail carries the title for a screen reader, and Back returns", async (layout) => {
    const onBack = vi.fn()
    render(tree(layout, <h2>Northwind Mail</h2>, onBack))
    // The list column (its h1 with it) is out of reach while the detail is over it.
    const list = screen.getByText("rows").closest("section")!
    expect(list).toHaveAttribute("aria-hidden", "true")
    expect(list).toHaveAttribute("inert")
    // So the one h1 a screen reader finds is the detail's, with the same words, drawn for no one.
    const [title, ...rest] = screen.getAllByRole("heading", { level: 1 })
    expect(rest).toHaveLength(0)
    expect(title).toHaveTextContent("All items")
    expect(title).toHaveClass("sr-only")
    // The item's own title stays a level below.
    expect(screen.getByRole("heading", { level: 2, name: "Northwind Mail" })).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Back" }))
    expect(onBack).toHaveBeenCalledTimes(1)
  })

  it.each(["phone", "medium"] as const)("at %s width, with nothing open, the list is the screen", (layout) => {
    render(tree(layout, null))
    expect(h1s()).toEqual(["All items"])
    expect(screen.getByText("rows").closest("section")).not.toHaveAttribute("aria-hidden")
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull()
  })
})

describe("a screen inside another's frame", () => {
  it("keeps its actions and draws no second title", () => {
    render(
      <Embedded>
        <PaneHeader title="Devices">
          <button>Add device</button>
        </PaneHeader>
      </Embedded>,
    )
    expect(screen.queryByRole("heading")).toBeNull()
    expect(screen.getByRole("button", { name: "Add device" })).toBeInTheDocument()
  })
})
