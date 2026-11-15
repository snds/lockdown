import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PlacesProvider } from "@/app/places"
import { SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { Status } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { THEME_KEY, followColorScheme } from "../theme"
import { WebSettingsView } from "./web-settings-view"

/*
 * Settings → Appearance → Theme in the web vault
 * (docs/plan/62-unicron-1-0.md §3): the same row as the desktop's, over
 * src/theme.ts, which keeps the choice in this browser.
 */

const STATUS = { account: null, devices: 1, devices_revoked: 0 } as unknown as Status

function browserPrefers(scheme: "light" | "dark") {
  window.matchMedia = ((query: string) => ({
    matches: query.includes("dark") ? scheme === "dark" : scheme === "light",
    media: query,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia
}

function mount() {
  return render(
    <ClientProvider value={fakeClient({})}>
      <SettingsProvider>
        {/* Appearance is one of Settings' sections now (doc 64): the test opens on it. */}
        <PlacesProvider value={{ nav: { area: "settings", view: "all", section: "appearance" }, surface: "web", go: () => {}, navigate: () => {} }}>
          <WebSettingsView status={STATUS} onChanged={() => {}} onNotice={() => {}} server="https://vault.example" login="sam" onLock={() => {}} onSignOut={() => {}} />
        </PlacesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("the web vault's Theme row", () => {
  beforeEach(() => {
    window.localStorage.clear()
    document.documentElement.classList.remove("dark")
    browserPrefers("light")
    followColorScheme()
  })

  it("shows System, Stone and Graphite, with System on until the person chooses", async () => {
    mount()
    const row = await screen.findByRole("group", { name: "Theme" })
    const chips = within(row).getAllByRole("radio")
    expect(chips.map((c) => c.textContent)).toEqual(["System", "Stone", "Graphite"])
    expect(within(row).getByRole("radio", { name: "System" })).toHaveAttribute("aria-checked", "true")
    expect(document.documentElement).not.toHaveClass("dark")
  })

  it("a chip changes the whole page at once and keeps the choice in this browser, with nothing to save", async () => {
    mount()
    const user = userEvent.setup()
    await user.click(await screen.findByRole("radio", { name: "Graphite" }))
    await waitFor(() => expect(screen.getByRole("radio", { name: "Graphite" })).toHaveAttribute("aria-checked", "true"))
    expect(document.documentElement).toHaveClass("dark")
    expect(window.localStorage.getItem(THEME_KEY)).toBe("graphite")
    // No toast and no error line: the choice is not a setting of the vault, so nothing was sent to be saved.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()

    await user.click(screen.getByRole("radio", { name: "System" }))
    await waitFor(() => expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute("aria-checked", "true"))
    expect(document.documentElement).not.toHaveClass("dark")
    expect(window.localStorage.getItem(THEME_KEY)).toBeNull()
  })
})
