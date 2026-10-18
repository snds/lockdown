import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { UserTemplateView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { TemplatesManager } from "./template-designer"

const GYM: UserTemplateView = {
  id: "t1",
  vault_id: "v1",
  name: "Gym locker",
  summary: "",
  icon: "",
  fields: [{ key: "locker", field_id: "tpl.locker", label: "Locker", kind: "text", required: true }],
  versions: 1,
}

function mount(client: Client, onNotice = vi.fn()) {
  render(
    <ClientProvider value={client}>
      <TemplatesProvider>
        <TemplatesManager open onOpenChange={() => {}} onNotice={onNotice} />
      </TemplatesProvider>
    </ClientProvider>,
  )
  return onNotice
}

describe("a person's own templates (D232)", () => {
  it("makes one with ordered fields, each keyed from its label", async () => {
    const saveUserTemplate = vi.fn<Client["saveUserTemplate"]>(() => Promise.resolve("t9"))
    const onNotice = mount(fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([]), saveUserTemplate }))
    const user = userEvent.setup()
    expect(await screen.findByText(/None yet/)).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "New template" }))
    await user.type(await screen.findByLabelText("Name"), "Pet")
    await user.type(screen.getByLabelText("Field 1's label"), "Microchip number")
    await user.click(screen.getByRole("switch", { name: "Field 1 is needed" }))
    await user.click(screen.getByRole("button", { name: "Add a field" }))
    await user.type(screen.getByLabelText("Field 2's label"), "Vet's portal password")
    await user.selectOptions(screen.getByLabelText("Field 2's kind"), "hidden")
    await user.click(screen.getByRole("button", { name: "Move field 2 up" }))
    await user.click(screen.getByRole("button", { name: "Save template" }))
    await waitFor(() => expect(saveUserTemplate).toHaveBeenCalled())
    expect(saveUserTemplate.mock.calls[0]![0]).toEqual({
      id: undefined,
      vault: undefined,
      template: {
        name: "Pet",
        summary: "",
        fields: [
          { key: "vet_s_portal_password", label: "Vet's portal password", kind: "hidden", required: false },
          { key: "microchip_number", label: "Microchip number", kind: "text", required: true },
        ],
      },
    })
    expect(onNotice).toHaveBeenCalledWith('Made the template "Pet".')
  })

  it("changes one keeping its keys, and deletes one after a confirm", async () => {
    const saveUserTemplate = vi.fn<Client["saveUserTemplate"]>(() => Promise.resolve("t1"))
    const deleteUserTemplate = vi.fn<Client["deleteUserTemplate"]>(() => Promise.resolve())
    mount(fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve([GYM]), saveUserTemplate, deleteUserTemplate }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Change Gym locker" }))
    const label = await screen.findByLabelText("Field 1's label")
    await user.clear(label)
    await user.type(label, "Locker number")
    await user.click(screen.getByRole("button", { name: "Save template" }))
    await waitFor(() => expect(saveUserTemplate).toHaveBeenCalled())
    const sent = saveUserTemplate.mock.calls[0]![0]
    expect(sent.id).toBe("t1")
    expect(sent.template.fields[0]).toEqual({ key: "locker", label: "Locker number", kind: "text", required: true })
    await user.click(await screen.findByRole("button", { name: "Delete Gym locker" }))
    await user.click(await screen.findByRole("button", { name: "Delete" }))
    await waitFor(() => expect(deleteUserTemplate).toHaveBeenCalledWith("t1"))
  })
})
