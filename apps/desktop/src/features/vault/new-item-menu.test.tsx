import { cleanup, render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { PlacesProvider, type Nav } from "@/app/places"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider } from "@/core/client"
import type { ItemSummary, UserTemplateView } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { displayTemplate } from "@/app/template-model"
import { TEMPLATES } from "@/test/templates-fixture"

import { NewItemMenu } from "./new-item-menu"

const GYM: UserTemplateView = {
  id: "7d0c1f1e-2f4b-4c62-9c1e-0a6c2b1e5f10",
  vault_id: "v",
  name: "Gym locker",
  summary: "",
  icon: "running",
  fields: [{ key: "locker", field_id: "tpl.locker", label: "Locker", kind: "text", required: true }],
  versions: 1,
}

const row = (id: string, template: string, state = "live"): ItemSummary =>
  ({ id, kind: template.split("/")[0]!, template, state, title: id, site: "", username: "", edited_at: 0, crown_jewel: false, methods: [], passkeys: 0, codes: 0, expires_at: null, conflict_copy: false, warnings: [], views: [], subtitle: "", copy: null }) as ItemSummary

function renderMenu(onPick: (id: string) => void, { users = [], rows, nav = { area: "vault", view: "all" } }: { users?: UserTemplateView[]; rows?: ItemSummary[]; nav?: Nav } = {}) {
  render(
    <ClientProvider value={fakeClient({ templates: () => Promise.resolve(TEMPLATES), userTemplates: () => Promise.resolve(users) })}>
      <PlacesProvider value={{ nav, surface: "desktop", go: () => {}, navigate: () => {} }}>
        <TemplatesProvider>
          <NewItemMenu onPick={onPick} rows={rows} />
        </TemplatesProvider>
      </PlacesProvider>
    </ClientProvider>,
  )
}

/** The menu's own rows, in order, without the groups' submenus. */
const topRows = async () => (await screen.findAllByRole("menuitem")).map((m) => m.textContent?.replace(/\s+/g, " ").trim())

// Base UI 1.9's Menu.GroupLabel throws outside a group; opening the menu
// is what found it (the web vault's end-to-end run), so open it here.
it("opens with five kinds first, then the four groups, and picks one by its template id (brief E1, Q18)", async () => {
  const onPick = vi.fn()
  renderMenu(onPick)
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  // The fixture's kinds: no One-time code template here, so four come first; a group with nothing left is not drawn.
  expect((await topRows()).slice(0, 7)).toEqual(["Login", "Card", "Secure note", "Wi-Fi network", "Sign-ins and money", "Identities and documents", "Other"])
  // A passkey is made by a site, never by hand: not first, and in no group.
  expect(screen.queryByRole("menuitem", { name: /Passkey/ })).toBeNull()
  await userEvent.click(screen.getByRole("menuitem", { name: /^Login/ }))
  expect(onPick).toHaveBeenCalledWith("login")
})

it("counts each kind's items in two digits, said as the plain number", async () => {
  renderMenu(vi.fn(), { rows: [row("a", "login"), row("b", "login"), row("c", "login", "trashed"), row("d", "wi-fi")] })
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  const login = await screen.findByRole("menuitem", { name: /^Login/ })
  expect(login).toHaveTextContent("02")
  expect(login).toHaveAccessibleName(/Login, 2$/)
  expect(screen.getByRole("menuitem", { name: /^Wi-Fi network/ })).toHaveTextContent("01")
  expect(screen.getByRole("menuitem", { name: /^Secure note/ })).not.toHaveTextContent(/\d/)
})

it("puts a kind first in its own view, and takes it out of its group", async () => {
  renderMenu(vi.fn(), { nav: { area: "vault", view: "crypto-wallet" } })
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  expect((await topRows()).slice(0, 5)).toEqual(["Crypto wallet", "Login", "Card", "Secure note", "Wi-Fi network"])
  // Its group had only it: no group is left to draw.
  expect(screen.queryByRole("menuitem", { name: "Sign-ins and money" })).toBeNull()
})

it("lists a kind's variants in its group and the person's own templates (D231, D232)", async () => {
  const onPick = vi.fn()
  renderMenu(onPick, { users: [GYM] })
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  // Each group is a submenu on a desktop: its rows a group of the same name.
  await userEvent.click(await screen.findByRole("menuitem", { name: "Identities and documents" }))
  const ids = await screen.findByRole("group", { name: "Identities and documents" })
  await userEvent.click(within(ids).getByRole("menuitem", { name: /^Passport/ }))
  expect(onPick).toHaveBeenLastCalledWith("identity/passport")
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  const mine = await screen.findByRole("group", { name: "Your templates 001" })
  await userEvent.click(within(mine).getByRole("menuitem", { name: /Gym locker/ }))
  expect(onPick).toHaveBeenLastCalledWith(`user:${GYM.id}`)
  // "Your templates" at the foot opens the manager with the template listed.
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  await userEvent.click(await screen.findByRole("menuitem", { name: "Your templates" }))
  expect(await screen.findByRole("heading", { name: "Your templates" })).toBeInTheDocument()
  expect(screen.getByText("Gym locker")).toBeInTheDocument()
})

it("every creatable template is in the menu once, variants included; no Passkey", async () => {
  renderMenu(vi.fn())
  await userEvent.click(screen.getByRole("button", { name: "New item" }))
  const names: string[] = (await topRows()).slice(0, 4) as string[]
  for (const group of ["Sign-ins and money", "Identities and documents", "Networks and machines", "Other"]) {
    // A fresh menu for each group, so one submenu is open at a time.
    cleanup()
    renderMenu(vi.fn())
    await userEvent.click(screen.getByRole("button", { name: "New item" }))
    const trigger = (await screen.findAllByRole("menuitem")).find((m) => m.textContent?.trim() === group)
    if (!trigger) continue
    await userEvent.click(trigger)
    const rows = within(await screen.findByRole("group", { name: group })).getAllByRole("menuitem")
    names.push(...rows.map((r) => r.textContent?.trim() ?? ""))
  }
  const creatable = TEMPLATES.kinds.filter((t) => t.creatable).map((t) => displayTemplate(t).name)
  expect([...names].sort()).toEqual([...creatable].sort())
  expect(names).not.toContain("Passkey")
})
