import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { ThemeProvider } from "next-themes"

import { SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { Status } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { SettingsView } from "./settings-view"
import { PlacesProvider } from "@/app/places"
import { ThemeSetting, type ThemeChoice } from "./theme-setting"

/*
 * Settings → Appearance → Theme (docs/plan/62-unicron-1-0.md §3): System,
 * Stone or Graphite, kept on this device. jsdom has no `matchMedia`, which
 * next-themes asks for the device's own setting; each test says what the
 * device prefers.
 */
function devicePrefers(scheme: "light" | "dark") {
  window.matchMedia = ((query: string) => ({
    matches: query.includes("dark") ? scheme === "dark" : scheme === "light",
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia
}

const chips = () => within(screen.getByRole("radiogroup", { name: "Theme" })).getAllByRole("radio")

describe("the Theme row", () => {
  it("is one row of three chips, one of them on, each saying light or dark only in its description", () => {
    render(<ThemeSetting value="system" onChange={() => {}} />)
    expect(screen.getByRole("group", { name: "Theme" })).toBeInTheDocument()
    expect(chips().map((c) => c.textContent)).toEqual(["System", "Stone", "Graphite"])
    expect(chips().map((c) => c.getAttribute("aria-checked"))).toEqual(["true", "false", "false"])
    expect(screen.getByRole("radio", { name: "System" })).toHaveAccessibleDescription("Follows your device's light or dark setting.")
    expect(screen.getByRole("radio", { name: "Stone" })).toHaveAccessibleDescription("Light")
    expect(screen.getByRole("radio", { name: "Graphite" })).toHaveAccessibleDescription("Dark")
    // The words on the chips are the grounds' names, never "light" or "dark".
    for (const chip of chips()) expect(chip.textContent).not.toMatch(/light|dark/i)
  })

  it("tells its owner the choice at once, by a press and by the arrow keys", async () => {
    const picked: ThemeChoice[] = []
    const { rerender } = render(<ThemeSetting value="system" onChange={(c) => picked.push(c)} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("radio", { name: "Graphite" }))
    expect(picked).toEqual(["graphite"])
    rerender(<ThemeSetting value="graphite" onChange={(c) => picked.push(c)} />)
    expect(screen.getByRole("radio", { name: "Graphite" })).toHaveAttribute("aria-checked", "true")
    // One of many: the arrow keys move and choose, and wrap.
    screen.getByRole("radio", { name: "Graphite" }).focus()
    await user.keyboard("{ArrowLeft}")
    expect(picked).toEqual(["graphite", "stone"])
    await user.keyboard("{ArrowRight}{ArrowRight}")
    expect(picked.at(-1)).toBe("system")
  })
})

describe("Settings → Appearance → Theme on the desktop", () => {
  const status = { account: null, devices: 1, devices_revoked: 0 } as unknown as Status

  function mount() {
    return render(
      <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
        <ClientProvider value={fakeClient({})}>
          <SettingsProvider>
            {/* Appearance is one of Settings' sections now (doc 64): the test opens on it. */}
            <PlacesProvider value={{ nav: { area: "settings", view: "all", section: "appearance" }, surface: "desktop", go: () => {}, navigate: () => {} }}>
              <SettingsView status={status} onStatus={() => {}} lock={null} onLock={() => {}} onChanged={() => {}} onNotice={() => {}} />
            </PlacesProvider>
          </SettingsProvider>
        </ClientProvider>
      </ThemeProvider>,
    )
  }

  beforeEach(() => {
    window.localStorage.clear()
    document.documentElement.classList.remove("dark", "light")
  })

  it("starts on System and follows the device", async () => {
    devicePrefers("dark")
    mount()
    const row = await screen.findByRole("group", { name: "Theme" })
    expect(within(row).getByRole("radio", { name: "System" })).toHaveAttribute("aria-checked", "true")
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"))
    expect(window.localStorage.getItem("theme")).toBeNull()
  })

  it("Stone and Graphite change the whole app at once and are kept on this device, where a new start reads them", async () => {
    devicePrefers("dark")
    const first = mount()
    const user = userEvent.setup()
    await user.click(await screen.findByRole("radio", { name: "Stone" }))
    await waitFor(() => expect(document.documentElement).not.toHaveClass("dark"))
    expect(screen.getByRole("radio", { name: "Stone" })).toHaveAttribute("aria-checked", "true")
    // next-themes' own store, in its own words: Stone is its light.
    expect(window.localStorage.getItem("theme")).toBe("light")

    await user.click(screen.getByRole("radio", { name: "Graphite" }))
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"))
    expect(window.localStorage.getItem("theme")).toBe("dark")

    // A new start (the app closed and opened again) on a device that prefers light: the choice holds.
    first.unmount()
    document.documentElement.classList.remove("dark", "light")
    devicePrefers("light")
    mount()
    expect(await screen.findByRole("radio", { name: "Graphite" })).toHaveAttribute("aria-checked", "true")
    await waitFor(() => expect(document.documentElement).toHaveClass("dark"))

    // Back to System: the device's own setting again.
    await user.click(screen.getByRole("radio", { name: "System" }))
    await waitFor(() => expect(document.documentElement).not.toHaveClass("dark"))
    expect(window.localStorage.getItem("theme")).toBe("system")
  })
})
