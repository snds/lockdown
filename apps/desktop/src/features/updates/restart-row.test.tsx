import { act, render, screen } from "@testing-library/react"

import { AppFrame, FloatButton, NavItem } from "@unicron/ui"

import { updateMark, useUpdateMark } from "./restart-row"
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

/** The three ways into Settings, each carrying the mark (cross-app decisions §3). */
function WaysIn() {
  const mark = useUpdateMark()
  return (
    <AppFrame layout="expanded">
      <NavItem label="Settings" icon="settings" mark={mark} />
      <FloatButton label="Profile" icon="user--avatar" size="l" mark={mark} />
    </AppFrame>
  )
}

describe("a ready update, on the way into Settings", () => {
  it("is a mark joined to the name only while the update is ready, and never a restart button", async () => {
    const u = updateClient(BASE)
    render(
      <UpdateClientProvider value={u.client}>
        <WaysIn />
      </UpdateClientProvider>,
    )
    await act(async () => {})
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument()
    for (const state of ["checking", "downloading", "up_to_date", "failed", "available"] as const) {
      u.push({ ...BASE, state, asked: true })
      expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument()
      expect(screen.getByRole("button", { name: "Profile" })).toBeInTheDocument()
    }
    u.push({ ...BASE, state: "ready", offered: NEXT })
    expect(await screen.findByRole("button", { name: `Settings, update ${NEXT} ready` })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: `Profile, update ${NEXT} ready` })).toBeInTheDocument()
    // Restart to update lives in Settings → About (D755): nothing here restarts.
    expect(screen.queryByRole("button", { name: /restart/i })).toBeNull()
    expect(u.calls).toEqual([])
    u.push({ ...BASE, state: "idle" })
    expect(await screen.findByRole("button", { name: "Settings" })).toBeInTheDocument()
  })

  it("says nothing where the host has no updater (the web vault)", () => {
    expect(updateMark(null)).toBeUndefined()
    expect(updateMark({ ...BASE, state: "ready", offered: null })).toEqual({ square: true, label: "update ready" })
  })
})
