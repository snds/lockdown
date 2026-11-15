import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import { HygieneClientProvider, noHygieneClient, type MemoryReport } from "@/features/hygiene/hygiene-client"
import { fakeClient } from "@/test/fake-client"

import { noQuickClient, QuickClientProvider, type HotkeyStatus, type QuickClient } from "./quick-client"
import { memoryWords, QuickSettings } from "./quick-settings"

const MEMORY: MemoryReport = { supported: true, pages_locked: 48, page_size: 16384, pages_locked_ever: 112, refused: 0, core_dumps_off: true }

function mount(quick: Partial<QuickClient>, memory: MemoryReport | null = MEMORY) {
  return render(
    <ClientProvider value={fakeClient({ getSettings: () => Promise.resolve(DEFAULT_SETTINGS) })}>
      <SettingsProvider>
        <QuickClientProvider value={{ ...noQuickClient, ...quick }}>
          <HygieneClientProvider value={{ ...noHygieneClient, memory: () => Promise.resolve(memory) }}>
            <QuickSettings onNotice={() => {}} />
          </HygieneClientProvider>
        </QuickClientProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("Settings → Quick access (brief T6)", () => {
  it("names the shortcut as a key in the switch's label, and turns it off silently", async () => {
    let on = true
    const setHotkey = vi.fn((next: boolean): Promise<HotkeyStatus> => ((on = next), Promise.resolve({ on, registered: on, label: "⌥ Space" })))
    mount({ hotkeyStatus: () => Promise.resolve({ on, registered: true, label: "⌥ Space" }), setHotkey })
    const user = userEvent.setup()
    const toggle = await screen.findByRole("switch", { name: /Open quick access from anywhere/ })
    expect(toggle).toHaveAccessibleName("Open quick access from anywhere ⌥ Space")
    expect(screen.getByText("⌥ Space").tagName).toBe("KBD")
    await user.click(toggle)
    expect(setHotkey).toHaveBeenCalledWith(false)
    await waitFor(() => expect(toggle).not.toBeChecked())
    expect(screen.queryByRole("alert")).toBeNull()
  })

  it("says why this computer won't give the shortcut, and a refused change in the core's words", async () => {
    mount({
      hotkeyStatus: () => Promise.resolve({ on: true, registered: false, label: "⌥ Space", problem: "This Wayland session doesn't let apps set a shortcut." }),
      setHotkey: () => Promise.reject({ code: "refused", message: "The shortcut is taken by another app." }),
    })
    const user = userEvent.setup()
    const toggle = await screen.findByRole("switch", { name: /Open quick access/ })
    expect(toggle).toHaveAccessibleDescription(/doesn't let apps set a shortcut/)
    await user.click(toggle)
    expect(await screen.findByRole("alert")).toHaveTextContent("The shortcut is taken by another app.")
    expect(toggle).toBeChecked()
  })

  it("keeps what is out of swap in Details, shut", async () => {
    mount({ hotkeyStatus: () => Promise.resolve(null) })
    const user = userEvent.setup()
    const details = await screen.findByRole("button", { name: /Details/ })
    expect(details).toHaveAttribute("aria-expanded", "false")
    await user.click(details)
    const row = await screen.findByRole("group", { name: "Out of swap" })
    expect(within(row).getByText("Crash dumps are off.")).toBeInTheDocument()
    expect(memoryWords({ ...MEMORY, supported: false }).value).toBe("Not on this build")
    expect(memoryWords({ ...MEMORY, refused: 2, core_dumps_off: false }).line).toBe("This computer refused 2.")
  })

  it("draws nothing in a shell with neither", async () => {
    const { container } = mount({}, null)
    await waitFor(() => expect(container.textContent).toBe(""))
  })
})
