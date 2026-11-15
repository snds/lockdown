import { act, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { LockedProvider } from "@/app/locked"
import { notify } from "@/app/notify"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { Access, CopyTarget, FieldView, Finding, ItemDetail, ItemSummary, SecurityReport } from "@/core/types"
import { DepthClientProvider, type ExposureView } from "@/features/depth/depth-client"
import { fakeClient } from "@/test/fake-client"
import { malformedLists } from "@/test/a11y"
import { fakeDepth } from "@/test/fake-depth"
import { TEMPLATES } from "@/test/templates-fixture"

import { bottomCopyOf, bottomRowOf, overlineOf } from "./detail-model"
import { DetailPane } from "./detail-pane"

vi.mock("@/app/notify", async (importOriginal) => ({ ...(await importOriginal<typeof import("@/app/notify")>()), notify: vi.fn(() => 1) }))

const plain = (field_id: string, label: string, value: string, kind = "text"): FieldView => ({ field_id, label, kind, hidden: false, value, mask: null })
const hidden = (field_id: string, label: string, kind = "hidden"): FieldView => ({ field_id, label, kind, hidden: true, value: null, mask: "••••••••" })
const item = (kind: string, fields: FieldView[], over: Partial<ItemDetail> = {}): ItemDetail => ({ id: "i1", kind, state: "live", version: 1, edited_at: 0, crown_jewel: false, fields: [plain("title", "title", "Thing"), ...fields], ...over })
const summary = (over: Partial<ItemSummary> = {}): ItemSummary => ({ id: "i1", kind: "login", title: "Thing", subtitle: "", state: "live", codes: 0, conflict_copy: false, copy: null, ...over }) as ItemSummary

function mount(client: Client, opts: { rows?: ItemSummary[]; onTrashed?: () => void; onReplaced?: (id: string, notice: string) => void } = {}) {
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <DetailPane id="i1" rows={opts.rows ?? []} onEdit={() => {}} onTrashed={opts.onTrashed ?? (() => {})} onChanged={() => {}} onNotice={() => {}} onReplaced={opts.onReplaced} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

const shows = (detail: ItemDetail, over: Partial<Client> = {}) => fakeClient({ templates: () => Promise.resolve(TEMPLATES), show: () => Promise.resolve(detail), ...over })

beforeEach(() => {
  localStorage.clear()
  vi.mocked(notify).mockClear()
})

describe("the detail frame (brief I1)", () => {
  it("the overline names the kind, then what tells it apart; the title; the mono line names the site and the edit, never 'added' (W3)", async () => {
    const edited = Date.UTC(2026, 10, 8, 12) / 1000
    mount(shows(item("login", [plain("user", "username", "sam"), hidden("pw", "password", "password"), plain("url.0", "website", "https://store.example.test/", "url")], { edited_at: edited })))
    expect(await screen.findByRole("heading", { name: "Thing" })).toBeInTheDocument()
    expect(screen.getByText("Login")).toBeInTheDocument()
    expect(screen.getByText(/^store\.example\.test \/\/ edited 2026\.11\.0[78]$/)).toBeInTheDocument()
    expect(screen.queryByText(/added/)).not.toBeInTheDocument()
  })

  it("says the methods, a variant, a brand and the state in the overline", () => {
    const facts = { passkeys: true, codes: true }
    expect(overlineOf(item("login", [hidden("pw", "p")]), null, facts)).toBe("Login // passkey + code")
    expect(overlineOf(item("login", []), null, facts)).toBe("Login // passkey + code // no password")
    expect(overlineOf(item("card", [plain("card.type", "Brand", "Visa")]), null, { passkeys: false, codes: false })).toBe("Card // Visa")
    expect(overlineOf(item("identity", [], { access: "view" }), TEMPLATES.kinds.find((k) => k.variant === "passport")!, { passkeys: false, codes: false })).toMatch(/^Identity \/\/ passport \/\/ shared with you \/\/ view$/)
    expect(overlineOf(item("login", [], { state: "trashed" }), null, { passkeys: false, codes: false })).toBe("Login // recently deleted")
    expect(overlineOf(item("code", []), null, { passkeys: false, codes: true, code: { variant: "steam", digits: 5 } })).toBe("Code // Steam")
  })

  it("loading draws skeleton rows; a failed open is a critical Empty State with the core's words, the id and Try again", async () => {
    let fail = true
    mount(
      fakeClient({
        templates: () => Promise.resolve(TEMPLATES),
        show: () => (fail ? Promise.reject({ code: "refused", message: "the field 'pw' was tampered with" }) : Promise.resolve(item("note", [plain("body", "Note", "Gate code 4411")]))),
      }),
    )
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Couldn't open this item.")
    expect(alert).toHaveTextContent("the field 'pw' was tampered with")
    expect(alert).toHaveTextContent("i1")
    fail = false
    await userEvent.setup().click(within(alert).getByRole("button", { name: "Try again" }))
    expect(await screen.findByText("Gate code 4411")).toBeInTheDocument()
  })

  it("an item with nothing filled in says so, with Edit", async () => {
    mount(shows(item("custom", [])))
    expect(await screen.findByText(/Nothing filled in yet\./)).toBeInTheDocument()
    expect(screen.getAllByRole("button", { name: "Edit" }).length).toBeGreaterThan(0)
    mount(shows(item("note", [])))
    expect(await screen.findByText(/An empty note\./)).toBeInTheDocument()
  })

  it("Details is shut by default and holds where the item is and its version", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")], { version: 7 })))
    const toggle = await screen.findByRole("button", { name: /^Details/ })
    expect(toggle).toHaveAttribute("aria-expanded", "false")
    await userEvent.setup().click(toggle)
    expect(toggle).toHaveAttribute("aria-expanded", "true")
    expect(screen.getByRole("button", { name: /^History/ })).toBeInTheDocument()
    expect(localStorage.getItem("lockdown.detail.details-open")).toBe("1")
  })
})

describe("Details: one list of rows, only those that apply (brief I4)", () => {
  const ROWS = ["Folder", "Tags", "Signed in elsewhere", "Crown jewel", "Ask before the security code", "Version"] as const
  const shown = () => ROWS.filter((name) => screen.queryByRole("group", { name }))
  // Details remembers on this device that it was opened, so a second item opens with it open.
  const openDetails = async () => {
    const toggle = await screen.findByRole("button", { name: /^Details/ })
    if (toggle.getAttribute("aria-expanded") !== "true") await userEvent.setup().click(toggle)
  }

  it("a login of one's own: where it is, signed in elsewhere, crown jewel and version; no card row", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")])))
    await openDetails()
    expect(shown()).toEqual(["Folder", "Tags", "Signed in elsewhere", "Crown jewel", "Version"])
    expect(screen.getByRole("group", { name: "Signed in elsewhere" })).toHaveTextContent("Nowhere")
    expect(screen.getByRole("group", { name: "Crown jewel" })).toHaveTextContent("No")
    expect(screen.queryByRole("switch", { name: "Include in exports and backups" })).not.toBeInTheDocument()
  })

  it("a note has no sign-in rows; a card says whether it asks before its security code", async () => {
    const { unmount } = mount(shows(item("note", [plain("body", "Note", "Gate code 4411")])))
    await openDetails()
    expect(shown()).toEqual(["Folder", "Tags", "Crown jewel", "Version"])
    unmount()
    mount(shows(item("card", [plain("card.type", "Brand", "Visa")], { reprompt: true })))
    await openDetails()
    expect(shown()).toEqual(["Folder", "Tags", "Crown jewel", "Ask before the security code", "Version"])
    expect(screen.getByRole("group", { name: "Ask before the security code" })).toHaveTextContent("Yes")
  })

  it("a share this person can only view: the version and nothing to press", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")], { access: "view" })))
    await openDetails()
    expect(shown()).toEqual(["Version"])
    expect(within(screen.getByRole("group", { name: "Version" })).queryByRole("button")).not.toBeInTheDocument()
  })

  it("a crown jewel: CHANGE asks today's question, and its two switches follow the version", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")], { crown_jewel: true, export_opt_in: false })))
    await openDetails()
    const user = userEvent.setup()
    const row = screen.getByRole("group", { name: "Crown jewel" })
    expect(row).toHaveTextContent("Yes")
    expect(screen.getByRole("switch", { name: "Include in exports and backups" })).not.toBeChecked()
    await user.click(within(row).getByRole("button", { name: "Change" }))
    expect(await screen.findByRole("dialog", { name: "Stop keeping it as a crown jewel?" })).toBeInTheDocument()
  })

  it("a crown jewel by its kind says Always, with nothing to change", async () => {
    mount(shows(item("crypto-wallet", [], { crown_jewel: true, crown_jewel_fixed: true })))
    await openDetails()
    const row = screen.getByRole("group", { name: "Crown jewel" })
    expect(row).toHaveTextContent("Always")
    expect(within(row).queryByRole("button")).not.toBeInTheDocument()
  })

  it("shut, its one line names where it is and who can get its values", async () => {
    const exposure: ExposureView = { id: "i1", tier: "ask", source: "chosen", default_tier: "fill", reason: null, choices: ["fill", "ask", "app-only"] }
    render(
      <ClientProvider value={shows(item("login", [plain("user", "username", "sam")]))}>
        <DepthClientProvider value={fakeDepth({ exposure: () => Promise.resolve(exposure) })}>
          <SettingsProvider>
            <TemplatesProvider>
              <GateProvider>
                <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
              </GateProvider>
            </TemplatesProvider>
          </SettingsProvider>
        </DepthClientProvider>
      </ClientProvider>,
    )
    expect(await screen.findByText("Ask each time")).toBeInTheDocument()
  })
})

describe("the item's ⋯ menu (brief I1)", () => {
  const login = (access?: Access) => item("login", [plain("user", "username", "sam"), hidden("pw", "password", "password")], { access })
  const rowsOf = async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "More for this item" }))
    return (await screen.findAllByRole("menuitem")).map((m) => m.textContent?.trim())
  }

  it("the person's own item: every row, Move to Recently deleted last", async () => {
    mount(shows(login()), { rows: [summary()] })
    const rows = await rowsOf()
    expect(rows).toEqual(["Add to favorites", "Folder and tags…", "Archive", "History", "Signing in on a TV?", "Keep as a crown jewel", "Move to Recently deleted"])
  })

  it("a share with edit keeps the menu; a view-only share and a fill-only share have none", async () => {
    const { unmount } = mount(shows(login("edit")))
    expect(await rowsOf()).toContain("Move to Recently deleted")
    unmount()
    for (const access of ["view", "fill-only"] as const) {
      const r = mount(shows(login(access)))
      await screen.findByRole("heading", { name: "Thing" })
      expect(screen.queryByRole("button", { name: "More for this item" })).not.toBeInTheDocument()
      expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument()
      r.unmount()
    }
  })

  it("in Recently deleted: Restore and Delete for good", async () => {
    mount(shows(item("login", [plain("user", "username", "sam")], { state: "trashed" })))
    expect(await rowsOf()).toEqual(["Restore", "History", "Delete for good"])
    expect(screen.getByText(/In Recently deleted\. Deleted for good after 30 days\./)).toBeInTheDocument()
  })

  it("Move to Recently deleted asks nothing, toasts UNDO (Q14), and UNDO restores", async () => {
    const trash = vi.fn<Client["trash"]>(() => Promise.resolve())
    const restore = vi.fn<Client["restore"]>(() => Promise.resolve())
    const onTrashed = vi.fn()
    const onReplaced = vi.fn()
    mount(shows(login(), { trash, restore }), { onTrashed, onReplaced })
    const user = userEvent.setup({ pointerEventsCheck: 0 })
    await user.click(await screen.findByRole("button", { name: "More for this item" }))
    await user.click(await screen.findByRole("menuitem", { name: "Move to Recently deleted" }))
    await waitFor(() => expect(trash).toHaveBeenCalledWith("i1"))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(onTrashed).toHaveBeenCalled()
    const [words, options] = vi.mocked(notify).mock.calls[0]!
    expect(words).toBe("Moved Thing to Recently deleted")
    expect(options?.action?.label).toBe("Undo")
    options!.action!.onClick()
    await waitFor(() => expect(restore).toHaveBeenCalledWith("i1"))
    await waitFor(() => expect(onReplaced).toHaveBeenCalledWith("i1", "Restored Thing"))
  })

  it("the star on the overline's line says what it does", async () => {
    const organize = vi.fn<Client["organize"]>(() => Promise.resolve(2))
    mount(shows(login(), { organize }), { rows: [summary({ favourite: true })] })
    const star = await screen.findByRole("button", { name: "Remove from favorites" })
    expect(star).toHaveAttribute("aria-pressed", "true")
    await userEvent.setup().click(star)
    await waitFor(() => expect(organize).toHaveBeenCalledWith("i1", { favourite: false }))
  })
})

describe("the phone's bottom row follows the row's quick copy (brief I1)", () => {
  const secret: CopyTarget = { field_id: "pw", how: "secret", label: "password" }
  const code: CopyTarget = { field_id: "code.0", how: "code", label: "code" }
  it("a secret: the eye and its copy; a code: copy only; nothing to copy: no row (Q17); Recently deleted: Restore", () => {
    expect(bottomRowOf({ state: "live" }, secret, true)).toEqual({ start: "reveal", end: "copy-secret" })
    expect(bottomRowOf({ state: "live" }, code, true)).toEqual({ start: null, end: "copy-code" })
    expect(bottomRowOf({ state: "live" }, null, true)).toEqual({ start: null, end: null })
    expect(bottomRowOf({ state: "trashed" }, secret, true)).toEqual({ start: null, end: "restore" })
    expect(bottomRowOf({ state: "trashed" }, secret, false)).toEqual({ start: null, end: null })
    expect(bottomRowOf({ state: "live", access: "fill-only" }, secret, false)).toEqual({ start: null, end: null })
  })
  it("a note with a secret gets copy-secret on its first secret field; a note with nothing secret gets no row", () => {
    const network = { field_id: "note.net", label: "Network password", kind: "password", hidden: true }
    const text = { field_id: "note.body", label: "Note", kind: "multiline", hidden: false }
    const target = bottomCopyOf(null, [text, network])
    expect(target).toEqual({ field_id: "note.net", how: "secret", label: "network password" })
    expect(bottomRowOf({ state: "live" }, target, true)).toEqual({ start: "reveal", end: "copy-secret" })
    expect(bottomCopyOf(null, [text])).toBeNull()
    expect(bottomCopyOf(null, [{ field_id: "c", label: "Code", kind: "totp", hidden: true }])).toBeNull()
    expect(bottomCopyOf(secret, [network])).toBe(secret)
  })
})

describe("one rule for every kind (brief I2)", () => {
  it("a Wi-Fi network: its section, the template's labels, yes or no, and large type on the password as a text action", async () => {
    mount(shows(item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password"), plain("wifi.hidden", "hidden", "true", "boolean")])))
    const network = await screen.findByRole("region", { name: "Network" })
    expect(network).toHaveTextContent("home-net")
    expect(within(network).getByText("Hidden network")).toBeInTheDocument()
    expect(network).toHaveTextContent("Yes")
    expect(within(network).getByRole("button", { name: "Show large type" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Guest card" })).toBeInTheDocument()
  })

  it("a card: the art from its real fields, the number and code masked, the fill rule said", async () => {
    const { container } = mount(shows(item("card", [hidden("card.number", "number"), plain("card.holder", "holder", "A. Person"), plain("card.expiry", "expires", "2029-08"), hidden("card.cvc", "security code"), plain("card.type", "Brand", "Visa"), plain("custom.0", "Bank phone", "+1 555 0100", "phone")])))
    const card = await screen.findByRole("region", { name: "Card" })
    expect(within(card).getAllByText("Card number").length).toBeGreaterThan(0)
    expect(within(card).getAllByText("Security code").length).toBeGreaterThan(0)
    expect(card).toHaveTextContent("A. Person")
    expect(card).toHaveTextContent("08/29")
    const art = container.querySelector('[data-slot="tile"]')
    expect(art).toHaveTextContent("Visa")
    expect(art).toHaveTextContent("Expires 08/29")
    expect(art).not.toHaveTextContent(/\d{4}$/)
    expect(screen.getByRole("region", { name: "Your fields" })).toHaveTextContent("Bank phone")
    expect(screen.getByText(/always takes a second press/)).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Thing" }).className).toContain("type-title")
  })

  it("a passkey is view only: no Edit, and its key is never drawn", async () => {
    mount(shows(item("passkey", [plain("passkey.0.rp_id", "relying party", "example.com"), hidden("passkey.0.key", "private key", "passkey")])))
    expect(await screen.findByText(/made by a site and used from the browser/)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument()
    expect(screen.getByText("Passkey // no password")).toBeInTheDocument()
  })

  it("a note: its body in Prose from the Markdown", async () => {
    const { container } = mount(shows(item("note", [plain("body", "Note", "# Gate\n\n**4411** after 22:00")])))
    expect(await screen.findByRole("region", { name: "Note" })).toBeInTheDocument()
    const prose = container.querySelector('[data-slot="prose"]')
    expect(prose?.querySelector("strong")).toHaveTextContent("4411")
  })

  it("an SSH key's public key is a code block with copy; the private key stays masked", async () => {
    mount(shows(item("ssh-key", [hidden("ssh.private_key", "Private key"), plain("ssh.public_key", "Public key", "ssh-ed25519 AAAAC3Nz example")])))
    expect(await screen.findByRole("button", { name: "Copy Public key" })).toBeInTheDocument()
    expect(screen.getByText("ssh-ed25519 AAAAC3Nz example")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /^(Hold to show|Show) Private key/ })).toBeInTheDocument()
  })

  it("a variant from its own template, with its section", async () => {
    mount(shows(item("identity", [hidden("passport.number", "n"), plain("passport.country", "c", "GB"), plain("expires_at", "e", "2029-08-14")], { template: "identity/passport" })))
    const section = await screen.findByRole("region", { name: "Passport" })
    expect(section).toHaveTextContent("2029.08.14")
    expect(within(section).getByText("Issuing country")).toBeInTheDocument()
  })

  it("an empty field is not drawn", async () => {
    mount(shows(item("login", [plain("user", "username", "sam"), plain("notes", "Notes", "")])))
    await screen.findByRole("heading", { name: "Thing" })
    expect(screen.queryByText("Notes")).not.toBeInTheDocument()
  })
})

describe("websites on an item (brief I3)", () => {
  const site = (n: number, rule?: string): FieldView => ({ ...plain(`url.${n}`, "Website", `https://s${n}.example.test`, "url"), match_rule: rule })
  it("OPEN and COPY on each; the rule only when it is not the default; SHOW ALL past three", async () => {
    mount(shows(item("login", [plain("user", "username", "sam"), site(0, "exact_host"), site(1, "base_domain"), site(2), site(3), site(4)])))
    const sites = await screen.findByRole("region", { name: "Websites" })
    expect(within(sites).getAllByRole("listitem")).toHaveLength(3)
    expect(within(sites).getAllByRole("button", { name: /^Open/ })).toHaveLength(3)
    expect(within(sites).getAllByRole("button", { name: /^Copy/ })).toHaveLength(3)
    expect(within(sites).getByText("Anywhere on this site")).toBeInTheDocument()
    expect(within(sites).queryByText("This address only")).not.toBeInTheDocument()
    await userEvent.setup().click(within(sites).getByRole("button", { name: "Show all 5" }))
    expect(within(sites).getAllByRole("listitem")).toHaveLength(5)
  })
})

describe("the Use zone is a list of rows (design review R7)", () => {
  const login = item("login", [plain("user", "username", "sam"), hidden("pw", "password", "password"), { ...plain("url", "website", "https://mail.example.test", "url"), match_rule: "base_domain" }])
  const card = item("card", [hidden("card.number", "number"), plain("card.holder", "holder", "A. Person"), plain("custom.0", "Bank phone", "+1 555 0100", "phone")])
  const wifi = item("wi-fi", [plain("wifi.ssid", "Network name", "home-net"), hidden("wifi.passphrase", "password")])

  it("a login's sign-in rows and websites: each child a list item", async () => {
    const { container } = mount(shows(login))
    const signIn = await screen.findByRole("region", { name: "Sign-in" })
    const items = within(within(signIn).getByRole("list")).getAllByRole("listitem")
    expect(items.length).toBeGreaterThanOrEqual(2)
    expect(items[0]).toHaveTextContent("sam")
    const sites = screen.getByRole("region", { name: "Websites" })
    expect(within(within(sites).getByRole("list")).getAllByRole("listitem")).toHaveLength(1)
    expect(malformedLists(container)).toEqual([])
  })

  it("a card and a Wi-Fi network: no list holds anything but list items", async () => {
    for (const [detail, block] of [
      [card, "Card"],
      [wifi, "Network"],
    ] as const) {
      const { container, unmount } = mount(shows(detail))
      await screen.findByRole("region", { name: block })
      expect(malformedLists(container)).toEqual([])
      unmount()
    }
  })
})

describe("one Security box (design review R2)", () => {
  it("draws the item's Security box once, however often the lock poll redraws the detail", async () => {
    const finding = {
      id: "f1",
      kind: "weak",
      severity: "important",
      title: "Weak password",
      detail: "It could be guessed.",
      key: "weak:pw",
      item_id: "i1",
      item_title: "Thing",
      item_kind: "login",
      field_id: "login.password",
      site: null,
      related: [],
      setup_url: null,
      change_password: true,
      playbook: "weak",
      alert: null,
    } as unknown as Finding
    const report = { findings: [finding], dismissed: [], counts: { urgent: 0, important: 1, suggested: 0 } } as unknown as SecurityReport
    const client = fakeClient({
      templates: () => Promise.resolve(TEMPLATES),
      show: () => Promise.resolve(item("login", [plain("login.username", "username", "sam")])),
      securityReport: () => Promise.resolve(report),
      securityPlaybooks: () => Promise.resolve([]),
    })
    const depth = fakeDepth({
      exposure: () => Promise.resolve({ id: "i1", tier: "fill", source: "default", default_tier: "fill", reason: null, choices: ["fill", "ask", "app-only"] } satisfies ExposureView),
    })
    const tree = (locked: boolean) => (
      <ClientProvider value={client}>
        <DepthClientProvider value={depth}>
          <LockedProvider value={locked}>
            <SettingsProvider>
              <TemplatesProvider>
                <GateProvider>
                  <DetailPane id="i1" rows={[]} onEdit={() => {}} onTrashed={() => {}} onChanged={() => {}} onNotice={() => {}} />
                </GateProvider>
              </TemplatesProvider>
            </SettingsProvider>
          </LockedProvider>
        </DepthClientProvider>
      </ClientProvider>
    )
    localStorage.setItem("lockdown.detail.details-open", "1")
    const { rerender } = render(tree(false))
    await screen.findByRole("region", { name: "Security" })
    // Who can get its values is one row of Details now (brief I4), drawn once however often the pane redraws.
    await screen.findByRole("group", { name: "Who can get its values" })
    for (let poll = 0; poll < 3; poll += 1) {
      rerender(tree(false))
      await act(() => Promise.resolve())
    }
    expect(screen.getAllByRole("region", { name: "Security" })).toHaveLength(1)
    expect(screen.getAllByRole("group", { name: "Who can get its values" })).toHaveLength(1)
  })
})
