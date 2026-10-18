import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { BackupStatus, GateInput } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { AliasField } from "./alias-field"
import { AliasSettings } from "./alias-settings"
import { BackupIndicator } from "./backup-indicator"
import { BridgesProvider, type BridgesClient, type FederationMap, type IncidentView } from "./bridges-client"
import { agoWords, backupLine, siteHost } from "./bridges-model"
import { ClockBanner } from "./clock"
import { EmergencySheetButton } from "./emergency-sheet"
import { FederationSection } from "./federation"
import { FillFixPanel } from "./fill-fix"
import { IncidentsSection } from "./incidents"
import { GuideButton } from "./setup-guide"
import { fakeBridges } from "./fake-bridges"

const NOW = Math.floor(Date.now() / 1000)

function mount(ui: ReactNode, bridges: BridgesClient | null, over: Partial<Client> = {}) {
  const client = fakeClient({
    getSettings: () => Promise.resolve(DEFAULT_SETTINGS),
    approvalRoute: () => Promise.resolve({ route: "window", method: "this window" }),
    ...over,
  })
  return render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <GateProvider>
          <BridgesProvider value={bridges}>{ui}</BridgesProvider>
        </GateProvider>
      </SettingsProvider>
    </ClientProvider>,
  )
}

describe("the bridges views", () => {
  it("say nothing where the host has no bridges client (the web vault)", () => {
    const { container } = mount(
      <>
        <ClockBanner />
        <FederationSection onOpenItem={vi.fn()} onChanged={vi.fn()} onNotice={vi.fn()} />
        <IncidentsSection onOpenItem={vi.fn()} onAction={vi.fn()} onNotice={vi.fn()} />
        <AliasField site="https://x.example" onAddress={vi.fn()} />
        <AliasSettings onNotice={vi.fn()} />
        <FillFixPanel itemId="i1" crownJewel={false} canEdit onNotice={vi.fn()} />
        <EmergencySheetButton onNotice={vi.fn()} />
        <GuideButton itemId="i1" onNotice={vi.fn()} />
      </>,
      null,
    )
    expect(container.textContent).toBe("")
  })

  it("the map shows what falls with each provider, what to fix, and asks about what the browser saw", async () => {
    const map: FederationMap = {
      providers: [
        {
          id: "google",
          name: "Google",
          account: { item_id: "g", title: "Google", username: "sean@gmail.com", host: "accounts.google.com", has_password: true, second_step: false, crown_jewel: false },
          candidates: [],
          sites: [{ item_id: "s1", title: "Shop", username: "", host: "shop.example", has_password: false, second_step: false, crown_jewel: false }],
          warnings: ["Your Google account has no two-step sign-in saved in Lockdown."],
        },
      ],
      pending: [{ id: 7, provider: "apple", provider_name: "Apple", host: "news.example", logins: [], confirmed: false }],
    }
    const federationAccept = vi.fn((_p: number, _i: string | null) => Promise.resolve("new"))
    const bridges = fakeBridges({ federationMap: () => Promise.resolve(map), federationAccept })
    const onOpenItem = vi.fn()
    mount(<FederationSection onOpenItem={onOpenItem} onChanged={vi.fn()} onNotice={vi.fn()} />, bridges)
    const google = await screen.findByRole("region", { name: "Google" })
    expect(within(google).getByText("1 site")).toBeInTheDocument()
    expect(within(google).getByText(/no two-step sign-in/)).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(within(google).getByRole("button", { name: /Shop · shop.example · no password of its own/ }))
    expect(onOpenItem).toHaveBeenCalledWith("s1")
    expect(screen.getByText("Did you sign in to news.example with Apple?")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Yes, save a login" }))
    expect(federationAccept).toHaveBeenCalledWith(7, null)
  })

  it("a guided flow asks what it is about, then ticks steps and items off", async () => {
    const steps = (done: boolean): IncidentView["steps"] => [
      { id: "change", title: "Change the password on that site", detail: "Use Change password.", action: "change_passwords", done, items: [{ item_id: "i1", title: "Shop", host: "shop.example", done: false }] },
      { id: "verify", title: "Check it worked", detail: "Choose Check now.", action: "check", done: false, items: [] },
    ]
    const view = (picked: string | null, done = false): IncidentView => ({
      id: "site-breach",
      title: "A site I use was breached",
      when: "The site told you.",
      scope: "item",
      picked,
      choices: [{ value: "i1", label: "Shop (shop.example)" }],
      started_at: picked ? NOW : null,
      steps: steps(done),
      finished: false,
    })
    const incident = vi.fn((_id: string, pick?: string | null) => Promise.resolve(view(pick ?? null)))
    const incidentMark = vi.fn((_id: string, _s: string, _i: string | null, done: boolean) => Promise.resolve(view("i1", done)))
    const bridges = fakeBridges({
      incidents: () => Promise.resolve([{ id: "site-breach", title: "A site I use was breached", when: "The site told you.", scope: "item", started_at: null, steps_done: 0, steps: 2 }]),
      incident,
      incidentMark,
    })
    const onAction = vi.fn()
    mount(<IncidentsSection onOpenItem={vi.fn()} onAction={onAction} onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Start" }))
    const dialog = await screen.findByRole("dialog")
    const first = within(dialog).getByRole("checkbox", { name: "Done: Change the password on that site" })
    expect(first.getAttribute("aria-disabled") === "true" || first.hasAttribute("disabled") || first.hasAttribute("data-disabled")).toBe(true)
    await user.selectOptions(within(dialog).getByLabelText("Which site"), "i1")
    await user.click(within(dialog).getByRole("button", { name: "Start" }))
    expect(incident).toHaveBeenLastCalledWith("site-breach", "i1")
    await user.click(await within(dialog).findByRole("checkbox", { name: "Done: Change the password on that site" }))
    expect(incidentMark).toHaveBeenCalledWith("site-breach", "change", null, true)
    await user.click(within(dialog).getByRole("button", { name: "Check now" }))
    expect(onAction).toHaveBeenCalledWith("check")
  })

  it("an alias comes through the gate into the user name", async () => {
    const aliasGenerate = vi.fn((_item: string, _site: string, gate?: GateInput) =>
      gate?.confirmed
        ? Promise.resolve({ address: "shop.x1@simplelogin.com", service: "simple_login", service_name: "SimpleLogin", monitoring_problem: null })
        : Promise.reject({ code: "approval_needed", message: "confirm in this window to use your alias service's key", detail: "window" }),
    )
    const bridges = fakeBridges({
      aliasOverview: () =>
        Promise.resolve({
          services: [],
          configured: [{ item_id: "k1", title: "SimpleLogin API key", service: "simple_login", service_name: "SimpleLogin", domain: null, base: null, made: 0 }],
        }),
      aliasGenerate,
    })
    const onAddress = vi.fn()
    mount(<AliasField site="https://Shop.example/signup" onAddress={onAddress} />, bridges)
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "New alias from SimpleLogin" }))
    const confirm = await screen.findByRole("button", { name: /confirm/i })
    await user.click(confirm)
    await waitFor(() => expect(onAddress).toHaveBeenCalledWith("shop.x1@simplelogin.com"))
    expect(aliasGenerate).toHaveBeenLastCalledWith("k1", "shop.example", expect.objectContaining({ confirmed: true }))
  })

  it("settings take an alias service's key as bytes and switch monitoring on", async () => {
    const aliasServiceAdd = vi.fn((_input: { service: string; key: number[] }) => Promise.resolve("k1"))
    const aliasMonitoringSet = vi.fn((on: boolean) => Promise.resolve({ enabled: on, addresses: ["a@duck.com"], has_account: true }))
    const bridges = fakeBridges({
      aliasOverview: () =>
        Promise.resolve({
          services: [{ id: "duck_duck_go", name: "DuckDuckGo", key_hint: "Copy the token.", needs_domain: false, takes_domain: false, self_hostable: false }],
          configured: [],
        }),
      aliasMonitoring: () => Promise.resolve({ enabled: false, addresses: ["a@duck.com"], has_account: true }),
      aliasServiceAdd,
      aliasMonitoringSet,
    })
    mount(<AliasSettings onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("API key"), "tok-123")
    await user.click(screen.getByRole("button", { name: "Set it up" }))
    await waitFor(() => expect(aliasServiceAdd).toHaveBeenCalled())
    const sent = aliasServiceAdd.mock.calls[0]?.[0]
    expect(sent?.service).toBe("duck_duck_go")
    // The key went as bytes, and those bytes were wiped after the call.
    expect(sent?.key.every((b) => b === 0)).toBe(true)
    expect((screen.getByLabelText("API key") as HTMLInputElement).value).toBe("")
    await user.click(screen.getByRole("switch", { name: "Watch my aliases for data breaches" }))
    expect(aliasMonitoringSet).toHaveBeenCalledWith(true)
  })

  it("a login's fill fix is set and shown in words", async () => {
    const siteFixSet = vi.fn((_id: string, fix: { never: boolean; fields: unknown[] }) => Promise.resolve(fix as never))
    const bridges = fakeBridges({ siteFix: () => Promise.resolve({ never: false, fields: [] }), siteFixSet })
    mount(<FillFixPanel itemId="i1" crownJewel={false} canEdit onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText("Field name on the page"), "login_id")
    await user.click(screen.getByRole("button", { name: "Add fix" }))
    expect(siteFixSet).toHaveBeenCalledWith("i1", { never: false, fields: [{ match: { name: "login_id" }, role: "username" }] })
    expect(await screen.findByText('The field "login_id" is the user name')).toBeInTheDocument()
    await user.click(screen.getByRole("switch", { name: "Never fill on this site" }))
    expect(siteFixSet).toHaveBeenLastCalledWith("i1", expect.objectContaining({ never: true }))
  })

  it("the clock line shows only when the clocks disagree", async () => {
    const clock = { checked_at: NOW, offset_secs: 45, verdict: "corrected" as const, correcting: true, words: "This computer's clock is 45 seconds behind your server's." }
    mount(<ClockBanner />, fakeBridges({ clockStatus: () => Promise.resolve(null), clockCheck: () => Promise.resolve(clock) }))
    expect(await screen.findByText(/45 seconds behind/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Clock settings" })).toBeInTheDocument()
  })

  it("the emergency sheet opens as a page to print", async () => {
    const bridges = fakeBridges({
      emergencySheet: () =>
        Promise.resolve({
          made_at: NOW,
          title: "Lockdown emergency sheet",
          intro: "Keep it with your recovery kit.",
          sections: [{ heading: "Your account", lines: ["Your Lockdown account is me@example.com."] }],
          blanks: ["Where I keep my recovery kit:"],
        }),
    })
    mount(<EmergencySheetButton onNotice={vi.fn()} />, bridges)
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Emergency sheet" }))
    const sheet = await screen.findByRole("article", { name: "Emergency sheet" })
    expect(within(sheet).getByText("Your account")).toBeInTheDocument()
    expect(within(sheet).getByText("Where I keep my recovery kit:")).toBeInTheDocument()
    const print = vi.spyOn(window, "print").mockImplementation(() => {})
    await user.click(screen.getByRole("button", { name: "Print" }))
    expect(print).toHaveBeenCalled()
  })

  it("the backup line outside Settings says when the last backup ran", async () => {
    const status: BackupStatus = {
      enabled: true,
      folder: "/backups",
      interval_hours: 24,
      keep_last: 10,
      key_set: true,
      last_success_at: NOW - 3 * 3600,
      last_error: null,
      next_due_at: null,
      crown_jewels_left_out: 0,
      backups: [],
    }
    const onOpen = vi.fn()
    mount(<BackupIndicator onOpen={onOpen} />, null, { backupStatus: () => Promise.resolve(status) })
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "Backed up 3 hours ago. Open backup settings" }))
    expect(onOpen).toHaveBeenCalled()
    expect(backupLine({ ...status, last_success_at: NOW - 3 * 86400 }, NOW)).toEqual({ text: "Backed up 3 days ago", late: true })
    expect(backupLine({ ...status, enabled: false }, NOW).text).toBe("Backups are off")
    expect(agoWords(30)).toBe("just now")
    expect(siteHost("shop.example/x")).toBe("shop.example")
    expect(siteHost("")).toBe("")
  })
})

describe("the export dialog's age choice", () => {
  it("locks the export to the public keys pasted, through the gate", async () => {
    const { ExportDialog } = await import("@/features/export/export-dialog")
    const exportChoose = vi.fn(() => Promise.resolve({ token: 9, path: "/tmp/out", name: "out" }))
    const exportSaveAge = vi.fn((_input: { target: number; recipients: string[] }, gate?: GateInput) =>
      gate?.confirmed
        ? Promise.resolve({
            path: "/tmp/out.lockdown.age",
            bytes: 10,
            preview: { format: "lockdown" as const, items: 1, attachments: 0, crown_jewels_included: 0, crown_jewels_left_out: 0, left_out: [], dropped: [], encrypted: true },
          })
        : Promise.reject({ code: "approval_needed", detail: "window", message: "confirm in this window to export the vault" }),
    )
    mount(<ExportDialog open onClose={() => {}} onNotice={vi.fn()} />, fakeBridges({ exportSaveAge }), {
      exportPreview: () =>
        Promise.resolve({ format: "lockdown", items: 1, attachments: 0, crown_jewels_included: 0, crown_jewels_left_out: 0, left_out: [], dropped: [], encrypted: false }),
      exportChoose,
    })
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText("Kind of file"), "lockdown-age")
    await user.click(await screen.findByRole("button", { name: "Choose where to save…" }))
    expect(await screen.findByText(/Paste the age public key/)).toBeInTheDocument()
    await user.type(screen.getByLabelText("The public keys it opens with"), "age1aaa\nage1bbb")
    await user.click(screen.getByRole("button", { name: "Choose where to save…" }))
    await user.click(await screen.findByRole("button", { name: /confirm/i }))
    await waitFor(() => expect(exportSaveAge).toHaveBeenLastCalledWith({ target: 9, recipients: ["age1aaa", "age1bbb"], vault: null }, expect.objectContaining({ confirmed: true })))
    expect(exportChoose).toHaveBeenCalledWith("age", true)
  })
})
