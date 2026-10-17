import {
  ApprovalSheet,
  type ApprovalContextRow,
  type ApprovalDenyReason,
  type ApprovalKind,
  type ApprovalOrigin,
} from "@unicron/ui/security/approval-sheet"

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
  /** Unix seconds; the sheet denies on its own when it passes. */
  expires_at?: number
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
 * The seam for approvals on desktop (brief §3.2 Approvals; D99, D101).
 * unicron's ApprovalSheet does the number matching, the arming delay, the
 * timeout and the origin rule; this host supplies the signed request's
 * facts and routes the decision. Nothing in `lockdown-app` produces a
 * pending request yet: joining a device goes through the account
 * (`account_login`), and no agent or bridge asks anything of this device.
 * When the core gains `approvals()` / `approve_request` / `deny_request`,
 * the Devices screen mounts this for each request; until then the
 * component is wired and tested here and drawn nowhere.
 */
export function DeviceApproval({ request, busy, error, onApprove, onDeny, onReport, onClosed }: DeviceApprovalProps) {
  return (
    <ApprovalSheet
      kind={request.kind}
      origin={request.origin}
      context={request.context}
      matching={{ mode: "enter", digits: request.digits, expected: request.expected }}
      expiresAt={request.expires_at === undefined ? undefined : request.expires_at * 1000}
      busy={busy}
      error={error}
      onApprove={() => onApprove(request.id)}
      onDeny={(reason) => onDeny(request.id, reason)}
      onReport={() => onReport(request.id)}
      onOpenChange={(open) => !open && onClosed?.()}
    />
  )
}
