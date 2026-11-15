import { useCallback, useState } from "react"

import { ApprovalSheet } from "@unicron/ui/security/approval-sheet"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { RequestSource } from "@/features/approvals/request-source"
import { RAN_OUT_WORDS, WAITING_WORDS, isCancelled, usePolledRequests, useReview, type Waiting } from "@/features/approvals/requests"

import { useBrowserClient, type PendingPairing } from "./browser-client"

/** The sentence over the code (D156). */
const COMPARE = "Approve only if the Lockdown extension in your browser shows this code."
/** What pairing allows, said before the yes (D156; brief A1). */
const NOTE = "A paired browser can ask to fill the sign-ins, codes and cards on this computer. It never sees your vault."

const KIND: Record<PendingPairing["family"], string> = { chromium: "Chromium", firefox: "Firefox" }

/**
 * A browser asking to pair (docs/plan/16-browser-extension.md §3): the app
 * shows the browser's name and the code the extension shows, and the person
 * approves only when the two match. Approving goes through the `Approve`
 * gate; a request nobody answers runs out in two minutes.
 *
 * From doc 84 (brief A1, X3 row 2) the request waits as the needs-you,
 * PAIRING REQUEST // 01:58 LEFT, "{name} wants to pair.", REVIEW and DECLINE;
 * it never opens by itself. REVIEW opens unicron's ApprovalSheet in its
 * compare mode: the BROWSER and KIND rows, the code in DigitGroups, THEY
 * MATCH through the gate or CODES DIFFER. Closing it decides nothing.
 */
export function PairingDialog({ onNotice }: { onNotice: (text: string) => void }) {
  const client = useBrowserClient()
  const gate = useGate()
  const pending = useCallback(() => client.pending(), [client])
  const { waiting, look, forget } = usePolledRequests(pending, (r: PendingPairing) => r.request_id, (r) => r.expires_in_secs)
  // The review holds what it was opened on: a request that runs out while it is open stays on screen as ran out.
  const { open, show, close, closeIf, hidden } = useReview<PendingPairing>()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  // The sheet says when the time ran out (its `timeout`); the sentence follows it, in step with EXPIRED and CLOSE.
  const [ranOut, setRanOut] = useState<string | null>(null)

  const review = (w: Waiting<PendingPairing>) => {
    setError(null)
    show(w)
  }

  const answer = async (w: Waiting<PendingPairing>, approve: boolean) => {
    setBusy(w.id)
    setError(null)
    try {
      await gate.run((g) => client.answer(w.request.request_id, approve, g))
      forget(w.id)
      onNotice(approve ? `Paired ${w.request.name}` : `Declined: ${w.request.name}`)
      closeIf(w.id)
      look()
    } catch (e) {
      if (isCancelled(e)) return
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
          origin="push"
          tier="standard"
          overline="Pairing request"
          title={`Pair ${open.w.request.name}?`}
          context={[
            { label: "Browser", value: open.w.request.name },
            { label: "Kind", value: KIND[open.w.request.family] },
          ]}
          matching={{ mode: "compare", code: open.w.request.code, prompt: COMPARE }}
          expiresAt={open.w.expiresAt}
          note={ranOut === `${open.w.id}:${open.attempt}` ? RAN_OUT_WORDS : busy === open.w.id ? WAITING_WORDS : NOTE}
          denyLabel="Codes differ"
          dismiss="keep"
          busy={busy === open.w.id}
          error={error ?? undefined}
          onApprove={() => void answer(open.w, true)}
          onDeny={(reason) => (reason === "timeout" ? setRanOut(`${open.w.id}:${open.attempt}`) : void answer(open.w, false))}
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
              id: `pairing:${w.id}`,
              priority: 2,
              title: "Pairing request",
              line: `${w.request.name} wants to pair.`,
              action: { label: "Review", onClick: () => review(w) },
              textAction: { label: "Decline", onClick: () => void answer(w, false), busy: busy === w.id },
              shows: "every",
              expiresAt: w.expiresAt,
              expiredWords: "Pairing request ran out",
            }}
          />
        ))}
    </>
  )
}
