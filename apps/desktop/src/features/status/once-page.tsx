import { useCallback, useMemo } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

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
 * Lockdown keeps no copy; the person prints or writes it down.
 */
export function OncePage({ page, onClose }: OncePageProps) {
  const open = page !== null
  const text = page?.text ?? null
  const values = useMemo(() => (text ? kitValues(text) : { secretKey: null, recoveryCode: null }), [text])
  const encoder = useMemo(() => new TextEncoder(), [])
  const secretKey = useCallback(() => Promise.resolve(encoder.encode(typed(values.secretKey ?? ""))), [encoder, values.secretKey])
  const recoveryCode = useCallback(() => Promise.resolve(encoder.encode(typed(values.recoveryCode ?? ""))), [encoder, values.recoveryCode])
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent data-print-page="" size="xl">
        <DialogHeader>
          <DialogTitle>{page?.title}</DialogTitle>
          <DialogDescription>
            Shown once and never kept. Print it or write it down, and put the page somewhere safe, away from your
            devices.
          </DialogDescription>
        </DialogHeader>
        {page?.lines.map((l) => (
          <p key={l} className="text-sm text-muted-foreground">
            {l}
          </p>
        ))}
        {values.secretKey && <LargeTypeView print getSecret={secretKey} title="Secret Key" groupSize={4} />}
        {values.recoveryCode && <LargeTypeView print getSecret={recoveryCode} title="Recovery code" groupSize={4} />}
        {text && <pre className="overflow-x-auto rounded-md border bg-muted p-3 font-mono text-xs leading-relaxed">{text}</pre>}
        <DialogFooter>
          <Button onClick={() => window.print()} variant="outline">
            Print
          </Button>
          <Button onClick={onClose}>I have it</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
