import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, GateInput } from "@/core/types"

import { ItemCrownJewel } from "./crown-jewel"
import { ItemField } from "./item-field"

const field: FieldView = { field_id: "phrase", label: "Recovery phrase", kind: "hidden", hidden: true, value: null, mask: "••••••••" }
const SECRET = "abandon ability able"

function fakeClient(overrides: Partial<Client>): Client {
  const refuse = () => Promise.reject({ code: "refused", message: "not in this test" })
  const base = new Proxy({} as Client, { get: () => refuse })
  return Object.assign(Object.create(base), overrides)
}

/** This computer: a platform check for `use`, the account password for `reveal` (D101). */
function crownJewelClient(over: Partial<Client> = {}) {
  const reveal = vi.fn((_id: string, _f: string, gate?: GateInput) =>
    gate?.password === "correct horse"
      ? Promise.resolve(new TextEncoder().encode(SECRET))
      : gate?.password !== undefined
        ? Promise.reject({ code: "refused", message: "the password didn't match" })
        : Promise.reject({ code: "approval_needed", detail: "password", message: "your account password is needed to reveal this protected value" }),
  )
  const copySecret = vi.fn(() => Promise.resolve(30))
  const client = fakeClient({
    approvalRoute: (kind) => Promise.resolve(kind === "reveal" ? { route: "password", method: "password" } : { route: "platform", method: "Touch ID" }),
    reveal,
    copySecret,
    ...over,
  })
  return { client, reveal, copySecret }
}

function mount(client: Client) {
  return render(
    <ClientProvider value={client}>
      <GateProvider>
        <ItemCrownJewel title="Ledger">
          <ItemField itemId="i1" field={field} onNotice={() => {}} />
        </ItemCrownJewel>
      </GateProvider>
    </ClientProvider>,
  )
}

const confirmButton = () => screen.getByRole("button", { name: "Confirm" })

describe("the crown-jewel treatment on this host (D98, D101)", () => {
  it("draws the surface in the app and reveals after the password gate, judged by the core", async () => {
    const { client, reveal } = crownJewelClient()
    const { container } = mount(client)
    const user = userEvent.setup()
    expect(screen.getByRole("region", { name: "Crown jewel: Ledger" })).toBeInTheDocument()
    expect(screen.getByText("Crown jewel")).toBeInTheDocument()

    // Inside the surface the reveal is a tap, whatever the setting says.
    await user.click(await screen.findByRole("button", { name: "Show Recovery phrase" }))
    const dialog = await screen.findByRole("dialog")
    expect(dialog).toHaveTextContent("Confirm it's you")
    expect(dialog).toHaveTextContent("To show Ledger")
    await user.type(screen.getByLabelText("Account password"), "correct horse")
    await waitFor(() => expect(confirmButton()).toBeEnabled())
    await user.click(confirmButton())

    await waitFor(() => expect(container.textContent).toContain(SECRET))
    expect(reveal).toHaveBeenCalledWith("i1", "phrase", { password: "correct horse" })
    expect(reveal).toHaveBeenCalledTimes(1)
  })

  it("asks again with the tries left when the core says the password didn't match", async () => {
    const { client, reveal } = crownJewelClient()
    const { container } = mount(client)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Show Recovery phrase" }))
    await screen.findByRole("dialog")
    await user.type(screen.getByLabelText("Account password"), "hunter2")
    await waitFor(() => expect(confirmButton()).toBeEnabled())
    await user.click(confirmButton())

    const again = await screen.findByText(/Password incorrect\. 2 tries left\./)
    expect(again).toBeInTheDocument()
    await user.type(screen.getByLabelText("Account password"), "correct horse")
    await waitFor(() => expect(confirmButton()).toBeEnabled())
    await user.click(confirmButton())
    await waitFor(() => expect(container.textContent).toContain(SECRET))
    expect(reveal).toHaveBeenCalledTimes(2)
  })

  it("copies by reference after the platform's own check: nothing is fetched for the webview", async () => {
    const { client, reveal, copySecret } = crownJewelClient()
    mount(client)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Copy Recovery phrase" }))
    await waitFor(() => expect(copySecret).toHaveBeenCalledWith("i1", "phrase", 30, {}))
    expect(reveal).not.toHaveBeenCalled()
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })
})
