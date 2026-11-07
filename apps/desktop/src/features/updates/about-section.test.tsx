import { act, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider } from "@/core/client"
import { fakeClient } from "@/test/fake-client"

import { AboutSection } from "./about-section"
import { RestartRow } from "./restart-row"
import { UpdateClientProvider, type UpdateClient, type UpdateStatus } from "./update-client"
import { useUpdateNotices } from "./use-update-notices"

const BASE: UpdateStatus = {
  version: "0.1.0-main.20261107093000",
  channel: "main",
  enabled: true,
  off: null,
  auto_check: true,
  state: "idle",
  percent: null,
  offered: null,
  error: null,
  link: null,
  link_why: null,
  asked: false,
}

const NEXT = "0.1.0-main.20261107120000"

/** A client that answers `status` with `first` and lets the test push changes. */
function updateClient(first: UpdateStatus) {
  let handler: ((s: UpdateStatus) => void) | null = null
  const calls: string[] = []
  const client: UpdateClient = {
    status: () => Promise.resolve(first),
    check: () => {
      calls.push("check")
      return Promise.resolve({ ...first, state: "checking", asked: true })
    },
    restart: () => {
      calls.push("restart")
      return Promise.resolve()
    },
    setAuto: (on) => {
      calls.push(`auto ${on}`)
      return Promise.resolve({ ...first, auto_check: on })
    },
    onStatus: (h) => {
      handler = h
      return Promise.resolve(() => {
        handler = null
      })
    },
  }
  return { client, calls, push: (s: UpdateStatus) => act(() => handler?.(s)) }
}

function mount(update: UpdateClient | null, opened: string[] = []) {
  return render(
    <ClientProvider value={fakeClient({ openSite: (url) => (opened.push(url), Promise.resolve()) })}>
      <UpdateClientProvider value={update}>
        <AboutSection />
      </UpdateClientProvider>
    </ClientProvider>,
  )
}

describe("Settings → About", () => {
  it("isn't shown where the host has no updater (the web vault)", () => {
    const { container } = mount(null)
    expect(container).toBeEmptyDOMElement()
  })

  it("shows the version and channel, and the idle line for each switch position", async () => {
    mount(updateClient(BASE).client)
    expect(await screen.findByText("Lockdown 0.1.0-main.20261107093000 · main channel (every merged change)")).toBeInTheDocument()
    expect(screen.getByRole("status")).toHaveTextContent("Lockdown checks for updates when it opens and every six hours.")
    expect(screen.getByRole("switch", { name: "Check for updates automatically" })).toBeChecked()
  })

  it("says automatic checks are off, and the switch turns them back on", async () => {
    const u = updateClient({ ...BASE, auto_check: false })
    mount(u.client)
    expect(await screen.findByRole("status")).toHaveTextContent("Automatic checks are off. Check whenever you like.")
    await userEvent.click(screen.getByRole("switch", { name: "Check for updates automatically" }))
    expect(u.calls).toEqual(["auto true"])
  })

  it("checks when asked, and walks through checking, downloading, ready", async () => {
    const u = updateClient(BASE)
    mount(u.client)
    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }))
    expect(u.calls).toEqual(["check"])
    u.push({ ...BASE, state: "checking", asked: true })
    expect(screen.getByRole("status")).toHaveTextContent("Checking for updates…")
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled()
    u.push({ ...BASE, state: "downloading", offered: NEXT, percent: 42 })
    expect(screen.getByRole("status")).toHaveTextContent(`Downloading version ${NEXT}… 42%`)
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled()
    u.push({ ...BASE, state: "ready", offered: NEXT })
    expect(screen.getByRole("status")).toHaveTextContent(`Version ${NEXT} is ready. Restart to update, or it installs when you quit.`)
    expect(screen.getByText("Restarting locks Lockdown. Unlock it with your master password as usual.")).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Restart to update" }))
    expect(u.calls).toEqual(["check", "restart"])
  })

  it("says when it is up to date", async () => {
    mount(updateClient({ ...BASE, state: "up_to_date" }).client)
    expect(await screen.findByRole("status")).toHaveTextContent("Lockdown is up to date.")
    expect(screen.queryByRole("button", { name: "Restart to update" })).not.toBeInTheDocument()
  })

  it("says what went wrong, in the shell's words", async () => {
    mount(updateClient({ ...BASE, state: "failed", error: "The update was refused: its signature didn't match Lockdown's key." }).client)
    expect(await screen.findByRole("status")).toHaveTextContent("The update was refused: its signature didn't match Lockdown's key.")
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled()
  })

  it("points to the newer build where this copy can't replace itself", async () => {
    const opened: string[] = []
    const link = "https://github.com/snds/lockdown/releases/tag/main-build"
    mount(
      updateClient({ ...BASE, state: "available", offered: NEXT, link, link_why: "Lockdown can't update itself when installed from a Linux package." }).client,
      opened,
    )
    expect(await screen.findByRole("status")).toHaveTextContent(`Version ${NEXT} is out. Lockdown can't update itself when installed from a Linux package.`)
    await userEvent.click(screen.getByRole("button", { name: "Get it from GitHub" }))
    expect(opened).toEqual([link])
  })

  it("says in one line when the build has updates off, and offers nothing to press", async () => {
    const off = "Updates are off in this build: it was made without the update key."
    mount(updateClient({ ...BASE, enabled: false, off }).client)
    expect(await screen.findByRole("status")).toHaveTextContent(off)
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled()
    expect(screen.queryByRole("switch")).not.toBeInTheDocument()
  })
})

describe("the sidebar's Restart to update", () => {
  it("shows only once an update is ready", async () => {
    const u = updateClient(BASE)
    render(
      <UpdateClientProvider value={u.client}>
        <RestartRow onError={() => {}} />
      </UpdateClientProvider>,
    )
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    u.push({ ...BASE, state: "ready", offered: NEXT })
    const row = await screen.findByRole("button", { name: `Restart to update to version ${NEXT}. Lockdown locks first.` })
    await userEvent.click(row)
    expect(u.calls).toEqual(["restart"])
  })
})

describe("a check someone asked for", () => {
  function Notices({ show, onNotice }: { show: boolean; onNotice: (t: string) => void }) {
    useUpdateNotices(show, onNotice)
    return null
  }

  it("is told when it ends, and not for the automatic check or while Settings shows it", async () => {
    const u = updateClient(BASE)
    const told: string[] = []
    const { rerender } = render(
      <UpdateClientProvider value={u.client}>
        <Notices show onNotice={(t) => told.push(t)} />
      </UpdateClientProvider>,
    )
    await act(async () => {})
    u.push({ ...BASE, state: "checking", asked: true })
    u.push({ ...BASE, state: "up_to_date", asked: true })
    u.push({ ...BASE, state: "up_to_date", asked: false })
    expect(told).toEqual(["Lockdown is up to date."])
    rerender(
      <UpdateClientProvider value={u.client}>
        <Notices show={false} onNotice={(t) => told.push(t)} />
      </UpdateClientProvider>,
    )
    u.push({ ...BASE, state: "failed", error: "Lockdown couldn't reach GitHub. Check your connection and try again.", asked: true })
    expect(told).toEqual(["Lockdown is up to date."])
  })
})
