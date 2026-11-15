import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { AppFrame, Dialog, DialogContent, DialogHeader, DialogTitle, Toaster, toast } from "@unicron/ui"

import { WaitingHost, NeedsYouSlot, NeedsYouStrip, WaitingOnYou, leftWords, pick, titleOf, useNeedsYouSource, type NeedsYouEntry, type NeedsYouPlace } from "./needs-you"

/*
 * Brief X3: one needs-you at a time, by the priority table; one yellow shape;
 * `// +1` when more wait; a screen's own lower item as an ordinary row; no
 * close and no swipe on the strip; rows 1 to 3 as the compact line over an
 * open layer; a request that runs out leaves with a toast.
 */

const noop = () => {}
const entry = (over: Partial<NeedsYouEntry> & Pick<NeedsYouEntry, "id" | "priority">): NeedsYouEntry => ({
  title: `Row ${over.priority}`,
  line: `Line ${over.priority}.`,
  action: { label: "Review", onClick: noop },
  textAction: { label: "Decline", onClick: noop },
  shows: "every",
  ...over,
})
const everywhere: NeedsYouPlace = { sharing: false, item: null }

function Source({ e }: { e: NeedsYouEntry | null }) {
  useNeedsYouSource(e)
  return null
}

function Harness({ entries, place = everywhere, phone = false, children }: { entries: Array<NeedsYouEntry | null>; place?: NeedsYouPlace; phone?: boolean; children?: ReactNode }) {
  return (
    <AppFrame layout={phone ? "phone" : "expanded"}>
      <WaitingHost place={place}>
        {entries.map((e, i) => (
          <Source key={i} e={e} />
        ))}
        <NeedsYouSlot />
        <NeedsYouStrip />
        <WaitingOnYou />
        {children}
      </WaitingHost>
      <Toaster />
    </AppFrame>
  )
}

const yellow = () => [...document.querySelectorAll<HTMLElement>('[data-slot="needs-you"], [data-slot="accessory"][data-tone="needs-you"]')].filter((el) => !el.closest('[aria-hidden="true"]') && !el.hasAttribute("data-covered"))

afterEach(() => {
  act(() => {
    toast.dismiss()
  })
})

describe("the priority table (brief X3)", () => {
  it("shows the first true entry and counts the rest", () => {
    const seven = entry({ id: "kit", priority: 7 })
    const one = entry({ id: "sign-in", priority: 1, expiresAt: 100_000 })
    const nine = entry({ id: "drill", priority: 9 })
    expect(pick([seven, nine, one], everywhere, 0)).toEqual({ top: one, more: 2, waiting: [] })
    // Run out: it no longer counts.
    expect(pick([seven, nine, one], everywhere, 100_000).top).toBe(seven)
  })

  it("matches where it shows: an item's conflict only on that item, a reset only on Sharing", () => {
    const conflict = entry({ id: "c", priority: 5, shows: { item: "i1" } })
    const reset = entry({ id: "r", priority: 6, shows: "sharing" })
    expect(pick([conflict, reset], everywhere, 0).top).toBeNull()
    expect(pick([conflict, reset], { sharing: false, item: "i1" }, 0).top).toBe(conflict)
    expect(pick([conflict, reset], { sharing: true, item: null }, 0).top).toBe(reset)
  })

  it("puts the screen's own lower item under WAITING ON YOU, never a second block", () => {
    const one = entry({ id: "sign-in", priority: 1 })
    const conflict = entry({ id: "c", priority: 5, shows: { item: "i1" } })
    const kit = entry({ id: "kit", priority: 7 })
    expect(pick([kit, conflict, one], { sharing: false, item: "i1" }, 0)).toEqual({ top: one, more: 2, waiting: [conflict] })
  })

  it("writes the time left as decisions §13 does", () => {
    expect(leftWords(100)).toBe("01:40")
    expect(leftWords(8)).toBe("08 s")
    expect(titleOf(entry({ id: "x", priority: 1, title: "Sign-in request", expiresAt: 100_000 }), 0)).toBe("Sign-in request // 01:40 left")
  })
})

describe("the desktop block", () => {
  it("draws one block, the higher one, with `// +1`, one solid action and no close", () => {
    render(<Harness entries={[entry({ id: "drill", priority: 9, title: "Recovery drill // due" }), entry({ id: "kit", priority: 7, title: "Recovery kit // not tested" })]} />)
    const block = screen.getByRole("region", { name: /Recovery kit \/\/ not tested/ })
    expect(block).toHaveTextContent("// +1")
    expect(block).toHaveAccessibleName("Recovery kit // not tested, 1 more waiting")
    expect(yellow()).toHaveLength(1)
    // One solid action, one text action, and nothing that closes it.
    expect(block.querySelectorAll('[data-variant="needs-you"]')).toHaveLength(1)
    expect(within(block).getAllByRole("button").map((b) => b.textContent)).toEqual(["Review", "Decline"])
    expect(within(block).queryByRole("button", { name: /close|dismiss/i })).toBeNull()
  })

  it("says it once, not every tick", () => {
    render(<Harness entries={[entry({ id: "kit", priority: 7, title: "Recovery kit // not tested", line: "Test your recovery kit." })]} />)
    expect(document.querySelector("[data-needs-you-announce]")).toHaveTextContent("Recovery kit // not tested. Test your recovery kit.")
  })

  it("shows the screen's own lower item as an ordinary row with secondary buttons", () => {
    render(<Harness place={{ sharing: false, item: "i1" }} entries={[entry({ id: "c", priority: 5, title: "Conflict // 2 copies", shows: { item: "i1" } }), entry({ id: "one", priority: 1, title: "Sign-in request" })]} />)
    expect(yellow()).toHaveLength(1)
    const waiting = screen.getByRole("region", { name: "Waiting on you" })
    expect(within(waiting).getByRole("heading")).toHaveTextContent(/Waiting on you\s*01/i)
    expect(within(waiting).getByRole("listitem")).toHaveTextContent("Conflict // 2 copies")
    expect(waiting.querySelector('[data-variant="needs-you"]')).toBeNull()
  })

  it("leaves when it runs out, and a toast says so", async () => {
    render(<Harness entries={[entry({ id: "sign-in", priority: 1, title: "Sign-in request", expiresAt: Date.now() + 1200, expiredWords: "Sign-in request ran out" })]} />)
    expect(screen.getByRole("region", { name: /Sign-in request \/\/ 0\d s left/ })).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByRole("region", { name: /Sign-in request/ })).toBeNull(), { timeout: 4000 })
    expect(await screen.findByText("Sign-in request ran out")).toBeInTheDocument()
  })
})

describe("the phone strip", () => {
  it("is the one yellow shape, with no close and no swipe away; OPEN opens a sheet with both actions", async () => {
    const test = vi.fn()
    const skip = vi.fn()
    render(<Harness phone entries={[entry({ id: "kit", priority: 7, title: "Recovery kit // not tested", line: "Test your recovery kit.", action: { label: "Test it now", onClick: test }, textAction: { label: "Skip", onClick: skip } })]} />)
    const strip = document.querySelector<HTMLElement>('[data-slot="accessory"]')!
    expect(strip).toHaveAttribute("data-tone", "needs-you")
    expect(yellow()).toHaveLength(1)
    expect(within(strip).queryByRole("button", { name: /close|dismiss/i })).toBeNull()
    // A sideways swipe does not put it away.
    fireEvent.pointerDown(strip, { pointerId: 1, clientX: 300, clientY: 10 })
    fireEvent.pointerMove(strip, { pointerId: 1, clientX: 20, clientY: 12 })
    fireEvent.pointerUp(strip, { pointerId: 1, clientX: 20, clientY: 12 })
    expect(document.querySelector('[data-slot="accessory"][data-tone="needs-you"]')).toBeInTheDocument()

    const user = userEvent.setup()
    // The text is one button and the action a sibling (C5); the action says OPEN for rows 6 to 9.
    await user.click(within(strip).getAllByRole("button", { name: "Open" }).at(-1)!)
    const sheet = await screen.findByRole("dialog", { name: "Test your recovery kit." })
    await user.click(within(sheet).getByRole("button", { name: "Skip" }))
    expect(skip).toHaveBeenCalledTimes(1)
    expect(test).not.toHaveBeenCalled()
  })

  it("names REVIEW for a request, and runs it straight from the strip", async () => {
    const review = vi.fn()
    render(<Harness phone entries={[entry({ id: "sign-in", priority: 1, title: "Sign-in request", action: { label: "Review", onClick: review } })]} />)
    const strip = document.querySelector<HTMLElement>('[data-slot="accessory"]')!
    await userEvent.setup().click(within(strip).getAllByRole("button", { name: "Review" }).at(-1)!)
    expect(review).toHaveBeenCalledTimes(1)
  })
})

describe("under an open layer", () => {
  function Open() {
    return (
      <Dialog open>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New login</DialogTitle>
          </DialogHeader>
        </DialogContent>
      </Dialog>
    )
  }

  it("a timed request shows as the compact line over the dialog, and the block behind hides", async () => {
    const review = vi.fn()
    render(
      <Harness entries={[entry({ id: "sign-in", priority: 1, title: "Sign-in request", expiresAt: Date.now() + 100_000, action: { label: "Review", onClick: review } })]}>
        <Open />
      </Harness>,
    )
    const line = await waitFor(() => {
      const el = document.querySelector<HTMLElement>('[data-needs-you-layer] [data-slot="needs-you"][data-size="compact"]')
      expect(el).not.toBeNull()
      return el!
    })
    // The block is covered: one yellow shape shows.
    expect(document.querySelector('[data-slot="needs-you"][data-size="default"]')).toHaveAttribute("data-covered")
    expect(yellow()).toHaveLength(1)
    // The dialog did not change: no notice inside it.
    expect(screen.getByRole("dialog").querySelector('[data-slot="dialog-notice"]')).toBeNull()
    await userEvent.setup().click(within(line).getByRole("button", { name: "Review" }))
    expect(review).toHaveBeenCalledTimes(1)
    expect(screen.getByRole("dialog", { name: "New login" })).toBeInTheDocument()
  })

  it("a reminder waits behind the scrim: no compact line", async () => {
    render(
      <Harness entries={[entry({ id: "kit", priority: 7 })]}>
        <Open />
      </Harness>,
    )
    await act(async () => {
      await new Promise((r) => setTimeout(r, 150))
    })
    expect(document.querySelector("[data-needs-you-layer]")).toBeNull()
  })
})
