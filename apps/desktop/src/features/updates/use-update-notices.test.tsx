import { act, render } from "@testing-library/react"
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
