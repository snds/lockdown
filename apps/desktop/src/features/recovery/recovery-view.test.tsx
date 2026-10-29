import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider } from "@/core/client"
import type { Status } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { RecoveryView } from "./recovery-view"

const status = { recovery: "made", account: null } as unknown as Status

describe("the Recovery screen", () => {
  it("points trusted contacts at emergency access, which is built, and opens it (design review R5)", async () => {
    const onEmergency = vi.fn()
    render(
      <ClientProvider value={fakeClient({})}>
        <RecoveryView status={status} onChanged={() => {}} onNotice={() => {}} onEmergency={onEmergency} />
      </ClientProvider>,
    )
    const row = screen.getByText("Trusted contacts").closest("[data-slot=item]") as HTMLElement
    expect(row).toHaveTextContent("Set it up in Sharing → Emergency access.")
    expect(row).not.toHaveTextContent("later")
    await userEvent.setup().click(within(row).getByRole("button", { name: "Open Emergency access" }))
    expect(onEmergency).toHaveBeenCalledTimes(1)
  })
})
