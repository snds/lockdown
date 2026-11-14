import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { BackupStatus } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { BackupIndicator } from "./backup-indicator"
import { BridgesProvider, type BridgesClient } from "./bridges-client"
import { agoWords, backupLine, siteHost } from "./bridges-model"

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
