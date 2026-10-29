import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import type { Status } from "@/core/types"
import { useDemoEntry } from "@/features/unlock/demo-entry"

import DemoMount from "./demo-mount"
import { DevtoolsClientProvider } from "./devtools-client"
import { fakeDevtools } from "./fake-devtools"

const status = { vault: "demo" } as unknown as Status

/** What the first run would show. */
function Explore() {
  const entry = useDemoEntry()
  return entry ? <button onClick={entry.explore}>Explore the demo</button> : null
}

describe("the demo's mount", () => {
  it("offers the demo at first run, opens it with its bar, and leaves", async () => {
    const { client } = fakeDevtools(status)
    const onOpened = vi.fn()
    const onClosed = vi.fn()
    render(
      <DevtoolsClientProvider value={client}>
        <Explore />
        <DemoMount onOpened={onOpened} onClosed={onClosed} />
      </DevtoolsClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Explore the demo" }))
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(status))
    const bar = await screen.findByRole("region", { name: "Demo" })
    expect(bar).toHaveTextContent("You're exploring the demo. Nothing here is real.")
    expect(document.documentElement).toHaveAttribute("data-lockdown-demo")
    // While the demo is open, the first run's link is gone.
    expect(screen.queryByRole("button", { name: "Explore the demo" })).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Leave the demo" }))
    await waitFor(() => expect(onClosed).toHaveBeenCalled())
    expect(screen.queryByRole("region", { name: "Demo" })).not.toBeInTheDocument()
    expect(document.documentElement).not.toHaveAttribute("data-lockdown-demo")
  })

  it("answers the Developer menu: open, reset and a pack", async () => {
    const demoReset = vi.fn()
    const demoLoadPack = vi.fn()
    const { client, pick } = fakeDevtools(status)
    const fake = { ...client }
    fake.demoReset = (...a) => {
      demoReset(...a)
      return client.demoReset()
    }
    fake.demoLoadPack = (...a) => {
      demoLoadPack(...a)
      return client.demoLoadPack()
    }
    const onOpened = vi.fn()
    render(
      <DevtoolsClientProvider value={fake}>
        <DemoMount onOpened={onOpened} onClosed={() => {}} />
      </DevtoolsClientProvider>,
    )
    await waitFor(() => expect(screen.queryByRole("region", { name: "Demo" })).not.toBeInTheDocument())
    act(() => pick("demo_open"))
    expect(await screen.findByRole("region", { name: "Demo" })).toBeInTheDocument()
    act(() => pick("demo_reset"))
    await waitFor(() => expect(demoReset).toHaveBeenCalled())
    act(() => pick("demo_load"))
    await waitFor(() => expect(demoLoadPack).toHaveBeenCalled())
    await waitFor(() => expect(onOpened).toHaveBeenCalledTimes(3))
  })

  it("draws nothing in a shell without the demo", async () => {
    const { client } = fakeDevtools(status, { status: () => Promise.resolve(null) })
    render(
      <DevtoolsClientProvider value={client}>
        <Explore />
        <DemoMount onOpened={() => {}} onClosed={() => {}} />
      </DevtoolsClientProvider>,
    )
    await new Promise((r) => setTimeout(r, 20))
    expect(screen.queryByRole("button", { name: "Explore the demo" })).not.toBeInTheDocument()
    expect(screen.queryByRole("region", { name: "Demo" })).not.toBeInTheDocument()
  })
})
