import { act, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { RestartRow } from "./restart-row"
import { UpdateClientProvider, type UpdateClient, type UpdateStatus } from "./update-client"

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
    reload: () => {
      calls.push("reload")
      return Promise.resolve()
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
