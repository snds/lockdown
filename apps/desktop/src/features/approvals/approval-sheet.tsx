import {
  ApprovalSheet,
  type ApprovalContextRow,
  type ApprovalDenyReason,
  type ApprovalKind,
  type ApprovalOrigin,
} from "@unicron/ui/security/approval-sheet"

import { RequestSource } from "./request-source"
import { requestLine, useReview } from "./requests"

/**
 * A request from elsewhere that this device may approve, as the core will
 * hand it over once it carries them (the enrolment handshake's other
 * side, an agent grant, a scanned device code). Every fact comes from the
 * signed request; nothing here is inferred.
 */
export interface PendingApproval {
  id: string
  kind: ApprovalKind
  /**
   * Where the request came from. `scan` is set by the host only for a
   * device code read from a screen in front of the person (a camera, or a
   * code typed from that screen); a pasted or messaged code is `push`,
   * and the component then renders no approve control at all (brief §6
   * rule 4, D99). This is a fact about the request, never a setting.
   */
  origin: ApprovalOrigin
  /** What · who · where · when. */
  context: ApprovalContextRow[]
  /** The digits the other side shows, from the signed request (never generated here). */
  expected: string
  digits: 2 | 3
  /** Unix seconds; the sheet shows it ran out when it passes. */
  expires_at?: number
  /** The request in its own words, its what and who ("Sam's new phone wants to join"). Default: the kind's question. */
  title?: string
}

interface DeviceApprovalProps {
  request: PendingApproval
  busy?: boolean
  error?: string
  /** Through the `Approve` gate (D101): `gate.run(g => client.approveRequest(id, g))` once the core has it. */
  onApprove: (id: string) => void
  onDeny: (id: string, reason: ApprovalDenyReason) => void
  /** "This wasn't me": deny, revoke the requesting session, log it. */
  onReport: (id: string) => void
  onClosed?: () => void
}

/**
 * The seam for approvals on desktop (brief §3.2 Approvals; D99, D101; brief
 * A1's last row). unicron's ApprovalSheet does the number matching, the
 * arming delay, the timeout and the origin rule; this host supplies the
 * signed request's facts and routes the decision: enter the digits the other
 * side shows, then APPROVE (the host runs the gate), DENY, or THIS WASN'T ME.
 * Closing it decides nothing (doc 84, Q3): the request keeps waiting. Nothing
 * in `lockdown-app` produces a pending request yet: joining a device goes
 * through the account (`account_login`), and no agent or bridge asks
 * anything of this device. When the core gains `approvals()` /
 * `approve_request` / `deny_request`, the shell mounts `DeviceRequests` with
 * them; until then the component is wired and tested here and drawn nowhere.
 */
export function DeviceApproval({ request, busy, error, onApprove, onDeny, onReport, onClosed }: DeviceApprovalProps) {
  return (
    <ApprovalSheet
      kind={request.kind}
      origin={request.origin}
      context={request.context}
      matching={{ mode: "enter", digits: request.digits, expected: request.expected }}
      expiresAt={request.expires_at === undefined ? undefined : request.expires_at * 1000}
      title={request.title}
      busy={busy}
      error={error}
      dismiss="keep"
      onApprove={() => onApprove(request.id)}
      onDeny={(reason) => onDeny(request.id, reason)}
      onReport={() => onReport(request.id)}
      onOpenChange={(open) => !open && onClosed?.()}
    />
  )
}

/**
 * Brief X3 row 3, for when the core sends device requests: each waits as the
 * needs-you, DEVICE REQUEST with its what and who, and REVIEW only (no text
 * action: the no needs the review, where THIS WASN'T ME sits beside it). The
 * review never opens by itself. A running-out request (`expires_at`) counts
 * down in the overline. No component mounts this yet (see `DeviceApproval`).
 */
export function DeviceRequests({ requests, busy, error, onApprove, onDeny, onReport }: Omit<DeviceApprovalProps, "request" | "onClosed"> & { requests: PendingApproval[] }) {
  // One review at a time; its row steps aside while it is open and a moment after (doc 84 §1).
  const { open, show, close, hidden } = useReview<PendingApproval>()
  const reviewing = requests.find((r) => r.id === open?.w.id) ?? null
  return (
    <>
      {requests
        .filter((r) => !hidden(r.id))
        .map((r) => (
          <RequestSource
            key={r.id}
            entry={{
              id: `device:${r.id}`,
              priority: 3,
              title: "Device request",
              line: requestLine(r),
              action: { label: "Review", onClick: () => show({ id: r.id, request: r, expiresAt: r.expires_at === undefined ? Infinity : r.expires_at * 1000 }) },
              shows: "every",
              expiresAt: r.expires_at === undefined ? undefined : r.expires_at * 1000,
              expiredWords: "Device request ran out",
            }}
          />
        ))}
      {reviewing && <DeviceApproval request={reviewing} busy={busy} error={error} onApprove={onApprove} onDeny={onDeny} onReport={onReport} onClosed={close} />}
    </>
  )
}
