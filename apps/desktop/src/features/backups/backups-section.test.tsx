import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { BackupStatus, GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { BackupsSection } from "./backups-section"
import { crownLine, crownSwitchHelp, statusLine, waitingLine } from "./backups-model"

const off: BackupStatus = {
  enabled: false,
  folder: null,
  interval_hours: 24,
  keep_last: 10,
  key_set: false,
  last_success_at: null,
  last_error: null,
  next_due_at: null,
  crown_jewels_left_out: 0,
  backups: [],
  include_crown_jewels: true,
  crown_jewels_waiting: 0,
}

const on: BackupStatus = {
  ...off,
  enabled: true,
  folder: "/home/ada/Backups",
  interval_hours: 6,
  key_set: true,
  last_success_at: 1_760_000_000,
  next_due_at: 1_760_021_600,
  backups: [{ name: "lockdown-backup-20251009-084000.lockdown", bytes: 4096, made_at: 1_760_000_000 }],
}

describe("Backups in Settings", () => {
  it("turns backups on with a folder from the OS dialog and a password, through the gate", async () => {
    const backupSetup = vi.fn<Client["backupSetup"]>((_input, gate?: GateInput) =>
      gate?.confirmed ? Promise.resolve(on) : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window" }),
    )
    const client = fakeClient({
      backupStatus: () => Promise.resolve(off),
      backupPickFolder: () => Promise.resolve({ token: 9, path: "/home/ada/Backups", name: "Backups" }),
      backupSetup,
      approvalRoute: () => Promise.resolve({ route: "window", method: "window" }),
    })
    render(
      <ClientProvider value={client}>
        <GateProvider>
          <BackupsSection onChanged={() => {}} onNotice={() => {}} />
        </GateProvider>
      </ClientProvider>,
    )
    expect(await screen.findByText("Automatic backups are off.")).toBeInTheDocument()
    expect(screen.getByRole("switch", { name: "Include crown jewels" })).toBeChecked()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Turn on backups" }))
    expect(await screen.findByText(/at least 8 characters/)).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /Choose folder/ }))
    expect(await screen.findByText("/home/ada/Backups")).toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText("How often"), "6")
    await user.type(screen.getByLabelText("Backup password"), "a long backup password")
    await user.type(screen.getByLabelText("The same password again"), "a long backup password")
    await user.click(screen.getByRole("button", { name: "Turn on backups" }))
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    expect(await screen.findByText("On: every 6 hours to /home/ada/Backups, keeping the newest 10.")).toBeInTheDocument()
    const input = backupSetup.mock.calls.at(-1)![0]
    expect(input).toMatchObject({ folder: 9, intervalHours: 6, keepLast: 10 })
    expect(input.password!.every((b) => b === 0)).toBe(true)
    expect(screen.getByRole("button", { name: "Restore…" })).toBeInTheDocument()
  })

  it("restores one of the folder's backups through the import preview", async () => {
    const backupRestorePreview = vi.fn<Client["backupRestorePreview"]>(() => Promise.reject({ code: "refused", message: "this file is protected with a password; type it to open the file" }))
    const client = fakeClient({ backupStatus: () => Promise.resolve(on), backupRestorePreview, importFormats: () => Promise.resolve([]) })
    render(
      <ClientProvider value={client}>
        <BackupsSection onChanged={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Restore…" }))
    await waitFor(() => expect(backupRestorePreview).toHaveBeenCalledWith("lockdown-backup-20251009-084000.lockdown", undefined))
    expect(await screen.findByLabelText("The file's password")).toBeInTheDocument()
  })

  it("the crown-jewel switch goes through the gate both ways and brings the count back when off", async () => {
    let current: BackupStatus = { ...on, crown_jewels_waiting: 2 }
    const backupCrownJewels = vi.fn<Client["backupCrownJewels"]>((next, gate?: GateInput) => {
      if (!gate?.confirmed) return Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window" })
      current = next ? { ...on, crown_jewels_waiting: 2 } : { ...on, include_crown_jewels: false, crown_jewels_left_out: 2, crown_jewels_waiting: 0 }
      return Promise.resolve(current)
    })
    const onNotice = vi.fn()
    const client = fakeClient({
      backupStatus: () => Promise.resolve(current),
      backupCrownJewels,
      approvalRoute: () => Promise.resolve({ route: "window", method: "window" }),
    })
    render(
      <ClientProvider value={client}>
        <GateProvider>
          <BackupsSection onChanged={() => {}} onNotice={onNotice} />
        </GateProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    const toggle = await screen.findByRole("switch", { name: "Include crown jewels" })
    expect(toggle).toBeChecked()
    expect(screen.getByText(/go into every locked backup, protected by its password/)).toBeInTheDocument()
    // Said once, until the next backup runs.
    expect(screen.getByRole("status")).toHaveTextContent("Your next backup will be the first to include your 2 crown jewels.")

    await user.click(toggle)
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    await waitFor(() => expect(screen.getByRole("switch", { name: "Include crown jewels" })).not.toBeChecked())
    expect(backupCrownJewels).toHaveBeenLastCalledWith(false, { confirmed: true })
    expect(screen.getByText(/2 crown jewels aren't in your backups/)).toBeInTheDocument()
    expect(screen.getByText("Crown jewels stay out of locked backups unless you include them one by one.")).toBeInTheDocument()
    expect(screen.queryByText(/Your next backup will be the first/)).not.toBeInTheDocument()
    expect(onNotice).toHaveBeenLastCalledWith("Crown jewels stay out of new backups. Backups you already have keep what they hold.")

    await user.click(screen.getByRole("switch", { name: "Include crown jewels" }))
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    await waitFor(() => expect(screen.getByRole("switch", { name: "Include crown jewels" })).toBeChecked())
    expect(backupCrownJewels).toHaveBeenLastCalledWith(true, { confirmed: true })
    expect(screen.queryByText(/aren't in your backups/)).not.toBeInTheDocument()
  })

  it("words", () => {
    expect(statusLine({ ...on, interval_hours: 24 })).toBe("On: every day to /home/ada/Backups, keeping the newest 10.")
    expect(waitingLine({ ...on, crown_jewels_waiting: 1 })).toBe("Your next backup will be the first to include your 1 crown jewel.")
    expect(waitingLine({ ...on, include_crown_jewels: false, crown_jewels_waiting: 1 })).toBeNull()
    // An older core says nothing of the switch: on, as the core defaults.
    const older = { ...on, include_crown_jewels: undefined, crown_jewels_waiting: undefined, crown_jewels_left_out: 1 }
    expect(crownLine(older)).toBeNull()
    expect(waitingLine(older)).toBeNull()
    expect(crownLine({ ...on, include_crown_jewels: false, crown_jewels_left_out: 1 })).toMatch(/^1 crown jewel isn't in your backups/)
    expect(crownSwitchHelp(false)).toMatch(/one by one/)
  })
})
