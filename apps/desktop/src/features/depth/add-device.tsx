import { useCallback, useEffect, useRef, useState } from "react"

import { AdaptiveDialog, Banner, Button, Progress, Spinner } from "@unicron/ui"
import { ApprovalSheet } from "@unicron/ui/security/approval-sheet"
import { QrCode } from "@unicron/ui/security/qr-code"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import { copiedWords } from "@/features/sharing/link-once"
import { countdown } from "@/lib/format"
import { RAN_OUT_WORDS, isCancelled } from "@/features/approvals/requests"

import { POLL_MS, useDepthClient, type LinkView } from "./depth-client"

interface AddDeviceProps {
  onNotice: (text: string) => void
  onChanged: () => void
}

/** What the new device does with the code (brief T2; the join screen's own words). */
export const ADD_DEVICE_SENTENCE =
  "On the new device, choose “I already use Lockdown”, then scan this code or type the one under it. It works once, for ten minutes."

/** The last seconds of a link, when its rule turns critical (brief A1). */
const CRITICAL_SECS = 20

/** The time left on a link, in seconds, ticking once a second while it shows. */
function useSecondsLeft(expiresAt: number | null): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (expiresAt === null) return
    const t = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(t)
  }, [expiresAt])
  return expiresAt === null ? 0 : Math.max(0, expiresAt - now / 1000)
}

/**
 * Add a device (docs/plan/22-security-depth.md §2; brief T2 and A1, doc 85).
 * ADD DEVICE opens a link: a review dialog (a sheet on a phone) with the
 * QR code and the code to type, a COPY (through the core's clipboard
 * path, Lockdown 1), and a segmented rule for the ten
 * minutes it works. When the new device joins, the dialog becomes the
 * compare review on unicron's ApprovalSheet: NEW DEVICE // MM:SS LEFT,
 * "Add {name}?", the six digits both screens show, THEY MATCH through the
 * `Approve` gate or THEY DON'T MATCH. The root key and, when this device
 * keeps it, the Secret Key go over the handshake; the server relays frames
 * it can't read. Closing either layer before the yes declines the link.
 */
export function AddDevice({ onNotice, onChanged }: AddDeviceProps) {
  const depth = useDepthClient()
  const client = useClient()
  const { settings } = useSettings()
  const gate = useGate()
  const [copied, setCopied] = useState<number | null>(null)
  const [link, setLink] = useState<LinkView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [ranOut, setRanOut] = useState(false)
  const [total, setTotal] = useState(600)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  // Never more than the link was offered with: the clock above may have been read before the offer came.
  const left = Math.min(total, useSecondsLeft(link ? link.expires_at : null))

  const stop = useCallback(() => {
    if (timer.current) clearInterval(timer.current)
    timer.current = null
  }, [])

  useEffect(() => {
    if (!depth || !link || link.state !== "waiting") return stop
    timer.current = setInterval(() => {
      depth
        .linkPoll()
        .then(setLink)
        .catch((e) => {
          stop()
          setError(describeError(e))
        })
    }, POLL_MS)
    return stop
  }, [depth, link, stop])

  if (!depth) return null

  const start = async () => {
    setError(null)
    setRanOut(false)
    setBusy(true)
    try {
      const offered = await depth.linkOffer()
      setTotal(Math.max(1, offered.expires_at - Date.now() / 1000))
      setLink(offered)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  // COPY (Lockdown 1): the core copies the code it made, on the clipboard path every secret takes
  // (cleared on the settings' timer, kept out of clipboard history; D316). Nothing the webview holds is written.
  const copy = async () => {
    setError(null)
    try {
      const c = await client.copyLinkCode(settings.clipboard_clear_secs)
      setCopied(c.clears_in_secs)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const close = () => {
    setCopied(null)
    stop()
    if (link && link.state !== "approved") void depth.linkDecline().catch(() => undefined)
    setLink(null)
    setError(null)
  }

  const approve = async () => {
    setBusy(true)
    setError(null)
    try {
      const lines = await gate.run((g) => depth.linkApprove(g))
      onNotice(lines.join(" "))
      setLink(null)
      onChanged()
    } catch (e) {
      if (!isCancelled(e)) setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const comparing = link?.state === "compare" && !!link.sas
  const offering = link !== null && !comparing
  const closed = link?.state === "declined" || link?.state === "closed"
  const name = link?.device_name ?? "the new device"

  return (
    <>
      <Button variant="secondary" onClick={start} disabled={busy} loading={busy && !link}>
        Add device
      </Button>
      {error && !link && <Banner size="compact" title={error} />}
      <AdaptiveDialog
        open={offering}
        onOpenChange={(o) => !o && close()}
        size="review"
        overline={closed ? "Device link // closed" : `Device link // ${countdown(left)} left`}
        title="Add a device"
        description={closed ? "This link closed before a device joined. Make a new one to try again." : ADD_DEVICE_SENTENCE}
        footer={
          <Button variant="secondary" onClick={close}>
            {closed ? "Close" : "Cancel"}
          </Button>
        }
      >
        {link && !closed && (
          <div className="flex flex-col items-center gap-4">
            <QrCode modules={link.qr_rows ?? []} label="QR code to add a device" />
            <p aria-label="Device code" className="type-code-display text-text-body tabular-nums">
              {link.code}
            </p>
            <Button variant="ghost" onClick={() => void copy()}>
              Copy
            </Button>
            {copied !== null && (
              <p aria-live="polite" className="type-caption text-text-secondary">
                {copiedWords(copied)}
              </p>
            )}
            <div className="w-full">
              <Progress
                variant="segmented"
                segments={30}
                value={left}
                max={total}
                tone={left <= CRITICAL_SECS ? "critical" : "neutral"}
                aria-label={`Time left: ${countdown(left)}`}
              />
            </div>
            <p className="type-caption text-text-secondary inline-flex items-center gap-2">
              <Spinner />
              Waiting for the new device
            </p>
            {error && <Banner size="compact" title={error} />}
          </div>
        )}
      </AdaptiveDialog>
      {comparing && link && (
        <ApprovalSheet
          key={`${link.code}:${link.sas}`}
          kind="device"
          origin="pairing"
          tier="standard"
          overline="New device"
          title={`Add ${name}?`}
          context={[]}
          matching={{ mode: "none" }}
          expiresAt={link.expires_at * 1000}
          note={
            ranOut ? (
              RAN_OUT_WORDS
            ) : (
              <>
                <span className="type-body text-text-body block">Approve only if {name} shows these six digits.</span>
                {/* Plain labelled text, as the joining side shows them (doc 78): DigitGroups has no
                    character between its groups, so the two sides' text would not compare (doc 85, gap 1). */}
                <span aria-label="Digits to compare" className="type-code-display text-text-body block py-3 tabular-nums">
                  {link.sas}
                </span>
              </>
            )
          }
          approveLabel="They match"
          denyLabel="They don't match"
          busy={busy}
          error={error ?? undefined}
          onApprove={() => void approve()}
          onDeny={(reason) => (reason === "timeout" ? setRanOut(true) : close())}
          onReport={close}
          onOpenChange={(o) => !o && !busy && close()}
        />
      )}
    </>
  )
}
