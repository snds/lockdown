import { useRef, useState } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverline,
  DialogTitle,
  FieldRow,
  FieldRowAction,
} from "@unicron/ui"

import { describeError } from "@/core/client"
import { usePrinting } from "@/features/paper/paper-client"
import { SaveError, useRowSize } from "@/features/settings/setting-rows"
import { OnPaper } from "@/features/status/once-page"
import { fullDate } from "@/lib/format"

import { useBridges, type EmergencySheet } from "./bridges-client"

/**
 * The emergency sheet as a viewer (brief T3, doc 81; docs/plan/23-bridges.md
 * §10): one printable page of what to do and where things are if this
 * computer is lost, the password forgotten or the server gone, with blanks
 * to fill in by hand. It holds no password, key or item. The core's title,
 * intro, sections and blanks, on paper (black on white in both themes);
 * PRINT goes through the host (docs/plan/38-mac-fixes.md §3) and prints the
 * page alone; DONE closes it.
 */
export function EmergencySheetViewer({ sheet, onClose }: { sheet: EmergencySheet | null; onClose: () => void }) {
  const { print, printing } = usePrinting()
  const [error, setError] = useState<string | null>(null)
  // A viewer opens at its top, the dialog itself, named by its title (brief 4.0): not on PRINT.
  const viewerRef = useRef<HTMLDivElement>(null)
  const close = () => {
    setError(null)
    onClose()
  }
  return (
    <Dialog open={sheet !== null} onOpenChange={(o) => !o && close()}>
      {/* Printed alone: the app behind it, the vault included, stays off the paper (app-extension.css). */}
      <DialogContent data-print-page="" size="viewer" ref={viewerRef} initialFocus={viewerRef}>
        {sheet && (
          <>
            <DialogHeader>
              <DialogOverline>Emergency sheet // no secrets on it</DialogOverline>
              <DialogTitle>
                {sheet.title}
              </DialogTitle>
              <DialogDescription size="body">{sheet.intro}</DialogDescription>
            </DialogHeader>
            <OnPaper label="Emergency sheet">
              <div className="flex flex-col gap-6 p-6 print:p-0">
                <p className="type-meta text-text-label">{`Made ${fullDate(sheet.made_at)}`}</p>
                {sheet.sections.map((s) => (
                  <section key={s.heading} className="flex flex-col gap-2">
                    <h3 className="type-subtitle">{s.heading}</h3>
                    <ul className="flex flex-col gap-1">
                      {s.lines.map((l) => (
                        <li key={l} className="type-body">
                          {l}
                        </li>
                      ))}
                    </ul>
                  </section>
                ))}
                {sheet.blanks.length > 0 && (
                  <section aria-label="To fill in by hand" className="flex flex-col gap-8">
                    {sheet.blanks.map((b) => (
                      <p key={b} className="type-body rule-b pb-8">
                        {b}
                      </p>
                    ))}
                  </section>
                )}
              </div>
            </OnPaper>
            {error && (
              <p data-print-hide="" role="alert" className="type-caption text-critical-text">
                {error}
              </p>
            )}
            <DialogFooter data-print-hide="" sticky>
              <Button variant="secondary" loading={printing} onClick={() => void print().catch((e) => setError(describeError(e)))}>
                Print
              </Button>
              <Button onClick={close}>Done</Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

/**
 * EMERGENCY SHEET in Recovery (brief T3): what it is, and OPEN, which reads
 * the sheet from the core and shows it. A failed read is one critical line
 * under the row. Where the host has no bridges client (the web vault) the
 * row is not drawn.
 */
export function EmergencySheetRow() {
  const bridges = useBridges()
  const size = useRowSize()
  const [sheet, setSheet] = useState<EmergencySheet | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  if (!bridges) return null
  const open = async () => {
    setBusy(true)
    setError(null)
    try {
      setSheet(await bridges.emergencySheet())
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="flex flex-col">
      <FieldRow
        size={size}
        label="Emergency sheet"
        description="One page for a future you. No secrets on it."
        action={
          <FieldRowAction loading={busy} onClick={() => void open()}>
            Open
          </FieldRowAction>
        }
      />
      <SaveError text={error} />
      <EmergencySheetViewer sheet={sheet} onClose={() => setSheet(null)} />
    </div>
  )
}
