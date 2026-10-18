import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import type { UserTemplateView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { NewItemMenu } from "./new-item-menu"

const GYM: UserTemplateView = {
  id: "7d0c1f1e-2f4b-4c62-9c1e-0a6c2b1e5f10",
  vault_id: "v",
  name: "Gym locker",
  summary: "",
  icon: "fitness_center",
  fields: [{ key: "locker", field_id: "tpl.locker", label: "Locker", kind: "text", required: true }],
  versions: 1,
}

function renderMenu(onPick: (id: string) => void, users: UserTemplateView[] = []) {
  render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve(users) })}>
      <TemplatesProvider>
        <NewItemMenu onPick={onPick} />
      </TemplatesProvider>
    </ClientProvider>,
  )
}

// Base UI 1.9's Menu.GroupLabel throws outside a group; opening the menu
// is what found it (the web vault's end-to-end run), so open it here.
it("opens with its kinds in groups and picks one by its template id", async () => {
  const onPick = vi.fn()
  renderMenu(onPick)
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  expect(await screen.findByRole("group", { name: "Sign-ins and money" })).toBeInTheDocument()
  expect(screen.getByRole("group", { name: "Identities and documents" })).toBeInTheDocument()
  await userEvent.click(screen.getByRole("menuitem", { name: /Login/ }))
  expect(onPick).toHaveBeenCalledWith("login")
})

it("lists a kind's variants and the person's own templates (D231, D232)", async () => {
  const onPick = vi.fn()
  renderMenu(onPick, [GYM])
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  await userEvent.click(await screen.findByRole("menuitem", { name: /Passport/ }))
  expect(onPick).toHaveBeenLastCalledWith("identity/passport")
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  expect(await screen.findByRole("group", { name: "Your templates" })).toBeInTheDocument()
  await userEvent.click(screen.getByRole("menuitem", { name: /Gym locker/ }))
  expect(onPick).toHaveBeenLastCalledWith(`user:${GYM.id}`)
  // The passkey is never made by hand.
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  expect(await screen.findByRole("menuitem", { name: /Note/ })).toBeInTheDocument()
  expect(screen.queryByRole("menuitem", { name: /Passkey/ })).toBeNull()
  // Changing them opens the manager with the template listed.
  await userEvent.click(await screen.findByRole("menuitem", { name: /Change your templates/ }))
  expect(await screen.findByRole("heading", { name: "Your templates" })).toBeInTheDocument()
  expect(screen.getByText("Gym locker")).toBeInTheDocument()
})
