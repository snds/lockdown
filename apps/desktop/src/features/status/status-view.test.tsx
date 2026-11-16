import { render, screen, waitFor, within } from "@testing-library/react"
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

describe("the Status screen (brief T4, doc 82)", () => {
  it("has no filled button: Check now is secondary, Sync now a text action (design review R19)", () => {
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const filled = screen.getAllByRole("button").filter((b) => b.className.split(" ").includes("bg-primary"))
    expect(filled).toEqual([])
    expect(screen.getByRole("button", { name: "Check now" })).toHaveAttribute("data-variant", "secondary")
    expect(screen.getByRole("button", { name: "Sync now" })).toHaveAttribute("data-variant", "ghost")
  })

  it("puts each proof in a row with its word, and ids and hashes only under Details (R14)", async () => {
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const proofs = screen.getByRole("region", { name: "Locked, even from us" })
    expect(within(proofs).getByText("Never leaves this computer.")).toBeInTheDocument()
    expect(within(proofs).getByText("Made here. The server has never seen it.")).toBeInTheDocument()
    expect(within(proofs).getAllByText("Verified")).toHaveLength(4)
    // Green is Lockdown, not "verified" (Lockdown 6, D701): every proof's badge is neutral, VERIFIED with a check glyph.
    for (const badge of proofs.querySelectorAll('[data-slot="badge"]')) expect(badge).toHaveAttribute("data-tone", "neutral")
    for (const word of within(proofs).getAllByText("Verified")) expect(word.closest('[data-slot="badge"]')?.querySelector("svg")).not.toBeNull()
    expect(within(proofs).getByText(/^Checked /)).toBeInTheDocument()
    // The persona is a fact of this device.
    expect(screen.getByRole("region", { name: "This device" })).toHaveTextContent("Personal (Personal)")
    expect(screen.queryByText(status.device_id)).not.toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: /Details/ }))
    expect(screen.getByText(status.device_id)).toBeInTheDocument()
    expect(screen.getByText(`3 // ${"ab".repeat(32)}`)).toBeInTheDocument()
  })

  it("shows the audit's words when a sync is refused, and keeps the verified log", async () => {
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
    expect(screen.getByText(/^Checked /)).toBeInTheDocument()

    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Sync now" }))
    await waitFor(() => expect(sync).toHaveBeenCalled())
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Sync refused: key transparency: log entry 0")
    expect(alert).toHaveTextContent("swapped a device key")
    // The remembered view stays on screen beside the refusal.
    expect(screen.getByText(/^Checked /)).toBeInTheDocument()
  })

  it("asks for the password in the Sign in form, saying why, when the server won't renew the session (docs/plan/33)", async () => {
    const words =
      "This device's session was ended from one of your devices or by whoever runs the server, so it can't renew itself. Log in again with your master password."
    const sync = vi.fn(() => Promise.reject({ code: "sign_in_needed", message: words, detail: "ended" }))
    render(
      <ClientProvider value={fakeClient({ sync })}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Sync now" }))
    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByRole("heading", { name: "Sign in" })).toBeInTheDocument()
    expect(dialog).toHaveTextContent("was ended from one of your devices")
    // A kit exists: the form asks for its Secret Key too.
    expect(within(dialog).getByText("Secret Key")).toBeInTheDocument()
    expect(screen.queryByText(/^Sync refused/)).not.toBeInTheDocument()
  })

  it("keeps the reason in a critical Banner with Sign in while the account needs a sign-in", async () => {
    const needs: Status = {
      ...status,
      recovery: "none",
      account: {
        ...status.account!,
        sign_in: { reason: "idle", message: "This device went unused for longer than the server allows (30 days unless whoever runs it changed that), so its session can't be renewed. Log in again with your master password." },
      },
    }
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={needs} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const banner = screen.getByRole("alert")
    expect(banner).toHaveTextContent("Lockdown isn't syncing")
    expect(banner).toHaveTextContent("went unused for longer than the server allows")
    const user = userEvent.setup()
    await user.click(within(banner).getByRole("button", { name: "Sign in" }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("went unused")
    // No kit on this device: no Secret Key to ask for.
    expect(within(dialog).queryByText("Secret Key")).not.toBeInTheDocument()
  })

  it("says a revoked device joins again, and offers no sign-in (docs/plan/35)", async () => {
    const words = "This device was revoked from your account. Join it again from a device you still use."
    const revoked: Status = { ...status, account: { ...status.account!, sign_in: { reason: "revoked", message: words } } }
    const sync = vi.fn(() => Promise.reject({ code: "sign_in_needed", message: words, detail: "revoked" }))
    render(
      <ClientProvider value={fakeClient({ sync, status: () => Promise.resolve(revoked) })}>
        <StatusView status={revoked} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getByRole("alert")).toHaveTextContent(words)
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Sync now" }))
    await waitFor(() => expect(sync).toHaveBeenCalled())
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(screen.queryByText(/^Sync refused/)).not.toBeInTheDocument()
  })

  it("says the log is not checked yet before the first sync", () => {
    const unchecked: Status = { ...status, account: { ...status.account!, transparency: null } }
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={unchecked} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getByText("Not checked yet. The next sync checks it.")).toBeInTheDocument()
  })

  it("Check now: a pass says nothing and puts its lines under Details; a failure is a critical Banner (D95)", async () => {
    const audit = vi
      .fn<() => Promise<string[]>>()
      .mockResolvedValueOnce(["tree head: 3 entries, root 0123456789abcdef, signed 2026-10-17", "3 entries of this account verified: every device key is one your identity lists"])
      .mockRejectedValueOnce({ code: "refused", message: "key transparency: the tree head is smaller than the one remembered (3 → 2): a rollback" })
    const onNotice = vi.fn()
    render(
      <ClientProvider value={fakeClient({ audit })}>
        <StatusView status={status} onStatus={() => {}} onNotice={onNotice} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Check now" }))
    await waitFor(() => expect(audit).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(onNotice).not.toHaveBeenCalled()
    // The pass says so on its row, quietly (Lockdown 37).
    expect(screen.getByRole("region", { name: "Locked, even from us" })).toHaveTextContent(/Checked Today \d{2}:\d{2}/)
    await user.click(screen.getByRole("button", { name: /Details/ }))
    expect(screen.getByRole("list", { name: "Last check" })).toHaveTextContent("3 entries of this account verified")

    await user.click(screen.getByRole("button", { name: "Check now" }))
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("The check failed")
    expect(alert).toHaveTextContent("a rollback")
    expect(alert).toHaveTextContent("Nothing was changed")
  })

  it("lists what was held back for support under Details, in the check's own words, and nothing when none was", async () => {
    const detail = "Family: head manifest 7 by bob@example.com held back: the vault's history didn't check out at manifest 7: the device signature didn't check out"
    const { rerender } = render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={{ ...status, held_back: [detail] }} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    await userEvent.setup().click(screen.getByRole("button", { name: /Details/ }))
    const list = screen.getByRole("list", { name: "Held back until they verify" })
    expect(list).toHaveTextContent(detail)
    rerender(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={status} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.queryByRole("list", { name: "Held back until they verify" })).not.toBeInTheDocument()
  })

  it("shows a local account no server words: one line and Set up sync (brief T4)", async () => {
    const local: Status = { ...status, account: { ...status.account!, local: true, server: "lockdown-local://this-device" } }
    const { container } = render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={local} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(container.textContent ?? "").not.toMatch(/the log/)
    await userEvent.setup().click(screen.getByRole("button", { name: /Details/ }))
    expect(screen.getByText("Your vault is kept on this computer only.")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Set up sync" })).toBeInTheDocument()
    const text = container.textContent ?? ""
    for (const word of [/server/i, /public log/i, /mirror/i, /sync now/i, /check now/i, /session/i, /login/i, /lockdown-local/]) expect(text).not.toMatch(word)
  })

  it("offers Register and Sign in, both secondary, with no account", async () => {
    render(
      <ClientProvider value={fakeClient({})}>
        <StatusView status={{ ...status, account: null }} onStatus={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getByRole("button", { name: "Register" })).toHaveAttribute("data-variant", "secondary")
    expect(screen.getByRole("button", { name: "Sign in" })).toHaveAttribute("data-variant", "secondary")
    await userEvent.setup().click(screen.getByRole("button", { name: "Register" }))
    expect(await screen.findByRole("dialog")).toHaveTextContent("Register an account")
  })
})
