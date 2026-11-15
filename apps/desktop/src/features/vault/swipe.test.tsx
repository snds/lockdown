import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState, type ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import type { Nav } from "@/app/places"
import { SettingsProvider } from "@/app/settings"
import { AppShell } from "@/app/shell"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput, ItemSummary, VaultView as VaultInfo } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { swipeOffer } from "@/features/organization/row-menu"
import { SharingClientProvider } from "@/features/sharing/sharing-client"
import { fakeClient } from "@/test/fake-client"
import { fakeSharing } from "@/test/fake-sharing"
import { TEMPLATES } from "@/test/templates-fixture"

import { VaultView } from "./vault-view"

// Phone rows swipe (brief V1's table, rules.md 7.1, contract 6.4, States / 01). jsdom lays nothing
// out: every box is a 390 px phone row, and unicron's knobs fall back to their values (reveal 24,
// edge 24, tile 80, snap at half the tiles, a full swipe past 60% of the row: 234 px).
const WIDTH = 390

const VAULTS: VaultInfo[] = [{ id: "v1", name: "Personal", primary: true, items: 4, trash_days: 30, folders: [] }]

const row = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  kind: "login",
  state: "live",
  title: id,
  site: "",
  username: "",
  edited_at: 0,
  crown_jewel: false,
  methods: ["password"],
  passkeys: 0,
  codes: 0,
  expires_at: null,
  conflict_copy: false,
  warnings: [],
  views: [],
  subtitle: "",
  copy: { field_id: "pw", how: "secret", label: "password" },
  vault_id: "v1",
  folder: null,
  tags: [],
  favourite: false,
  ...over,
})

const base: Partial<Client> = {
  templates: () => Promise.resolve(TEMPLATES),
  userTemplates: () => Promise.resolve([]),
  approvalRoute: () => Promise.resolve({ route: "window" as const, method: "window" }),
  securityReport: () => Promise.reject({ code: "refused", message: "no report here" }),
}

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

/** The real shell around the real list, as the app mounts them. */
function Harness({ rows, start = { area: "vault", view: "all" }, onNotice }: { rows: ItemSummary[]; start?: Nav; onNotice: (text: string) => void }) {
  const [nav, setNav] = useState<Nav>(start)
  return (
    <AppShell nav={nav} onNavigate={setNav} rows={rows} onLock={() => {}} onSaved={() => {}}>
      {nav.area === "vault" ? <VaultView view={nav.view} rows={rows} error={null} onChanged={() => {}} onNotice={onNotice} selected={null} onSelect={() => {}} onPalette={() => {}} scope={nav.scope} /> : <p>elsewhere</p>}
    </AppShell>
  )
}

function mount(over: Partial<Client>, rows: ItemSummary[], start?: Nav) {
  const onNotice = vi.fn<(text: string) => void>()
  const ui: ReactNode = <Harness rows={rows} start={start} onNotice={onNotice} />
  render(
    <ClientProvider value={fakeClient({ ...base, ...over })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <SharingClientProvider value={fakeSharing({})}>
                <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>{ui}</VaultsProvider>
              </SharingClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>,
  )
  return { onNotice }
}

/** The swipe row around the list row named `title`. */
async function swipeRowOf(title: string): Promise<HTMLElement> {
  const button = await screen.findByRole("button", { name: new RegExp(`^${title}\\b`) })
  const root = button.closest<HTMLElement>('[data-slot="swipe-row"]')
  if (!root) throw new Error(`${title} does not swipe`)
  return root
}

const content = (root: HTMLElement) => root.querySelector<HTMLElement>('[data-slot="swipe-row-content"]')!
const offsetOf = (root: HTMLElement) => Number(/translateX\((-?[\d.]+)px\)/.exec(content(root).style.transform)?.[1] ?? 0)
/** The tiles a swipe shows, from the row outwards. */
const tilesOf = (root: HTMLElement, side: "leading" | "trailing") => Array.from(root.querySelectorAll(`[data-slot="swipe-row-${side}"] [data-slot="swipe-action"]`)).map((t) => t.textContent)

/** A finger's drag from `from` to `to` (client x), in steps, then let go. */
function drag(el: HTMLElement, from: number, to: number) {
  fireEvent.pointerDown(el, { button: 0, pointerId: 1, isPrimary: true, clientX: from, clientY: 40 })
  for (let i = 1; i <= 8; i++) fireEvent.pointerMove(el, { pointerId: 1, clientX: from + ((to - from) * i) / 8, clientY: 40 })
  fireEvent.pointerUp(el, { pointerId: 1, clientX: to, clientY: 40 })
}
/** A full swipe to the left: past 60% of the row. */
const fullLeft = (el: HTMLElement) => drag(el, 360, 40)
/** Opened to the left, short of a full swipe: past half the tiles, under 234 px. */
const openLeft = (el: HTMLElement) => drag(el, 360, 200)

let rect: ReturnType<typeof vi.spyOn>
beforeEach(() => {
  window.localStorage.clear()
  setWindow(WIDTH, 844)
  rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ x: 0, y: 0, top: 0, left: 0, bottom: 92, right: WIDTH, width: WIDTH, height: 92, toJSON: () => ({}) }) as DOMRect,
  )
})
afterEach(() => {
  rect.mockRestore()
  setWindow(1024, 768)
})

describe("what each row's swipe offers (brief V1's table)", () => {
  it("names the tiles of every kind of row", () => {
    expect(swipeOffer(row("a", { codes: 1 }))).toEqual({ leading: "favorite", trailing: ["more", "code", "copy"], full: true })
    expect(swipeOffer(row("a", { favourite: true }))).toEqual({ leading: "unfavorite", trailing: ["more", "copy"], full: true })
    expect(swipeOffer(row("a", { copy: null }))).toEqual({ leading: "favorite", trailing: ["more"], full: false })
    expect(swipeOffer(row("a", { crown_jewel: true }))).toEqual({ leading: "favorite", trailing: ["more", "copy"], full: true })
    expect(swipeOffer(row("a", { state: "archived" }))).toEqual({ leading: null, trailing: ["more", "unarchive"], full: true })
    expect(swipeOffer(row("a", { state: "trashed", codes: 1 }))).toEqual({ leading: null, trailing: ["more", "restore"], full: true })
    expect(swipeOffer(row("a", { access: "fill-only" }))).toEqual({ leading: null, trailing: [], full: false })
    // A code item's quick copy is its code: one tile.
    expect(swipeOffer(row("a", { kind: "code", codes: 1, copy: { field_id: "code.0", how: "code", label: "code" } })).trailing).toEqual(["more", "copy"])
  })
})

describe("phone row swipes (States / 01, rules.md 7.3)", () => {
  it("draws MORE, CODE and the quick copy outermost and primary on the left, FAVORITE on the right", async () => {
    mount({}, [row("Kestrel", { codes: 1 })])
    const root = await swipeRowOf("Kestrel")
    openLeft(root)
    expect(offsetOf(root)).toBe(-240)
    expect(tilesOf(root, "trailing")).toEqual(["More", "Code", "Password"])
    const tiles = root.querySelectorAll('[data-slot="swipe-row-trailing"] [data-slot="swipe-action"]')
    expect(tiles[2]).toHaveAttribute("data-tone", "primary")
    expect(tiles[0]).toHaveAttribute("data-tone", "neutral")
    fireEvent.keyDown(root, { key: "Escape" })
    // A right swipe, clear of the back edge, reaches FAVORITE.
    drag(root, 60, 150)
    expect(tilesOf(root, "leading")).toEqual(["Favorite"])
  })

  it("copies on a full swipe, through the core, with X2's toast", async () => {
    const copySecret = vi.fn<Client["copySecret"]>(() => Promise.resolve({ clears_in_secs: 30, copy: 1 }))
    const { onNotice } = mount({ copySecret }, [row("Kestrel")])
    fullLeft(await swipeRowOf("Kestrel"))
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("Kestrel", "pw", 30, {}))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Copied the password for Kestrel, clears in 30 s"))
  })

  it("never yields a gated item's value from a swipe alone: the gate asks first", async () => {
    const copySecret = vi.fn<Client["copySecret"]>((_id, _f, _s, gate?: GateInput) =>
      gate?.confirmed ? Promise.resolve({ clears_in_secs: 30, copy: 2 }) : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to copy this protected value" }),
    )
    const { onNotice } = mount({ copySecret }, [row("Cold wallet", { crown_jewel: true, copy: { field_id: "seed", how: "secret", label: "recovery phrase" } })])
    fullLeft(await swipeRowOf("Cold wallet"))
    const question = await screen.findByRole("dialog", { name: "Copy this protected value?" })
    // The swipe's own call carried no proof, and the core refused it: nothing was copied yet.
    expect(copySecret).toHaveBeenCalledTimes(1)
    expect(copySecret.mock.calls[0]![3]).toEqual({})
    expect(onNotice).not.toHaveBeenCalled()
    // Escape never approves.
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.keyboard("{Escape}")
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Nothing was copied"))
    expect(copySecret).toHaveBeenCalledTimes(1)
    // Asked again, a yes copies.
    fullLeft(await swipeRowOf("Cold wallet"))
    await user.click(within(await screen.findByRole("dialog", { name: "Copy this protected value?" })).getByRole("button", { name: "Copy" }))
    await waitFor(() => expect(copySecret).toHaveBeenLastCalledWith("Cold wallet", "seed", 30, { confirmed: true }))
    expect(onNotice).toHaveBeenLastCalledWith("Copied the recovery phrase for Cold wallet, clears in 30 s")
    expect(question).toBeDefined()
  })

  it("unarchives an archived row on a full swipe, and offers no favorite", async () => {
    const unarchive = vi.fn<Client["unarchive"]>(() => Promise.resolve())
    const copySecret = vi.fn<Client["copySecret"]>()
    const { onNotice } = mount({ unarchive, copySecret }, [row("Old gym", { state: "archived", views: ["archived"] })], { area: "vault", view: "archived" })
    const root = await swipeRowOf("Old gym")
    openLeft(root)
    expect(tilesOf(root, "trailing")).toEqual(["More", "Unarchive"])
    fireEvent.keyDown(root, { key: "Escape" })
    drag(root, 60, 300)
    expect(root.querySelector('[data-slot="swipe-row-leading"]')).toBeNull()
    fullLeft(root)
    await waitFor(() => expect(unarchive).toHaveBeenCalledWith("Old gym"))
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Unarchived Old gym"))
    expect(copySecret).not.toHaveBeenCalled()
  })

  it("offers no copy in Recently deleted: MORE and RESTORE, and a full swipe restores", async () => {
    const restore = vi.fn<Client["restore"]>(() => Promise.resolve())
    const copySecret = vi.fn<Client["copySecret"]>()
    const copyCode = vi.fn<Client["copyCode"]>()
    mount({ restore, copySecret, copyCode }, [row("gone", { state: "trashed", views: ["deleted"], codes: 1 })], { area: "vault", view: "deleted" })
    const root = await swipeRowOf("gone")
    openLeft(root)
    expect(tilesOf(root, "trailing")).toEqual(["More", "Restore"])
    fireEvent.keyDown(root, { key: "Escape" })
    fullLeft(root)
    await waitFor(() => expect(restore).toHaveBeenCalledWith("gone"))
    expect(copySecret).not.toHaveBeenCalled()
    expect(copyCode).not.toHaveBeenCalled()
  })

  it("runs nothing on a full swipe of a row with nothing to copy: MORE only, and the row stops open", async () => {
    const organize = vi.fn<Client["organize"]>(() => Promise.resolve(2))
    mount({ organize }, [row("Bike lock", { kind: "custom", copy: null, methods: [] })])
    const root = await swipeRowOf("Bike lock")
    fullLeft(root)
    expect(tilesOf(root, "trailing")).toEqual(["More"])
    expect(offsetOf(root)).toBe(-80)
    expect(screen.queryByRole("menu")).not.toBeInTheDocument()
    expect(organize).not.toHaveBeenCalled()
  })

  it("puts every swipe action in the row's ⋯ menu, once, and MORE opens that menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    mount({}, [row("Kestrel", { codes: 1 })])
    await user.click(await screen.findByRole("button", { name: "More for Kestrel" }))
    const menu = await screen.findByRole("menu")
    const items = within(menu).getAllByRole("menuitem").map((m) => m.textContent)
    for (const name of ["Favorite", "More", "Code", "Password"]) expect(items).toContain(name)
    // The swipe's FAVORITE stands for the menu's own: no second favorite.
    expect(items).not.toContain("Add to favorites")
    for (const name of ["Folder and tags…", "Archive", "History", "Move to Recently deleted"]) expect(items).toContain(name)
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument())
    // MORE, tapped on the open row, opens the same menu.
    const root = await swipeRowOf("Kestrel")
    openLeft(root)
    // The click that ends a drag is swallowed in its own task; a tap comes later.
    await act(() => new Promise((r) => setTimeout(r, 0)))
    fireEvent.click(within(root.querySelector<HTMLElement>('[data-slot="swipe-row-trailing"]')!).getByText("More"))
    expect(await screen.findByRole("menu")).toBeInTheDocument()
  })

  it("keeps the restore and unarchive in the menu once each when they swipe", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    mount({}, [row("gone", { state: "trashed", views: ["deleted"] })], { area: "vault", view: "deleted" })
    await user.click(await screen.findByRole("button", { name: "More for gone" }))
    const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((m) => m.textContent)
    expect(items.filter((i) => i === "Restore")).toHaveLength(1)
    expect(items).toEqual(["More", "Restore", "History", "Delete for good"])
  })

  it("does not swipe a fill-only share", async () => {
    mount({}, [row("Shared router", { access: "fill-only" })])
    const button = await screen.findByRole("button", { name: /^Shared router\b/ })
    expect(button.closest('[data-slot="swipe-row"]')).toBeNull()
  })

  it("snaps under reduced motion: no settle motion on the row", async () => {
    const matchMedia = window.matchMedia
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      writable: true,
      value: (query: string) => ({ matches: query.includes("reduce"), media: query, onchange: null, addEventListener: () => {}, removeEventListener: () => {}, addListener: () => {}, removeListener: () => {}, dispatchEvent: () => false }),
    })
    try {
      mount({}, [row("Kestrel")])
      const root = await swipeRowOf("Kestrel")
      openLeft(root)
      expect(offsetOf(root)).toBe(-160)
      expect(content(root).className).not.toMatch(/transition/)
    } finally {
      Object.defineProperty(window, "matchMedia", { configurable: true, writable: true, value: matchMedia })
    }
  })

  it("does not swipe beside the sidebar: the rows keep their ⋯ menu only", async () => {
    setWindow(1440, 900)
    mount({}, [row("Kestrel")])
    const button = await screen.findByRole("button", { name: /^Kestrel\b/ })
    expect(button.closest('[data-slot="swipe-row"]')).toBeNull()
  })
})
