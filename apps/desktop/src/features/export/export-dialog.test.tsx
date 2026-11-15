import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { ExportPreview, GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ExportDialog } from "./export-dialog"
import { carriesLine, folderOf, leftOutLine, passwordProblem } from "./export-model"

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
    const { exportChoose, exportSave, onNotice } = mount({})
    const user = userEvent.setup()
    // The kinds are Radio Rows with their sentences, the recommended one first and chosen.
    const kinds = screen.getAllByRole("radio")
    expect(kinds[0]).toHaveAccessibleName("A Lockdown backup, locked with a password (recommended)")
    expect(kinds[0]).toBeChecked()
    const region = await screen.findByRole("region", { name: "What the file will hold" })
    await waitFor(() => expect(region).toHaveTextContent("This file will hold 12 items, 2 attached files."))
    expect(region).toHaveTextContent("1 crown jewel stays out: Cold wallet.")

    await user.type(screen.getByLabelText("A password for the file"), "abcd1234")
    await user.type(screen.getByLabelText("The same password again"), "abcd1235")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    // A field's problem shows under that field.
    expect(await screen.findByText("The two passwords don't match.")).toBeInTheDocument()
    expect(exportChoose).not.toHaveBeenCalled()

    await user.clear(screen.getByLabelText("The same password again"))
    await user.type(screen.getByLabelText("The same password again"), "abcd1234")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    // The core's gate: here, a plain confirm.
    await user.click(within(await screen.findByRole("dialog", { name: "Export the vault?" })).getByRole("button", { name: "Export" }))
    expect(await screen.findByRole("status")).toHaveTextContent("Saved to /home/ada")
    expect(onNotice).toHaveBeenLastCalledWith("Saved to /home/ada")
    expect(exportChoose).toHaveBeenCalledWith("lockdown", true)
    const input = exportSave.mock.calls.at(-1)![0]
    expect(input).toMatchObject({ target: 4, format: "lockdown", plainConfirmed: false })
    expect(input.password!.every((b) => b === 0)).toBe(true)
  })

  it("an unprotected file needs the person's yes first", async () => {
    const { exportSave } = mount({})
    const user = userEvent.setup()
    await user.click(screen.getByRole("radio", { name: "A spreadsheet of your logins (not protected)" }))
    expect(await screen.findByText("3 items that aren't logins: a CSV holds logins only")).toBeInTheDocument()
    // The yes waits for the person's yes, with its reason under it, and stays in the tab order.
    expect(screen.getByRole("button", { name: "Choose where to save…" })).toHaveAccessibleDescription(/Tick the box/)
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    expect(exportSave).not.toHaveBeenCalled()
    await user.click(screen.getByRole("checkbox", { name: /I understand this file won't be protected/ }))
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await user.click(within(await screen.findByRole("dialog", { name: "Export the vault?" })).getByRole("button", { name: "Export" }))
    expect(await screen.findByText(/This file isn't protected/)).toBeInTheDocument()
    expect(exportSave.mock.calls.at(-1)![0]).toMatchObject({ format: "csv", plainConfirmed: true, password: undefined })
  })

  it("closing the save dialog writes nothing", async () => {
    const { exportSave } = mount({ exportChoose: () => Promise.resolve(null) })
    const user = userEvent.setup()
    await screen.findByText(/12 items/)
    await user.type(screen.getByLabelText("A password for the file"), "abcd1234")
    await user.type(screen.getByLabelText("The same password again"), "abcd1234")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await waitFor(() => expect(screen.getByRole("button", { name: "Choose where to save…" })).toBeEnabled())
    expect(exportSave).not.toHaveBeenCalled()
  })
})

describe("crown jewels in a locked backup (doc 48)", () => {
  it("a locked Lockdown backup counts every crown jewel; the readable one still leaves them out", async () => {
    const exportPreview = vi.fn<Client["exportPreview"]>((f, _vault, sealed) =>
      Promise.resolve(
        f === "lockdown" && sealed
          ? { ...preview("lockdown"), crown_jewels_included: 1, crown_jewels_left_out: 0, left_out: [], crown_jewels_all: true }
          : preview(f as ExportPreview["format"]),
      ),
    )
    mount({ exportPreview })
    const user = userEvent.setup()
    const region = await screen.findByRole("region", { name: "What the file will hold" })
    await waitFor(() => expect(region).toHaveTextContent("This file will hold 12 items, 2 attached files, 1 crown jewel, protected by the file's password."))
    expect(region).not.toHaveTextContent("stays out")
    expect(exportPreview).toHaveBeenCalledWith("lockdown", null, true)

    await user.click(screen.getByRole("radio", { name: "A readable Lockdown file (not protected)" }))
    await waitFor(() => expect(region).toHaveTextContent("1 crown jewel stays out: Cold wallet."))
    expect(exportPreview).toHaveBeenLastCalledWith("lockdown", null, false)
  })
})

describe("the export wording", () => {
  it("checks passwords and names what stays out", () => {
    expect(passwordProblem("short", "short")).toMatch(/at least 8/)
    expect(passwordProblem("long enough", "long enough")).toBeNull()
    expect(leftOutLine({ ...preview("cxf"), crown_jewels_left_out: 0, left_out: [] })).toBeNull()
    expect(carriesLine({ ...preview("cxf"), crown_jewels_included: 2 })).toBe("This file will hold 12 items, 2 crown jewels you chose to include.")
    expect(folderOf("/Users/sam/Desktop/lockdown-export.ldx")).toBe("/Users/sam/Desktop")
    expect(folderOf("C:\\Users\\sam\\out.ldx")).toBe("C:\\Users\\sam")
  })
})
