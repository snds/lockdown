import { act, render, screen, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"

import { GateProvider } from "@/app/gate"
import { DEFAULT_SETTINGS, SettingsProvider } from "@/app/settings"
import { ClientProvider, type Client } from "@/core/client"
import type { BackupStatus } from "@/core/types"
import { fakeClient } from "@/test/fake-client"
import { useBackupTrouble } from "./backup-indicator"
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

  it("a failed backup is told outside Settings, in the core's words, until it works again (D754)", async () => {
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
    function Trouble() {
      const error = useBackupTrouble(true)
      return <output>{error ?? "fine"}</output>
    }
    const told = async (backup: BackupStatus | Error) => {
      const answer = vi.fn(() => (backup instanceof Error ? Promise.reject(backup) : Promise.resolve(backup)))
      const view = mount(<Trouble />, null, { backupStatus: answer })
      await waitFor(() => expect(answer).toHaveBeenCalled())
      await act(async () => {})
      const text = screen.getByRole("status").textContent
      view.unmount()
      return text
    }
    expect(await told(status)).toBe("fine")
    expect(await told({ ...status, last_error: "The backup folder isn't there." })).toBe("The backup folder isn't there.")
    // Backups that are off are not broken.
    expect(await told({ ...status, enabled: false, last_error: "The backup folder isn't there." })).toBe("fine")
    // A host with no backups (the web vault refuses the call) says nothing.
    expect(await told(new Error("refused"))).toBe("fine")
    // Closed (locked), it says nothing and asks nothing.
    const asked = vi.fn(() => Promise.resolve({ ...status, last_error: "x" }))
    function Closed() {
      return <output>{useBackupTrouble(false) ?? "fine"}</output>
    }
    mount(<Closed />, null, { backupStatus: asked })
    await act(async () => {})
    expect(asked).not.toHaveBeenCalled()
    expect(backupLine({ ...status, last_success_at: NOW - 3 * 86400 }, NOW)).toEqual({ text: "Backed up 3 days ago", late: true })
    expect(backupLine(status, NOW)).toEqual({ text: "Backed up 3 hours ago", late: false })
    expect(backupLine({ ...status, enabled: false }, NOW).text).toBe("Backups are off")
    expect(agoWords(30)).toBe("just now")
    expect(siteHost("shop.example/x")).toBe("shop.example")
    expect(siteHost("")).toBe("")
  })
})
