import { useCallback, useState } from "react"

import { ApprovalSheet } from "@unicron/ui/security/approval-sheet"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { RequestSource } from "@/features/approvals/request-source"
import { RAN_OUT_WORDS, WAITING_WORDS, isCancelled, useApproveWords, usePolledRequests, useReview, type Waiting } from "@/features/approvals/requests"

import { usePasskeyClient, type PasskeyApproval } from "./passkey-client"

/** The sentence under the review's facts (D208's ask, in one line). */
const NOTE = "Approve only if you just started this sign-in in your browser."

/**
 * A site that requires Lockdown to check it's you, on a computer whose
 * `Approve` gate is the master password (docs/plan/19-passkeys.md §5): the
 * browser waits while the app asks, here and never in the page.
 *
 * From doc 84 (brief A1, X3 row 1) the request waits as the needs-you,
 * SIGN-IN REQUEST // 01:57 LEFT with the request's own words, REVIEW and
 * DECLINE; it never opens by itself. REVIEW opens unicron's ApprovalSheet (a
 * sheet on a phone, a review 560 dialog wider) with the browser's name, the
 * countdown and the yes named by the gate's route. Approving runs the gate;
 * DECLINE answers no at once; closing the review decides nothing, and the
 * request keeps waiting until it runs out (two minutes, D208).
 */
export function PasskeyApprovalDialog({ onNotice }: { onNotice: (text: string) => void }) {
  const client = usePasskeyClient()
  const gate = useGate()
  const yes = useApproveWords()
  const pending = useCallback(() => client.pending(), [client])
  const { waiting, look, forget } = usePolledRequests(pending, (r: PasskeyApproval) => r.request_id, (r) => r.expires_in_secs)
  // The review holds what it was opened on: a request that runs out while it is open stays on screen as ran out.
  const { open, show, close, closeIf, hidden } = useReview<PasskeyApproval>()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  // The sheet says when the time ran out (its `timeout`); the sentence follows it, in step with EXPIRED and CLOSE.
  const [ranOut, setRanOut] = useState<string | null>(null)

  const review = (w: Waiting<PasskeyApproval>) => {
    setError(null)
    show(w)
  }

  const answer = async (w: Waiting<PasskeyApproval>, approve: boolean) => {
    setBusy(w.id)
    setError(null)
    try {
      await gate.run((g) => client.answer(w.request.request_id, approve, g))
      forget(w.id)
      onNotice(approve ? `Approved: ${w.request.text}` : `Declined: ${w.request.text}`)
      closeIf(w.id)
      look()
    } catch (e) {
      // Backing out of the gate returns to the review: nothing was approved, and nothing failed.
      if (isCancelled(e)) return
      // The answer failed: the review shows the core's sentence (opened for it when DECLINE was pressed on the block).
      setError(describeError(e))
      show(w)
    } finally {
      setBusy(null)
    }
  }

  return (
    <>
      {/* The review first: it is what this component draws; the rows only tell the shell what waits. */}
      {open && (
        <ApprovalSheet
          key={`${open.w.id}:${open.attempt}`}
          kind="bridge"
          origin="pairing"
          tier="standard"
          overline="Sign-in request"
          title={open.w.request.text}
          context={[{ label: "Browser", value: open.w.request.browser }]}
          matching={{ mode: "none" }}
          expiresAt={open.w.expiresAt}
          note={ranOut === `${open.w.id}:${open.attempt}` ? RAN_OUT_WORDS : busy === open.w.id ? WAITING_WORDS : NOTE}
          approveLabel={yes.label}
          approveIcon={yes.icon}
          denyLabel="Decline"
          dismiss="keep"
          busy={busy === open.w.id}
          error={error ?? undefined}
          onApprove={() => void answer(open.w, true)}
          // Running out is the core's to say; no answer goes for it.
          onDeny={(reason) => (reason === "timeout" ? setRanOut(`${open.w.id}:${open.attempt}`) : void answer(open.w, false))}
          // The sheet always offers "This wasn't me"; for a passkey request it can only decline (a unicron gap, doc 84 §6).
          onReport={() => void answer(open.w, false)}
          onOpenChange={(o) => !o && close()}
        />
      )}
      {waiting
        .filter((w) => !hidden(w.id))
        .map((w) => (
          <RequestSource
            key={w.id}
            entry={{
              id: `passkey:${w.id}`,
              priority: 1,
              title: "Sign-in request",
              line: w.request.text,
              action: { label: "Review", onClick: () => review(w) },
              textAction: { label: "Decline", onClick: () => void answer(w, false), busy: busy === w.id },
              shows: "every",
              expiresAt: w.expiresAt,
              expiredWords: "Sign-in request ran out",
            }}
          />
        ))}
    </>
  )
}
