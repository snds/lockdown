import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { BrowserClientProvider, type BrowserClient } from "./browser-client"
import { BrowsersSection } from "./browsers-section"

function fake(overrides: Partial<BrowserClient>): BrowserClient {
  return {
    status: () => Promise.resolve({ listening: true, problem: null, host_binary: "/opt/lockdown/lockdown-native-host", browsers: [] }),
    pending: () => Promise.resolve([]),
    answer: () => Promise.resolve(),
    paired: () => Promise.resolve([]),
    unpair: () => Promise.resolve(),
    install: () => Promise.resolve([]),
    ...overrides,
  }
}

describe("Settings → Browsers", () => {
  it("connects a browser, lists paired ones and removes one", async () => {
    const install = vi.fn(() => Promise.resolve([{ browser: "chrome" as const, manifest: "/x.json", registry_key: null }]))
    const unpair = vi.fn(() => Promise.resolve())
    render(
      <BrowserClientProvider
        value={fake({
          status: () =>
            Promise.resolve({ listening: true, problem: null, host_binary: "/h", browsers: [{ browser: "chrome", name: "Chrome", connected: false }] }),
          paired: () => Promise.resolve([{ client_id: "c1", name: "Firefox on Linux", family: "firefox", extension_id: "lockdown@snds.design", paired_at: 1_760_000_000 }]),
          install,
          unpair,
        })}
      >
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Connect" }))
    expect(install).toHaveBeenCalledWith(["chrome"])
    expect(await screen.findByText("Firefox on Linux")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Remove" }))
    expect(unpair).toHaveBeenCalledWith("c1")
  })

  it("says why the extension can't reach the app", async () => {
    render(
      <BrowserClientProvider value={fake({ status: () => Promise.resolve({ listening: false, problem: "the browser host isn't installed at /x", host_binary: "/x", browsers: [] }) })}>
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    expect(await screen.findByText(/can't reach this app: the browser host isn't installed/)).toBeInTheDocument()
  })

  it("says how the browser host is checked", async () => {
    const detail = "This is a development build without a code signature, so the other program is checked by its path and SHA-256 only."
    render(
      <BrowserClientProvider
        value={fake({ status: () => Promise.resolve({ listening: true, problem: null, host_binary: "/h", browsers: [], peer_check: { by: "path_and_hash", detail } }) })}
      >
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    expect(await screen.findByText(detail)).toBeInTheDocument()
  })
})
