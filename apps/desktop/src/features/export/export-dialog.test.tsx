import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { ExportPreview, GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ExportDialog } from "./export-dialog"
import { leftOutLine, passwordProblem } from "./export-model"

const preview = (format: ExportPreview["format"]): ExportPreview => ({
  format,
  items: 12,
  attachments: format === "lockdown" ? 2 : 0,
  crown_jewels_included: 0,
  crown_jewels_left_out: 1,
  left_out: [{ id: "w", title: "Cold wallet", kind: "crypto-wallet", reason: "It's a crown jewel" }],
  dropped: format === "csv" ? ["3 items that aren't logins: a CSV holds logins only"] : [],
  encrypted: false,
})

function mount(over: Partial<Client>) {
  const exportChoose = vi.fn<Client["exportChoose"]>(() => Promise.resolve({ token: 4, path: "/home/ada/out", name: "out" }))
  const exportSave = vi.fn<Client["exportSave"]>((input, gate?: GateInput) =>
    gate?.confirmed
      ? Promise.resolve({ path: "/home/ada/out", bytes: 100, preview: { ...preview(input.format as ExportPreview["format"]), encrypted: !!input.password } })
      : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to export the vault" }),
  )
  const client = fakeClient({
    exportPreview: (f) => Promise.resolve(preview(f as ExportPreview["format"])),
    approvalRoute: () => Promise.resolve({ route: "window", method: "window" }),
    exportChoose,
    exportSave,
    ...over,
  })
  const onNotice = vi.fn()
  render(
    <ClientProvider value={client}>
      <GateProvider>
        <ExportDialog open onClose={() => {}} onNotice={onNotice} />
      </GateProvider>
    </ClientProvider>,
  )
  return { exportChoose, exportSave, onNotice }
}

describe("the export dialog", () => {
  it("says what stays out, locks the file with a password, and saves where the person chose", async () => {
    const { exportChoose, exportSave } = mount({})
    const user = userEvent.setup()
    const region = await screen.findByRole("region", { name: "What the file will hold" })
    await waitFor(() => expect(region).toHaveTextContent("This file will hold 12 items, 2 attached files."))
    expect(region).toHaveTextContent("1 crown jewel stays out: Cold wallet.")

    await user.type(screen.getByLabelText("A password for the file"), "long enough pw")
    await user.type(screen.getByLabelText("The same password again"), "long enough pX")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    expect(await screen.findByText("The two passwords don't match.")).toBeInTheDocument()
    expect(exportChoose).not.toHaveBeenCalled()

    await user.clear(screen.getByLabelText("The same password again"))
    await user.type(screen.getByLabelText("The same password again"), "long enough pw")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    // The core's gate: here, a plain confirm.
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    expect(await screen.findByText("Saved to /home/ada/out.")).toBeInTheDocument()
    expect(exportChoose).toHaveBeenCalledWith("lockdown", true)
    const input = exportSave.mock.calls.at(-1)![0]
    expect(input).toMatchObject({ target: 4, format: "lockdown", plainConfirmed: false })
    expect(input.password!.every((b) => b === 0)).toBe(true)
  })

  it("an unprotected file needs the person's yes first", async () => {
    const { exportSave } = mount({})
    const user = userEvent.setup()
    await user.selectOptions(screen.getByLabelText("Kind of file"), "csv")
    expect(await screen.findByText("3 items that aren't logins: a CSV holds logins only")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    expect(await screen.findByText(/Tick the box/)).toBeInTheDocument()
    await user.click(screen.getByRole("checkbox", { name: /I understand this file won't be protected/ }))
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    expect(await screen.findByText(/This file isn't protected/)).toBeInTheDocument()
    expect(exportSave.mock.calls.at(-1)![0]).toMatchObject({ format: "csv", plainConfirmed: true, password: undefined })
  })

  it("closing the save dialog writes nothing", async () => {
    const { exportSave } = mount({ exportChoose: () => Promise.resolve(null) })
    const user = userEvent.setup()
    await screen.findByText(/12 items/)
    await user.type(screen.getByLabelText("A password for the file"), "long enough pw")
    await user.type(screen.getByLabelText("The same password again"), "long enough pw")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await waitFor(() => expect(screen.getByRole("button", { name: "Choose where to save…" })).toBeEnabled())
    expect(exportSave).not.toHaveBeenCalled()
  })
})

describe("the export wording", () => {
  it("checks passwords and names what stays out", () => {
    expect(passwordProblem("short", "short")).toMatch(/at least 8/)
    expect(passwordProblem("long enough", "long enough")).toBeNull()
    expect(leftOutLine({ ...preview("cxf"), crown_jewels_left_out: 0, left_out: [] })).toBeNull()
  })
})
