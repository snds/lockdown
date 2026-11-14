import { act, render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useMemo, useState } from "react"

import { FloatButton, TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { MasterDetail, PaneHeader, useDetailTopBar } from "@/components/master-detail"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { useDetailBottomRow } from "./detail-bottom-row"
import { usePlaces, type Nav } from "./places"
import { AppShell } from "./shell"

/*
 * The chrome of pushed screens, short screens and folds (brief 3.3 and X6,
 * doc 65). jsdom lays nothing out: the window's numbers, the media queries
 * and the visual viewport are what unicron's frame reads, so they are set
 * here the way a phone, a phone on its side, a keyboard and a fold set them.
 */

const VAULTS: VaultView[] = [{ id: "v1", name: "Personal", primary: true, items: 0, trash_days: 30, folders: [] }]
const ROW: ItemSummary = {
  id: "i1",
  kind: "login",
  state: "live",
  title: "Northwind Mail",
  site: "",
  username: "",
  edited_at: 0,
  crown_jewel: false,
  methods: [],
  passkeys: 0,
  codes: 0,
  expires_at: null,
  conflict_copy: false,
  warnings: [],
  views: [],
  subtitle: "",
  copy: null,
  vault_id: "v1",
  folder: null,
  tags: [],
  favourite: false,
}

/** An open item that hands its buttons up, and draws them itself where the shell does not. */
function Detail({ onCopy, onEdit }: { onCopy: () => void; onEdit: () => void }) {
  const row = useMemo(
    () => ({
      start: <FloatButton label="Copy password" icon="copy" size="l" onClick={onCopy} />,
      end: <FloatButton label="Edit" icon="edit" kind="primary" size="l" onClick={onEdit} />,
    }),
    [onCopy, onEdit],
  )
  const more = useMemo(() => <FloatButton label="More actions" icon="overflow-menu--horizontal" size="l" />, [])
  const inRow = useDetailBottomRow(row)
  const inBar = useDetailTopBar(more)
  return (
    <article>
      <h2>Northwind Mail</h2>
      <input aria-label="Notes" />
      {!inRow && (
        <>
          <button onClick={onCopy}>Copy password</button>
          <button onClick={onEdit}>Edit</button>
        </>
      )}
      {!inBar && <button>More actions</button>}
    </article>
  )
}

function Screen({ open, onCopy, onEdit }: { open: boolean; onCopy: () => void; onEdit: () => void }) {
  const { nav } = usePlaces()
  if (nav.area !== "vault") return <p>{nav.area}</p>
  return <MasterDetail header={<PaneHeader title="All items" />} title="All items" list={<p>rows</p>} detail={open ? <Detail onCopy={onCopy} onEdit={onEdit} /> : null} onBack={() => {}} emptyLabel="Pick an item." />
}

function Harness({ open = true, onCopy = () => {}, onEdit = () => {} }: { open?: boolean; onCopy?: () => void; onEdit?: () => void }) {
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  return (
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]) })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>
                <AppShell nav={nav} onNavigate={setNav} rows={[ROW]} onLock={() => {}} onSaved={() => {}}>
                  <Screen open={open} onCopy={onCopy} onEdit={onEdit} />
                </AppShell>
              </VaultsProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>
  )
}

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

/** The media queries a device answers: a touch screen, a fold. */
function setMedia(answers: Record<string, boolean>) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({ matches: answers[query] ?? false, media: query, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }),
  })
}

afterEach(() => {
  // jsdom has none of these; leave none behind for the next test.
  Reflect.deleteProperty(window, "matchMedia")
  Reflect.deleteProperty(window, "visualViewport")
  Reflect.deleteProperty(window, "viewport")
})

const bottomRow = () => document.querySelector<HTMLElement>('[data-slot="app-frame-bottom-row"]')
const railColumn = () => document.querySelector<HTMLElement>('[data-slot="app-frame-rail"]')

describe("a pushed screen's chrome on a phone (brief 3.3)", () => {
  beforeEach(() => {
    window.localStorage.clear()
    setWindow(390, 844)
  })

  it("useDetailBottomRow draws at 390: its buttons at the foot, no tab bar, the top bar's actions after back", async () => {
    const onCopy = vi.fn()
    const onEdit = vi.fn()
    const user = userEvent.setup()
    render(<Harness onCopy={onCopy} onEdit={onEdit} />)
    const row = bottomRow()!
    expect(row).toHaveAttribute("data-variant", "detail")
    expect(screen.queryByRole("tablist", { name: "Sections" })).toBeNull()
    // One of each: the shell draws them, and the screen leaves its own out.
    expect(screen.getAllByRole("button", { name: "Copy password" })).toHaveLength(1)
    await user.click(within(row).getByRole("button", { name: "Copy password" }))
    await user.click(within(row).getByRole("button", { name: "Edit" }))
    expect(onCopy).toHaveBeenCalledTimes(1)
    expect(onEdit).toHaveBeenCalledTimes(1)
    // The top bar: back, then what the screen handed up.
    const bar = document.querySelector<HTMLElement>('[data-slot="top-bar"]')!
    expect(within(bar).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Back", "More actions"])
  })

  it("draws nothing at 1440: the screen keeps its own buttons", () => {
    setWindow(1440, 900)
    render(<Harness />)
    expect(bottomRow()).toBeNull()
    expect(document.querySelector('[data-slot="top-bar"]')).toBeNull()
    expect(screen.getByRole("button", { name: "Copy password" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "More actions" })).toBeInTheDocument()
  })

  it("goes with the screen: back on the list, the tab bar's row returns", () => {
    const { rerender } = render(<Harness />)
    expect(bottomRow()).toHaveAttribute("data-variant", "detail")
    rerender(<Harness open={false} />)
    expect(bottomRow()).toHaveAttribute("data-variant", "tabs")
    expect(screen.getByRole("tablist", { name: "Sections" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Copy password" })).toBeNull()
  })
})

describe("while the on-screen keyboard is up (brief X6)", () => {
  /** A touch screen, and a keyboard over the lower half of the 844 px page once a field has focus. */
  function keyboard() {
    setMedia({ "(pointer: coarse)": true })
    Object.defineProperty(window, "visualViewport", { configurable: true, value: { height: 420, width: 390, offsetTop: 0, scale: 1, addEventListener() {}, removeEventListener() {} } })
  }

  it("the bottom row hides, on a section's root and on a pushed screen", async () => {
    keyboard()
    const { unmount } = render(<Harness open={false} />)
    expect(screen.getByRole("tablist", { name: "Sections" })).toBeInTheDocument()
    // The field a list keeps for itself: focus it as a person would.
    const search = document.createElement("input")
    document.body.append(search)
    act(() => search.focus())
    expect(bottomRow()).toBeNull()
    expect(screen.queryByRole("tablist", { name: "Sections" })).toBeNull()
    act(() => search.blur())
    expect(screen.getByRole("tablist", { name: "Sections" })).toBeInTheDocument()
    search.remove()
    unmount()

    render(<Harness />)
    expect(bottomRow()).toHaveAttribute("data-variant", "detail")
    act(() => screen.getByRole("textbox", { name: "Notes" }).focus())
    expect(bottomRow()).toBeNull()
    // So the screen's own buttons stand in for it.
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument()
  })
})

describe("short screens and folds (brief X6, cross-app decisions §14)", () => {
  it("at 844 × 390 the tab bar stands vertical, in the trailing column with Search and create", () => {
    setWindow(844, 390)
    render(<Harness open={false} />)
    const tabs = screen.getByRole("tablist", { name: "Sections" })
    expect(tabs).toHaveAttribute("aria-orientation", "vertical")
    const column = railColumn()!
    expect(column).toContainElement(tabs)
    expect(within(column).getByRole("button", { name: "Search" })).toBeInTheDocument()
    expect(within(column).getByRole("button", { name: "New item" })).toBeInTheDocument()
  })

  it("at 844 × 390 a pushed screen's bottom row is the trailing column too", () => {
    setWindow(844, 390)
    render(<Harness />)
    const column = railColumn()!
    expect(within(column).getByRole("button", { name: "Copy password" })).toBeInTheDocument()
    expect(within(column).getByRole("button", { name: "Edit" })).toBeInTheDocument()
    expect(screen.queryByRole("tablist", { name: "Sections" })).toBeNull()
  })

  it("on a fold held like a book, list and detail split at the hinge, with Nothing open. in the far half", () => {
    setMedia({ "(horizontal-viewport-segments: 2)": true })
    Object.defineProperty(window, "viewport", { configurable: true, value: { segments: [{ width: 358, height: 512 }, { width: 358, height: 512 }] } })
    setWindow(717, 512)
    render(<Harness open={false} />)
    expect(document.querySelector('[data-slot="fold-panes"]')).toHaveAttribute("data-split", "columns")
    // Both halves show: the list is in reach, and the detail says nothing is open.
    const list = screen.getByText("rows").closest("section")!
    expect(list).not.toHaveAttribute("inert")
    expect(screen.getByText("Nothing open.")).toBeInTheDocument()
  })
})
