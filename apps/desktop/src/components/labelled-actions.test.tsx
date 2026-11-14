import { act, render, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { CrownJewelGate } from "@unicron/ui/security/crown-jewel"

import { GateProvider, useGate } from "@/app/gate"
import { ClientProvider } from "@/core/client"
import type { DeviceInfo, Status } from "@/core/types"
import { DeviceApproval } from "@/features/approvals/approval-sheet"
import { BrowserClientProvider, type BrowserClient, type PendingPairing } from "@/features/browsers/browser-client"
import { PairingDialog } from "@/features/browsers/pairing-dialog"
import { DevicesView } from "@/features/devices/devices-view"
import { PasskeyApprovalDialog } from "@/features/passkeys/passkey-approval-dialog"
import { PasskeyClientProvider, type PasskeyClient } from "@/features/passkeys/passkey-client"
import { fakeClient } from "@/test/fake-client"

import sheet from "../styles/dialog-actions.css?raw"

/*
 * Design review R1: approve and deny keep their words at every width. jsdom
 * lays nothing out and reads no media queries, so "narrow" here is the window
 * at 390 px plus the rules unicron's classes resolve to below `sm`, written
 * out by hand; each action is then found by its accessible name and must show
 * the same words, with nothing in it hidden and no icon standing in for them.
 */

const PHONE = 390

beforeEach(() => {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: PHONE })
  window.dispatchEvent(new Event("resize"))
})

/** The action named `words` shows `words`: no screen-reader-only part, no icon instead. */
function expectWords(words: string, scope: HTMLElement = document.body) {
  const action = within(scope).getByRole("button", { name: words })
  expect(action).toHaveTextContent(words, { normalizeWhitespace: true })
  for (const el of [action, ...action.querySelectorAll("*")]) expect(el.getAttribute("class") ?? "").not.toMatch(/sr-only|rounded-full|size-10/)
  expect(action.querySelector("svg[data-slot=icon]")).toBeNull()
  return action
}

/** Below `sm` the row stacks full width in source order, so the primary is the last child. */
function expectStackedPrimaryLast(primary: HTMLElement) {
  const row = primary.parentElement!
  expect(row).toHaveAttribute("data-slot", "labelled-actions")
  expect(row.className).toMatch(/(^| )flex-col( |$)/)
  expect(row.className).toContain("max-sm:*:w-full")
  expect(row.lastElementChild).toBe(primary)
}

const pairing: PendingPairing = { request_id: "a1", code: "653540", name: "Chrome on Linux", family: "chromium", extension_id: "x", expires_in_secs: 110 }

describe("dialog actions at 390 px (R1)", () => {
  it("pairing a browser: The codes differ, then Pair", async () => {
    const client: BrowserClient = {
      status: () => Promise.resolve({ listening: true, problem: null, host_binary: "", browsers: [] }),
      pending: () => Promise.resolve([pairing]),
      answer: () => Promise.resolve(),
      paired: () => Promise.resolve([]),
      unpair: () => Promise.resolve(),
      install: () => Promise.resolve([]),
    }
    render(
      <BrowserClientProvider value={client}>
        <PairingDialog onNotice={() => {}} />
      </BrowserClientProvider>,
    )
    const dialog = await screen.findByRole("dialog")
    expectWords("The codes differ", dialog)
    expectStackedPrimaryLast(expectWords("Pair", dialog))
  })

  it("a passkey request: Decline, then Approve", async () => {
    const client = {
      pending: () => Promise.resolve([{ request_id: "p1", text: "Sign in to shop.example as sam", browser: "Firefox", expires_in_secs: 110 }]),
      answer: () => Promise.resolve(),
    } as unknown as PasskeyClient
    render(
      <PasskeyClientProvider value={client}>
        <PasskeyApprovalDialog onNotice={() => {}} />
      </PasskeyClientProvider>,
    )
    const dialog = await screen.findByRole("dialog")
    expectWords("Decline", dialog)
    expectStackedPrimaryLast(expectWords("Approve", dialog))
  })

  it("revoking a device: Cancel, then Revoke", async () => {
    const devices: DeviceInfo[] = [
      { id: "d-mine", name: "This laptop", platform: "linux", enrolled_at: 1, revoked_at: null, this_device: true },
      { id: "d-phone", name: "Old phone", platform: "android", enrolled_at: 1, revoked_at: null, this_device: false },
    ]
    const status = { devices: 2, devices_revoked: 0, account: null } as unknown as Status
    render(
      <ClientProvider value={fakeClient({ deviceList: () => Promise.resolve(devices) })}>
        <DevicesView status={status} onChanged={() => {}} onNotice={() => {}} />
      </ClientProvider>,
    )
    await userEvent.setup().click(await screen.findByRole("button", { name: "Revoke" }))
    const dialog = await screen.findByRole("dialog")
    expectWords("Cancel", dialog)
    expectStackedPrimaryLast(expectWords("Revoke", dialog))
  })

  it("the plain confirm on a computer with no built-in check: Cancel, then Confirm", async () => {
    // The core asks for the window's confirm the first time, then takes the call.
    const call = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce({ code: "approval_needed", message: "Revoke the old phone.", detail: "window" })
      .mockResolvedValue(undefined)
    function Asker() {
      const gate = useGate()
      return <button onClick={() => void gate.run(call).catch(() => {})}>Ask</button>
    }
    render(
      <ClientProvider value={fakeClient({ approvalRoute: () => Promise.resolve({ route: "window", method: "window" }) })}>
        <GateProvider>
          <Asker />
        </GateProvider>
      </ClientProvider>,
    )
    await userEvent.setup().click(screen.getByRole("button", { name: "Ask" }))
    const dialog = await screen.findByRole("dialog")
    expectWords("Cancel", dialog)
    expectStackedPrimaryLast(expectWords("Confirm", dialog))
  })
})

/*
 * Two unicron components draw their own actions: the crown-jewel gate and
 * the approval sheet. Before unicron 1.0 they drew round icon buttons below
 * `sm`, and styles/dialog-actions.css gave them their words back. From 1.0
 * both keep their words at every width by themselves (docs/plan/62-unicron-1-0.md
 * §4), so these tests hold unicron to it: each action, found by its
 * accessible name at 390 px, shows the same words as text in the button,
 * nothing between the words and the button hides them, and the button is
 * not the old round icon button. An icon may stand beside the words, never
 * in their place. The old sheet is still applied, with the rule unicron's
 * `max-sm:sr-only` resolved to, so a release that hid the words again would
 * fail here by its class and by the style it computes to.
 */
const BELOW_SM = `
.max-sm\\:sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
`

function applySheets() {
  const style = document.createElement("style")
  // The sheet's @media block holds only layout; jsdom reads no media queries.
  style.textContent = BELOW_SM + sheet.replace(/@media[\s\S]*$/, "")
  document.head.append(style)
  return () => style.remove()
}

/** A class that hides the element itself, always or by the window's width (`max-sm:sr-only`); a class for a child or a state is not one. */
const HIDES = /^(?:(?:max-)?(?:sm|md|lg|xl|2xl):)*(?:sr-only|hidden|invisible)$/

/** The element whose own text holds `words`: the action itself, or one inside it. */
function wordsHolder(action: HTMLElement, words: string): Element | null {
  const walker = document.createTreeWalker(action, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) if (node.textContent?.includes(words)) return node.parentElement
  return null
}

function expectComputedWords(words: string, scope: HTMLElement) {
  const action = within(scope).getByRole("button", { name: new RegExp(`^${words}`) })
  const holder = wordsHolder(action, words)
  expect(holder, `${words}: the words are text in the button`).not.toBeNull()
  // From the words up to the button, nothing takes them out of sight.
  for (let el: Element | null = holder; el; el = el === action ? null : el.parentElement) {
    expect((el.getAttribute("class") ?? "").split(/\s+/).filter((c) => HIDES.test(c)), words).toEqual([])
    const css = getComputedStyle(el)
    expect(css.position, words).not.toBe("absolute")
    expect(["", "none"], words).toContain(css.clipPath)
    expect(css.display, words).not.toBe("none")
  }
  expect(action.getAttribute("class") ?? "", words).not.toMatch(/rounded-full|size-10/)
}

describe("unicron's own dialog actions at 390 px (R1)", () => {
  let remove: () => void
  beforeEach(() => {
    remove = applySheets()
  })
  afterEach(() => remove())

  it("the crown-jewel gate keeps Cancel and Confirm", async () => {
    render(<CrownJewelGate open level="password" reason="Show the recovery phrase." onConfirm={() => {}} />)
    const dialog = await screen.findByRole("dialog")
    expectComputedWords("Cancel", dialog)
    expectComputedWords("Confirm", dialog)
  })

  it("the approval sheet keeps Approve, Deny and This wasn't me", async () => {
    render(
      <DeviceApproval
        request={{ id: "r1", kind: "device", origin: "pairing", context: [{ label: "Who", value: "Laptop" }], expected: "42", digits: 2 }}
        onApprove={() => {}}
        onDeny={() => {}}
        onReport={() => {}}
      />,
    )
    const dialog = await screen.findByRole("dialog")
    const user = userEvent.setup()
    const cells = within(dialog).getAllByRole("textbox", { name: /Digit \d of 2/ })
    await user.type(cells[0]!, "4")
    await user.type(cells[1]!, "2")
    await act(() => new Promise((r) => setTimeout(r, 600)))
    expectComputedWords("Approve", dialog)
    expectComputedWords("Deny", dialog)
    expectComputedWords("This wasn't me", dialog)
  })
})
