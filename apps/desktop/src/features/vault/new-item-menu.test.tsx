import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { NewItemMenu } from "./new-item-menu"

// Base UI 1.9's Menu.GroupLabel throws outside a group; opening the menu
// is what found it (the web vault's end-to-end run), so open it here.
it("opens with its kinds under the label and picks one", async () => {
  const onPick = vi.fn()
  render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES) })}>
      <TemplatesProvider>
        <NewItemMenu onPick={onPick} />
      </TemplatesProvider>
    </ClientProvider>,
  )
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  expect(await screen.findByRole("group", { name: "New item" })).toBeInTheDocument()
  await userEvent.click(screen.getByRole("menuitem", { name: /Login/ }))
  expect(onPick).toHaveBeenCalledWith("login")
})
