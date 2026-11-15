import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { fakeDepth } from "@/test/fake-depth"

import { DepthClientProvider, type DepthClient, type SecurityKeyList } from "./depth-client"
import { SecurityKeysSection } from "./security-keys"

const added = Date.UTC(2026, 7, 14, 12) / 1000
const list: SecurityKeyList = {
  keys: [
    {
      id: "k1",
      name: "Blue key",
      alg: "ES256",
      created_at: added,
      last_used: null,
    },
    {
      id: "k2",
      name: "Spare key",
      alg: "EdDSA",
      created_at: added,
      last_used: Math.floor(Date.now() / 1000) - 60,
    },
  ],
  rp_id: "vault.example",
}

const mount = (depth: DepthClient, onNotice = vi.fn()) =>
  render(
    <DepthClientProvider value={depth}>
      <SecurityKeysSection onNotice={onNotice} />
    </DepthClientProvider>,
  )

describe("Security keys (brief T2)", () => {
  it("lists each key with when it was added and last used", async () => {
    mount(fakeDepth({ securityKeys: () => Promise.resolve(list) }))
    // The list is in the page before the keys arrive: wait for its rows, not only for the list.
    const rows = await within(await screen.findByRole("list", { name: "Security keys" })).findAllByRole("listitem")
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent("Blue key")
    expect(rows[0]).toHaveTextContent("not used yet")
    expect(rows[0]).toHaveTextContent("added 2026.08.14")
    expect(rows[1]).toHaveTextContent(/last used (Today|Yesterday) \d{2}:\d{2}/)
    expect(screen.getByText("A new device asks for one of these after your password.")).toBeInTheDocument()
  })

  it("ADD KEY opens the server's page and says to come back", async () => {
    const securityKeyAdd = vi.fn(() => Promise.resolve())
    const onNotice = vi.fn()
    mount(
      fakeDepth({
        securityKeys: () => Promise.resolve({ keys: [], rp_id: "vault.example" }),
        securityKeyAdd,
      }),
      onNotice,
    )
    expect(await screen.findByText("Add a security key or a passkey so a stolen password alone can't sign a new device in.")).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: "Add key" }))
    expect(securityKeyAdd).toHaveBeenCalled()
    expect(onNotice).toHaveBeenCalledWith("Use your key on the page that opened, then come back here")
  })

  it("asks before it removes a key, and Cancel removes nothing", async () => {
    const securityKeyRemove = vi.fn(() => Promise.resolve({ ...list, keys: [list.keys[1]!] }))
    const onNotice = vi.fn()
    mount(
      fakeDepth({
        securityKeys: () => Promise.resolve(list),
        securityKeyRemove,
      }),
      onNotice,
    )
    const user = userEvent.setup()
    const [blue] = within(await screen.findByRole("list", { name: "Security keys" })).getAllByRole("listitem")
    await user.click(within(blue!).getByRole("button", { name: "Remove" }))
    let question = await screen.findByRole("dialog", {
      name: "Remove Blue key?",
    })
    await user.click(within(question).getByRole("button", { name: "Cancel" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(securityKeyRemove).not.toHaveBeenCalled()

    await user.click(
      within(screen.getAllByRole("listitem")[0]!).getByRole("button", {
        name: "Remove",
      }),
    )
    question = await screen.findByRole("dialog", { name: "Remove Blue key?" })
    await user.click(within(question).getByRole("button", { name: "Remove" }))
    await waitFor(() => expect(securityKeyRemove).toHaveBeenCalledWith("k1", {}))
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(1))
    expect(onNotice).toHaveBeenCalledWith("Removed Blue key")
  })
})
