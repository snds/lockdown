import { act, render } from "@testing-library/react"

import { Toaster } from "@unicron/ui"

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
  function Notices({ show, onAsked }: { show: boolean; onAsked: () => void }) {
    useUpdateNotices(show, onAsked)
    return null
  }

  it("opens About as it starts, once per check, and never raises a toast (cross-app decisions §3)", async () => {
    const u = updateClient(BASE)
    const asked = vi.fn()
    const { rerender } = render(
      <UpdateClientProvider value={u.client}>
        <Notices show onAsked={asked} />
        <Toaster />
      </UpdateClientProvider>,
    )
    await act(async () => {})
    // The automatic check opens nothing.
    u.push({ ...BASE, state: "checking", asked: false })
    u.push({ ...BASE, state: "up_to_date", asked: false })
    expect(asked).not.toHaveBeenCalled()
    // An asked one opens About once, at its start; its outcome is About's to say.
    u.push({ ...BASE, state: "checking", asked: true })
    u.push({ ...BASE, state: "downloading", percent: 40, asked: true })
    u.push({ ...BASE, state: "ready", offered: "0.1.0-main.20261107120000", asked: true })
    expect(asked).toHaveBeenCalledTimes(1)
    // A second ask is a second check.
    u.push({ ...BASE, state: "checking", asked: true })
    u.push({ ...BASE, state: "failed", error: "Lockdown couldn't reach GitHub. Check your connection and try again.", asked: true })
    expect(asked).toHaveBeenCalledTimes(2)
    // While About already shows it, nothing opens.
    rerender(
      <UpdateClientProvider value={u.client}>
        <Notices show={false} onAsked={asked} />
        <Toaster />
      </UpdateClientProvider>,
    )
    u.push({ ...BASE, state: "checking", asked: true })
    expect(asked).toHaveBeenCalledTimes(2)
    await act(async () => {})
    expect(document.querySelectorAll("[data-sonner-toast]")).toHaveLength(0)
  })
})
