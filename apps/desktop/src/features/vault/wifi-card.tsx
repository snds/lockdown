import { useEffect, useLayoutEffect, useRef, useState } from "react"

import { AdaptiveDialog, Button, DigitGroups, Icon, QrCode, Tile } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { useLocked } from "@/app/locked"
import { describeError, useClient } from "@/core/client"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { usePrinting } from "@/features/paper/paper-client"
import { countdown } from "@/lib/format"

import { WIFI_CARD_LIFETIME_S } from "./depth-model"

/** The core's grid (its width, then one byte a module, `1` dark) as the rows unicron's QrCode draws. */
function qrRows(grid: Uint8Array): boolean[][] {
  const w = grid[0] ?? 0
  return Array.from({ length: w }, (_, y) => Array.from({ length: w }, (_, x) => grid[1 + y * w + x] === 1))
}

/** What scanning it does, never what it holds (unicron QrCode's label). */
const guestCodeLabel = (ssid: string) => `QR code to join ${ssid}`

interface WifiCardProps {
  itemId: string
  ssid: string
  hasPassword: boolean
}

/**
 * The Wi-Fi guest card (matrix `wifi-items`; D239): a QR code a guest's
 * phone camera joins the network from, on a Tile in a viewer (full screen
 * on a phone; brief I9) that hides
 * itself after a minute, when the window loses focus and when the vault
 * locks, and a printable card. The code holds the network's password, so
 * it comes from the core as bytes through the `Reveal` gate for a crown
 * jewel, counts as a use, and is wiped when the card closes. Printing
 * reveals the password once more (the same gate) for the card's words,
 * which are drawn only until printing is over. Printing goes through the
 * host (docs/plan/38-mac-fixes.md §3); while a native print panel is up,
 * the minute stops and losing the window to the panel doesn't hide the
 * card, so the panel prints the card and not an empty page.
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
  const { print: printPage, printing } = usePrinting()
  const printingNow = useRef(false)
  // The card opens with focus on DONE, the layer's one primary: Enter or Escape puts it away.
  const done = useRef<HTMLButtonElement | null>(null)
  // A layout effect, not a plain one: it runs in the same task as the
  // render that drops the words, before any event can land. A plain effect
  // runs after paint, so a blur in between would still read "printing" and
  // leave the card up.
  useLayoutEffect(() => {
    printingNow.current = printing
  }, [printing])

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
    // The minute stands still while a print panel is up.
    const tick = window.setInterval(() => setLeft((n) => (printingNow.current ? n : n - 1)), 1000)
    const blur = () => {
      if (!printingNow.current) close()
    }
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
      await new Promise((r) => window.setTimeout(r, 0))
      try {
        await printPage()
      } finally {
        setPrinted(null)
      }
    } catch (e) {
      setError(describeError(e))
    }
  }

  useCaptureProtect(visible)
  const rows = visible && grid ? qrRows(grid) : null
  return (
    <>
      <Button variant="ghost" size="sm" onClick={() => void open()}>
        <Icon name="qr-code" data-icon="inline-start" />
        Guest card
      </Button>
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      {/* A viewer (960 on a desktop, full screen on a phone): the card, its minute, PRINT CARD and DONE (brief I9). */}
      <AdaptiveDialog
        open={visible}
        onOpenChange={(o) => !o && close()}
        size="viewer"
        height="full"
        overline="Wi-Fi // Guest card"
        title={`Join ${ssid}`}
        initialFocus={done}
        footer={
          <>
            <Button variant="secondary" onClick={() => void print()}>
              Print card
            </Button>
            <Button ref={done} onClick={close}>
              Done
            </Button>
          </>
        }
      >
        {rows && (
          <div className="flex flex-col items-center gap-4">
            <Tile>
              <span className="flex flex-col items-center gap-4">
                <span className="type-title text-foreground">{ssid}</span>
                <QrCode modules={rows} label={guestCodeLabel(ssid)} />
              </span>
            </Tile>
            <p className="type-caption text-support-foreground">Scan with a phone camera to join.</p>
            <p className="type-overline text-secondary-live">{`Hides in ${countdown(left)}`}</p>
            <p className="sr-only">It hides itself after a minute, or when you leave this window.</p>
          </div>
        )}
        {rows && printed !== null && (
          // The paper, drawn only while it prints: black on white, alone (app-extension.css [data-print-card]).
          <div data-print-card hidden className="print:block">
            <p className="type-display-compact">Wi-Fi: {ssid}</p>
            {printed && <DigitGroups value={printed} size="lg" label="Password" />}
            <QrCode modules={rows} label={guestCodeLabel(ssid)} />
            <p className="type-body">Scan with a phone camera to join.</p>
          </div>
        )}
      </AdaptiveDialog>
    </>
  )
}
