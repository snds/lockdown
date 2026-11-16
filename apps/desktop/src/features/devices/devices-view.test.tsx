import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { vi } from "vitest"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { AccountStatus, DeviceInfo, Status } from "@/core/types"
import { DepthClientProvider } from "@/features/depth/depth-client"
import { LiveClientProvider, type LiveClient, type SessionView } from "@/features/sessions/live-client"
import { fakeClient } from "@/test/fake-client"
import { fakeDepth } from "@/test/fake-depth"

import { lastUse, platformName } from "./devices-model"
import { DevicesView } from "./devices-view"

const NOW = Math.floor(Date.now() / 1000)

const mine: DeviceInfo = {
  id: "d-mine-0001",
  name: "This laptop",
  platform: "macos",
  enrolled_at: NOW - 86400 * 40,
  revoked_at: null,
  this_device: true,
}
const phone: DeviceInfo = {
  id: "d-phone-0002",
  name: "Old phone",
  platform: "ios",
  enrolled_at: NOW - 86400 * 30,
  revoked_at: null,
  this_device: false,
}
const tablet: DeviceInfo = {
  id: "d-tablet-003",
  name: "Kitchen tablet",
  platform: "android",
  enrolled_at: Date.UTC(2026, 2, 11, 12) / 1000,
  revoked_at: null,
  this_device: false,
}
const gone: DeviceInfo = {
  id: "d-gone-00004",
  name: "Spare laptop",
  platform: "linux",
  enrolled_at: NOW - 86400 * 90,
  revoked_at: Date.UTC(2026, 9, 2, 12) / 1000,
  this_device: false,
}

const session = (device: DeviceInfo, last_seen: number, current = false): SessionView => ({
  id: `s-${device.id}`,
  device_id: device.id,
  device_name: device.name,
  platform: device.platform,
  address: "203.0.113.7",
  client: "lockdown/0.9.0 (macos)",
  created_at: NOW - 86400 * 3,
  last_seen,
  expires_at: NOW + 86400,
  current,
})

const server: AccountStatus = {
  server: "https://vault.example",
  login: "sam@example.com",
  synced_seq: 1,
  synced_rev: 1,
  last_sync_at: NOW,
  session_expires_at: NOW + 3600,
  transparency: null,
}
const statusWith = (account: AccountStatus | null, devices = 3): Status => ({ devices, devices_revoked: 1, account }) as unknown as Status

function live(sessions: SessionView[]): LiveClient {
  return {
    sessionList: () => Promise.resolve(sessions),
    sessionEnd: () => Promise.resolve(),
    sessionEndOthers: () => Promise.resolve(sessions.length - 1),
    onLive: () => Promise.resolve(() => {}),
  }
}

function mount(client: Partial<Client>, ui: ReactNode, sessions: SessionView[] | null = null) {
  const keys = fakeDepth({
    securityKeys: () => Promise.resolve({ keys: [], rp_id: "vault.example" }),
  })
  const inner = <DepthClientProvider value={keys}>{ui}</DepthClientProvider>
  return render(
    <ClientProvider
      value={fakeClient({
        approvalRoute: () => Promise.resolve({ route: "password", method: "password" }),
        ...client,
      })}
    >
      <GateProvider>{sessions ? <LiveClientProvider value={live(sessions)}>{inner}</LiveClientProvider> : inner}</GateProvider>
    </ClientProvider>,
  )
}

const rowOf = (name: string) => screen.getAllByRole("listitem").find((r) => r.textContent?.includes(name))!

describe("the Devices section (brief T2)", () => {
  it("lists this device first, the rest as the core lists them, platforms cased, the last use from the session list, and the revoked one last", async () => {
    const lastSeen = NOW - 120
    mount(
      { deviceList: () => Promise.resolve([gone, tablet, phone, mine]) },
      <DevicesView status={statusWith(server)} onChanged={vi.fn()} onNotice={vi.fn()} />,
      [session(mine, NOW, true), session(phone, lastSeen)],
    )
    const list = await screen.findByRole("list", { name: "Devices" })
    const rows = within(list).getAllByRole("listitem")
    expect(rows.map((r) => within(r).getByText(/This laptop|Old phone|Kitchen tablet|Spare laptop/).textContent)).toEqual([
      "This laptop",
      "Kitchen tablet",
      "Old phone",
      "Spare laptop",
    ])
    expect(rows[0]).toHaveTextContent("Now")
    expect(within(rows[0]!).getByText("This device")).toBeInTheDocument()
    expect(within(rows[0]!).getByText("macOS")).toBeInTheDocument()
    expect(within(rows[0]!).queryByRole("button", { name: "Revoke" })).toBeNull()
    // Its last use comes from its session, not from the day it was added.
    // The same words as a session row (Lockdown 25), with how long ago as the tip.
    await waitFor(() => expect(rows[2]).toHaveTextContent(/Last used (Today|Yesterday) \d{2}:\d{2}/))
    expect(within(rows[2]!).getByText(/^Last used /)).toHaveAttribute("aria-description", "2 minutes ago")
    expect(within(rows[2]!).getByText("iOS")).toBeInTheDocument()
    // No live session: the day it was added.
    expect(rows[1]).toHaveTextContent("Added 2026.03.11")
    expect(within(rows[1]!).getByText("Android")).toBeInTheDocument()
    expect(within(rows[3]!).getByText("Revoked 2026.10.02")).toBeInTheDocument()
    expect(within(rows[3]!).queryByRole("button", { name: "Revoke" })).toBeNull()
    // Ids are not on the rows; they sit under Details (R14).
    expect(list).not.toHaveTextContent("d-phone")
    await userEvent.setup().click(screen.getByRole("button", { name: /Details/ }))
    // A value is cut in the middle when it is long, so its text spans two parts: read the row's.
    await waitFor(() => expect(screen.getByRole("button", { name: /Details/ })).toHaveAttribute("aria-expanded", "true"))
    expect(document.body).toHaveTextContent("d-phone-0002")
    expect(document.body).toHaveTextContent("s-d-phone-0002")
  })

  it("asks before a revoke and adds no gate of its own: the core's gate asks only when the core says so (X5, D101)", async () => {
    const deviceRevoke = vi.fn(() => Promise.resolve(["Revoked Old phone. It can't open the vault or sync any more."]))
    const onNotice = vi.fn()
    mount(
      { deviceList: () => Promise.resolve([mine, phone]), deviceRevoke },
      <DevicesView status={statusWith(null, 2)} onChanged={vi.fn()} onNotice={onNotice} />,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Revoke" }))
    const question = await screen.findByRole("dialog", {
      name: "Revoke Old phone?",
    })
    expect(question).toHaveTextContent("A revocation never comes back.")
    expect(deviceRevoke).not.toHaveBeenCalled()
    // Focus starts on Cancel, so Enter never revokes by accident.
    await waitFor(() => expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus())
    await user.click(within(question).getByRole("button", { name: "Revoke" }))
    await waitFor(() => expect(deviceRevoke).toHaveBeenCalledWith("d-phone-0002", {}))
    expect(deviceRevoke).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("dialog", { name: "Confirm it's you" })).toBeNull()
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith("Revoked Old phone. It can't open the vault or sync any more."))
  })

  it("cancelling the question revokes nothing", async () => {
    const deviceRevoke = vi.fn(() => Promise.resolve([]))
    mount(
      { deviceList: () => Promise.resolve([mine, phone]), deviceRevoke },
      <DevicesView status={statusWith(null, 2)} onChanged={vi.fn()} onNotice={vi.fn()} />,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Revoke" }))
    const question = await screen.findByRole("dialog", {
      name: "Revoke Old phone?",
    })
    await user.click(within(question).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(deviceRevoke).not.toHaveBeenCalled()
  })

  it("says when this is the only device, and offers Add device with a server", async () => {
    mount({ deviceList: () => Promise.resolve([mine]) }, <DevicesView status={statusWith(server, 1)} onChanged={vi.fn()} onNotice={vi.fn()} />, [
      session(mine, NOW, true),
    ])
    expect(await screen.findByText("Only this device. A second device is what gets you back in when this one is lost.")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Add device" })).toBeInTheDocument()
    expect(await screen.findByText("Signed in")).toBeInTheDocument()
    expect(screen.getByText("Security keys", { selector: "h2 *, h2" })).toBeInTheDocument()
  })

  it("on a local account: no Signed in, no Security keys, and Set up server instead of Add device", async () => {
    mount(
      { deviceList: () => Promise.resolve([mine]) },
      <DevicesView status={statusWith({ ...server, local: true }, 1)} onChanged={vi.fn()} onNotice={vi.fn()} />,
      [session(mine, NOW, true)],
    )
    expect(await screen.findByRole("button", { name: "Set up server" })).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Add device" })).toBeNull()
    expect(screen.queryByText("Signed in")).toBeNull()
    expect(screen.queryByText("Security keys")).toBeNull()
    expect(screen.queryByText(/^Only this device/)).toBeNull()
  })

  it("shows a refusal in the core's words", async () => {
    mount(
      {
        deviceList: () => Promise.reject({ code: "refused", message: "the vault is locked" }),
      },
      <DevicesView status={statusWith(null)} onChanged={vi.fn()} onNotice={vi.fn()} />,
    )
    expect(await screen.findByRole("alert")).toHaveTextContent("the vault is locked")
  })

  it("marks the device a link named", async () => {
    mount(
      { deviceList: () => Promise.resolve([mine, phone]) },
      <DevicesView status={statusWith(null, 2)} onChanged={vi.fn()} onNotice={vi.fn()} focus="d-phone-0002" />,
    )
    await screen.findByRole("list", { name: "Devices" })
    expect(rowOf("Old phone")).toHaveAttribute("aria-current", "true")
    await waitFor(() => expect(rowOf("Old phone")).toHaveFocus())
  })
})

describe("the Devices words", () => {
  it("cases platforms and leaves an unknown one as it came", () => {
    expect(["macos", "ios", "ipados", "windows", "linux", "android", "haiku"].map(platformName)).toEqual([
      "macOS",
      "iOS",
      "iPadOS",
      "Windows",
      "Linux",
      "Android",
      "haiku",
    ])
  })

  it("takes a device's last use from its newest session", () => {
    expect(lastUse(phone, [session(phone, 10), { ...session(phone, 30), id: "s2" }, session(mine, 50)])).toBe(30)
    expect(lastUse(tablet, [session(phone, 10)])).toBeNull()
  })
})
