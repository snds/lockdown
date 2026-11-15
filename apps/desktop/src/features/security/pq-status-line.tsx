import { useEffect, useState } from "react"

import { Button, FieldRow, FieldRowValue, Icon, type IconName } from "@unicron/ui"

import { useLive } from "@/features/sessions/use-live"

import { usePqStatusClient, type PqStatus } from "./pq-status-client"

/** The icon carries the state with the words; colour never does it alone. */
const ICONS: Record<PqStatus["state"], IconName> = {
  on: "security",
  waiting: "hourglass",
  unknown: "help",
}

interface PqStatusLineProps {
  /** Opens a device on the Devices screen. Without it, devices are named but not linked. */
  onDevice?: (id: string) => void
  /** Bumped by the screen to read the line again (after "Check now", a sync). */
  refresh?: number
}

/**
 * Where the stronger, quantum-safe signatures stand for this account, as a
 * row of Health's Protections
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
  // A row of Health's Protections (brief H1): the core's headline as the value, its explanation under
  // it, and each device it waits for a text link to that device in Settings.
  return (
    <div role="status" aria-label="Quantum-safe signatures" data-state={status.state}>
      <FieldRow
        label="Quantum-safe signatures"
        description={
          <>
            {status.explanation}
            {status.state === "waiting" && onDevice && waiting.length > 0 && (
              <span role="list" aria-label="Devices to update" className="mt-1 flex flex-wrap gap-x-3">
                {waiting.map((d) => (
                  <span role="listitem" key={d.id}>
                    <Button size="inline" variant="link" onClick={() => onDevice(d.id)}>
                      {d.name}
                    </Button>
                  </span>
                ))}
              </span>
            )}
          </>
        }
      >
        <FieldRowValue truncate="wrap">
          <span className="inline-flex items-baseline gap-2">
            <Icon name={ICONS[status.state]} />
            {status.headline}
          </span>
        </FieldRowValue>
      </FieldRow>
    </div>
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
