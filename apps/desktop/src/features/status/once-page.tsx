import { useCallback, useMemo, useState } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, Spinner } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

import { describeError } from "@/core/client"
import { usePaper, usePrinting } from "@/features/paper/paper-client"

/** The kit page as the dialog holds it: decoded once by the caller, dropped on close. */
export interface ShownPage {
  title: string
  lines: string[]
  text: string | null
}

interface OncePageProps {
  page: ShownPage | null
  onClose: () => void
}

/** The two values on the kit page (`render_kit` in lockdown-app), as printed. */
export function kitValues(text: string): { secretKey: string | null; recoveryCode: string | null } {
  const pick = (label: string) => {
    const m = text.match(new RegExp(`^${label}:\\s*(\\S+)`, "m"))
    return m?.[1] ?? null
  }
  return { secretKey: pick("Secret Key"), recoveryCode: pick("Recovery code") }
}

/** The characters a person types, without the printed dashes (the kit reads both). */
const typed = (value: string) => value.replace(/-/g, "")

/**
 * The recovery kit's page, shown once. The Secret Key and the recovery
 * code are set in unicron's LargeTypeView print variant (every character
 * captioned, so O and 0 cannot be confused on paper), then the page as
 * written. The caller decoded the bytes and zeroed them (`decodeAndWipe`);
 * the string lives in the caller's state only while this dialog is open.
 * Lockdown keeps no copy; the person prints it, saves it as a file or
 * writes it down. Print goes through the host (the desktop's shell opens
 * the webview's own print panel; docs/plan/38-mac-fixes.md §3); "Save as
 * PDF…" sends the page back to the core, which lays it out like this page
 * and writes it only where the person picks (§4). Where the host has no
 * way to keep a file, the button is not there.
 */
export function OncePage({ page, onClose }: OncePageProps) {
  const open = page !== null
  const text = page?.text ?? null
  const paper = usePaper()
  const { print, printing } = usePrinting()
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const save = async () => {
    if (!paper.saveKitPdf || !page?.text) return
    setSaving(true)
    setError(null)
    setSaved(null)
    try {
      if ((await paper.saveKitPdf(page.lines, page.text)) === "saved") setSaved("Saved where you chose. Lockdown kept no copy.")
    } catch (e) {
      setError(describeError(e))
    } finally {
      setSaving(false)
    }
  }
  const close = () => {
    setSaved(null)
    setError(null)
    onClose()
  }
  const values = useMemo(() => (text ? kitValues(text) : { secretKey: null, recoveryCode: null }), [text])
  const encoder = useMemo(() => new TextEncoder(), [])
  const secretKey = useCallback(() => Promise.resolve(encoder.encode(typed(values.secretKey ?? ""))), [encoder, values.secretKey])
  const recoveryCode = useCallback(() => Promise.resolve(encoder.encode(typed(values.recoveryCode ?? ""))), [encoder, values.recoveryCode])
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent data-print-page="" size="xl">
        <DialogHeader>
          <DialogTitle>{page?.title}</DialogTitle>
          <DialogDescription>
            Shown once and never kept. Print it, save it as a file or write it down, and put it somewhere safe, away
            from your devices.
          </DialogDescription>
        </DialogHeader>
        {page?.lines.map((l) => (
          <p key={l} className="text-sm text-muted-foreground">
            {l}
          </p>
        ))}
        {values.secretKey && <LargeTypeView print getSecret={secretKey} title="Secret Key" groupSize={4} />}
        {values.recoveryCode && <LargeTypeView print getSecret={recoveryCode} title="Recovery code" groupSize={4} />}
        {/* It scrolls sideways on a phone, so a keyboard can reach it (review R18). */}
        {text && (
          <pre tabIndex={0} role="region" aria-label="The page as text" className="overflow-x-auto rounded-md border bg-muted p-3 font-mono text-xs leading-relaxed">
            {text}
          </pre>
        )}
        {/* What the file is, before anyone makes one; never on the paper. */}
        {paper.saveKitPdf && text && (
          <p data-print-hide="" className="text-sm text-muted-foreground">
            A saved file holds everything needed to get into your account with your password. Keep it off this computer: on a USB
            drive, as a printed copy, or in a safe. Lockdown keeps no copy.
          </p>
        )}
        {saved && (
          <p data-print-hide="" role="status" className="text-sm">
            {saved}
          </p>
        )}
        {error && (
          <p data-print-hide="" role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter data-print-hide="">
          <Button onClick={() => void print()} variant="outline" disabled={printing}>
            Print
          </Button>
          {paper.saveKitPdf && text && (
            <Button onClick={() => void save()} variant="outline" disabled={saving}>
              {saving ? <Spinner data-icon="inline-start" /> : null}
              Save as PDF…
            </Button>
          )}
          <Button onClick={close}>I have it</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
