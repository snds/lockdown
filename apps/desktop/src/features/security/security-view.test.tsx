import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { SettingsProvider, DEFAULT_SETTINGS } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { Finding, Playbook, SecurityReport } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ItemSecurity } from "./item-security"
import { SecurityView } from "./security-view"
import { summaryWords } from "./security-model"

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
  id: "breached:i1:pw",
  kind: "breached",
  severity: "urgent",
  title: "Password found in a data breach",
  detail: "This password has been seen 10,437,277 times in data breaches.",
  key: "breached:pw",
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
const lookalike = finding({
  id: "lookalike:i2",
  kind: "lookalike",
  title: "Website looks like another site",
  detail: "paypa1.com looks a lot like paypal.com.",
  key: "lookalike",
  item_id: "i2",
  item_title: "Pay",
  field_id: null,
  change_password: false,
  playbook: "lookalike-site",
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
  { id: "lookalike-site", title: "This website's address looks like another site's", summary: "Fake sites copy addresses.", steps: ["Compare.", "Dismiss."] },
]

function mount(client: Client) {
  const handlers = { onOpenItem: vi.fn(), onDevices: vi.fn(), onChanged: vi.fn(), onNotice: vi.fn() }
  render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <SecurityView {...handlers} />
      </SettingsProvider>
    </ClientProvider>,
  )
  return handlers
}

const base = (over: Partial<Client>) =>
  fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    securityPlaybooks: () => Promise.resolve(playbooks),
    ...over,
  })

describe("the Security screen", () => {
  it("groups findings by how pressing they are, with counts, and opens each one's steps and item", async () => {
    const client = base({ securityReport: () => Promise.resolve(report([breached, finding({}), twoStep])) })
    const { onOpenItem } = mount(client)
    expect(await screen.findByText("1 to fix now, 1 to fix soon and 1 worth doing.")).toBeInTheDocument()
    const now = screen.getByRole("list", { name: "Fix now" })
    expect(within(now).getByText("Password found in a data breach")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Fix soon (1)" })).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "Worth doing (1)" })).toBeInTheDocument()

    const user = userEvent.setup()
    await user.click(within(now).getByRole("button", { name: "What to do" }))
    const dialog = await screen.findByRole("dialog")
    expect(within(dialog).getByText("This password has shown up in a data breach")).toBeInTheDocument()
    const steps = within(dialog).getByRole("list", { name: "Steps" })
    expect(within(steps).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Change this password now.", "Choose Check now."])
    await user.click(within(dialog).getByRole("button", { name: "Open item" }))
    expect(onOpenItem).toHaveBeenCalledWith("i1")
    // Nothing on the screen is a password.
    expect(document.body.textContent).not.toMatch(/hunter2|5BAA6/)
  })

  it("snoozes a finding on its item for a week and reloads the list", async () => {
    const securityDismiss = vi.fn((_id: string, _key: string, _until: number | null) => Promise.resolve(report([breached])))
    const client = base({ securityReport: () => Promise.resolve(report([breached, finding({})])), securityDismiss })
    const { onChanged, onNotice } = mount(client)
    const soon = await screen.findByRole("list", { name: "Fix soon" })
    const user = userEvent.setup()
    await user.click(within(soon).getByRole("button", { name: "Dismiss: Easy-to-guess password" }))
    await user.click(await screen.findByRole("menuitem", { name: "Remind me in a week" }))
    await waitFor(() => expect(securityDismiss).toHaveBeenCalledTimes(1))
    const [id, key, until] = securityDismiss.mock.calls[0]!
    expect([id, key]).toEqual(["i1", "weak:pw"])
    expect(until! - NOW).toBeGreaterThanOrEqual(7 * 86_400 - 5)
    expect(until! - NOW).toBeLessThanOrEqual(7 * 86_400 + 5)
    await waitFor(() => expect(screen.queryByRole("list", { name: "Fix soon" })).not.toBeInTheDocument())
    expect(onChanged).toHaveBeenCalled()
    expect(onNotice.mock.calls[0]?.[0]).toMatch(/^Snoozed until /)
  })

  it("checks again by itself when the last breach check is stale, and says what it found", async () => {
    const stale = report([], { breach: { enabled: true, source: "api.pwnedpasswords.com", checked_at: null, checked: 0, unchecked: 3, message: null } })
    const securityCheck = vi.fn(() => Promise.resolve(report([breached])))
    const client = base({ securityReport: () => Promise.resolve(stale), securityCheck })
    mount(client)
    await waitFor(() => expect(securityCheck).toHaveBeenCalledTimes(1))
    expect(await screen.findByText("1 to fix now.")).toBeInTheDocument()
    expect(screen.getByText(/Checked 3 of 3 passwords against known breaches \(api\.pwnedpasswords\.com\)/)).toBeInTheDocument()
  })

  it("opens a site's change-password page through the shell and its two-step set-up page", async () => {
    const securityChangePassword = vi.fn(() => Promise.resolve({ url: "https://github.com/.well-known/change-password", kind: "well_known" as const, checked: true }))
    const openSite = vi.fn(() => Promise.resolve())
    const client = base({ securityReport: () => Promise.resolve(report([finding({}), twoStep])), securityChangePassword, openSite })
    const { onNotice } = mount(client)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Change password" }))
    await waitFor(() => expect(securityChangePassword).toHaveBeenCalledWith("i1"))
    expect(onNotice).toHaveBeenCalledWith("Opened the site's change-password page in your browser.")
    await user.click(screen.getByRole("button", { name: "Turn it on" }))
    expect(openSite).toHaveBeenCalledWith("https://docs.github.com/2fa")
  })

  it("says all clear in plain words and turns the breach check off from its own settings", async () => {
    const setSettings = vi.fn((s) => Promise.resolve(s))
    const client = base({ securityReport: () => Promise.resolve(report([])), setSettings })
    mount(client)
    expect(await screen.findByText("All clear")).toBeInTheDocument()
    expect(summaryWords(report([]))).toBe("Nothing needs your attention right now.")
    const user = userEvent.setup()
    await user.click(screen.getByRole("switch", { name: "Check my passwords against known data breaches" }))
    await waitFor(() => expect(setSettings).toHaveBeenCalledWith(expect.objectContaining({ breach_check: false })))
  })
})

describe("an item's own security panel", () => {
  it("warns first and loudest when the website looks like another site", async () => {
    const client = base({ securityReport: () => Promise.resolve(report([lookalike, finding({ item_id: "i2", id: "weak:i2:pw" })])) })
    render(
      <ClientProvider value={client}>
        <ItemSecurity itemId="i2" onNotice={() => {}} />
      </ClientProvider>,
    )
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Careful: this website looks like another site")
    expect(alert).toHaveTextContent("paypa1.com looks a lot like paypal.com.")
    expect(within(screen.getByRole("list", { name: "Security findings" })).getByText("Easy-to-guess password.")).toBeInTheDocument()
  })

  it("shows nothing for an item with no findings, or when the report doesn't load", async () => {
    const securityReport = vi.fn(() => Promise.resolve(report([lookalike])))
    const { container, rerender } = render(
      <ClientProvider value={base({ securityReport })}>
        <ItemSecurity itemId="i1" onNotice={() => {}} />
      </ClientProvider>,
    )
    await waitFor(() => expect(securityReport).toHaveBeenCalled())
    expect(container).toBeEmptyDOMElement()
    rerender(
      <ClientProvider value={base({})}>
        <ItemSecurity itemId="i2" onNotice={() => {}} />
      </ClientProvider>,
    )
    await new Promise((r) => setTimeout(r, 0))
    expect(container).toBeEmptyDOMElement()
  })
})
