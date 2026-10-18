import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { FieldView, ItemDetail } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"

import { DetailPane } from "./detail-pane"
import { draftToFields, keyFor, linkText, parseLink, qrPath } from "./depth-model"

const plain = (field_id: string, label: string, value: string, kind = "text"): FieldView => ({ field_id, label, kind, hidden: false, value, mask: null })
const hidden = (field_id: string, label: string, kind = "hidden"): FieldView => ({ field_id, label, kind, hidden: true, value: null, mask: "••••••••" })
const item = (kind: string, fields: FieldView[], over: Partial<ItemDetail> = {}): ItemDetail => ({
  id: "i1",
  kind,
  state: "live",
  version: 1,
  edited_at: 0,
  crown_jewel: false,
  fields: [plain("title", "title", "Thing"), ...fields],
  ...over,
})

function mount(client: Client, locked = false, onChanged = vi.fn()) {
  const tree = (l: boolean) => (
    <ClientProvider value={client}>
      <LockedProvider value={l}>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={onChanged} onNotice={() => {}} />
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </LockedProvider>
    </ClientProvider>
  )
  const r = render(tree(locked))
  return { ...r, relock: (l: boolean) => r.rerender(tree(l)), onChanged }
}

const base = { templates: () => Promise.resolve(TEMPLATES), approvalRoute: () => Promise.resolve({ route: "window" as const, method: "window" }) }

/** A 21-module grid with the three finder corners dark, as the core's `wifi_qr` sends it. */
function grid(): Uint8Array {
  const w = 21
  const g = new Uint8Array(1 + w * w)
  g[0] = w
  for (const [x0, y0] of [
    [0, 0],
    [14, 0],
    [0, 14],
  ] as const)
    for (let i = 0; i < 7; i += 1) {
      g[1 + y0 * w + x0 + i] = 1
      g[1 + (y0 + i) * w + x0] = 1
    }
  return g
}

describe("vault depth in the detail (docs/plan/21-vault-depth.md)", () => {
  it("offers large type on a login's password too", async () => {
    mount(fakeClient({ ...base, show: () => Promise.resolve(item("login", [plain("user", "username", "me"), hidden("pw", "password")])) }))
    const methods = await screen.findByRole("region", { name: "Sign-in methods" })
    expect(within(methods).getByRole("button", { name: "Show large type" })).toBeInTheDocument()
  })

  it("draws a variant from its own template (D231)", async () => {
    mount(fakeClient({ ...base, show: () => Promise.resolve(item("identity", [hidden("passport.number", "n"), plain("passport.country", "c", "GB")], { template: "identity/passport" })) }))
    const section = await screen.findByRole("region", { name: "Passport" })
    expect(within(section).getByText("Issuing country")).toBeInTheDocument()
    expect(within(section).getAllByText("Passport number").length).toBeGreaterThan(0)
  })

  it("a link names its target and reveals through it; a gone one says so (D235)", async () => {
    const reveal = vi.fn<Client["reveal"]>(() => Promise.resolve(new TextEncoder().encode("k7Hq3mWx9rTa4nPe6uYc")))
    const linked: FieldView = { ...plain("custom.0", "Guest password", "w1#wifi.passphrase", "linked"), link: { item_id: "w1", field_id: "wifi.passphrase", title: "Home Wi-Fi", label: "Password", secret: true, code: false, missing: false } }
    const gone: FieldView = { ...plain("custom.1", "Old", "w2#pw", "linked"), link: { item_id: "w2", field_id: "pw", title: "", label: "", secret: false, code: false, missing: true } }
    mount(fakeClient({ ...base, reveal, show: () => Promise.resolve(item("custom", [linked, gone])) }))
    expect((await screen.findAllByText(/Guest password · Password from Home Wi-Fi/)).length).toBeGreaterThan(0)
    expect(screen.getByText(/Points at an item or field that is gone/)).toBeInTheDocument()
    expect(screen.queryByText("w1#wifi.passphrase")).toBeNull()
  })

  it("shows the Wi-Fi guest card through the gate and hides it on lock (D239)", async () => {
    let held: Uint8Array | null = null
    const wifiQr = vi.fn<Client["wifiQr"]>(() => {
      held = grid()
      return Promise.resolve(held)
    })
    const { relock } = mount(fakeClient({ ...base, wifiQr, show: () => Promise.resolve(item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password")])) }))
    await userEvent.click(await screen.findByRole("button", { name: "Guest card" }))
    expect(await screen.findByRole("img", { name: "The code to join home-net" })).toBeInTheDocument()
    expect(wifiQr.mock.calls[0]![0]).toBe("i1")
    expect(screen.getByText(/hides itself in 60 seconds/)).toBeInTheDocument()
    relock(true)
    await waitFor(() => expect(screen.queryByRole("img", { name: "The code to join home-net" })).toBeNull())
    expect(Array.from(held!).every((b) => b === 0)).toBe(true)
  })

  it("prints the guest card with the password, revealed once for the print", async () => {
    const reveal = vi.fn<Client["reveal"]>(() => Promise.resolve(new TextEncoder().encode("k7Hq3mWx")))
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    mount(fakeClient({ ...base, reveal, wifiQr: () => Promise.resolve(grid()), show: () => Promise.resolve(item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password")])) }))
    await userEvent.click(await screen.findByRole("button", { name: "Guest card" }))
    await userEvent.click(await screen.findByRole("button", { name: "Print a card" }))
    await waitFor(() => expect(print).toHaveBeenCalled())
    expect(reveal.mock.calls[0]!.slice(0, 2)).toEqual(["i1", "wifi.passphrase"])
    await waitFor(() => expect(screen.queryByText("k7Hq3mWx")).toBeNull())
    print.mockRestore()
  })

  it("logs a sign-in on someone else's screen and marks it signed out (D237)", async () => {
    const logAway = vi.fn<Client["logAway"]>(() => Promise.resolve(2))
    const clearAway = vi.fn<Client["clearAway"]>(() => Promise.resolve(3))
    const away = [{ place: "Hotel TV", at: 100, remind_at: 200, due: true }]
    const { onChanged } = mount(fakeClient({ ...base, logAway, clearAway, show: () => Promise.resolve(item("login", [hidden("pw", "password")], { away })) }))
    const block = await screen.findByRole("region", { name: "Signed in elsewhere" })
    expect(within(block).getByText("Hotel TV")).toBeInTheDocument()
    expect(within(block).getByText("Sign out now")).toBeInTheDocument()
    await userEvent.click(within(block).getByRole("button", { name: "Signed out" }))
    await waitFor(() => expect(clearAway.mock.calls[0]!.slice(0, 2)).toEqual(["i1", 100]))
    await userEvent.click(within(block).getByRole("button", { name: "Signing in on a TV?" }))
    await userEvent.type(await screen.findByLabelText("Where"), "Rental flat")
    await userEvent.selectOptions(screen.getByLabelText("Remind me"), "72")
    await userEvent.click(screen.getByRole("button", { name: "Remind me" }))
    await waitFor(() => expect(logAway.mock.calls[0]![0]).toEqual({ id: "i1", place: "Rental flat", remindInHours: 72 }))
    expect(onChanged).toHaveBeenCalledWith(expect.stringContaining("Rental flat"))
  })

  it("attaches from the disk through the shell and names the room left (D233, D234)", async () => {
    const attachChoose = vi.fn<Client["attachChoose"]>(() => Promise.resolve({ token: 7, path: "/home/me/lease.pdf", name: "lease.pdf" }))
    const attachChosen = vi.fn<Client["attachChosen"]>(() => Promise.resolve("a1"))
    const attach = vi.fn<Client["attach"]>()
    const storageUsage = () => Promise.resolve({ used: 1_200_000_000, quota: 5_000_000_000, used_words: "1.2 GB", quota_words: "5 GB" })
    const { onChanged } = mount(fakeClient({ ...base, attachChoose, attachChosen, attach, storageUsage, show: () => Promise.resolve(item("note", [], { attachments: [] })) }))
    expect(await screen.findByText("1.2 GB of 5 GB used for files on your server.")).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "Attach a file" }))
    await waitFor(() => expect(attachChosen.mock.calls[0]!.slice(0, 2)).toEqual(["i1", 7]))
    expect(attach).not.toHaveBeenCalled()
    expect(onChanged).toHaveBeenCalledWith('Attached "lease.pdf".')
  })

  it("saves a file to the disk through the shell, never as bytes here", async () => {
    const attachmentSaveChoose = vi.fn<Client["attachmentSaveChoose"]>(() => Promise.resolve({ token: 9, path: "/tmp/lease.pdf", name: "lease.pdf" }))
    const attachmentSave = vi.fn<Client["attachmentSave"]>(() => Promise.resolve())
    const attachment = vi.fn<Client["attachment"]>()
    mount(
      fakeClient({
        ...base,
        attachmentSaveChoose,
        attachmentSave,
        attachment,
        storageUsage: () => Promise.resolve(null),
        show: () => Promise.resolve(item("note", [], { attachments: [{ id: "a1", name: "lease.pdf", mime: "application/pdf", size: 10 }] })),
      }),
    )
    await userEvent.click(await screen.findByRole("button", { name: "Save lease.pdf" }))
    await waitFor(() => expect(attachmentSave.mock.calls[0]!.slice(0, 3)).toEqual(["i1", "a1", 9]))
    expect(attachmentSaveChoose).toHaveBeenCalledWith("lease.pdf")
    expect(attachment).not.toHaveBeenCalled()
  })
})

describe("the vault-depth helpers", () => {
  it("draws a QR grid as one path inside its quiet zone", () => {
    const g = new Uint8Array([2, 1, 0, 0, 1])
    expect(qrPath(g)).toEqual({ size: 10, d: "M4 4h1v1h-1zM5 5h1v1h-1z" })
  })

  it("keys a template's fields from their labels, keeping the saved ones", () => {
    expect(keyFor("Locker number", [])).toBe("locker_number")
    expect(keyFor("Locker number", ["locker_number"])).toBe("locker_number_2")
    expect(keyFor("¡¡!!", [])).toBe("field")
    expect(
      draftToFields([
        { key: "locker", label: "Locker no.", kind: "text", required: true },
        { key: null, label: "Locker", kind: "hidden", required: false },
      ]),
    ).toEqual([
      { key: "locker", label: "Locker no.", kind: "text", required: true },
      { key: "locker_2", label: "Locker", kind: "hidden", required: false },
    ])
  })

  it("reads and writes a link's two halves", () => {
    expect(linkText("w1", "pw")).toBe("w1#pw")
    expect(linkText("", "pw")).toBe("")
    expect(parseLink("w1#wifi.passphrase")).toEqual({ itemId: "w1", fieldId: "wifi.passphrase" })
    expect(parseLink("nothing")).toEqual({ itemId: "", fieldId: "" })
  })
})
