import { render, waitFor } from "@testing-library/react"

import { PasskeyClientProvider, type PasskeyClient, type PasskeyUpgrade } from "./passkey-client"
import { upgradeToast, usePasskeyUpgrades } from "./use-passkey-upgrades"

const github: PasskeyUpgrade = { item_id: "i1", title: "GitHub", site: "github.com", at: 1_761_700_000 }

function Watch({ open, notify }: { open: boolean; notify: (u: PasskeyUpgrade) => void }) {
  usePasskeyUpgrades(open, notify)
  return null
}

function client(upgrades: () => Promise<PasskeyUpgrade[]>): PasskeyClient {
  return {
    list: () => Promise.resolve([]),
    remove: () => Promise.resolve(0),
    restore: () => Promise.resolve(0),
    pending: () => Promise.resolve([]),
    answer: () => Promise.resolve(),
    upgrades,
    upgradeSeen: () => Promise.resolve(0),
  }
}

describe("the notice after a site's automatic passkey upgrade", () => {
  it("names the site in the toast's words", () => {
    expect(upgradeToast(github)).toBe("github.com added a passkey to this login")
  })

  it("tells each upgrade the core hands out, once, while the vault is open", async () => {
    let queue = [github]
    const upgrades = vi.fn(() => {
      const out = queue
      queue = []
      return Promise.resolve(out)
    })
    const notify = vi.fn()
    const { rerender } = render(
      <PasskeyClientProvider value={client(upgrades)}>
        <Watch open notify={notify} />
      </PasskeyClientProvider>,
    )
    await waitFor(() => expect(notify).toHaveBeenCalledWith(github))
    expect(notify).toHaveBeenCalledTimes(1)
    // Locked: no more asking.
    upgrades.mockClear()
    rerender(
      <PasskeyClientProvider value={client(upgrades)}>
        <Watch open={false} notify={notify} />
      </PasskeyClientProvider>,
    )
    expect(upgrades).not.toHaveBeenCalled()
  })

  it("a core that can't answer is no toast and no error", async () => {
    const notify = vi.fn()
    const upgrades = vi.fn(() => Promise.reject(new Error("locked")))
    render(
      <PasskeyClientProvider value={client(upgrades)}>
        <Watch open notify={notify} />
      </PasskeyClientProvider>,
    )
    await waitFor(() => expect(upgrades).toHaveBeenCalled())
    expect(notify).not.toHaveBeenCalled()
  })
})
