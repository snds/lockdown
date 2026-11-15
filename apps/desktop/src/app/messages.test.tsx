import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { useState, type ReactNode } from "react"

import { AppFrame, AppFrameColumn, AppFrameMain, Toaster, TooltipProvider, toast } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { MasterDetail, PageColumn, PaneHeader } from "@/components/master-detail"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { awayNews } from "@/features/vault/use-away-reminder"
import { expiryNews } from "@/features/organization/expiry"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { MessageBoardProvider, BannerSlot, criticalBanner, firstCritical, useNews, type CriticalActions, type CriticalSources, type NewsItem } from "./banners"
import { copiedWords, notify, savedWords, toastWords } from "./notify"
import { usePlaces, type Nav } from "./places"
import { AppShell } from "./shell"
import { IDLE_ANNOUNCE, StatusLineItems, StatusProvider, StatusStrip, statusRow, useStatusItem, type LiveStatus, type StatusFact } from "./status"

/*
 * Brief X2: the five channels. Live status in the status line and the
 * strip, finished work in a toast, broken-and-fixable in one critical
 * Banner, news in one dismissable info Banner; never a toast for an
 * update, an error or the daily news. And the scrolled phone chrome of
 * States / 02.
 */

function setWindow(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width })
  Object.defineProperty(window, "innerHeight", { configurable: true, value: height })
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
}

async function settle() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
}

const toasts = () => Array.from(document.querySelectorAll<HTMLElement>("[data-sonner-toast]"))
const announcer = () => document.querySelector<HTMLElement>('[data-slot="app-frame-announcer"]')!
const statusLine = () => document.querySelector<HTMLElement>('[data-slot="app-frame-status-line-status"]')

const live = (over: Partial<LiveStatus> = {}): LiveStatus => ({ idleLeft: null, onStay: () => {}, work: null, ...over })

afterEach(() => {
  act(() => {
    toast.dismiss()
  })
  vi.useRealTimers()
  setWindow(1440, 900)
})

describe("live status: one at a time, the first that is true (X2 table 1)", () => {
  const fact: StatusFact = { text: "Refresh every 30 s" }
  it("puts the idle lock first, then the work, then the screen's own line", () => {
    expect(statusRow({ idleLeft: 8, work: "syncing" }, fact)).toEqual({ kind: "idle", left: 8 })
    expect(statusRow({ idleLeft: null, work: "syncing" }, fact)).toEqual({ kind: "work", work: "syncing" })
    expect(statusRow({ idleLeft: null, work: null }, fact)).toEqual({ kind: "screen", fact })
    expect(statusRow({ idleLeft: null, work: null }, null)).toBeNull()
  })

  function Line({ status, phone = false, children }: { status: LiveStatus; phone?: boolean; children?: ReactNode }) {
    return (
      <AppFrame layout={phone ? "phone" : "expanded"}>
        <StatusProvider live={status} fallback={{ text: "Health // 03 to fix" }}>
          <div data-slot="app-frame-status-line-status">
            <StatusLineItems />
          </div>
          <StatusStrip />
          {children}
        </StatusProvider>
      </AppFrame>
    )
  }

  it("on a desktop, counts the idle lock down in the live tone with ten segments and STAY UNLOCKED, said once", async () => {
    const stay = vi.fn()
    const { rerender } = render(<Line status={live({ idleLeft: 8, onStay: stay })} />)
    await settle()
    const item = statusLine()!.querySelector<HTMLElement>('[data-slot="status-item"]')!
    expect(item).toHaveAttribute("data-tone", "live")
    expect(item).toHaveTextContent("Locking in")
    expect(item).toHaveTextContent("// 08 S")
    expect(item.querySelector('[data-slot="status-item-countdown"]')).toHaveAttribute("aria-hidden", "true")
    expect(item.querySelector('[data-slot="status-item-progress"]')).toBeInTheDocument()
    fireEvent.click(within(item).getByRole("button", { name: "Stay unlocked" }))
    expect(stay).toHaveBeenCalledOnce()
    expect(announcer().textContent).toBe(IDLE_ANNOUNCE)
    // Each tick changes the drawn seconds and nothing that is said.
    for (const left of [7, 6, 5, 4]) {
      rerender(<Line status={live({ idleLeft: left, onStay: stay })} />)
      await settle()
    }
    expect(item).toHaveTextContent("// 04 S")
    // Said a second time, the region would read the words again with a trailing space.
    expect(announcer().textContent).toBe(IDLE_ANNOUNCE)
    // A status line is never yellow.
    expect(document.querySelector('[data-tone="needs-you"]')).toBeNull()
  })

  it("shows the work in words while it runs, then the screen's own line", async () => {
    const { rerender } = render(<Line status={live({ work: "opening" })} />)
    expect(statusLine()).toHaveTextContent("Opening your vault")
    rerender(<Line status={live({ work: "syncing" })} />)
    expect(statusLine()).toHaveTextContent("Syncing")
    rerender(<Line status={live()} />)
    expect(statusLine()).toHaveTextContent("Health // 03 to fix")
  })

  it("lets a screen say its own line while it shows, over the host's", () => {
    function Codes() {
      useStatusItem({ text: "Refresh every 30 s" })
      return null
    }
    const { rerender } = render(
      <Line status={live()}>
        <Codes />
      </Line>,
    )
    expect(statusLine()).toHaveTextContent("Refresh every 30 s")
    rerender(<Line status={live()} />)
    expect(statusLine()).toHaveTextContent("Health // 03 to fix")
  })

  it("on a phone, the idle lock is a live strip, LOCKING IN // 08 S over No activity, with STAY and the bar; said once", async () => {
    setWindow(390, 844)
    const stay = vi.fn()
    const { rerender } = render(<Line phone status={live({ idleLeft: 8, onStay: stay })} />)
    await settle()
    const strip = document.querySelector<HTMLElement>('[data-slot="accessory"]')!
    expect(strip).toHaveAttribute("data-tone", "live")
    expect(strip).toHaveTextContent("Locking in")
    expect(strip).toHaveTextContent("No activity")
    expect(strip.querySelector('[data-slot="progress"]')).toBeInTheDocument()
    fireEvent.click(within(strip).getByRole("button", { name: "Stay" }))
    expect(stay).toHaveBeenCalledOnce()
    for (const left of [7, 6, 5]) {
      rerender(<Line phone status={live({ idleLeft: left, onStay: stay })} />)
      await settle()
    }
    expect(announcer().textContent).toBe(IDLE_ANNOUNCE)
    rerender(<Line phone status={live({ work: "opening" })} />)
    expect(document.querySelector('[data-slot="accessory"]')).toHaveTextContent("Unlocking")
    expect(document.querySelector('[data-slot="accessory"]')).toHaveTextContent("Opening your vault")
    expect(document.querySelector('[data-slot="accessory"]')).toHaveAttribute("data-tone", "neutral")
    rerender(<Line phone status={live()} />)
    expect(document.querySelector('[data-slot="accessory"]')).toBeNull()
  })
})

describe("finished: a toast (X2 table 2)", () => {
  it("has no full stop and names the thing", () => {
    expect(toastWords("Saved.")).toBe("Saved")
    expect(toastWords("Checking…")).toBe("Checking…")
    expect(savedWords("Steam", false)).toBe("Saved Steam")
    expect(savedWords("Steam", true)).toBe("Saved Steam with a new password")
    expect(copiedWords("password", "Steam", 30)).toBe("Copied the password for Steam, clears in 30 s")
  })

  it("stands at the content's bottom left, three at most, each with one action at most", async () => {
    render(
      <AppFrame layout="expanded">
        <AppFrameMain>
          <AppFrameColumn />
        </AppFrameMain>
        <Toaster />
      </AppFrame>,
    )
    act(() => {
      notify("Archived Steam", { action: { label: "Undo", onClick: () => {} } })
      notify("Archived Spotify.")
      notify("Archived Stripe")
      notify("Archived Synology")
      notify("Archived Tailscale")
    })
    await settle()
    expect(toasts().filter((el) => el.getAttribute("data-visible") === "true")).toHaveLength(3)
    for (const el of toasts()) expect(el.querySelectorAll("button").length).toBeLessThanOrEqual(1)
    expect(screen.getByText("Archived Spotify")).toBeInTheDocument()
  })
})

const NONE: CriticalSources = { signIn: null, split: null, sync: null, backup: null }
const ACTS: CriticalActions = { signIn: vi.fn(), seeStatus: vi.fn(), tryAgain: vi.fn(), openBackups: vi.fn() }

function Banners({ sources, news = null, place = "items", onDismiss = () => {} }: { sources: CriticalSources; news?: NewsItem | null; place?: "items" | "all-items" | "sharing" | "status" | "backups"; onDismiss?: () => void }) {
  const kind = firstCritical(sources)
  return (
    <MessageBoardProvider critical={kind ? criticalBanner(kind, sources, ACTS) : null} news={news} onDismissNews={onDismiss}>
      <BannerSlot place={place} />
      <Toaster />
    </MessageBoardProvider>
  )
}

describe("broken and fixable: one critical Banner (X2 table 4)", () => {
  const cases: Array<[keyof CriticalSources, string, string, string, keyof CriticalActions]> = [
    ["signIn", "Your session was ended on another device.", "Lockdown isn't syncing", "Sign in", "signIn"],
    ["split", "Split view: the mirror at kt.witness.example disagrees.", "Split view", "See status", "seeStatus"],
    ["sync", "The server didn't answer.", "Couldn't sync", "Try again", "tryAgain"],
    ["backup", "The backup folder isn't there.", "The last backup didn't work", "Open backups", "openBackups"],
  ]

  it.each(cases)("%s shows exactly one, with its one action, and leaves when it clears; never a toast", async (key, words, title, action, act) => {
    const { rerender } = render(<Banners sources={{ ...NONE, [key]: words }} />)
    await settle()
    const alerts = screen.getAllByRole("alert")
    expect(alerts).toHaveLength(1)
    expect(alerts[0]).toHaveTextContent(title)
    // No close button: it cannot be dismissed.
    expect(within(alerts[0]!).getAllByRole("button")).toHaveLength(1)
    fireEvent.click(within(alerts[0]!).getByRole("button", { name: action }))
    expect(ACTS[act]).toHaveBeenCalled()
    expect(toasts()).toHaveLength(0)
    rerender(<Banners sources={NONE} />)
    expect(screen.queryByRole("alert")).toBeNull()
  })

  it("shows the first true one only, and in the section it belongs to", () => {
    const all: CriticalSources = { signIn: "Sign in again.", split: "Split view: x.", sync: "No answer.", backup: "No folder." }
    const { rerender } = render(<Banners sources={all} />)
    expect(screen.getAllByRole("alert")).toHaveLength(1)
    expect(screen.getByRole("alert")).toHaveTextContent("Lockdown isn't syncing")
    rerender(<Banners sources={{ ...NONE, split: "Split view: x." }} place="sharing" />)
    expect(screen.getByRole("alert")).toHaveTextContent("Split view")
    // On Status the split view sits on the page its action would open: no action there.
    rerender(<Banners sources={{ ...NONE, split: "Split view: x." }} place="status" />)
    expect(within(screen.getByRole("alert")).queryByRole("button")).toBeNull()
    rerender(<Banners sources={{ ...NONE, backup: "No folder." }} place="sharing" />)
    expect(screen.queryByRole("alert")).toBeNull()
    rerender(<Banners sources={{ ...NONE, backup: "No folder." }} place="backups" />)
    expect(screen.getByRole("alert")).toHaveTextContent("The last backup didn't work")
  })
})

describe("news that can wait: one info Banner on All items (X2 table 5)", () => {
  function News({ items }: { items: NewsItem[] }) {
    const news = useNews()
    const [added, setAdded] = useState(false)
    if (!added) {
      setAdded(true)
      queueMicrotask(() => act(() => items.forEach(news.add)))
    }
    return <Banners sources={NONE} news={news.news} place="all-items" onDismiss={news.dismiss} />
  }

  beforeEach(() => window.localStorage.clear())

  it("shows one at a time, with SHOW and a close button, and a dismissed one stays away for the day; never a toast", async () => {
    const show = vi.fn()
    const items: NewsItem[] = [
      { id: "expiry", ...expiryNews([{ title: "Passport", days: 12 }, { title: "Visa card", days: 40 }])!, action: { label: "Show", onClick: show } },
      { id: "away", ...awayNews([{ title: "Netflix", place: "the hotel TV" }])!, action: { label: "Show", onClick: show } },
    ]
    const { unmount } = render(<News items={items} />)
    await settle()
    const banner = screen.getByRole("status")
    expect(banner).toHaveTextContent("2 items expire soon")
    expect(banner).toHaveTextContent("Passport expires in 12 days.")
    fireEvent.click(within(banner).getByRole("button", { name: "Show" }))
    expect(show).toHaveBeenCalledOnce()
    fireEvent.click(within(banner).getByRole("button", { name: "Dismiss" }))
    expect(screen.getByRole("status")).toHaveTextContent("Sign out of Netflix on the hotel TV")
    expect(toasts()).toHaveLength(0)
    unmount()
    // The next unlock today: the dismissed one stays away.
    render(<News items={items} />)
    await settle()
    expect(screen.getAllByRole("status")).toHaveLength(1)
    expect(screen.getByRole("status")).toHaveTextContent("Sign out of Netflix")
  })

  it("is not on Items' other views, Sharing or Status", () => {
    const item: NewsItem = { id: "expiry", title: "2 items expire soon", action: { label: "Show", onClick: () => {} } }
    for (const place of ["items", "sharing", "status"] as const) {
      const { unmount } = render(<Banners sources={NONE} news={item} place={place} />)
      expect(screen.queryByText("2 items expire soon")).toBeNull()
      unmount()
    }
  })
})

/* The shell itself: where the channels sit, the scrolled chrome, pull to refresh. */

const VAULTS: VaultView[] = [{ id: "v1", name: "Personal", primary: true, items: 0, trash_days: 30, folders: [] }]
const ROWS: ItemSummary[] = Array.from({ length: 12 }, (_, i) => ({
  id: `i${i}`,
  kind: "login",
  state: "live",
  title: `Login ${i}`,
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
}))

function Screen() {
  const { nav } = usePlaces()
  if (nav.area === "vault") return <MasterDetail header={<PaneHeader title="All items" />} title="All items" list={<p>rows</p>} detail={null} onBack={() => {}} emptyLabel="Pick an item." />
  return (
    <PageColumn>
      <PaneHeader title={nav.area === "codes" ? "Codes" : nav.area === "security" ? "Health" : "Settings"} />
    </PageColumn>
  )
}

function Shell({ status = live(), sources = NONE, onSync = () => Promise.resolve(), onCheck = () => Promise.resolve(), start = { area: "vault", view: "all" } as Nav }: { status?: LiveStatus; sources?: CriticalSources; onSync?: () => Promise<unknown>; onCheck?: () => Promise<unknown>; start?: Nav }) {
  const [nav, setNav] = useState<Nav>(start)
  const kind = firstCritical(sources)
  return (
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]) })}>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <VaultsProvider value={{ vaults: VAULTS, reload: () => {} }}>
                <StatusProvider live={status}>
                  <MessageBoardProvider critical={kind ? criticalBanner(kind, sources, ACTS) : null} news={null} onDismissNews={() => {}}>
                    <AppShell nav={nav} onNavigate={setNav} rows={ROWS} onLock={() => {}} onSaved={() => {}} onSync={onSync} onCheck={onCheck}>
                      <Screen />
                    </AppShell>
                  </MessageBoardProvider>
                </StatusProvider>
              </VaultsProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ClientProvider>
  )
}

describe("the shell carries the channels", () => {
  it("on a desktop: the critical Banner under the list's head, Check again in it, no status-line button and no toast for it", async () => {
    setWindow(1440, 900)
    const onSync = vi.fn(() => Promise.resolve())
    render(<Shell sources={{ ...NONE, sync: "The server didn't answer." }} onSync={onSync} />)
    await settle()
    expect(screen.getAllByRole("alert")).toHaveLength(1)
    fireEvent.click(screen.getByRole("button", { name: "Check again" }))
    expect(onSync).toHaveBeenCalledOnce()
    expect(statusLine()?.querySelectorAll("button") ?? []).toHaveLength(0)
    expect(toasts()).toHaveLength(0)
  })

  it("on a phone: the Banner under the page head, after the chips", async () => {
    setWindow(390, 844)
    render(<Shell sources={{ ...NONE, backup: "The backup folder isn't there." }} />)
    await settle()
    const head = document.querySelector<HTMLElement>('[data-slot="page-head"]')!.parentElement!
    const chips = within(head).getByRole("radiogroup", { name: "Show" })
    const alert = within(head).getByRole("alert")
    expect(chips.compareDocumentPosition(alert) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it("scrolled on a phone (States / 02): the small centred title, the mark only, the active tab alone, the strip in the bottom row, a hairline", async () => {
    setWindow(390, 844)
    render(<Shell status={live({ work: "syncing" })} />)
    await settle()
    const column = document.querySelector<HTMLElement>("[data-column][data-shown]")!
    const top = () => document.querySelector<HTMLElement>('[data-slot="app-frame-top-row"]')!
    const bottom = () => document.querySelector<HTMLElement>('[data-slot="app-frame-bottom-row"]')!
    expect(top()).not.toHaveAttribute("data-minimized")
    // Above the bottom row before the scroll.
    expect(bottom().querySelector('[data-slot="app-frame-strip"] [data-slot="accessory"]')).toBeInTheDocument()
    column.scrollTop = 400
    fireEvent.scroll(column)
    await settle()
    expect(top()).toHaveAttribute("data-minimized")
    expect(bottom()).toHaveAttribute("data-minimized")
    expect(within(bottom()).getByRole("tablist", { name: "Sections" })).toHaveAttribute("data-minimized")
    // The strip joined the row, beside the active tab; still two bars at most.
    expect(bottom().querySelector('[data-slot="app-frame-bottom-row-strip"] [data-slot="accessory"]')).toBeInTheDocument()
    expect(bottom().querySelector('[data-slot="app-frame-strip"] [data-slot="accessory"]')).toBeNull()
    // The small title is the switcher; the wordmark is only for a screen reader now.
    expect(within(top()).getByRole("button", { name: /Switch vault/ })).toBeInTheDocument()
    expect(top().querySelector('[data-slot="app-lockup-name"]')?.closest(".sr-only, [class*='sr-only']")).toBeTruthy()
  })

  it("pull to refresh syncs on Items and Codes and runs CHECK NOW on Health; elsewhere a column has none", async () => {
    setWindow(390, 844)
    const onSync = vi.fn(() => Promise.resolve())
    const onCheck = vi.fn(() => Promise.resolve())
    for (const [start, has] of [
      [{ area: "vault", view: "all" }, true],
      [{ area: "codes", view: "codes" }, true],
      [{ area: "security", view: "all" }, true],
      [{ area: "settings", view: "all" }, false],
    ] as Array<[Nav, boolean]>) {
      const { unmount } = render(<Shell start={start} onSync={onSync} onCheck={onCheck} />)
      await settle()
      const column = document.querySelector<HTMLElement>("[data-column][data-shown]")
      // A column with a refresh is ready for a pull at its top; one without leaves the pull to the browser.
      expect(column?.hasAttribute("data-pull-ready"), `${start.area}`).toBe(has)
      unmount()
    }
    // Health keeps its own CHECK NOW: no "Check again" there.
    render(<Shell start={{ area: "security", view: "all" }} onSync={onSync} onCheck={onCheck} />)
    expect(screen.queryByRole("button", { name: "Check again" })).toBeNull()
  })
})
