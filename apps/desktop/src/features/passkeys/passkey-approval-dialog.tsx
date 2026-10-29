import { useCallback, useEffect, useState } from "react"

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"

import { usePasskeyClient, type PasskeyApproval } from "./passkey-client"

/** How often the app looks for a passkey request waiting for the person. */
const POLL_MS = 1500

/**
 * A site that requires Lockdown to check it's you, on a computer whose
 * `Approve` gate is the master password (docs/plan/19-passkeys.md §5):
 * the browser waits while this dialog asks, here in the app and never in
 * the page. Approving runs the gate; declining, or two minutes with no
 * answer, tells the site no.
 */
export function PasskeyApprovalDialog({ onNotice }: { onNotice: (text: string) => void }) {
  const client = usePasskeyClient()
  const gate = useGate()
  const [request, setRequest] = useState<PasskeyApproval | null>(null)
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
      onNotice(approve ? `Approved: ${request.text}.` : `Declined: ${request.text}.`)
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
          <DialogTitle>{request ? `${request.text}?` : "A passkey request"}</DialogTitle>
          <DialogDescription>
            {request?.browser ?? "A browser"} is asking to use your passkey, and the site wants Lockdown to check it's you. Approve only if you just
            started this in your browser.
          </DialogDescription>
        </DialogHeader>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <LabelledActions>
          <LabelledAction kind="cancel" onClick={() => void answer(false)} disabled={busy}>
            Decline
          </LabelledAction>
          <LabelledAction kind="confirm" onClick={() => void answer(true)} disabled={busy}>
            Approve
          </LabelledAction>
        </LabelledActions>
      </DialogContent>
    </Dialog>
  )
}
