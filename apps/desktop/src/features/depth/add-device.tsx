import { useCallback, useEffect, useRef, useState } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, FieldError, Icon, Spinner } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"

import { POLL_MS, useDepthClient, type LinkView } from "./depth-client"
import { QrCode } from "./qr-code"

interface AddDeviceProps {
  onNotice: (text: string) => void
  onChanged: () => void
}

/**
 * "Add a device" (docs/plan/22-security-depth.md §2): this device opens a
 * link, the new one joins it with the code or the QR code, both show six
 * digits, and approving here goes through the Approve gate. The root key
 * and, when this device keeps it, the Secret Key go over the handshake;
 * the server relays frames it can't read.
 */
export function AddDevice({ onNotice, onChanged }: AddDeviceProps) {
  const depth = useDepthClient()
  const gate = useGate()
  const [link, setLink] = useState<LinkView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const open = link !== null
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)

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
    setBusy(true)
    try {
      setLink(await depth.linkOffer())
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const close = () => {
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
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <Button variant="outline" onClick={start} disabled={busy}>
        <Icon name="devices" data-icon="inline-start" />
        Add a device
      </Button>
      {error && !open && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Dialog open={open} onOpenChange={(o) => !o && close()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{link?.state === "compare" ? "Do the digits match?" : "Add a device"}</DialogTitle>
            <DialogDescription>
              {link?.state === "compare"
                ? `Check that ${link.device_name ?? "the new device"} shows the same six digits. Approve only if they match.`
                : "On the new device, choose “Join from another device” and scan this code, or type the code under it. It works once, for ten minutes."}
            </DialogDescription>
          </DialogHeader>
          {link?.state === "waiting" && (
            <div className="flex flex-col items-center gap-3">
              <QrCode rows={link.qr_rows ?? []} label="Device link QR code" />
              <p className="font-mono text-2xl tracking-widest" aria-label="Device code">
                {link.code}
              </p>
              <Spinner />
            </div>
          )}
          {link?.state === "compare" && (
            <p className="text-center font-mono text-4xl tracking-[0.3em]" aria-label="Digits to compare">
              {link.sas}
            </p>
          )}
          {error && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={close} disabled={busy}>
              {link?.state === "compare" ? "They don't match" : "Cancel"}
            </LabelledAction>
            {link?.state === "compare" && (
              <LabelledAction kind="confirm" onClick={approve} disabled={busy}>
                Approve
              </LabelledAction>
            )}
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </>
  )
}
