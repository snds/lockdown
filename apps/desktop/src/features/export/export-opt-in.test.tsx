import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput, ItemDetail } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ExportOptIn } from "./export-opt-in"

const wallet: ItemDetail = { id: "w1", kind: "crypto-wallet", state: "live", version: 1, edited_at: 1, crown_jewel: true, export_opt_in: false, fields: [] }

describe("a crown jewel's export opt-in (D121)", () => {
  it("turning it on goes through the gate and the item says so after", async () => {
    let on = false
    const setExportOptIn = vi.fn<Client["setExportOptIn"]>((_id, next, gate?: GateInput) => {
      if (next && !gate?.confirmed) return Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window" })
      on = next
      return Promise.resolve(2)
    })
    const client = fakeClient({
      setExportOptIn,
      show: () => Promise.resolve({ ...wallet, export_opt_in: on }),
      approvalRoute: () => Promise.resolve({ route: "window", method: "window" }),
    })
    const changed = vi.fn()
    render(
      <ClientProvider value={client}>
        <GateProvider>
          <ExportOptIn item={wallet} onChanged={changed} onNotice={() => {}} />
        </GateProvider>
      </ClientProvider>,
    )
    expect(screen.getByText(/stay out of exports and backups unless you choose/)).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("switch", { name: "Include in exports and backups" }))
    await user.click(await screen.findByRole("button", { name: "Confirm" }))
    await waitFor(() => expect(changed).toHaveBeenCalledWith(expect.objectContaining({ export_opt_in: true })))
    expect(setExportOptIn).toHaveBeenLastCalledWith("w1", true, { confirmed: true })
  })
})
