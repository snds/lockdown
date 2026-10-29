import { render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { ClientProvider } from "@/core/client"
import type { Status } from "@/core/types"
import { malformedLists } from "@/test/a11y"
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

  it("draws one page heading and its rows as a list of items (design review R7, R18)", () => {
    const { container } = render(
      <ClientProvider value={fakeClient({})}>
        <RecoveryView status={status} onChanged={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(screen.getAllByRole("heading", { level: 1 }).map((h) => h.textContent)).toEqual(["Recovery"])
    const list = screen.getByText("Trusted contacts").closest("[role=list]") as HTMLElement
    expect(within(list).getAllByRole("listitem")).toHaveLength(3)
    expect(malformedLists(container)).toEqual([])
  })
})
