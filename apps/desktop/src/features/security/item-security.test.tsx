import { render, screen, waitFor, within } from "@testing-library/react"

import userEvent from "@testing-library/user-event"

import { PlacesProvider } from "@/app/places"
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

describe("an item's own findings in its notice slot (brief I8)", () => {
  it("warns first and loudest when the website looks like another site: one critical Banner, the rest one line to Health (brief I8, D130)", async () => {
    const go = vi.fn()
    const client = base({ securityReport: () => Promise.resolve(report([finding({ item_id: "i2", id: "weak:i2:pw" }), lookalike])) })
    render(
      <ClientProvider value={client}>
        <PlacesProvider value={{ nav: { area: "vault", view: "all" }, surface: "desktop", go, navigate: () => {} }}>
          <ItemSecurity itemId="i2" onNotice={() => {}} />
        </PlacesProvider>
      </ClientProvider>,
    )
    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("Website looks like another site")
    expect(alert).toHaveTextContent("paypa1.com looks a lot like paypal.com. Don't sign in there unless you're sure it's the real site.")
    // One Banner only: the easy-to-guess password is not a second one, it is counted in the line.
    expect(screen.queryByText("Easy-to-guess password")).toBeNull()
    expect(screen.getAllByRole("alert")).toHaveLength(1)
    expect(screen.queryByRole("status")).toBeNull()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "001 more in Health" }))
    expect(go).toHaveBeenCalledWith("health")
    // WHAT TO DO opens the finding's playbook, where its fix is.
    await user.click(within(alert).getByRole("button", { name: "What to do" }))
    expect(await screen.findByRole("dialog")).toHaveTextContent("This website's address looks like another site's")
  })

  it("an urgent finding is critical and any other a warning; the item's state outranks a finding but never a look-alike", async () => {
    const urgent = finding({ id: "breached:i1:pw", kind: "breached", severity: "urgent", title: "Password found in a breach", playbook: "breached-password" })
    const { rerender } = render(
      <ClientProvider value={base({ securityReport: () => Promise.resolve(report([finding({}), urgent])) })}>
        <ItemSecurity itemId="i1" onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(await screen.findByRole("alert")).toHaveTextContent("Password found in a breach")
    expect(screen.getByRole("button", { name: "001 more in Health" })).toBeInTheDocument()

    rerender(
      <ClientProvider value={base({ securityReport: () => Promise.resolve(report([finding({ id: "weak:i3:pw", item_id: "i3" })])) })}>
        <ItemSecurity itemId="i3" onNotice={() => {}} />
      </ClientProvider>,
    )
    expect(await screen.findByRole("status")).toHaveTextContent("Easy-to-guess password")
    expect(screen.queryByRole("alert")).toBeNull()

    // Archived: the state's Banner shows and the finding is counted under it.
    rerender(
      <ClientProvider value={base({ securityReport: () => Promise.resolve(report([finding({ id: "weak:i3:pw", item_id: "i3" })])) })}>
        <ItemSecurity itemId="i3" onNotice={() => {}} state={<p>Archived. It still syncs and exports.</p>} />
      </ClientProvider>,
    )
    expect(await screen.findByText("Archived. It still syncs and exports.")).toBeInTheDocument()
    expect(screen.queryByText("Easy-to-guess password")).toBeNull()
    expect(screen.getByRole("button", { name: "001 more in Health" })).toBeInTheDocument()

    // A look-alike outranks the state and an urgent finding, whatever order the report lists them in.
    rerender(
      <ClientProvider value={base({ securityReport: () => Promise.resolve(report([finding({ id: "weak:i2:pw", item_id: "i2" }), { ...urgent, id: "breached:i2:pw", item_id: "i2" }, lookalike])) })}>
        <ItemSecurity itemId="i2" onNotice={() => {}} state={<p>Archived. It still syncs and exports.</p>} />
      </ClientProvider>,
    )
    expect(await screen.findByRole("alert")).toHaveTextContent("Website looks like another site")
    expect(screen.queryByText("Archived. It still syncs and exports.")).toBeNull()
    expect(screen.getByRole("button", { name: "002 more in Health" })).toBeInTheDocument()
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
