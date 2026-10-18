import { useEffect, useRef, useState } from "react"

import { Button, Dialog, DialogAction, DialogActions, DialogContent, DialogDescription, DialogHeader, DialogTitle, Icon } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { useLocked } from "@/app/locked"
import { describeError, useClient } from "@/core/client"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"

import { WIFI_CARD_LIFETIME_S, qrPath } from "./depth-model"

function QrCode({ grid, label }: { grid: Uint8Array; label: string }) {
  const { size, d } = qrPath(grid)
  // Black on white whatever the theme: phone cameras read a dark code on a light ground.
  return (
    <svg role="img" aria-label={label} viewBox={`0 0 ${size} ${size}`} width="288" height="288" shapeRendering="crispEdges">
      <rect width={size} height={size} fill="#ffffff" />
      <path d={d} fill="#000000" />
    </svg>
  )
}

interface WifiCardProps {
  itemId: string
  ssid: string
  hasPassword: boolean
}

/**
 * The Wi-Fi guest card (matrix `wifi-items`; D239): a QR code a guest's
 * phone camera joins the network from, full size in a sheet that hides
 * itself after a minute, when the window loses focus and when the vault
 * locks, and a printable card. The code holds the network's password, so
 * it comes from the core as bytes through the `Reveal` gate for a crown
 * jewel, counts as a use, and is wiped when the card closes. Printing
 * reveals the password once more (the same gate) for the card's words.
 */
export function WifiCard({ itemId, ssid, hasPassword }: WifiCardProps) {
  const client = useClient()
  const gate = useGate()
  const locked = useLocked()
  const [grid, setGrid] = useState<Uint8Array | null>(null)
  const [left, setLeft] = useState(WIFI_CARD_LIFETIME_S)
  const [error, setError] = useState<string | null>(null)
  const [printed, setPrinted] = useState<string | null>(null)
  const held = useRef<Uint8Array | null>(null)

  const close = () => {
    held.current?.fill(0)
    held.current = null
    setGrid(null)
    setPrinted(null)
  }

  const open = async () => {
    setError(null)
    try {
      const g = await gate.run((x) => client.wifiQr(itemId, x))
      held.current = g
      setLeft(WIFI_CARD_LIFETIME_S)
      setGrid(g)
    } catch (e) {
      setError(describeError(e))
    }
  }

  // Up only while the minute runs and the vault is open; once either ends
  // the grid is wiped and the sheet closes (the bytes, not just the view).
  const visible = grid !== null && left > 0 && !locked

  useEffect(() => {
    if (!visible) {
      held.current?.fill(0)
      return
    }
    const tick = window.setInterval(() => setLeft((n) => n - 1), 1000)
    const blur = () => close()
    window.addEventListener("blur", blur)
    return () => {
      window.clearInterval(tick)
      window.removeEventListener("blur", blur)
    }
  }, [visible])

  // Wiped if the item's pane goes away with the card up.
  useEffect(
    () => () => {
      held.current?.fill(0)
    },
    [],
  )

  const print = async () => {
    setError(null)
    try {
      let words = ""
      if (hasPassword) {
        const bytes = await gate.run((g) => client.reveal(itemId, "wifi.passphrase", g))
        words = new TextDecoder().decode(bytes)
        bytes.fill(0)
      }
      setPrinted(words)
      // Drawn, then printed, then dropped: the words exist only for the print.
      window.setTimeout(() => {
        window.print()
        setPrinted(null)
      }, 0)
    } catch (e) {
      setError(describeError(e))
    }
  }

  useCaptureProtect(visible)
  return (
    <>
      <Button variant="ghost" size="sm" onClick={() => void open()}>
        <Icon name="qr_code_2" data-icon="inline-start" />
        Guest card
      </Button>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Dialog open={visible} onOpenChange={(o) => !o && close()}>
        <DialogContent size="lg">
          <DialogHeader>
            <DialogTitle>Join {ssid}</DialogTitle>
            <DialogDescription>Point a phone's camera at the code. It hides itself in {Math.max(left, 0)} seconds, or when you leave this window.</DialogDescription>
          </DialogHeader>
          {visible && grid && (
            <div className="flex flex-col items-center gap-3">
              <QrCode grid={grid} label={`The code to join ${ssid}`} />
              <p className="text-lg font-semibold">{ssid}</p>
            </div>
          )}
          {visible && grid && printed !== null && (
            <div data-print-card hidden className="print:block">
              <p className="text-2xl font-semibold">Wi-Fi: {ssid}</p>
              {printed && <p className="font-mono text-3xl tracking-widest">{printed}</p>}
              <QrCode grid={grid} label={`The code to join ${ssid}`} />
              <p>Scan with a phone camera to join.</p>
            </div>
          )}
          <DialogActions>
            <DialogAction kind="cancel" onClick={close}>
              Hide
            </DialogAction>
            <DialogAction kind="confirm" onClick={() => void print()}>
              Print a card
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </>
  )
}
