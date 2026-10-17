import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { Settings } from "@/core/types"

import { DEFAULT_SETTINGS, SettingsProvider, useSettings } from "./settings"

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}

function Probe() {
  const { settings, loaded, update } = useSettings()
  return (
    <div>
      <output data-testid="density">{settings.density}</output>
      <output data-testid="loaded">{String(loaded)}</output>
      <button onClick={() => void update({ density: settings.density === "compact" ? "comfortable" : "compact" })}>flip</button>
    </div>
  )
}

describe("the density setting (D92)", () => {
  afterEach(() => {
    delete document.documentElement.dataset.density
  })

  it("lands on the root as data-density, read from the settings file and written back", async () => {
    let stored: Settings = { ...DEFAULT_SETTINGS, density: "compact" }
    const getSettings = vi.fn(() => Promise.resolve(stored))
    const setSettings = vi.fn((s: Settings) => {
      stored = s
      return Promise.resolve(s)
    })
    render(
      <ClientProvider value={fakeClient({ getSettings, setSettings })}>
        <SettingsProvider>
          <Probe />
        </SettingsProvider>
      </ClientProvider>,
    )
    // The defaults draw until the file is read.
    expect(document.documentElement.dataset.density).toBe("comfortable")
    await waitFor(() => expect(screen.getByTestId("loaded")).toHaveTextContent("true"))
    expect(screen.getByTestId("density")).toHaveTextContent("compact")
    expect(document.documentElement.dataset.density).toBe("compact")

    const user = userEvent.setup()
    await act(() => user.click(screen.getByRole("button", { name: "flip" })))
    await waitFor(() => expect(document.documentElement.dataset.density).toBe("comfortable"))
    expect(setSettings).toHaveBeenCalledWith(expect.objectContaining({ density: "comfortable", reveal: "hold" }))
  })
})
