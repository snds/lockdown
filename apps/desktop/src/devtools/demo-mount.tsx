import { useCallback, useEffect, useState } from "react"
import { toast } from "sonner"

import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { setDemoEntry } from "@/features/unlock/demo-entry"

import { DemoBar } from "./demo-bar"
import { type DemoOpened, type DemoState, useDevtoolsClient } from "./devtools-client"
import { MARKER } from "./marker"
import { tauriDevtoolsClient } from "./tauri-devtools"

export interface DemoMountProps {
  /** The demo opened (or was built again): show its vault. */
  onOpened: (status: Status) => void
  /** The demo closed: the app is back where it was, locked or at first run. */
  onClosed: () => void
}

/**
 * The demo's one mount point (feature `demo-vault`, docs/plan/40-devtools.md
 * §3). It draws the bar while the demo is open, puts "Explore the demo" on
 * the first run, and answers the Developer menu's demo items. Leaving goes
 * back to where the person came from: the first run or their locked vault.
 */
export default function DemoMount({ onOpened, onClosed }: DemoMountProps) {
  const client = useDevtoolsClient(tauriDevtoolsClient)
  const [demo, setDemo] = useState<DemoState | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let live = true
    client
      .status()
      .then((s) => live && s?.marker === MARKER && setDemo(s.demo ?? null))
      .catch(() => live && setDemo(null))
    return () => {
      live = false
    }
  }, [client])

  const opened = useCallback(
    (o: DemoOpened) => {
      setDemo({ open: true, came_from: o.came_from, pack: o.loaded.pack, items: o.loaded.items })
      onOpened(o.status)
    },
    [onOpened],
  )

  const act = useCallback(
    async (what: () => Promise<DemoOpened | null>, done?: string) => {
      setBusy(true)
      try {
        const o = await what()
        if (o) {
          opened(o)
          if (done) toast(done)
        }
      } catch (e) {
        toast.error(describeError(e))
      } finally {
        setBusy(false)
      }
    },
    [opened],
  )

  const explore = useCallback(() => void act(() => client.demoOpen()), [act, client])

  // The first run's way in, while the demo is built in and closed.
  const available = demo !== null
  const open = demo?.open ?? false
  useEffect(() => {
    if (!available || open) return
    setDemoEntry({ explore, busy })
    return () => setDemoEntry(null)
  }, [available, open, explore, busy])

  useEffect(
    () =>
      client.onMenu((action) => {
        if (action === "demo_open") explore()
        else if (action === "demo_reset") void act(() => client.demoReset(), "The demo was built again from its pack.")
        else if (action === "demo_load") void act(() => client.demoLoadPack(), "The demo now holds the pack you picked.")
      }),
    [client, act, explore],
  )

  const leave = async () => {
    setBusy(true)
    try {
      await client.demoLeave()
      setDemo((d) => (d ? { open: false, items: 0 } : d))
      onClosed()
    } catch (e) {
      toast.error(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  if (!open) return null
  return <DemoBar pack={demo?.pack} leaving={busy} onLeave={() => void leave()} />
}
