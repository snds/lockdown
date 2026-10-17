import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { Status } from "@/core/types"

import { StatusView } from "./status-view"

const status: Status = {
  root: "/tmp/snds/identity",
  data: "/tmp/snds/lockdown",
  profile_id: "0199c0de-0000-7000-8000-000000000001",
  manager: "lockdown",
  recovery: "made",
  device_id: "0199c0de-0000-7000-8000-000000000002",
  device_name: "Lockdown on test",
  persona_id: "0199c0de-0000-7000-8000-000000000003",
  persona_label: "Personal (Personal)",
  vault_id: "0199c0de-0000-7000-8000-000000000004",
  head_seq: 3,
  head_hash: "ab".repeat(32),
  live_items: 1,
  devices: 1,
  devices_revoked: 0,
  identity_seq: 1,
  account: {
    server: "https://vault.example",
    login: "me@example.com",
    synced_seq: 3,
    synced_rev: 2,
    last_sync_at: 1_760_000_000,
    session_expires_at: 1_760_100_000,
    transparency: { log_size: 3, root_prefix: "0123456789abcdef", audited: 3, checked_at: 1_760_000_000 },
  },
}

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), { status: () => Promise.resolve(status) }, overrides)
}

describe("the Status screen", () => {
  it("shows the verified log, and the audit's words when a sync is refused", async () => {
    const sync = vi.fn(() =>
      Promise.reject({
        code: "refused",
        message:
          "key transparency: log entry 0: the server bound device 0199c0de to a key that isn't the one your identity lists for it; the server swapped a device key",
      }),
    )
    render(
      <ClientProvider value={fakeClient({ sync })}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getByText(/^3 entries, checked /)).toBeInTheDocument()

    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Sync now" }))
    await waitFor(() => expect(sync).toHaveBeenCalled())
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Sync refused: key transparency: log entry 0")
    expect(alert).toHaveTextContent("swapped a device key")
    // The remembered view stays on screen beside the refusal.
    expect(screen.getByText(/^3 entries, checked /)).toBeInTheDocument()
  })

  it("says the log is not checked yet before the first sync", () => {
    const unchecked: Status = { ...status, account: { ...status.account!, transparency: null } }
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={unchecked} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getByText("not checked yet; the next sync verifies it")).toBeInTheDocument()
  })

  it("Check now runs the audit and shows its words, pass or fail (D95)", async () => {
    const audit = vi
      .fn<() => Promise<string[]>>()
      .mockResolvedValueOnce(["tree head: 3 entries, root 0123456789abcdef, signed 2026-10-17", "3 entries of this account verified: every device key is one your identity lists"])
      .mockRejectedValueOnce({ code: "refused", message: "key transparency: the tree head is smaller than the one remembered (3 → 2): a rollback" })
    render(
      <ClientProvider value={fakeClient({ audit })}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Check now" }))
    const status1 = await screen.findByRole("status")
    expect(status1).toHaveTextContent("Checked just now")
    expect(status1).toHaveTextContent("3 entries of this account verified")

    await user.click(screen.getByRole("button", { name: "Check now" }))
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("The check failed"))
    expect(screen.getByRole("status")).toHaveTextContent("a rollback")
    expect(screen.getByRole("status")).toHaveTextContent("Nothing was changed")
  })
})
