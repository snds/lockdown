import { useCallback, useEffect, useState } from "react"

import { Dialog, DialogAction, DialogActions, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"

import { spaced, useBrowserClient, type PendingPairing } from "./browser-client"

/** How often the app looks for a browser asking to pair, while unlocked. */
const POLL_MS = 1500

/**
 * A browser asking to pair (docs/plan/16-browser-extension.md §3): the app
 * shows the browser's name and the code the extension shows, and the person
 * approves only when the two match. Approving goes through the `Approve`
 * gate; a request nobody answers expires in two minutes.
 */
export function PairingDialog({ onNotice }: { onNotice: (text: string) => void }) {
  const client = useBrowserClient()
  const gate = useGate()
  const [request, setRequest] = useState<PendingPairing | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const look = useCallback(() => {
    client
      .pending()
      .then((list) => setRequest((cur) => (cur && list.some((p) => p.request_id === cur.request_id) ? cur : (list[0] ?? null))))
      .catch(() => setRequest(null))
  }, [client])

  useEffect(() => {
    look()
    const t = setInterval(look, POLL_MS)
    return () => clearInterval(t)
  }, [look])

  const answer = async (approve: boolean) => {
    if (!request) return
    setBusy(true)
    setError(null)
    try {
      await gate.run((g) => client.answer(request.request_id, approve, g))
      onNotice(approve ? `${request.name} is paired with Lockdown.` : `${request.name} was not paired.`)
      setRequest(null)
      look()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && !busy && void answer(false)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Connect {request?.name ?? "a browser"} to Lockdown?</DialogTitle>
          <DialogDescription>
            Approve only if the Lockdown extension in your browser shows this code: <strong>{request ? spaced(request.code) : ""}</strong>. A paired
            browser can ask to fill the sign-ins, codes and cards on this computer; it never sees your vault.
          </DialogDescription>
        </DialogHeader>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogActions>
          <DialogAction kind="cancel" onClick={() => void answer(false)} disabled={busy}>
            The codes differ
          </DialogAction>
          <DialogAction kind="confirm" onClick={() => void answer(true)} disabled={busy}>
            Pair
          </DialogAction>
        </DialogActions>
      </DialogContent>
    </Dialog>
  )
}
