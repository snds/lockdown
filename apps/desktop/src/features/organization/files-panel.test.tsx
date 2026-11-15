import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldTemplateView, Templates, VaultView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { FilesPanel, MAX_FILE_BYTES } from "./files-panel"
import { VaultsProvider } from "./vaults-context"

const VAULTS: VaultView[] = [
  {
    id: "v1",
    name: "Personal",
    primary: true,
    items: 2,
    trash_days: 30,
    folders: [
      { id: "f-work", name: "Work", parent: null, path: "Work" },
      { id: "f-clients", name: "Clients", parent: "f-work", path: "Work / Clients" },
    ],
  },
  { id: "v2", name: "Family", primary: false, items: 1, trash_days: 7, folders: [] },
]

// The core's Login template: its websites repeat.
const LOGIN_TEMPLATES: Templates = {
  ...TEMPLATES,
  kinds: TEMPLATES.kinds.map((t) =>
    t.kind === "login" ? { ...t, fields: t.fields.map((f): FieldTemplateView => (f.field_id === "url.0" ? { ...f, repeats: true } : f)) } : t,
  ),
}

function mount(client: Client, ui: ReactNode, vaults = VAULTS) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <VaultsProvider value={{ vaults, reload: () => {} }}>{ui}</VaultsProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const base = { templates: () => Promise.resolve(LOGIN_TEMPLATES), approvalRoute: () => Promise.resolve({ route: "window" as const, method: "window" }) }

describe("files on an item", () => {
  it("attaches the picked file's bytes and saves one back on request", async () => {
    const attach = vi.fn<Client["attach"]>(() => Promise.resolve("att-1"))
    const attachment = vi.fn<Client["attachment"]>(() => Promise.resolve(new Uint8Array([104, 105])))
    const onChanged = vi.fn()
    const createObjectURL = vi.fn(() => "blob:x")
    Object.assign(URL, { createObjectURL, revokeObjectURL: () => {} })
    mount(
      fakeClient({ ...base, attach, attachment }),
      <FilesPanel itemId="i1" files={[{ id: "att-0", name: "scan.pdf", mime: "application/pdf", size: 2048 }]} editable onChanged={onChanged} onNotice={() => {}} />,
    )
    expect(screen.getByText("2 KB · application/pdf")).toBeInTheDocument()
    const user = userEvent.setup()
    const file = new File([new Uint8Array([1, 2, 3])], "receipt.txt", { type: "text/plain" })
    await user.upload(screen.getByLabelText("File to attach"), file)
    await waitFor(() => expect(attach).toHaveBeenCalled())
    const [id, name, mime, bytes] = attach.mock.calls[0]!
    expect([id, name, mime]).toEqual(["i1", "receipt.txt", "text/plain"])
    // The webview's copy is wiped once the core has it.
    expect(Array.from(bytes)).toEqual([0, 0, 0])
    expect(onChanged).toHaveBeenCalledWith('Attached "receipt.txt".')
    await user.click(screen.getByRole("button", { name: "Save scan.pdf" }))
    await waitFor(() => expect(attachment).toHaveBeenCalledWith("i1", "att-0", {}))
    expect(createObjectURL).toHaveBeenCalled()
  })

  it("refuses a file larger than the app attaches, before reading it", async () => {
    const attach = vi.fn<Client["attach"]>()
    mount(fakeClient({ ...base, attach }), <FilesPanel itemId="i1" files={[]} editable onChanged={() => {}} onNotice={() => {}} />)
    const big = new File(["x"], "huge.iso")
    Object.defineProperty(big, "size", { value: MAX_FILE_BYTES + 1 })
    await userEvent.setup().upload(screen.getByLabelText("File to attach"), big)
    expect(await screen.findByRole("alert")).toHaveTextContent("huge.iso")
    expect(attach).not.toHaveBeenCalled()
  })
})
