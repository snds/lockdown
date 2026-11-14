import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { BridgesProvider, type BridgesClient } from "@/features/bridges/bridges-client"
import { fakeBridges } from "@/features/bridges/fake-bridges"

function mount(ui: ReactNode, bridges: BridgesClient | null, over: Partial<Client> = {}) {
  const client = fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    approvalRoute: () => Promise.resolve({ route: "window", method: "this window" }),
    ...over,
  })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <BridgesProvider value={bridges}>{ui}</BridgesProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("the export dialog's age choice", () => {
  it("locks the export to the public keys pasted, through the gate", async () => {
    const { ExportDialog } = await import("@/features/export/export-dialog")
    const exportChoose = vi.fn(() => Promise.resolve({ token: 9, path: "/tmp/out", name: "out" }))
    const exportSaveAge = vi.fn((_input: { target: number; recipients: string[] }, gate?: GateInput) =>
      gate?.confirmed
        ? Promise.resolve({
            path: "/tmp/out.lockdown.age",
            bytes: 10,
            preview: { format: "lockdown" as const, items: 1, attachments: 0, crown_jewels_included: 0, crown_jewels_left_out: 0, left_out: [], dropped: [], encrypted: true },
          })
        : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to export the vault" }),
    )
    mount(<ExportDialog open onClose={() => {}} onNotice={vi.fn()} />, fakeBridges({ exportSaveAge }), {
      exportPreview: () =>
        Promise.resolve({ format: "lockdown", items: 1, attachments: 0, crown_jewels_included: 0, crown_jewels_left_out: 0, left_out: [], dropped: [], encrypted: false }),
      exportChoose,
    })
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Kind of file"), "lockdown-age")
    await user.click(await screen.findByRole("button", { name: "Choose where to save…" }))
    expect(await screen.findByText(/Paste the age public key/)).toBeInTheDocument()
    await user.type(screen.getByLabelText("The public keys it opens with"), "age1aaa\nage1bbb")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await user.click(await screen.findByRole("button", { name: /confirm/i }))
    await waitFor(() => expect(exportSaveAge).toHaveBeenLastCalledWith({ target: 9, recipients: ["age1aaa", "age1bbb"], vault: null }, expect.objectContaining({ confirmed: true })))
    expect(exportChoose).toHaveBeenCalledWith("age", true)
  })
})
