import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider } from "@/core/client"
import { fakeClient } from "@/test/fake-client"

import { fitted, greyFrom } from "./frames"
import { ScanClientProvider, type ScanClient, type ScanOutcome } from "./scan-client"
import { ScanDialog } from "./scan-dialog"

const APPROVE: ScanOutcome = {
  found: true,
  kind: "device_code",
  decision: "approve",
  service: "Netflix",
  host: "www.netflix.com",
  code: "1234-5678",
  code_in_url: false,
  activation_id: "act-1",
  accounts: [
    { item_id: "n1", title: "Netflix", username: "family@example.com" },
    { item_id: "n2", title: "Netflix (work)", username: "me@example.com" },
  ],
}

function scanClient(over: Partial<ScanClient> = {}): ScanClient {
  return {
    frame: () => Promise.resolve(APPROVE),
    text: () => Promise.resolve(APPROVE),
    approve: () => Promise.resolve({ url: "https://www.netflix.com/tv8", host: "www.netflix.com", service: "Netflix", code: "1234-5678", code_in_url: false }),
    cancel: () => Promise.resolve(),
    contactFrame: () => Promise.resolve(null),
    ...over,
  }
}

function Harness({ scan, onNotice = () => {} }: { scan: ScanClient; onNotice?: (t: string) => void }) {
  return (
    <ClientProvider value={fakeClient({})}>
      <GateProvider>
        <ScanClientProvider value={scan}>
          <ScanDialog open onOpenChange={() => {}} onNotice={onNotice} />
        </ScanClientProvider>
      </GateProvider>
    </ClientProvider>
  )
}

async function paste(text: string) {
  const user = userEvent.setup()
  await user.click(screen.getByRole("tab", { name: "Paste" }))
  await user.type(screen.getByLabelText("The code's address"), text)
  await user.click(screen.getByRole("button", { name: "Check it" }))
  return user
}

describe("frames for the core", () => {
  it("are grey, row by row, and never wider than the bridge takes", () => {
    const rgba = new Uint8ClampedArray([255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255])
    const f = greyFrom(rgba, 3, 1)
    expect(Array.from(f.luma)).toEqual([255, 0, 76])
    expect(fitted(1920, 1080)).toEqual({ width: 640, height: 360 })
    expect(fitted(100, 50)).toEqual({ width: 100, height: 50 })
  })
})

describe("signing in a TV (docs/plan/26-desktop-finish.md §3)", () => {
  it("shows a paste's refusal in the core's words, with no way to approve", async () => {
    const refused: ScanOutcome = {
      ...APPROVE,
      decision: "refused",
      reason: "This code didn't come from a screen in front of you.",
      activation_id: undefined,
      accounts: [],
    }
    const text = vi.fn<ScanClient["text"]>(() => Promise.resolve(refused))
    render(<Harness scan={scanClient({ text })} />)
    await paste("https://www.netflix.com/tv8/1234-5678")
    expect(await screen.findByRole("alert")).toHaveTextContent("didn't come from a screen in front of you")
    expect(text).toHaveBeenCalledWith("https://www.netflix.com/tv8/1234-5678")
    expect(screen.getByText("Refused")).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /Sign in on/ })).not.toBeInTheDocument()
  })

  it("names the full domain, takes the login picked, and shows the code the page won't take", async () => {
    const approve = vi.fn<ScanClient["approve"]>(() => Promise.resolve({ url: "https://www.netflix.com/tv8", host: "www.netflix.com", service: "Netflix", code: "1234-5678", code_in_url: false }))
    const onNotice = vi.fn()
    render(<Harness scan={scanClient({ approve })} onNotice={onNotice} />)
    const user = await paste("anything")
    expect(await screen.findByText("www.netflix.com")).toBeInTheDocument()
    await user.click(screen.getByRole("radio", { name: /Netflix \(work\)/ }))
    await user.click(screen.getByRole("button", { name: "Sign in on www.netflix.com" }))
    await waitFor(() => expect(approve).toHaveBeenCalledWith("act-1", "n2", {}))
    expect(await screen.findByLabelText("Code 1 2 3 4 - 5 6 7 8")).toBeInTheDocument()
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("offers Netflix (work) first"))
  })

  it("closing before the yes ends the activation in the core", async () => {
    const cancel = vi.fn<ScanClient["cancel"]>(() => Promise.resolve())
    render(<Harness scan={scanClient({ cancel })} />)
    const user = await paste("anything")
    await user.click(await screen.findByRole("button", { name: "Don't sign in" }))
    expect(cancel).toHaveBeenCalledWith("act-1")
  })
})
