import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { ClientProvider } from "@/core/client"
import { fakeClient } from "@/test/fake-client"

import * as frames from "./frames"
import { fitted, greyFrom } from "./frames"
import { ScanClientProvider, type ScanClient, type ScanOutcome } from "./scan-client"
import { ScanDialog } from "./scan-dialog"

/** A camera that "sees" a code: the tests' stand-in for the webview's camera (jsdom has none). */
const camera = vi.hoisted(() => ({ on: false }))
vi.mock("./camera-view", async (real) => {
  const actual = await real<typeof import("./camera-view")>()
  return {
    ...actual,
    CameraView: (props: Parameters<typeof actual.CameraView>[0]) =>
      camera.on ? (
        <button type="button" onClick={() => void props.onFrame({ width: 1, height: 1, luma: new Uint8Array(1) })}>
          Show the TV's code
        </button>
      ) : (
        <actual.CameraView {...props} />
      ),
  }
})

afterEach(() => {
  camera.on = false
})

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

/** Points the camera at the TV: the core reads the frame. */
async function point() {
  camera.on = true
  return userEvent.setup()
}

async function scanned(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Show the TV's code" }))
}

/** The sheet takes no answer for its first half second (the arming delay). */
async function armed(sheet: HTMLElement) {
  await waitFor(() => expect(sheet.querySelector('[data-armed="true"]')).toBeInTheDocument())
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
  it("keeps the camera's error on the Camera tab; Picture and Paste don't show it (design review R6)", async () => {
    render(<Harness scan={scanClient()} />)
    // jsdom has no camera, so the Camera tab says so.
    const camera = await screen.findByRole("tabpanel")
    expect(within(camera).getByRole("alert")).toHaveTextContent("This window can't use a camera.")
    const user = userEvent.setup()
    for (const name of ["Picture", "Paste"]) {
      await user.click(screen.getByRole("tab", { name }))
      expect(screen.queryByText(/can't use a camera/)).not.toBeInTheDocument()
      expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    }
    await user.click(screen.getByRole("tab", { name: "Camera" }))
    expect(await within(screen.getByRole("tabpanel")).findByRole("alert")).toHaveTextContent("This window can't use a camera.")
  })

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
    const frame = vi.fn<ScanClient["frame"]>(() => Promise.resolve(APPROVE))
    const onNotice = vi.fn()
    const user = await point()
    render(<Harness scan={scanClient({ approve, frame })} onNotice={onNotice} />)
    await scanned(user)
    expect(frame).toHaveBeenCalledWith(expect.anything(), "camera")
    const sheet = await screen.findByRole("dialog", { name: "A TV or console sign-in" })
    expect(within(sheet).getByText("www.netflix.com")).toBeInTheDocument()
    // The TV's code in groups, read one character at a time.
    expect(within(sheet).getByText("TV's code: 1 2 3 4, 5 6 7 8")).toBeInTheDocument()
    await user.click(within(sheet).getByRole("radio", { name: /Netflix \(work\)/ }))
    const yes = within(sheet).getByRole("button", { name: /^Sign in on www\.netflix\.com/ })
    await waitFor(() => expect(yes).toBeEnabled())
    await user.click(yes)
    await waitFor(() => expect(approve).toHaveBeenCalledWith("act-1", "n2", {}))
    expect(await screen.findByText("Code: 1 2 3 4, 5 6 7 8")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Done" })).toBeInTheDocument()
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("offers Netflix (work) first"))
  })

  it("closing before the yes ends the activation in the core", async () => {
    const cancel = vi.fn<ScanClient["cancel"]>(() => Promise.resolve())
    const user = await point()
    render(<Harness scan={scanClient({ cancel })} />)
    await scanned(user)
    const sheet = await screen.findByRole("dialog", { name: "A TV or console sign-in" })
    await armed(sheet)
    const no = within(sheet).getByRole("button", { name: "Don't sign in" })
    await user.click(no)
    await waitFor(() => expect(cancel).toHaveBeenCalledWith("act-1"))
  })

  it("a paste renders no approve control, even when the core would say yes (ApprovalSheet's origin, D99, D310)", async () => {
    render(<Harness scan={scanClient()} />)
    await paste("anything")
    const sheet = await screen.findByRole("dialog", { name: "A TV or console sign-in" })
    expect(sheet.querySelector('[data-origin="push"]')).toBeInTheDocument()
    expect(within(sheet).queryByRole("button", { name: /Sign in on/ })).not.toBeInTheDocument()
    expect(within(sheet).queryByRole("radio")).not.toBeInTheDocument()
    expect(within(sheet).getByText(/only when you scan the screen in front of you/)).toBeInTheDocument()
    // Its no is the way back to the three ways in.
    const again = within(sheet).getByRole("button", { name: "Scan again" })
    await armed(sheet)
    await userEvent.setup().click(again)
    expect(await screen.findByRole("dialog", { name: "Sign in a TV or console" })).toBeInTheDocument()
  })

  it("a picture renders no approve control either", async () => {
    vi.spyOn(frames, "frameOfFile").mockResolvedValue({ width: 1, height: 1, luma: new Uint8Array(1) })
    const frame = vi.fn<ScanClient["frame"]>(() => Promise.resolve(APPROVE))
    render(<Harness scan={scanClient({ frame })} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole("tab", { name: "Picture" }))
    expect(screen.getByRole("button", { name: "Choose picture" })).toBeInTheDocument()
    await user.upload(screen.getByLabelText("A picture of the code"), new File([new Uint8Array([1])], "tv.png", { type: "image/png" }))
    await waitFor(() => expect(frame).toHaveBeenCalledWith(expect.anything(), "image"))
    const sheet = await screen.findByRole("dialog", { name: "A TV or console sign-in" })
    expect(within(sheet).queryByRole("button", { name: /Sign in on/ })).not.toBeInTheDocument()
    vi.restoreAllMocks()
  })
})
