import { useState } from "react"

import {
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Icon,
  Spinner,
} from "@unicron/ui"

import { describeError } from "@/core/client"

import { useBridges, type EmergencySheet } from "./bridges-client"

/**
 * The emergency sheet (docs/plan/23-bridges.md §10): one printable page of
 * what to do and where things are if this computer is lost, the password
 * forgotten or the server gone, with blanks to fill in by hand. It holds no
 * password, key or item; printing uses the system's own dialog.
 */
export function EmergencySheetButton({ onNotice }: { onNotice: (text: string) => void }) {
  const bridges = useBridges()
  const [sheet, setSheet] = useState<EmergencySheet | null>(null)
  const [busy, setBusy] = useState(false)
  if (!bridges) return null
  const open = async () => {
    setBusy(true)
    try {
      setSheet(await bridges.emergencySheet())
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <Button variant="outline" onClick={() => void open()} disabled={busy}>
        {busy ? <Spinner data-icon="inline-start" /> : <Icon name="print" data-icon="inline-start" />}
        Emergency sheet
      </Button>
      <Dialog open={sheet !== null} onOpenChange={(o) => !o && setSheet(null)}>
        <DialogContent size="lg">
          {sheet && (
            <>
              <DialogHeader>
                <DialogTitle>{sheet.title}</DialogTitle>
                <DialogDescription>{sheet.intro}</DialogDescription>
              </DialogHeader>
              <article aria-label="Emergency sheet" className="flex flex-col gap-3 text-sm">
                {sheet.sections.map((s) => (
                  <section key={s.heading} className="flex flex-col gap-1">
                    <h3 className="font-semibold">{s.heading}</h3>
                    <ul className="list-disc pl-5">
                      {s.lines.map((l) => (
                        <li key={l}>{l}</li>
                      ))}
                    </ul>
                  </section>
                ))}
                <section className="flex flex-col gap-3">
                  {sheet.blanks.map((b) => (
                    <p key={b} className="border-b pb-6">
                      {b}
                    </p>
                  ))}
                </section>
              </article>
              <DialogActions>
                <DialogAction kind="cancel" onClick={() => setSheet(null)}>
                  Close
                </DialogAction>
                <Button onClick={() => window.print()}>
                  <Icon name="print" data-icon="inline-start" />
                  Print
                </Button>
              </DialogActions>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}
