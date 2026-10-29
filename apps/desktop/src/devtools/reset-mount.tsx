import { useEffect, useState } from "react"

import { useDevtoolsClient } from "./devtools-client"
import { ResetDialog } from "./reset-dialog"
import { tauriDevtoolsClient } from "./tauri-devtools"

/**
 * The developer reset's one mount point (feature `dev-reset`,
 * docs/plan/40-devtools.md §2): it waits for Developer → Reset Lockdown…
 * and shows the confirm dialog. It draws nothing until then, locked or
 * not, at the first run too.
 */
export default function ResetMount() {
  const client = useDevtoolsClient(tauriDevtoolsClient)
  const [open, setOpen] = useState(false)
  const [opening, setOpening] = useState(0)
  useEffect(
    () =>
      client.onMenu((action) => {
        if (action !== "reset") return
        setOpening((n) => n + 1)
        setOpen(true)
      }),
    [client],
  )
  return <ResetDialog key={opening} client={client} open={open} onOpenChange={setOpen} />
}
