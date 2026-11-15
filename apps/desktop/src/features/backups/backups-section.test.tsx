import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { BackupStatus, GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { BackupsSection } from "./backups-section"
import { crownLine, crownSwitchHelp, lastBackupLine, nextLine, statusLine, waitingLine } from "./backups-model"

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

/** A FieldRow's value cut in the middle is in pieces in the DOM; this finds it whole. */
const rowValue = (path: string) => (_: string, el: Element | null) => el?.getAttribute("data-slot") === "field-row-value" && el.textContent === path

describe("Backups in Settings", () => {
  it("turns backups on with a folder from the OS dialog and a password, through the gate", async () => {
    const backupSetup = vi.fn<Client["backupSetup"]>((_input, gate?: GateInput) =>
      gate?.confirmed ? Promise.resolve(on) : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window" }),
    )
    const onNotice = vi.fn()
    const client = fakeClient({
      backupStatus: () => Promise.resolve(off),
      backupPickFolder: () => Promise.resolve({ token: 9, path: "/home/ada/Backups", name: "Backups" }),
      backupSetup,
      approvalRoute: () => Promise.resolve({ route: "window", method: "window" }),
    })
    render(
      <ClientProvider value={client}>
        <GateProvider>
          <BackupsSection onChanged={() => {}} onNotice={onNotice} />
        </GateProvider>
      </ClientProvider>,
    )
    expect(await screen.findByText(/Backups are off/)).toBeInTheDocument()
    const user = userEvent.setup()
    // The section's one primary waits for a folder, with its reason (#37).
    const turnOn = screen.getByRole("button", { name: "Turn on backups" })
    expect(turnOn).toHaveAccessibleDescription("Choose a folder first.")
    await user.click(turnOn)
    expect(backupSetup).not.toHaveBeenCalled()

    await user.click(screen.getByRole("button", { name: "Choose…" }))
    expect(await screen.findByText(rowValue("/home/ada/Backups"))).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Turn on backups" }))
    expect(await screen.findByText(/at least 8 characters/)).toBeInTheDocument()

    await user.type(screen.getByLabelText("Backup password"), "backup pw1")
    await user.type(screen.getByLabelText("The same password again"), "backup pw1")
    await user.click(screen.getByRole("button", { name: "Turn on backups" }))
    await user.click(within(await screen.findByRole("dialog", { name: "Go ahead?" })).getByRole("button", { name: "Continue" }))
    // On: the rows that save themselves.
    expect(await screen.findByRole("button", { name: "Back up now" })).toBeInTheDocument()
    expect(screen.getByText(rowValue("/home/ada/Backups"))).toBeInTheDocument()
    const input = backupSetup.mock.calls.at(-1)![0]
    expect(input).toMatchObject({ folder: 9, intervalHours: 24, keepLast: 10 })
    expect(input.password!.every((b) => b === 0)).toBe(true)
    expect(screen.getByRole("button", { name: "Restore…" })).toBeInTheDocument()
    expect(screen.getByRole("switch", { name: "Include crown jewels" })).toBeChecked()
    expect(onNotice).toHaveBeenLastCalledWith("Backups turned on")
  })

  it("off, HOW OFTEN is five choices behind CHANGE, and the choice goes with TURN ON BACKUPS", async () => {
    const backupSetup = vi.fn<Client["backupSetup"]>(() => Promise.resolve(on))
    const client = fakeClient({
      backupStatus: () => Promise.resolve({ ...off, key_set: true }),
      backupPickFolder: () => Promise.resolve({ token: 9, path: "/home/ada/Backups", name: "Backups" }),
      backupSetup,
    })
    render(
      <ClientProvider value={client}>
        <BackupsSection onChanged={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Change", description: /How often/ }))
    await user.click(await screen.findByRole("menuitemradio", { name: "Every 6 hours" }))
    expect(screen.getByText(rowValue("Every 6 hours"))).toBeInTheDocument()
    // A password kept from before is not asked again.
    expect(screen.queryByLabelText("Backup password")).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Choose…" }))
    await user.click(await screen.findByRole("button", { name: "Turn on backups" }))
    await waitFor(() => expect(backupSetup).toHaveBeenCalled())
    expect(backupSetup.mock.calls.at(-1)![0]).toMatchObject({ folder: 9, intervalHours: 6, password: undefined })
  })

  it("once on, a row saves by itself: KEEP through CHANGE, with no Save button", async () => {
    const backupSetup = vi.fn<Client["backupSetup"]>((input) => Promise.resolve({ ...on, keep_last: input.keepLast }))
    const client = fakeClient({ backupStatus: () => Promise.resolve(on), backupSetup, approvalRoute: () => Promise.resolve({ route: "window", method: "window" }) })
    render(
      <ClientProvider value={client}>
        <GateProvider>
          <BackupsSection onChanged={() => {}} onNotice={() => {}} />
        </GateProvider>
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Change", description: /Keep/ }))
    await user.click(await screen.findByRole("menuitemradio", { name: "The newest 20" }))
    await waitFor(() => expect(backupSetup).toHaveBeenCalled())
    expect(backupSetup.mock.calls.at(-1)![0]).toEqual({ folder: null, password: undefined, intervalHours: 6, keepLast: 20 })
    expect(await screen.findByText(rowValue("The newest 20"))).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /Save/ })).not.toBeInTheDocument()
  })

  it("turning off asks first, focus on Cancel, with its one child: forget the password", async () => {
    const backupOff = vi.fn<Client["backupOff"]>(() => Promise.resolve(off))
    const onNotice = vi.fn()
    const client = fakeClient({ backupStatus: () => Promise.resolve(on), backupOff })
    render(
      <ClientProvider value={client}>
        <BackupsSection onChanged={() => {}} onNotice={onNotice} />
      </ClientProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Turn off backups" }))
    const ask = await screen.findByRole("dialog", { name: "Turn off backups?" })
    expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus()
    expect(backupOff).not.toHaveBeenCalled()
    await user.click(within(ask).getByRole("checkbox", { name: "Also forget the backup password on this computer" }))
    await user.click(within(ask).getByRole("button", { name: "Turn off" }))
    await waitFor(() => expect(backupOff).toHaveBeenCalledWith(true))
    expect(await screen.findByText(/Backups are off/)).toBeInTheDocument()
    expect(onNotice).toHaveBeenLastCalledWith("Backups turned off")
  })

  it("a failed backup shows the critical Banner at the top, in the core's words, with BACK UP NOW", async () => {
    const backupNow = vi.fn<Client["backupNow"]>(() => Promise.resolve(on))
    const client = fakeClient({ backupStatus: () => Promise.resolve({ ...on, last_error: "The backup folder /Volumes/Backup isn't there." }), backupNow })
    render(
      <ClientProvider value={client}>
        <BackupsSection onChanged={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    const banner = await screen.findByRole("alert")
    expect(banner).toHaveTextContent("The last backup didn't work")
    expect(banner).toHaveTextContent("The backup folder /Volumes/Backup isn't there.")
    // One BACK UP NOW: the Banner's.
    expect(screen.getAllByRole("button", { name: "Back up now" })).toHaveLength(1)
    await userEvent.setup().click(within(banner).getByRole("button", { name: "Back up now" }))
    await waitFor(() => expect(backupNow).toHaveBeenCalled())
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument())
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
    await user.click(within(await screen.findByRole("dialog", { name: "Go ahead?" })).getByRole("button", { name: "Continue" }))
    await waitFor(() => expect(screen.getByRole("switch", { name: "Include crown jewels" })).not.toBeChecked())
    expect(backupCrownJewels).toHaveBeenLastCalledWith(false, { confirmed: true })
    expect(screen.getByText(/2 crown jewels aren't in your backups/)).toBeInTheDocument()
    expect(screen.getByText("Crown jewels stay out of locked backups unless you include them one by one.")).toBeInTheDocument()
    expect(screen.queryByText(/Your next backup will be the first/)).not.toBeInTheDocument()
    // A toast without a full stop (cross-app §12), naming what changed: new backups.
    expect(onNotice).toHaveBeenLastCalledWith("Crown jewels stay out of new backups")

    await user.click(screen.getByRole("switch", { name: "Include crown jewels" }))
    await user.click(within(await screen.findByRole("dialog", { name: "Go ahead?" })).getByRole("button", { name: "Continue" }))
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
    expect(lastBackupLine(off)).toBe("No backup yet")
    expect(lastBackupLine({ ...on, last_success_at: 1_000_000 }, 1_000_000 + 3 * 3600 + 5)).toBe("Backed up 3 hours ago")
    expect(lastBackupLine({ ...on, last_success_at: 1_000_000 }, 1_000_000 + 60)).toBe("Backed up 1 minute ago")
    expect(nextLine(off)).toBeNull()
    expect(nextLine({ ...on, next_due_at: 100 }, 200)).toBe("Due now, while Lockdown is unlocked.")
  })
})
