import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"

import { TooltipProvider } from "@unicron/ui"

import { GateProvider, useGate } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { MasterDetail, PageColumn, PaneHeader } from "@/components/master-detail"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { QuickClientProvider, type HotkeyStatus } from "@/features/quick/quick-client"
import { fakeClient } from "@/test/fake-client"
import { fakeQuick } from "@/test/fake-quick"
import { TEMPLATES } from "@/test/templates-fixture"

import { platformKeys, shortcuts, useRowMenuKeys, type RowMenuFrom } from "./keys"
import { placeIdOf, usePlaces, type Nav } from "./places"
import { AppShell } from "./shell"

/*
 * The keys (brief X6, doc 65) at the desktop's width: each key does what the
 * list says, the list names every key, and Escape puts a layer away without
 * ever saying yes.
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

function Screen() {
  const { nav } = usePlaces()
  if (nav.area === "vault" || nav.area === "search") {
    const title = nav.area === "search" ? "Search" : "All items"
    return <MasterDetail header={<PaneHeader title={title} />} title={title} list={<input aria-label="Filter" />} detail={null} onBack={() => {}} emptyLabel="Pick an item." />
  }
  return (
    <PageColumn>
      <PaneHeader title={nav.area} />
    </PageColumn>
  )
}

const HOTKEY: HotkeyStatus = { on: true, registered: true, label: "Ctrl+Shift+Space" }

function Harness({ hotkey = HOTKEY }: { hotkey?: HotkeyStatus | null }) {
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  return (
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]) })}>
      <QuickClientProvider value={fakeQuick([ROW], { hotkeyStatus: () => Promise.resolve(hotkey) })}>
        <TooltipProvider>
          <SettingsProvider>
            <TemplatesProvider>
              <GateProvider>
                <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>
                  <output data-testid="place">{placeIdOf(nav)}</output>
                  <AppShell nav={nav} onNavigate={setNav} rows={[ROW]} onLock={() => {}} onSaved={() => {}}>
                    <Screen />
                  </AppShell>
                </VaultsProvider>
              </GateProvider>
            </TemplatesProvider>
          </SettingsProvider>
        </TooltipProvider>
      </QuickClientProvider>
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

const where = () => screen.getByTestId("place").textContent
const keysList = () => screen.findByRole("dialog", { name: "Keyboard shortcuts" })
/** The keys of each row of the list, as its label reads them. */
const listed = (dialog: HTMLElement) => within(within(dialog).getByRole("list", { name: "Keys" })).getAllByRole("listitem").map((li) => Array.from(li.querySelectorAll('[data-slot="kbd"]')).map((k) => k.textContent))

describe("the keys at 1440 (brief X6)", () => {
  beforeEach(() => {
    window.localStorage.clear()
    setWindow(1440, 900)
  })

  it("/ opens Search, ⌘, and Ctrl+, open Settings, from anywhere but a field", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await user.keyboard("/")
    expect(where()).toBe("search")
    // Typed in a field, `/` is a letter.
    await user.click(screen.getByRole("treeitem", { name: /^All items/ }))
    await user.click(screen.getByRole("textbox", { name: "Filter" }))
    await user.keyboard("/")
    expect(where()).toBe("all")
    expect(screen.getByRole("textbox", { name: "Filter" })).toHaveValue("/")
    await user.keyboard("{Meta>},{/Meta}")
    expect(where()).toBe("settings")
    await user.click(screen.getByRole("treeitem", { name: /^All items/ }))
    await user.keyboard("{Control>},{/Control}")
    expect(where()).toBe("settings")
  })

  it("? opens the keys list: a form-sized dialog, KEYS over its title, and no primary", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await user.keyboard("?")
    const dialog = await keysList()
    expect(within(dialog).getByText("Keys")).toBeInTheDocument()
    // It asks nothing: no button but its close.
    expect(within(dialog).getAllByRole("button").map((b) => b.getAttribute("aria-label") ?? b.textContent)).toEqual(["Close"])
    // `?` again changes nothing, and opens no second layer.
    await user.keyboard("?")
    expect(screen.getAllByRole("dialog")).toHaveLength(1)
  })

  it("the list names every key, as this computer's keyboard writes it", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await user.keyboard("?")
    const dialog = await keysList()
    await waitFor(() => expect(within(dialog).getByText("Ctrl+Shift+Space")).toBeInTheDocument())
    expect(listed(dialog)).toEqual([["Ctrl+K"], ["Ctrl+Shift+Space"], ["Ctrl+Shift+L"], ["/"], ["Ctrl+,"], ["Ctrl+\\"], ["?"], ["↑ ↓", "Enter"], ["Menu", "Shift+F10"], ["Esc"]])
    for (const does of ["Quick access: items, places and Lock", "The quick-access window, from any app", "Fill in the browser, with the extension", "Search", "Settings", "Collapse or show the sidebar", "This list", "Move in a list; open the row", "The row's menu, as a right click opens it", "Close the top layer. In Search, clear the field, then leave"])
      expect(within(dialog).getByText(does)).toBeInTheDocument()
  })

  it("on a Mac it writes the Mac's keys", () => {
    const keys = shortcuts({ mac: true, quickWindow: { label: "⌥Space" } }).map((s) => s.keys)
    expect(keys).toEqual([["⌘K"], ["⌥Space"], ["⌘⇧L"], ["/"], ["⌘,"], ["⌘\\"], ["?"], ["↑ ↓", "↵"], ["⇧F10"], ["Esc"]])
  })

  it("never shows a Mac glyph off a Mac: the key's own words joined by + (Lockdown 12)", () => {
    expect(platformKeys("⌘⇧Space", false)).toBe("Ctrl+Shift+Space")
    expect(platformKeys("⌥ Space", false)).toBe("Alt+Space")
    expect(platformKeys("Ctrl+Shift+Space", false)).toBe("Ctrl+Shift+Space")
    expect(platformKeys("⌘⇧Space", true)).toBe("⌘⇧Space")
    const keys = shortcuts({ mac: false, quickWindow: { label: "⌘⇧Space" } }).map((s) => s.keys)
    expect(keys[1]).toEqual(["Ctrl+Shift+Space"])
    expect(keys.flat().join(" ")).not.toMatch(/[⌘⇧⌥⌃↵]/)
  })

  it("the default on Windows and Linux is Ctrl+Shift+Space, never Alt+Space (the window menu)", () => {
    expect(HOTKEY.label).toBe("Ctrl+Shift+Space")
    expect(HOTKEY.label).not.toMatch(/Alt\+Space/)
  })

  it("says when the system-wide shortcut is off, and leaves it out where there is no quick-access window", async () => {
    const user = userEvent.setup()
    const { unmount } = render(<Harness hotkey={{ ...HOTKEY, on: false }} />)
    await user.keyboard("?")
    expect(await within(await keysList()).findByText("Off. Turn it on in Settings, Quick access.")).toBeInTheDocument()
    unmount()
    render(<Harness hotkey={null} />)
    await user.keyboard("?")
    const dialog = await keysList()
    await waitFor(() => expect(listed(dialog)).toHaveLength(9))
    expect(within(dialog).queryByText("The quick-access window, from any app")).toBeNull()
  })

  it("? typed in a field is a letter", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await user.click(screen.getByRole("textbox", { name: "Filter" }))
    await user.keyboard("?")
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("Escape closes the keys list and gives focus back", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const row = screen.getByRole("treeitem", { name: /^All items/ })
    act(() => row.focus())
    await user.keyboard("?")
    await keysList()
    await user.keyboard("{Escape}")
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(row).toHaveFocus()
    // The keys wait while a layer is open: `/` inside the list goes nowhere.
    await user.keyboard("?")
    await keysList()
    await user.keyboard("/")
    expect(where()).toBe("all")
  })
})

describe("Escape never confirms", () => {
  /** Runs a call the core gates, and reports how it ended. */
  function Gated({ detail, onDone }: { detail: "window" | "password"; onDone: (how: string) => void }) {
    const { run } = useGate()
    const [calls, setCalls] = useState(0)
    const go = () => {
      let n = 0
      run(async (gate) => {
        n += 1
        setCalls(n)
        if (gate.confirmed || gate.password) return "done"
        throw { code: "approval_needed", message: "Export needs a check.", detail }
      })
        .then(onDone)
        .catch((e: { message: string }) => onDone(e.message))
    }
    return (
      <>
        <button onClick={go}>Export</button>
        <output data-testid="calls">{calls}</output>
      </>
    )
  }

  it.each(["window", "password"] as const)("the gate's %s check: Escape is Cancel, whichever control has focus", async (detail) => {
    const user = userEvent.setup()
    const onDone = vi.fn()
    render(
      <ClientProvider value={fakeClient({})}>
        <TooltipProvider>
          <GateProvider>
            <Gated detail={detail} onDone={onDone} />
          </GateProvider>
        </TooltipProvider>
      </ClientProvider>,
    )
    await user.click(screen.getByRole("button", { name: "Export" }))
    // The window check is unicron's ConfirmDialog (its yes is the action's verb, "Continue" when the core names none); the password gate is unicron's CrownJewelGate.
    const dialog = await waitFor(() => {
      const layer = document.querySelector<HTMLElement>('[role="dialog"], [role="alertdialog"]')
      expect(layer).not.toBeNull()
      return layer!
    })
    if (detail === "window") act(() => within(dialog).getByRole("button", { name: "Continue" }).focus())
    else await user.type(dialog.querySelector<HTMLInputElement>('input[type="password"]')!, "hunter2")
    await user.keyboard("{Escape}")
    await waitFor(() => expect(onDone).toHaveBeenCalledWith("Cancelled; nothing happened."))
    // The call ran once, without the check: nothing was confirmed.
    expect(screen.getByTestId("calls")).toHaveTextContent("1")
  })
})

describe("the row-menu keys (cross-app decisions §10)", () => {
  function Row({ onOpen }: { onOpen: (from: RowMenuFrom) => void }) {
    const menu = useRowMenuKeys(onOpen)
    return (
      <div role="listitem" tabIndex={0} onKeyDown={menu.onKeyDown} onContextMenu={menu.onContextMenu} aria-keyshortcuts={menu.keyshortcuts}>
        Northwind Mail
      </div>
    )
  }

  it("the Menu key, Shift+F10 and a right click open the row's menu; F10 alone does not", async () => {
    const user = userEvent.setup()
    const onOpen = vi.fn()
    render(<Row onOpen={onOpen} />)
    const row = screen.getByRole("listitem")
    expect(row).toHaveAttribute("aria-keyshortcuts", "ContextMenu Shift+F10")
    act(() => row.focus())
    await user.keyboard("{ContextMenu}")
    expect(onOpen).toHaveBeenLastCalledWith({ row, point: null })
    await user.keyboard("{Shift>}{F10}{/Shift}")
    expect(onOpen).toHaveBeenCalledTimes(2)
    expect(onOpen).toHaveBeenLastCalledWith({ row, point: null })
    await user.keyboard("{F10}")
    expect(onOpen).toHaveBeenCalledTimes(2)
    await new Promise((r) => setTimeout(r, 550))
    await user.pointer({ keys: "[MouseRight]", target: row, coords: { clientX: 40, clientY: 12 } })
    expect(onOpen).toHaveBeenCalledTimes(3)
    expect(onOpen).toHaveBeenLastCalledWith({ row, point: { x: 40, y: 12 } })
  })

  it("the contextmenu a browser sends after the key opens no second menu", async () => {
    const user = userEvent.setup()
    const onOpen = vi.fn()
    render(<Row onOpen={onOpen} />)
    const row = screen.getByRole("listitem")
    act(() => row.focus())
    await user.keyboard("{Shift>}{F10}{/Shift}")
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }))
    expect(onOpen).toHaveBeenCalledTimes(1)
  })
})
