import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { LiveClientProvider, type LiveClient, type LiveNotice, type SessionView } from "./live-client"
import { clientWords } from "./client-words"
import { SessionsSection } from "./sessions-section"

const laptop: SessionView = {
  id: "a".repeat(32),
  device_id: "dev-laptop",
  device_name: "Lockdown on laptop",
  platform: "macos",
  address: "203.0.113.7",
  client: "lockdown/0.1.0 (macos)",
  created_at: 1_700_000_000,
  last_seen: 1_700_000_600,
  expires_at: 1_700_028_800,
  current: true,
}
const phone: SessionView = { ...laptop, id: "b".repeat(32), device_id: "dev-phone-1234", device_name: null, address: "198.51.100.9", client: "lockdown/0.1.0 (linux)", current: false }

function fakeLive(over: Partial<LiveClient>): LiveClient & { emit: (n: LiveNotice) => void } {
  let handler: ((n: LiveNotice) => void) | null = null
  return {
    sessionList: () => Promise.resolve([laptop, phone]),
    sessionEnd: () => Promise.resolve(),
    sessionEndOthers: () => Promise.resolve(1),
    onLive: (h) => {
      handler = h
      return Promise.resolve(() => {
        handler = null
      })
    },
    emit: (n) => handler?.(n),
    ...over,
  }
}

describe("the Signed in section", () => {
  it("lists every session with its device, client, address and times, this device marked", async () => {
    render(
      <LiveClientProvider value={fakeLive({})}>
        <SessionsSection onNotice={() => {}} />
      </LiveClientProvider>,
    )
    const list = await screen.findByRole("list", { name: "Sessions" })
    const [first, second, ...rest] = within(list).getAllByRole("listitem")
    if (!first || !second) throw new Error("two rows")
    expect(rest).toHaveLength(0)
    const rows = [first, second] as const
    expect(within(rows[0]).getByText("this device")).toBeInTheDocument()
    expect(within(rows[0]).getByText(/Lockdown 0\.1\.0 on macOS · from 203\.0\.113\.7/)).toBeInTheDocument()
    expect(within(rows[1]).getByText("Device dev-phon")).toBeInTheDocument()
    // This device's own session has no End; the other one does.
    expect(within(rows[0]).queryByRole("button", { name: /End/ })).not.toBeInTheDocument()
    expect(within(rows[1]).getByRole("button", { name: "End the session on Device dev-phon" })).toBeInTheDocument()
  })

  it("ends one session after a confirm, and then every other one", async () => {
    const sessionEnd = vi.fn(() => Promise.resolve())
    const sessionEndOthers = vi.fn(() => Promise.resolve(1))
    const notice = vi.fn()
    render(
      <LiveClientProvider value={fakeLive({ sessionEnd, sessionEndOthers })}>
        <SessionsSection onNotice={notice} />
      </LiveClientProvider>,
    )
    await userEvent.click(await screen.findByRole("button", { name: "End the session on Device dev-phon" }))
    expect(sessionEnd).not.toHaveBeenCalled()
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "End" }))
    await waitFor(() => expect(sessionEnd).toHaveBeenCalledWith("b".repeat(32)))
    expect(notice).toHaveBeenCalledWith("Ended the session on Device dev-phon. It has to log in again.")

    await userEvent.click(screen.getByRole("button", { name: "End all other sessions" }))
    expect(screen.getByRole("dialog")).toHaveTextContent("End every other session?")
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "End" }))
    await waitFor(() => expect(sessionEndOthers).toHaveBeenCalledTimes(1))
    expect(notice).toHaveBeenLastCalledWith("Ended 1 other session.")
  })

  it("reads the list again when the live channel says it changed, and shows a refusal in words", async () => {
    const sessionList = vi.fn(() => Promise.resolve([laptop]))
    const live = fakeLive({ sessionList })
    render(
      <LiveClientProvider value={live}>
        <SessionsSection onNotice={() => {}} />
      </LiveClientProvider>,
    )
    await screen.findByText("Lockdown on laptop")
    expect(screen.queryByRole("button", { name: "End all other sessions" })).not.toBeInTheDocument()
    sessionList.mockResolvedValueOnce([laptop, phone])
    await act(async () => live.emit({ kind: "sessions" }))
    expect(await screen.findByText("Device dev-phon")).toBeInTheDocument()
    expect(sessionList).toHaveBeenCalledTimes(2)
    // Other notices don't.
    await act(async () => live.emit({ kind: "synced", pulled: 1, lines: [] }))
    expect(sessionList).toHaveBeenCalledTimes(2)

    sessionList.mockRejectedValueOnce({ code: "refused", message: "the session with the server has run out; log in again" })
    await act(async () => live.emit({ kind: "sessions" }))
    expect(await screen.findByRole("alert")).toHaveTextContent("log in again")
  })

  it("shows nothing where the host has no live client (the web vault today)", () => {
    const { container } = render(<SessionsSection onNotice={() => {}} />)
    expect(container).toBeEmptyDOMElement()
  })

  it("puts the client's name in words and leaves anything else as it came", () => {
    expect(clientWords("lockdown/0.2.0 (windows)")).toBe("Lockdown 0.2.0 on Windows")
    expect(clientWords("lockdown/0.2.0")).toBe("Lockdown 0.2.0")
    expect(clientWords("curl/8.5.0")).toBe("curl/8.5.0")
    expect(clientWords(null)).toBeNull()
  })
})
