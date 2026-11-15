import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { ThemeProvider } from "next-themes"

import { PlacesProvider, type Nav } from "@/app/places"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { DeviceInfo, Settings, Status } from "@/core/types"
import { LiveClientProvider, type LiveClient, type SessionView } from "@/features/sessions/live-client"
import { FirstRunClientProvider } from "@/features/unlock/first-run-client"
import { fakeClient } from "@/test/fake-client"
import { fakeFirstRun } from "@/test/fake-first-run"

import { findSettings } from "./settings-index"
import { SettingsView } from "./settings-view"
import { firstDevices, kitWords } from "./this-device"

/*
 * Settings (brief T0) and This Mac (brief T1, doc 79): every setting saves
 * at once and says nothing; a save the core refuses leaves the control on
 * its old value with one critical line in the core's words.
 */

const NOW = Math.floor(Date.now() / 1000)
const DEVICES: DeviceInfo[] = [
  { id: "d-ipad", name: "iPad", platform: "ios", enrolled_at: NOW - 86_400 * 30, revoked_at: null, this_device: false },
  { id: "d-mac", name: "MacBook", platform: "macos", enrolled_at: NOW - 86_400 * 90, revoked_at: null, this_device: true },
  { id: "d-old", name: "Old phone", platform: "android", enrolled_at: NOW - 86_400 * 400, revoked_at: NOW - 86_400 * 100, this_device: false },
  { id: "d-pc", name: "Work PC", platform: "windows", enrolled_at: NOW - 86_400 * 3, revoked_at: null, this_device: false },
  { id: "d-tab", name: "Tablet", platform: "android", enrolled_at: NOW - 86_400 * 2, revoked_at: null, this_device: false },
]
const STATUS = {
  recovery: "made",
  devices: 4,
  devices_revoked: 1,
  account: { server: "https://vault.rivera.example", login: "sam", local: false },
} as unknown as Status

function session(id: string, current: boolean): SessionView {
  return { id, device_id: id, device_name: id, platform: null, address: null, client: null, created_at: NOW, last_seen: NOW, expires_at: NOW + 3600, current }
}

interface Mount {
  settings?: Partial<Settings>
  client?: Partial<Client>
  nav?: Partial<Nav>
  live?: Partial<LiveClient>
  status?: Status
}

function mount({ settings = {}, client = {}, nav = {}, live, status = STATUS }: Mount = {}) {
  const saved: Settings[] = []
  const went: string[] = []
  const notices: string[] = []
  let current: Settings = { ...DEFAULT_SETTINGS, ...settings }
  const fake = fakeClient({
    getSettings: () => Promise.resolve(current),
    setSettings: (next) => {
      saved.push(next)
      current = next
      return Promise.resolve(next)
    },
    lockState: () => Promise.resolve({ locked: false, reason: null, idle_secs: 0, idle_timeout_secs: current.idle_lock_secs, hidden_timeout_secs: current.hidden_lock_secs }),
    approvalRoute: () => Promise.resolve({ route: "platform", method: "Touch ID" }),
    deviceList: () => Promise.resolve(DEVICES),
    probe: () => Promise.resolve({ name: "Sam" } as never),
    ...client,
  })
  const liveClient = live ? ({ sessionList: () => Promise.resolve([]), sessionEndOthers: () => Promise.resolve(0), ...live } as LiveClient) : null
  const view = render(
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <ClientProvider value={fake}>
        <FirstRunClientProvider value={fakeFirstRun({ setName: (n) => Promise.resolve(n) })}>
          <LiveClientProvider value={liveClient}>
            <SettingsProvider>
              <PlacesProvider value={{ nav: { area: "settings", view: "all", section: "this-device", ...nav }, surface: "desktop", go: (id) => went.push(id), navigate: () => {} }}>
                <SettingsView status={status} onStatus={() => {}} lock={null} onLock={() => {}} onChanged={() => {}} onNotice={(t) => notices.push(t)} />
              </PlacesProvider>
            </SettingsProvider>
          </LiveClientProvider>
        </FirstRunClientProvider>
      </ClientProvider>
    </ThemeProvider>,
  )
  return { ...view, saved, went, notices }
}

const row = (name: string) => screen.getByRole("group", { name })

beforeEach(() => {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia
})

describe("This Mac", () => {
  it("draws Security, Passkeys, Devices and Account in one column, with the drawn rows' values", async () => {
    mount({ settings: { idle_lock_secs: 300, clipboard_clear_secs: 90 } })
    for (const group of ["Security", "Passkeys", "Devices", "Account"]) expect(await screen.findByRole("heading", { name: new RegExp(`^${group}`) })).toBeInTheDocument()
    await waitFor(() => expect(row("Auto-lock")).toHaveTextContent("After 5 minutes"))
    expect(row("Clipboard")).toHaveTextContent("Clears after 90 s")
    await waitFor(() => expect(row("Approvals")).toHaveTextContent("Touch ID"))
    expect(screen.getByRole("switch", { name: "Offer Lockdown's passkeys in browsers" })).toBeChecked()
    // The deferred system-wide provider (D122) is not offered.
    expect(screen.queryByText(/passkey provider/i)).not.toBeInTheDocument()
    expect(screen.getByText("The vault also locks when this computer wakes from sleep.")).toBeInTheDocument()
    expect(row("Recovery kit")).toHaveTextContent("Made, not tested")
  })

  it("shows quick unlock as a read-only row with the core's reason where it doesn't work yet", async () => {
    mount()
    await waitFor(() => expect(row("Touch ID")).toHaveTextContent("Not yet"))
    expect(row("Touch ID")).toHaveAccessibleDescription("Touch ID unlock needs the signed Lockdown app.")
    expect(within(row("Touch ID")).queryByRole("switch")).not.toBeInTheDocument()
  })

  it("Auto-lock: CHANGE opens the seven choices with the current one marked, and a pick saves at once and silently", async () => {
    const m = mount({ settings: { idle_lock_secs: 300 } })
    const user = userEvent.setup()
    await waitFor(() => expect(row("Auto-lock")).toHaveTextContent("After 5 minutes"))
    await user.click(within(row("Auto-lock")).getByRole("button", { name: /Change/ }))
    const menu = await screen.findByRole("menu")
    const items = within(menu).getAllByRole("menuitemradio")
    expect(items.map((i) => i.textContent)).toEqual(["1 minute", "5 minutes", "10 minutes", "30 minutes", "1 hour", "4 hours", "Never"])
    expect(within(menu).getByRole("menuitemradio", { name: "5 minutes" })).toHaveAttribute("aria-checked", "true")
    await user.click(within(menu).getByRole("menuitemradio", { name: "Never" }))
    await waitFor(() => expect(m.saved.at(-1)?.idle_lock_secs).toBeNull())
    await waitFor(() => expect(row("Auto-lock")).toHaveTextContent("Never"))
    expect(m.notices).toEqual([])
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("each switch and the Reveal choice save at once", async () => {
    const m = mount()
    const user = userEvent.setup()
    await user.click(await screen.findByRole("switch", { name: "Lock when the window is out of view" }))
    await waitFor(() => expect(m.saved.at(-1)?.hidden_lock_secs).toBeNull())
    await user.click(screen.getByRole("switch", { name: "Offer Lockdown's passkeys in browsers" }))
    await waitFor(() => expect(m.saved.at(-1)?.browser_passkeys).toBe(false))
    await user.click(screen.getByRole("radio", { name: "Tap to reveal" }))
    await waitFor(() => expect(m.saved.at(-1)?.reveal).toBe("toggle"))
    expect(screen.getByRole("radio", { name: "Tap to reveal" })).toHaveAccessibleDescription(/30 seconds pass/)
    expect(m.notices).toEqual([])
  })

  it("a save the core refuses puts the old value back, with one critical line in the core's words", async () => {
    const m = mount({ settings: { clipboard_clear_secs: 30 }, client: { setSettings: () => Promise.reject({ code: "refused", message: "settings.cbor couldn't be written: the disk is full" }) } })
    const user = userEvent.setup()
    await waitFor(() => expect(row("Clipboard")).toHaveTextContent("Clears after 30 s"))
    await user.click(within(row("Clipboard")).getByRole("button", { name: /Change/ }))
    await user.click(await screen.findByRole("menuitemradio", { name: "90 seconds" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("settings.cbor couldn't be written: the disk is full")
    expect(screen.getAllByRole("alert")).toHaveLength(1)
    expect(row("Clipboard")).toHaveTextContent("Clears after 30 s")
    // A switch keeps its old position too.
    const sw = screen.getByRole("switch", { name: "Lock when the window is out of view" })
    expect(sw).toBeChecked()
    await user.click(sw)
    await waitFor(() => expect(screen.getAllByRole("alert")).toHaveLength(2))
    expect(sw).toBeChecked()
    expect(m.saved).toEqual([])
  })

  it("lists the first three devices, this one first, with REVOKE behind a Question; ALL opens Devices", async () => {
    const revoked: string[] = []
    const m = mount({ client: { deviceRevoke: (id) => (revoked.push(id), Promise.resolve(["Revoked iPad."])) } })
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: /^Devices/ })).toHaveTextContent("04")
    await waitFor(() => expect(row("MacBook")).toHaveTextContent("Now"))
    expect(within(row("MacBook")).queryByRole("button", { name: /Revoke/ })).not.toBeInTheDocument()
    expect(row("iPad")).toHaveTextContent(/^iPad\s*Added/)
    expect(screen.queryByRole("group", { name: "Old phone" })).not.toBeInTheDocument()
    expect(screen.queryByRole("group", { name: "Tablet" })).not.toBeInTheDocument()

    await user.click(within(row("iPad")).getByRole("button", { name: /Revoke/ }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Revoke iPad?")
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(dialog).getByRole("button", { name: "Revoke" }))
    await waitFor(() => expect(revoked).toEqual(["d-ipad"]))
    await user.click(screen.getByRole("button", { name: "All" }))
    expect(m.went).toEqual(["settings-devices"])
  })

  it("firstDevices: live ones only, this one first, three at most", () => {
    expect(firstDevices(DEVICES).map((d) => d.id)).toEqual(["d-mac", "d-ipad", "d-pc"])
  })

  it("names the kit's state and opens Recovery; ends the other sessions behind a Question", async () => {
    let ended = 0
    const m = mount({ live: { sessionList: () => Promise.resolve([session("a", true), session("b", false), session("c", false)]), sessionEndOthers: () => ((ended += 1), Promise.resolve(2)) } })
    const user = userEvent.setup()
    await waitFor(() => expect(row("Other sessions")).toHaveTextContent("2 others"))
    await user.click(within(row("Recovery kit")).getByRole("button", { name: /Open/ }))
    expect(m.went).toEqual(["settings-recovery"])
    await user.click(within(row("Other sessions")).getByRole("button", { name: /End all/ }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("End all other sessions?")
    expect(dialog).toHaveTextContent("This device stays signed in.")
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(dialog).getByRole("button", { name: "End all" }))
    await waitFor(() => expect(ended).toBe(1))
    expect(m.notices).toEqual(["Ended 2 other sessions"])
    expect(["Tested", "Made, not tested", "None yet"]).toEqual(["drilled", "made", "none"].map(kitWords))
  })

  it("a local account shows the kit row only", async () => {
    mount({ status: { ...STATUS, account: { server: "local", login: "", local: true } } as unknown as Status, live: {} })
    await waitFor(() => expect(row("Recovery kit")).toBeInTheDocument())
    expect(screen.queryByRole("group", { name: "Other sessions" })).not.toBeInTheDocument()
  })

  it("changes the name on the lock screen in a name prompt with its own Save", async () => {
    mount()
    const user = userEvent.setup()
    await waitFor(() => expect(row("Your name")).toHaveTextContent("Sam"))
    await user.click(within(row("Your name")).getByRole("button", { name: /Change/ }))
    const dialog = await screen.findByRole("dialog")
    const field = within(dialog).getByLabelText("Name")
    await user.clear(field)
    await user.type(field, "Sam Rivera")
    await user.click(within(dialog).getByRole("button", { name: "Save name" }))
    await waitFor(() => expect(row("Your name")).toHaveTextContent("Sam Rivera"))
  })
})

describe("the Settings frame", () => {
  it("lists every section under its group, with the marks: Recovery while the kit is untested", async () => {
    mount()
    const nav = await screen.findByRole("navigation", { name: "Settings sections" })
    for (const group of ["This PC", "Account", "Vault", "Lockdown"]) expect(within(nav).getByRole("group", { name: group })).toBeInTheDocument()
    expect(within(nav).getAllByRole("button").length).toBe(14)
    expect(within(nav).getByRole("button", { name: /^Recovery/ })).toHaveAccessibleName(/kit not tested/)
    expect(within(nav).getByRole("button", { name: /^About/ })).not.toHaveAccessibleName(/update/)
  })

  it("Find a setting finds a setting by its name and goes to its section", async () => {
    const m = mount()
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Find a setting"), "clip")
    const found = await screen.findByRole("region", { name: "Settings found" })
    expect(within(found).getByRole("heading")).toHaveTextContent("01")
    await user.click(within(found).getByRole("button", { name: /Clipboard/ }))
    expect(m.went).toEqual(["settings-this-device"])
  })

  it("says so when nothing matches", async () => {
    mount()
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Find a setting"), "zebra")
    expect(await screen.findByText("No setting for zebra.")).toBeInTheDocument()
  })

  it("findSettings matches every word, in list order, on the web's own rows too", () => {
    const sections = [
      { section: "this-device" as const, label: "This browser" },
      { section: "appearance" as const, label: "Appearance", description: "Theme and density, kept on this device." },
    ]
    expect(findSettings("web", "sign out", sections).map((f) => f.name)).toEqual(["Sign out of this tab"])
    expect(findSettings("desktop", "theme", sections).map((f) => f.name)).toEqual(["Appearance", "Theme"])
    expect(findSettings("desktop", "  ", sections)).toEqual([])
  })

  it("Appearance: Density as two chips that save at once", async () => {
    const m = mount({ nav: { section: "appearance" } })
    const user = userEvent.setup()
    const density = await screen.findByRole("radiogroup", { name: "Density" })
    expect(within(density).getByRole("radio", { name: "Comfortable" })).toHaveAttribute("aria-checked", "true")
    await user.click(within(density).getByRole("radio", { name: "Compact" }))
    await waitFor(() => expect(m.saved.at(-1)?.density).toBe("compact"))
  })
})
