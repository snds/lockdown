import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { SettingsProvider, DEFAULT_SETTINGS } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { DeviceInfo, SecurityReport, Status } from "@/core/types"
import { DevicesView } from "@/features/devices/devices-view"
import { LiveClientProvider, type LiveClient, type LiveNotice } from "@/features/sessions/live-client"
import { fakeClient } from "@/test/fake-client"

import { PqStatusClientProvider, type PqStatus, type PqStatusClient } from "./pq-status-client"
import { PqStatusLine } from "./pq-status-line"
import { SecurityView } from "./security-view"

const ON: PqStatus = {
  state: "on",
  reason: null,
  headline: "Quantum-safe signatures: on",
  explanation: "Every change your devices make to your vaults is signed in a way that even a future quantum computer couldn't fake.",
  without_level_1: [],
  without_level_2: [],
}

const WAITING: PqStatus = {
  state: "waiting",
  reason: null,
  headline: "Quantum-safe signatures: waiting for Old phone and Kitchen tablet",
  explanation: "These devices need the latest Lockdown. Once each is updated and opened, the stronger signatures switch on by themselves.",
  without_level_1: [{ id: "d-old-phone", name: "Old phone" }],
  without_level_2: [{ id: "d-kitchen", name: "Kitchen tablet" }],
}

const UNKNOWN: PqStatus = {
  state: "unknown",
  reason: "unreached",
  headline: "Quantum-safe signatures: can't check right now",
  explanation: "Lockdown hasn't reached your server since you unlocked. It checks again at the next sync.",
  without_level_1: [],
  without_level_2: [],
}

function client(...answers: Array<PqStatus | Error>): PqStatusClient & { calls: number } {
  const c = {
    calls: 0,
    status: () => {
      const a = answers[Math.min(c.calls, answers.length - 1)]!
      c.calls += 1
      return a instanceof Error ? Promise.reject(a) : Promise.resolve(a)
    },
  }
  return c
}

function mountLine(pq: PqStatusClient | null, onDevice?: (id: string) => void, live?: LiveClient) {
  return render(
    <LiveClientProvider value={live ?? null}>
      <PqStatusClientProvider value={pq}>
        <PqStatusLine onDevice={onDevice} />
      </PqStatusClientProvider>
    </LiveClientProvider>,
  )
}

describe("the post-quantum status line", () => {
  it("says on, in words and with an icon, and names no device", async () => {
    mountLine(client(ON), vi.fn())
    const line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    expect(within(line).getByText("Quantum-safe signatures: on")).toBeInTheDocument()
    expect(within(line).getByText(/couldn't fake/)).toBeInTheDocument()
    expect(line).toHaveAttribute("data-state", "on")
    // The state is in the icon too, not in a colour alone.
    expect(line.textContent).toContain("verified_user")
    expect(within(line).queryByRole("list")).not.toBeInTheDocument()
  })

  it("names each waiting device, level 1's first, each a link to that device on Devices", async () => {
    const onDevice = vi.fn()
    mountLine(client(WAITING), onDevice)
    const line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    expect(within(line).getByText("Quantum-safe signatures: waiting for Old phone and Kitchen tablet")).toBeInTheDocument()
    expect(within(line).getByText(/need the latest Lockdown/)).toBeInTheDocument()
    expect(line.textContent).toContain("hourglass_top")
    const devices = within(line).getByRole("list", { name: "Devices to update" })
    const links = within(devices).getAllByRole("button")
    expect(links.map((b) => b.textContent?.replace("devices", "").trim())).toEqual(["Old phone", "Kitchen tablet"])
    await userEvent.setup().click(links[1]!)
    expect(onDevice).toHaveBeenCalledWith("d-kitchen")
  })

  it("names the devices without links where the view can't go to Devices", async () => {
    mountLine(client(WAITING))
    const line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    expect(within(line).getByText(/waiting for Old phone and Kitchen tablet/)).toBeInTheDocument()
    expect(within(line).queryByRole("button")).not.toBeInTheDocument()
  })

  it("is neutral when it can't check, and a failed call is never on", async () => {
    const { unmount } = mountLine(client(UNKNOWN), vi.fn())
    let line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    expect(within(line).getByText("Quantum-safe signatures: can't check right now")).toBeInTheDocument()
    expect(line).toHaveAttribute("data-state", "unknown")
    expect(line.textContent).toContain("help")
    expect(within(line).queryByRole("button")).not.toBeInTheDocument()
    unmount()

    mountLine(client(new Error("the call didn't finish")), vi.fn())
    line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    expect(within(line).getByText("Quantum-safe signatures: can't check right now")).toBeInTheDocument()
    expect(line.textContent).not.toMatch(/: on/)
  })

  it("isn't drawn where the host has no client for it", () => {
    mountLine(null)
    expect(screen.queryByRole("status", { name: "Quantum-safe signatures" })).not.toBeInTheDocument()
  })

  it("reads the line again after a live sync, so the last device's update shows by itself", async () => {
    let push: ((n: LiveNotice) => void) | null = null
    const live: LiveClient = {
      sessionList: () => Promise.resolve([]),
      sessionEnd: () => Promise.resolve(),
      sessionEndOthers: () => Promise.resolve(0),
      onLive: (handler) => {
        push = handler
        return Promise.resolve(() => {})
      },
    }
    const pq = client(WAITING, ON)
    mountLine(pq, vi.fn(), live)
    expect(await screen.findByText(/waiting for Old phone/)).toBeInTheDocument()
    await waitFor(() => expect(push).not.toBeNull())
    act(() => push!({ kind: "synced", pulled: 0, lines: [] }))
    expect(await screen.findByText("Quantum-safe signatures: on")).toBeInTheDocument()
    expect(pq.calls).toBe(2)
  })
})

const NOW = Math.floor(Date.now() / 1000)
const report: SecurityReport = {
  findings: [],
  dismissed: [],
  counts: { urgent: 0, important: 0, suggested: 0 },
  passwords: 1,
  breach: { enabled: true, source: "api.pwnedpasswords.com", checked_at: NOW, checked: 1, unchecked: 0, message: null },
  alerts: { account: false, checked_at: null, message: null },
  directory: { source: "2fa.directory", licence: "MIT", date: "2026-10-16", sites: 1893 },
  generated_at: NOW,
}

describe("the line on the Security screen", () => {
  it("sits on the screen, and a waiting device opens the Devices screen at that device", async () => {
    const onDevices = vi.fn()
    render(
      <ClientProvider
        value={fakeClient({
          securityReport: () => Promise.resolve(report),
          securityPlaybooks: () => Promise.resolve([]),
          getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
        })}
      >
        <SettingsProvider>
          <PqStatusClientProvider value={client(WAITING)}>
            <SecurityView onOpenItem={vi.fn()} onDevices={onDevices} onChanged={vi.fn()} onNotice={vi.fn()} />
          </PqStatusClientProvider>
        </SettingsProvider>
      </ClientProvider>,
    )
    const line = await screen.findByRole("status", { name: "Quantum-safe signatures" })
    await userEvent.setup().click(within(line).getByRole("button", { name: /Old phone/ }))
    expect(onDevices).toHaveBeenCalledWith("d-old-phone")
  })
})

describe("the Devices screen, opened at one device", () => {
  it("marks that device and puts it in focus", async () => {
    const devices: DeviceInfo[] = [
      { id: "d-mine", name: "This laptop", platform: "linux", enrolled_at: NOW - 100, revoked_at: null, this_device: true },
      { id: "d-old-phone", name: "Old phone", platform: "android", enrolled_at: NOW - 50, revoked_at: null, this_device: false },
    ]
    const status = { devices: 2, devices_revoked: 0, account: null } as unknown as Status
    render(
      <ClientProvider value={fakeClient({ deviceList: () => Promise.resolve(devices) })}>
        <DevicesView status={status} onChanged={vi.fn()} onNotice={vi.fn()} focus="d-old-phone" />
      </ClientProvider>,
    )
    const rows = await screen.findAllByRole("listitem")
    const old = rows.find((r) => r.textContent?.includes("Old phone"))!
    expect(old).toHaveAttribute("aria-current", "true")
    await waitFor(() => expect(old).toHaveFocus())
    expect(rows.find((r) => r.textContent?.includes("This laptop"))).not.toHaveAttribute("aria-current")
  })
})
