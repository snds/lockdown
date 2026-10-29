import { useEffect, useState } from "react"

import { Button, Icon, Item, ItemContent, ItemDescription, ItemMedia, ItemTitle, type IconName } from "@unicron/ui"

import { useLive } from "@/features/sessions/use-live"

import { usePqStatusClient, type PqStatus } from "./pq-status-client"

/** The icon carries the state with the words; colour never does it alone. */
const ICONS: Record<PqStatus["state"], IconName> = {
  on: "verified_user",
  waiting: "hourglass_top",
  unknown: "help",
}

interface PqStatusLineProps {
  /** Opens a device on the Devices screen. Without it, devices are named but not linked. */
  onDevice?: (id: string) => void
  /** Bumped by the screen to read the line again (after "Check now", a sync). */
  refresh?: number
}

/**
 * Where the stronger, quantum-safe signatures stand for this account
 * (D346; docs/plan/30-pq-status-line.md): on, waiting for the devices
 * named (each a link to the Devices screen), or can't check right now.
 * The words are the core's, read again after every live sync. A failed
 * call reads as "can't check", never as on.
 */
export function PqStatusLine({ onDevice, refresh = 0 }: PqStatusLineProps) {
  const client = usePqStatusClient()
  const [status, setStatus] = useState<PqStatus | null>(null)
  // A sync another device's change set off may have moved the rollout.
  const [synced, setSynced] = useState(0)
  useLive((n) => {
    if (n.kind === "synced") setSynced((k) => k + 1)
  })

  useEffect(() => {
    if (!client) return
    let live = true
    client
      .status()
      .then((s) => live && setStatus(s))
      .catch(() => live && setStatus(UNREACHABLE))
    return () => {
      live = false
    }
  }, [client, refresh, synced])

  if (!client || !status) return null
  const waiting = [...status.without_level_1, ...status.without_level_2]
  return (
    <Item variant="outline" size="sm" role="status" aria-label="Quantum-safe signatures" data-state={status.state}>
      <ItemMedia variant="icon">
        <Icon name={ICONS[status.state]} />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>{status.headline}</ItemTitle>
        <ItemDescription>{status.explanation}</ItemDescription>
        {status.state === "waiting" && onDevice && waiting.length > 0 && (
          <ul aria-label="Devices to update" className="flex flex-wrap gap-1">
            {waiting.map((d) => (
              <li key={d.id}>
                <Button size="sm" variant="link" onClick={() => onDevice(d.id)}>
                  <Icon name="devices" data-icon="inline-start" />
                  {d.name}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </ItemContent>
    </Item>
  )
}

/** What the line says when the call itself failed. */
const UNREACHABLE: PqStatus = {
  state: "unknown",
  reason: "unreached",
  headline: "Quantum-safe signatures: can't check right now",
  explanation: "Lockdown couldn't read where they stand. It tries again when you come back to this screen.",
  without_level_1: [],
  without_level_2: [],
}
