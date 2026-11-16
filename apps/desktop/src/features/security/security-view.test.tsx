import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { GateProvider } from "@/app/gate"
import { SettingsProvider, DEFAULT_SETTINGS } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { ClientProvider, type Client } from "@/core/client"
import type { Finding, ItemDetail, ItemSummary, Playbook, SecurityReport } from "@/core/types"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { fakeClient } from "@/test/fake-client"
import { TEMPLATES } from "@/test/templates-fixture"
import { SecurityView } from "./security-view"
import { toFixCount } from "./security-model"

const NOW = Math.floor(Date.now() / 1000)

function finding(over: Partial<Finding>): Finding {
  return {
    id: "weak:i1:pw",
    kind: "weak",
    severity: "important",
    title: "Easy-to-guess password",
    detail: "This password would be guessed quickly.",
    key: "weak:pw",
    item_id: "i1",
    item_title: "GitHub",
    item_kind: "login",
    field_id: "pw",
    site: "https://github.com",
    related: [],
    setup_url: null,
    change_password: true,
    playbook: "weak-password",
    alert: null,
    at: null,
    dismissed: null,
    ...over,
  }
}

const breached = finding({
  id: "breached:i2:pw",
  kind: "breached",
  severity: "urgent",
  title: "Password found in a data breach",
  detail: "This password has been seen 10,437,277 times in data breaches.",
  key: "breached:pw",
  item_id: "i2",
  item_title: "Forum",
  site: "https://forum.example",
  playbook: "breached-password",
})
const twoStep = finding({
  id: "missing_2fa:i1",
  kind: "missing_2fa",
  severity: "suggested",
  title: "Two-step sign-in is available",
  detail: "GitHub lets you add a second step when you sign in.",
  key: "missing_2fa",
  field_id: null,
  change_password: false,
  setup_url: "https://docs.github.com/2fa",
  playbook: "missing-2fa",
})

function report(findings: Finding[], over: Partial<SecurityReport> = {}): SecurityReport {
  const counts = { urgent: 0, important: 0, suggested: 0 }
  for (const f of findings) counts[f.severity] += 1
  return {
    findings,
    dismissed: [],
    counts,
    passwords: 3,
    breach: { enabled: true, source: "api.pwnedpasswords.com", checked_at: NOW, checked: 3, unchecked: 0, message: null },
    alerts: { account: false, checked_at: null, message: null },
    directory: { source: "2fa.directory", licence: "MIT", date: "2026-10-16", sites: 1893 },
    generated_at: NOW,
    ...over,
  }
}

const playbooks: Playbook[] = [
  { id: "breached-password", title: "This password has shown up in a data breach", summary: "Criminals share lists.", steps: ["Change this password now.", "Choose Check now."] },
  { id: "weak-password", title: "This password is easy to guess", summary: "It is short.", steps: ["Change it.", "Choose Check now."] },
  { id: "missing-2fa", title: "This site offers two-step sign-in", summary: "A stolen password isn't enough.", steps: ["Turn it on.", "Dismiss this."] },
]

const row = (id: string, over: Partial<ItemSummary> = {}): ItemSummary => ({
  id,
  kind: "login",
  state: "live",
  title: id,
  site: "",
  username: "",
  edited_at: NOW,
  crown_jewel: false,
  methods: [],
  passkeys: 0,
  codes: 0,
  expires_at: null,
  conflict_copy: false,
  warnings: [],
  views: [],
  subtitle: "",
  copy: null,
  vault_id: "v1",
  ...over,
})

const github: ItemDetail = {
  id: "i1",
  kind: "login",
  state: "live",
  version: 3,
  edited_at: NOW,
  crown_jewel: false,
  fields: [{ field_id: "pw", label: "Password", kind: "password", hidden: true, value: null, mask: "•••••••••" }],
}

function mount(client: Client, rows: ItemSummary[] = []) {
  const handlers = { onOpenItem: vi.fn(), onDevices: vi.fn(), onChanged: vi.fn(), onNotice: vi.fn() }
  render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <VaultsProvider value={{ vaults: [{ id: "v1", name: "Personal", primary: true, items: 3, trash_days: 30, folders: [] }], reload: () => {} }}>
              <SecurityView rows={rows} {...handlers} />
            </VaultsProvider>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
  return handlers
}

const base = (over: Partial<Client>) =>
  fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    templates: () => Promise.resolve(TEMPLATES),
    approvalRoute: () => Promise.resolve({ route: "window", method: "this window" }),
    securityPlaybooks: () => Promise.resolve(playbooks),
    show: () => Promise.resolve(github),
    history: () =>
      Promise.resolve([
        { n: 3, edited_at: NOW - 86_400, edited_by: "This Mac", changed: ["Notes"], readable: true, current: true },
        { n: 2, edited_at: Date.UTC(2021, 2, 14, 12) / 1000, edited_by: "This Mac", changed: ["Password"], readable: true, current: false },
      ]),
    ...over,
  })

describe("Health", () => {
  it("leads with the count to fix: what to fix now and soon, and the Needs attention rows, and never a score", async () => {
    const r = report([breached, finding({}), twoStep])
    const securityCheck = vi.fn(() => Promise.resolve(r))
    const rows = [row("c1", { title: "Bank (conflict copy from Sam's phone)", conflict_copy: true, views: ["attention"] }), row("x")]
    mount(base({ securityReport: () => Promise.resolve(r), securityCheck }), rows)
    // 1 urgent + 1 important + 1 row in Needs attention.
    expect(await screen.findByText("003")).toBeInTheDocument()
    expect(toFixCount(r, rows)).toBe(3)
    expect(screen.getByText("TO FIX")).toBeInTheDocument()
    expect(document.body.textContent).not.toMatch(/\/100|score/i)
    // Metrics for the kinds the core reports, and no "Old".
    const kinds = screen.getByRole("group", { name: "Findings by kind" })
    expect(within(kinds).getByText("No second step")).toBeInTheDocument()
    expect(within(kinds).queryByText(/^Old$/)).not.toBeInTheDocument()
    // The lists, by D125's names, then Needs attention with its word.
    expect(within(screen.getByRole("list", { name: "Fix now" })).getByText("Forum")).toBeInTheDocument()
    expect(within(screen.getByRole("list", { name: "Fix soon" })).getByText("GitHub")).toBeInTheDocument()
    expect(within(screen.getByRole("list", { name: "Worth doing" })).getByText("No second step")).toBeInTheDocument()
    expect(within(screen.getByRole("list", { name: "Needs attention" })).getByText("Conflict")).toBeInTheDocument()
    // A fresh report needs no network check: the counts come from the cached report.
    expect(securityCheck).not.toHaveBeenCalled()
  })

  it("opens a finding in place: its Banner, the field with REVEAL, when it changed, where else it is used, the steps, and one primary", async () => {
    const securityChangePassword = vi.fn(() => Promise.resolve({ url: "https://github.com/.well-known/change-password", kind: "well_known" as const, checked: true }))
    const { onNotice } = mount(base({ securityReport: () => Promise.resolve(report([breached, finding({})])), securityChangePassword }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "GitHub" }))
    const detail = await screen.findByRole("article", { name: "GitHub" })
    expect(within(detail).getByText("Health // Weak password")).toBeInTheDocument()
    expect(within(detail).getByText("Easy-to-guess password")).toBeInTheDocument()
    expect(await within(detail).findByRole("button", { name: /show|reveal/i })).toBeInTheDocument()
    expect(await within(detail).findByRole("group", { name: "Last changed" })).toHaveTextContent("2021.03.14")
    expect(within(detail).getByRole("group", { name: "Also used on" })).toHaveTextContent("None")
    const steps = within(detail).getByRole("list", { name: "Steps" })
    expect(within(steps).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Change it.", "Choose Check now."])
    // One primary: OPEN {SITE}, which asks the shell for the change-password page.
    await user.click(within(detail).getByRole("button", { name: "Open github.com" }))
    await waitFor(() => expect(securityChangePassword).toHaveBeenCalledWith("i1"))
    expect(onNotice).toHaveBeenCalledWith("Opened the site's change-password page in your browser.")
    expect(within(detail).getByRole("button", { name: "Skip for now: GitHub" })).toBeInTheDocument()
    // Nothing on the screen is a password.
    expect(document.body.textContent).not.toMatch(/hunter2|5BAA6/)
  })

  it("skips a finding for a week on its item, then opens the next one", async () => {
    const securityDismiss = vi.fn((_id: string, _key: string, _until: number | null) => Promise.resolve(report([breached])))
    const { onChanged } = mount(base({ securityReport: () => Promise.resolve(report([finding({}), breached])), securityDismiss }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "GitHub" }))
    await user.click(await screen.findByRole("button", { name: "Skip for now: GitHub" }))
    await user.click(await screen.findByRole("menuitem", { name: "Remind me in a week" }))
    await waitFor(() => expect(securityDismiss).toHaveBeenCalledTimes(1))
    const [id, key, until] = securityDismiss.mock.calls[0]!
    expect([id, key]).toEqual(["i1", "weak:pw"])
    expect(until! - NOW).toBeGreaterThanOrEqual(7 * 86_400 - 5)
    expect(until! - NOW).toBeLessThanOrEqual(7 * 86_400 + 5)
    expect(onChanged).toHaveBeenCalled()
    expect(await screen.findByRole("article", { name: "Forum" })).toBeInTheDocument()
  })

  it("turns on two-step sign-in at the site's own page", async () => {
    const openSite = vi.fn(() => Promise.resolve())
    mount(base({ securityReport: () => Promise.resolve(report([twoStep])), openSite }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "GitHub" }))
    await user.click(await screen.findByRole("button", { name: "Turn it on" }))
    expect(openSite).toHaveBeenCalledWith("https://docs.github.com/2fa")
  })

  it("checks again by itself when the last breach check is stale, and says when it last ran", async () => {
    const stale = report([], { breach: { enabled: true, source: "api.pwnedpasswords.com", checked_at: null, checked: 0, unchecked: 3, message: null } })
    const securityCheck = vi.fn(() => Promise.resolve(report([breached])))
    mount(base({ securityReport: () => Promise.resolve(stale), securityCheck }))
    await waitFor(() => expect(securityCheck).toHaveBeenCalledTimes(1))
    expect(await within(await screen.findByRole("list", { name: "Fix now" })).findByText("Forum")).toBeInTheDocument()
    expect(screen.getByText("3 of 3 passwords checked against api.pwnedpasswords.com")).toBeInTheDocument()
  })

  it("keeps the report when a check fails, under a critical Banner with Try again", async () => {
    const securityCheck = vi.fn(() => Promise.reject({ code: "network", message: "The breach list didn't answer." }))
    mount(base({ securityReport: () => Promise.resolve(report([breached])), securityCheck }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Check now" }))
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("The check didn't finish.")
    expect(alert).toHaveTextContent("The breach list didn't answer.")
    expect(within(alert).getByRole("button", { name: "Try again" })).toBeInTheDocument()
    expect(within(screen.getByRole("list", { name: "Fix now" })).getByText("Forum")).toBeInTheDocument()
  })

  it("never says all clear when the report could not be read", async () => {
    mount(base({ securityReport: () => Promise.reject({ code: "refused", message: "The index is being rebuilt." }) }))
    expect(await screen.findByText("Couldn't read your health report.")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument()
    expect(screen.queryByText("All clear.")).not.toBeInTheDocument()
  })

  it("says all clear in plain words, and with the breach check off, says so and offers to turn it on", async () => {
    const off = report([], { breach: { enabled: false, source: "api.pwnedpasswords.com", checked_at: null, checked: 0, unchecked: 0, message: null } })
    mount(base({ securityReport: () => Promise.resolve(off) }))
    expect(await screen.findByText("All clear.")).toBeInTheDocument()
    expect(screen.getByText("None of your 3 saved passwords is weak, reused or known from a breach.")).toBeInTheDocument()
    expect(screen.getByText("Check is off")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Turn on" })).toBeInTheDocument()
    // How the checks work lives in Settings → Security checks now (R13).
    expect(screen.queryByRole("switch", { name: "Check my passwords against known data breaches" })).not.toBeInTheDocument()
  })

  it("brings a dismissed finding back from the Dismissed list", async () => {
    const gone = finding({ dismissed: { at: NOW - 86_400, until: null } })
    const securityRestore = vi.fn(() => Promise.resolve(report([finding({})])))
    mount(base({ securityReport: () => Promise.resolve(report([], { dismissed: [gone] })), securityRestore }))
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: /Dismissed/ }))
    await user.click(await screen.findByRole("button", { name: "Bring back: GitHub" }))
    await waitFor(() => expect(securityRestore).toHaveBeenCalledWith("i1", "weak:pw"))
  })
})
