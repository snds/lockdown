import { render, screen, waitFor, within } from "@testing-library/react"
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

const FIREFOX = { client_id: "c1", name: "Firefox on Linux", family: "firefox" as const, extension_id: "lockdown@snds.design", paired_at: 1_760_000_000 }

describe("Settings → Browsers (brief T6)", () => {
  it("connects a browser and says so", async () => {
    const install = vi.fn(() => Promise.resolve([{ browser: "chrome" as const, manifest: "/x.json", registry_key: null }]))
    const notices: string[] = []
    render(
      <BrowserClientProvider
        value={fake({
          status: () => Promise.resolve({ listening: true, problem: null, host_binary: "/h", browsers: [{ browser: "chrome", name: "Chrome", connected: false }, { browser: "firefox", name: "Firefox", connected: true }] }),
          install,
        })}
      >
        <BrowsersSection onNotice={(t) => notices.push(t)} />
      </BrowserClientProvider>,
    )
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: /Browsers on this computer/ })).toBeInTheDocument()
    expect(within(screen.getByRole("group", { name: "Firefox" })).getByText("Connected")).toBeInTheDocument()
    // One browser left to connect: no Connect all.
    expect(screen.queryByRole("button", { name: "Connect all" })).toBeNull()
    await user.click(screen.getByRole("button", { name: /^Connect/ }))
    expect(install).toHaveBeenCalledWith(["chrome"])
    await waitFor(() => expect(notices).toEqual(["Chrome connected"]))
  })

  it("lists paired browsers, and removing one asks first", async () => {
    const unpair = vi.fn(() => Promise.resolve())
    render(
      <BrowserClientProvider value={fake({ paired: () => Promise.resolve([FIREFOX]), unpair })}>
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    const user = userEvent.setup()
    expect(await screen.findByText("Firefox on Linux")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /^Remove/ }))
    // The Question comes first; nothing is unpaired until it is answered.
    const question = await screen.findByRole("dialog")
    expect(question).toHaveTextContent("Remove Firefox on Linux?")
    expect(unpair).not.toHaveBeenCalled()
    await user.click(within(question).getByRole("button", { name: "Cancel" }))
    expect(unpair).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: /^Remove/ }))
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove" }))
    await waitFor(() => expect(unpair).toHaveBeenCalledWith("c1"))
  })

  it("says why the extension can't reach the app", async () => {
    render(
      <BrowserClientProvider value={fake({ status: () => Promise.resolve({ listening: false, problem: "the browser host isn't installed at /x", host_binary: "/x", browsers: [] }) })}>
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    const row = await screen.findByRole("group", { name: "Extension" })
    expect(row).toHaveTextContent("Can't reach this app")
    expect(within(row).getByText("the browser host isn't installed at /x")).toBeInTheDocument()
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
    expect(screen.queryByRole("alert")).toBeNull()
  })

  it("a refused host is a critical Banner, in the core's words", async () => {
    render(
      <BrowserClientProvider
        value={fake({
          status: () =>
            Promise.resolve({ listening: false, problem: "The browser host isn't signed by this app's developer.", host_binary: "/h", browsers: [], peer_check: { by: "refused", detail: "The host's signature didn't match." } }),
        })}
      >
        <BrowsersSection onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    const banner = await screen.findByRole("alert")
    expect(banner).toHaveTextContent("Lockdown refused the browser host")
    expect(banner).toHaveTextContent("The host's signature didn't match.")
  })
})
