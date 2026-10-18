import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider, type Client } from "@/core/client"
import type { ImportPreview } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ImportWizard } from "./import-wizard"
import { confirmWords, doneLine, passwordAsked, summaryLine } from "./import-model"

const preview: ImportPreview = {
  token: 7,
  format: "bitwarden",
  format_name: "Bitwarden (.json)",
  total: 5,
  by_kind: [
    { kind: "note", name: "Note", count: 1 },
    { kind: "login", name: "Login", count: 3 },
    { kind: "card", name: "Payment card", count: 1 },
  ],
  attachments: 0,
  crown_jewels: 1,
  new: 5,
  updated: 0,
  unchanged: 0,
  problems: ["1 items in Bitwarden's trash were left out"],
  item_notes: ["Example Forum: 'Linked username' is a linked field in Bitwarden"],
}

const formats: Client["importFormats"] = () =>
  Promise.resolve([
    { id: "bitwarden", name: "Bitwarden (.json)", how: "In Bitwarden: Tools, Export vault.", may_need_password: true },
    { id: "lastpass", name: "LastPass (.csv)", how: "In LastPass: Advanced options, Export.", may_need_password: false },
  ])

function mount(client: Client, restore: string | null = null, onImported = vi.fn()) {
  render(
    <ClientProvider value={client}>
      <ImportWizard open restore={restore} onClose={() => {}} onImported={onImported} />
    </ClientProvider>,
  )
  return onImported
}

async function pickFile(user: ReturnType<typeof userEvent.setup>, name = "bitwarden_export.json") {
  const file = new File(['{"encrypted":false,"items":[]}'], name, { type: "application/json" })
  await user.upload(screen.getByLabelText("The file"), file)
  await user.click(screen.getByRole("button", { name: "Check the file" }))
}

describe("the import wizard", () => {
  it("previews counts and what can't come, then brings the items in", async () => {
    const importPreview = vi.fn<Client["importPreview"]>(() => Promise.resolve(preview))
    const importCommit = vi.fn<Client["importCommit"]>(() => Promise.resolve({ imported: 5, updated: 0, unchanged: 0, notes: [] }))
    const onImported = mount(fakeClient({ importFormats: formats, importPreview, importCommit, importCancel: () => Promise.resolve() }))
    const user = userEvent.setup()
    // The format's own how-to shows once it is picked.
    await user.selectOptions(await screen.findByLabelText("Where is it from?"), "lastpass")
    expect(screen.getByText("In LastPass: Advanced options, Export.")).toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText("Where is it from?"), "auto")
    await pickFile(user)

    const region = await screen.findByRole("region", { name: "What the file holds" })
    expect(region).toHaveTextContent("5 items ready to bring in.")
    expect(region).toHaveTextContent("3 × Login")
    expect(region).toHaveTextContent("1 item will be kept as a crown jewel")
    expect(screen.getByRole("region", { name: "What can't come along" })).toHaveTextContent("linked field")
    const sent = importPreview.mock.calls[0]![0]
    expect(sent.format).toBe("auto")
    expect(sent.fileName).toBe("bitwarden_export.json")
    expect(sent.bytes.length).toBeGreaterThan(0)
    expect(sent.password).toBeUndefined()

    await user.click(screen.getByRole("button", { name: "Bring in 5 items" }))
    await waitFor(() => expect(importCommit).toHaveBeenCalledWith(7))
    expect(await screen.findByText("5 items brought in.")).toBeInTheDocument()
    expect(onImported).toHaveBeenCalled()
  })

  it("asks for the file's own password, and again when it was wrong", async () => {
    let tries = 0
    const importPreview = vi.fn<Client["importPreview"]>((input) => {
      tries += 1
      if (!input.password) return Promise.reject({ code: "refused", message: "this file is protected with a password; type it to open the file" })
      if (tries === 2) return Promise.reject({ code: "refused", message: "that password didn't open the file, or the file was changed after it was made" })
      return Promise.resolve(preview)
    })
    mount(fakeClient({ importFormats: formats, importPreview }))
    const user = userEvent.setup()
    await pickFile(user)
    await user.type(await screen.findByLabelText("The file's password"), "nope")
    await user.click(screen.getByRole("button", { name: "Open the file" }))
    expect(await screen.findByText("That password didn't open the file. Try again.")).toBeInTheDocument()
    await user.type(screen.getByLabelText("The file's password"), "fixture-password")
    await user.click(screen.getByRole("button", { name: "Open the file" }))
    expect(await screen.findByRole("region", { name: "What the file holds" })).toBeInTheDocument()
    const last = importPreview.mock.calls.at(-1)![0]
    // The password went as bytes, and the window wiped its copy after the call.
    expect(last.password).toHaveLength("fixture-password".length)
    expect(last.password!.every((b) => b === 0)).toBe(true)
    expect(importPreview).toHaveBeenCalledTimes(3)
  })

  it("restores a backup from the folder without picking a file", async () => {
    const backupRestorePreview = vi.fn<Client["backupRestorePreview"]>(() =>
      Promise.resolve({ ...preview, format: "lockdown", format_name: "A Lockdown backup", new: 1, updated: 1, unchanged: 3 }),
    )
    mount(fakeClient({ importFormats: formats, backupRestorePreview }), "lockdown-backup-20261018-000000.lockdown")
    expect(await screen.findByText("5 items in this file: 1 new, 1 changed since the backup was made, 3 already here as they are.")).toBeInTheDocument()
    expect(backupRestorePreview).toHaveBeenCalledWith("lockdown-backup-20261018-000000.lockdown", undefined)
    expect(screen.getByRole("button", { name: "Bring in 2 items" })).toBeEnabled()
  })
})

describe("the import wording", () => {
  it("says what happened in plain words", () => {
    expect(passwordAsked({ code: "refused", message: "this file is protected with a password; type it" })).toBe("needed")
    expect(passwordAsked({ code: "refused", message: "something else" })).toBeNull()
    expect(summaryLine({ ...preview, total: 1 })).toBe("1 item ready to bring in.")
    expect(confirmWords({ ...preview, new: 0, updated: 0 })).toBe("Nothing to bring in")
    expect(doneLine({ imported: 0, updated: 2, unchanged: 1, notes: [] })).toBe("2 items updated from the backup, 1 item already here.")
  })
})
