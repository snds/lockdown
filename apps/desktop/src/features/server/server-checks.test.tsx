import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { ServerChecks, SplitWarning } from "./server-checks"
import { ServerClientProvider, type MirrorCheck, type MirrorsView, type RestoreDrillView, type ServerClient } from "./server-client"

const drill: RestoreDrillView = {
  reported: true,
  scheduled: true,
  last_run_at: 1_792_316_980,
  last_passed: true,
  last_success_at: 1_792_316_980,
  client_read_back: true,
  words: "The server last proved its backup restores on 2026-10-18, with a sign-in to the restored copy that read an item back.",
}

const agrees: MirrorCheck = { url: "https://mirror.example", named_by: "server", at: 1, state: "agrees", size: 12, checked_at: 1 }
const split: MirrorCheck = {
  url: "https://mine.example",
  named_by: "settings",
  at: 1,
  state: "split",
  message: "Split view: the log of 9 entries the mirror copied doesn't connect to the 12 this server showed this device; the server keeps two histories",
}

function fake(over: Partial<ServerClient> = {}, mirrors: MirrorsView = { configured: [], checks: [agrees] }): ServerClient {
  return {
    restoreDrill: () => Promise.resolve(drill),
    ktMirrors: () => Promise.resolve(mirrors),
    setKtMirrors: (urls) => Promise.resolve({ ...mirrors, configured: urls }),
    ...over,
  }
}

describe("Settings → This server", () => {
  it("shows the last successful restore as the server reports it, and each mirror's word", async () => {
    render(
      <ServerClientProvider value={fake()}>
        <ServerChecks onNotice={() => {}} />
      </ServerClientProvider>,
    )
    expect(await screen.findByText(/last proved its backup restores on 2026-10-18/)).toBeInTheDocument()
    expect(await screen.findByText(/Agrees with this server's log \(its copy has 12 entries\)/)).toBeInTheDocument()
    expect(screen.getByText("Named by the server")).toBeInTheDocument()
    expect(screen.getByText("Passed")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: /Log mirrors/ })).toHaveTextContent("01")
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("says a failed drill in its own words, and a server without drills too", async () => {
    const failed = { ...drill, last_passed: false, words: "The newest drill, on 2026-10-19, failed: tell whoever runs the server." }
    render(
      <ServerClientProvider value={fake({ restoreDrill: () => Promise.resolve(failed) })}>
        <ServerChecks onNotice={() => {}} />
      </ServerClientProvider>,
    )
    const row = (await screen.findByText(/failed: tell whoever runs the server/)).closest('[data-slot="field-row"]') as HTMLElement
    // The server's word, said once, in a Badge beside its sentence.
    expect(within(row).getByText("Failed")).toBeInTheDocument()
  })

  it("warns in words on a split view", async () => {
    render(
      <ServerClientProvider value={fake({}, { configured: ["https://mine.example"], checks: [agrees, split] })}>
        <ServerChecks onNotice={() => {}} />
      </ServerClientProvider>,
    )
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("This server's public log doesn't match a mirror")
    expect(alert).toHaveTextContent("https://mine.example")
    expect(alert).toHaveTextContent("Split view")
    // What the core pauses until it clears (D328 as changed by D375), and why.
    expect(alert).toHaveTextContent("pauses new sharing through this server")
    expect(alert).toHaveTextContent("naming an emergency contact and opening emergency access")
    expect(alert).toHaveTextContent("keeps syncing")
  })

  it("saves the mirrors you add, one per line, and shows the core's refusal", async () => {
    const setKtMirrors = vi.fn((urls: string[]) =>
      urls.some((u) => u.startsWith("http://"))
        ? Promise.reject({ code: "refused", message: "http://m.example isn't a mirror address: it must start with https://" })
        : Promise.resolve({ configured: urls, checks: [] }),
    )
    const onNotice = vi.fn()
    render(
      <ServerClientProvider value={fake({ setKtMirrors })}>
        <ServerChecks onNotice={onNotice} />
      </ServerClientProvider>,
    )
    const box = await screen.findByLabelText("Your own mirrors")
    await userEvent.type(box, "http://m.example")
    await userEvent.click(screen.getByRole("button", { name: "Save mirrors" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("must start with https://")
    await userEvent.clear(screen.getByLabelText("Your own mirrors"))
    await userEvent.type(screen.getByLabelText("Your own mirrors"), "https://a.example{enter}https://b.example")
    await userEvent.click(screen.getByRole("button", { name: "Save mirrors" }))
    expect(setKtMirrors).toHaveBeenLastCalledWith(["https://a.example", "https://b.example"])
    expect(onNotice).toHaveBeenCalledWith("Mirrors saved. The next sync compares against them")
  })

  it("says a server that sends no drill report so, in a Badge", async () => {
    render(
      <ServerClientProvider value={fake({ restoreDrill: () => Promise.resolve({ ...drill, reported: false, last_passed: null, last_success_at: null, words: "This server doesn't report restore drills." }) })}>
        <ServerChecks onNotice={() => {}} />
      </ServerClientProvider>,
    )
    expect(await screen.findByText("This server doesn't report restore drills.")).toBeInTheDocument()
    expect(screen.getByText("Not reported")).toBeInTheDocument()
  })

  it("isn't shown without a server client, and the warning is nothing without a split", () => {
    const { container } = render(<ServerChecks onNotice={() => {}} />)
    expect(container).toBeEmptyDOMElement()
    const { container: c2 } = render(<SplitWarning checks={[agrees]} />)
    expect(c2).toBeEmptyDOMElement()
  })
})
