import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { ItemDetail, UserTemplateView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { ItemEditor, type EditorTarget } from "./item-editor"

function mount(client: Client, target: EditorTarget, onSaved = vi.fn()) {
  render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <ItemEditor target={target} onClose={() => {}} onSaved={onSaved} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
  return onSaved
}

const templates = () => Promise.resolve(TEMPLATES)

const GYM: UserTemplateView = {
  id: "7d0c1f1e-2f4b-4c62-9c1e-0a6c2b1e5f10",
  vault_id: "v1",
  name: "Gym locker",
  summary: "Where my things are",
  icon: "",
  fields: [
    { key: "locker", field_id: "tpl.locker", label: "Locker", kind: "text", required: true },
    { key: "combo", field_id: "tpl.combo", label: "Combination", kind: "hidden", secret: true },
  ],
  versions: 1,
}

describe("vault depth in the editor (docs/plan/21-vault-depth.md)", () => {
  it("makes a variant by its template id (D231)", async () => {
    const addItem = vi.fn<Client["addItem"]>(() => Promise.resolve("p1"))
    mount(fakeClient({ templates, addItem }), { kind: "identity/passport", item: null })
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: "New passport" })).toBeInTheDocument()
    await user.type(screen.getByLabelText("Title"), "My passport")
    await user.type(screen.getByLabelText("Passport number"), "X1234567")
    await user.click(screen.getByRole("button", { name: "Add" }))
    await waitFor(() => expect(addItem).toHaveBeenCalled())
    const sent = addItem.mock.calls[0]![0]
    expect(sent.kind).toBe("identity/passport")
    expect(sent.fields[0]).toMatchObject({ field_id: "passport.number", action: "set" })
    expect(sent.fields[0]!.secret).toEqual(Array(8).fill(0))
  })

  it("makes an item from the person's own template, its secret as bytes (D232)", async () => {
    const addItem = vi.fn<Client["addItem"]>(() => Promise.resolve("g1"))
    mount(fakeClient({ templates, addItem, userTemplates: () => Promise.resolve([GYM]) }), { kind: `user:${GYM.id}`, item: null })
    const user = userEvent.setup()
    expect(await screen.findByRole("heading", { name: "New gym locker" })).toBeInTheDocument()
    await user.type(screen.getByLabelText("Title"), "Leisure centre")
    await user.type(screen.getByLabelText("Locker"), "41")
    await user.type(screen.getByLabelText("Combination"), "12-34")
    await user.click(screen.getByRole("button", { name: "Add" }))
    await waitFor(() => expect(addItem).toHaveBeenCalled())
    const sent = addItem.mock.calls[0]![0]
    expect(sent.kind).toBe(`user:${GYM.id}`)
    expect(sent.fields.find((f) => f.field_id === "tpl.locker")).toMatchObject({ action: "set", text: "41", label: "Locker", kind: "text" })
    expect(sent.fields.find((f) => f.field_id === "tpl.combo")).toMatchObject({ action: "set", label: "Combination", kind: "hidden" })
  })

  it("generates with the person's options and shows the core's preview, never a value (D230, D236)", async () => {
    const addItem = vi.fn<Client["addItem"]>(() => Promise.resolve("l1"))
    const generatorPreview = vi.fn<Client["generatorPreview"]>((recipe) =>
      Promise.resolve(
        recipe.style === "words"
          ? { words: "5 words joined by \"-\"", bits: 64.6, strength: "fair" as const, site: null }
          : { words: "20 characters", bits: 125, strength: "strong" as const, site: { host: "example.com", rule: "minlength: 8;", words: "at least 8 characters", source: "published" as const } },
      ),
    )
    mount(fakeClient({ templates, addItem, generatorPreview }), { kind: "login", item: null })
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "New login" })
    await user.type(screen.getByLabelText("Website"), "https://example.com")
    await user.click(screen.getByRole("button", { name: "Generate password" }))
    await user.click(screen.getByRole("button", { name: "Options" }))
    expect(await screen.findByText(/example.com's published rules apply/)).toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText("Make"), "words")
    fireEvent.change(screen.getByLabelText("Words"), { target: { value: "5" } })
    expect(await screen.findByText(/about 65 bits, fair/)).toBeInTheDocument()
    expect(generatorPreview).toHaveBeenLastCalledWith(expect.objectContaining({ style: "words", words: 5 }), "https://example.com", undefined)
    await user.type(screen.getByLabelText("Title"), "Example")
    await user.click(screen.getByRole("button", { name: "Add" }))
    await waitFor(() => expect(addItem).toHaveBeenCalled())
    const pw = addItem.mock.calls[0]![0].fields.find((f) => f.field_id === "pw")
    expect(pw).toEqual({ field_id: "pw", action: "generate", recipe: { style: "words", words: 5, separator: "-", capitalize: false, number: false } })
  })

  it("adds a link to another item's field and a code of your own (D235)", async () => {
    const editItem = vi.fn<Client["editItem"]>(() => Promise.resolve(2))
    const linkTargets = vi.fn<Client["linkTargets"]>(() =>
      Promise.resolve([{ item_id: "w1", title: "Home Wi-Fi", kind: "wi-fi", crown_jewel: false, fields: [{ field_id: "wifi.passphrase", label: "Password", secret: true, code: false }] }]),
    )
    const note: ItemDetail = {
      id: "n1",
      kind: "custom",
      state: "live",
      version: 1,
      edited_at: 0,
      crown_jewel: false,
      vault_id: "v1",
      fields: [{ field_id: "title", label: "title", kind: "text", hidden: false, value: "Guest info", mask: null }],
    }
    mount(fakeClient({ templates, editItem, linkTargets, approvalRoute: () => Promise.resolve({ route: "window", method: "window" }) }), { kind: "custom", item: note })
    const user = userEvent.setup()
    await screen.findByRole("heading", { name: "Edit Guest info" })
    await user.type(screen.getByLabelText("New field's label"), "Wi-Fi password")
    await user.selectOptions(screen.getByLabelText("New field's kind"), "linked")
    await user.click(screen.getByRole("button", { name: "Add field" }))
    await user.selectOptions(await screen.findByLabelText("Wi-Fi password: the item"), "w1")
    expect(linkTargets).toHaveBeenCalledWith("v1", "n1")
    await user.type(screen.getByLabelText("New field's label"), "Door code")
    await user.selectOptions(screen.getByLabelText("New field's kind"), "totp")
    await user.click(screen.getByRole("button", { name: "Add field" }))
    await user.type(screen.getByLabelText("Door code"), "GEZDGNBVGY3TQOJQ")
    await user.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() => expect(editItem).toHaveBeenCalled())
    const changes = editItem.mock.calls[0]![0].changes
    expect(changes).toContainEqual({ label: "Wi-Fi password", kind: "linked", action: "set", text: "w1#wifi.passphrase" })
    const code = changes.find((c) => c.label === "Door code")
    expect(code).toMatchObject({ kind: "totp", action: "set" })
    expect(code!.text).toBeUndefined()
  })
})
