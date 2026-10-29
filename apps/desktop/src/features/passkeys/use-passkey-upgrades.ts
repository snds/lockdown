import { useEffect } from "react"

import { dateLocal } from "@/lib/format"

import { usePasskeyClient, type PasskeyUpgrade } from "./passkey-client"

/** How often the app asks the core for automatic upgrades it made. */
export const UPGRADE_POLL_MS = 2000

/** The item's line after an automatic upgrade, until it is opened once. */
export function upgradeLine(u: { site: string; at: number }): string {
  return `Added by ${u.site} on ${dateLocal(u.at)}, after a password sign-in.`
}

/** The toast's words for one automatic upgrade (D210, D312 as changed by D375). */
export function upgradeToast(u: PasskeyUpgrade): string {
  return `${u.site} added a passkey to this login`
}

/**
 * While the vault is open: one toast for each passkey a site added by
 * itself after a password sign-in Lockdown saw (the automatic upgrade,
 * docs/plan/34-small-calls.md §1), named by its login, with "Show"
 * opening that login. The browser stayed quiet at the moment; this is
 * where the person hears of it. The core hands each one out once; the
 * item keeps its own line until it is opened.
 */
export function usePasskeyUpgrades(open: boolean, notify: (u: PasskeyUpgrade) => void) {
  const client = usePasskeyClient()
  useEffect(() => {
    if (!open) return
    let live = true
    const look = () => {
      client
        .upgrades()
        .then((list) => {
          if (live) for (const u of Array.isArray(list) ? list : []) notify(u)
        })
        .catch(() => {})
    }
    look()
    const t = setInterval(look, UPGRADE_POLL_MS)
    return () => {
      live = false
      clearInterval(t)
    }
  }, [client, open, notify])
}
