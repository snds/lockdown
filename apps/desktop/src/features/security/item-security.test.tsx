import { render, screen, waitFor, within } from "@testing-library/react"

import { DEFAULT_SETTINGS } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { Finding, Playbook, SecurityReport } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { ItemSecurity } from "./item-security"

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

const base = (over: Partial<Client>) =>
  fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    securityPlaybooks: () => Promise.resolve(playbooks),
    ...over,
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
