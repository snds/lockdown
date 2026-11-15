import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider } from "@/core/client"
import type { Settings } from "@/core/types"
import { fakeClient } from "@/test/fake-client"

import { breachListWords, SecuritySettings } from "./security-settings"

/* Settings → Security checks (brief T9, doc 80): three settings, saved at once and silently. */

function mount(start: Partial<Settings> = {}, refuse = false) {
  const saved: Settings[] = []
  let current: Settings = { ...DEFAULT_SETTINGS, ...start }
  const client = fakeClient({
    getSettings: () => Promise.resolve(current),
    setSettings: (next) => {
      if (refuse) return Promise.reject({ code: "io", message: "Couldn't write settings.cbor: the disk is full." })
      saved.push(next)
      current = next
      return Promise.resolve(next)
    },
  })
  const onSaved = vi.fn()
  render(
    <ClientProvider value={client}>
      <SettingsProvider>
        <SecuritySettings onSaved={onSaved} />
      </SettingsProvider>
    </ClientProvider>,
  )
  return { saved, onSaved }
}

describe("Settings → Security checks (brief T9)", () => {
  it("turns each check off at once, with nothing said", async () => {
    const m = mount()
    const user = userEvent.setup()
    await user.click(await screen.findByRole("switch", { name: "Check my passwords against known data breaches" }))
    await waitFor(() => expect(m.saved.at(-1)?.breach_check).toBe(false))
    await user.click(screen.getByRole("switch", { name: "Find each site's change-password page before opening it" }))
    await waitFor(() => expect(m.saved.at(-1)?.change_password_check).toBe(false))
    expect(m.onSaved).toHaveBeenCalledTimes(2)
    expect(screen.queryByRole("alert")).toBeNull()
  })

  it("names the breach list and changes it in a form with its own Save; empty is the public list", async () => {
    const m = mount()
    const user = userEvent.setup()
    const row = await screen.findByRole("group", { name: "Breach list" })
    expect(row).toHaveTextContent("The public Pwned Passwords list")
    await user.click(within(row).getByRole("button", { name: /Change/ }))
    const form = await screen.findByRole("dialog")
    await user.type(within(form).getByLabelText("Breach list address"), "https://hibp.corp.example/range/")
    await user.click(within(form).getByRole("button", { name: "Save list" }))
    await waitFor(() => expect(m.saved.at(-1)?.breach_endpoint).toBe("https://hibp.corp.example/range/"))
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull())
    expect(screen.getByRole("group", { name: "Breach list" })).toHaveTextContent("hibp.corp.example")
    expect(breachListWords(null)).toBe("The public Pwned Passwords list")
  })

  it("the breach list is disabled with its reason while the check is off (#37)", async () => {
    mount({ breach_check: false })
    const row = await screen.findByRole("group", { name: "Breach list" })
    expect(row).toHaveTextContent("The breach check is off.")
    expect(within(row).getByRole("button", { name: /Change/ })).toHaveAttribute("aria-disabled", "true")
  })

  it("a save the core refuses keeps the switch where it was, with the core's words under it", async () => {
    mount({}, true)
    const user = userEvent.setup()
    const toggle = await screen.findByRole("switch", { name: "Check my passwords against known data breaches" })
    await user.click(toggle)
    expect(await screen.findByRole("alert")).toHaveTextContent("the disk is full")
    expect(toggle).toBeChecked()
  })
})
