import { useCallback, useMemo, useRef, useState, type ReactNode } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogOverline, DialogTitle, SectionHeader } from "@unicron/ui"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"

import { describeError } from "@/core/client"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
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
 * A page to print, on screen as on paper: black on white in both themes
 * (brief 4.0, "Themes"). The stone ground and its ink, as unicron's QrCode
 * draws itself (`light` scopes the roles); the print rules
 * (`[data-print-page]`, app-extension.css) make it pure white and black.
 * It adds no padding: LargeTypeView's print variant has its own, and the
 * emergency sheet sets its own.
 */
export function OnPaper({ label, children }: { label: string; children: ReactNode }) {
  return (
    <article aria-label={label} className="light bg-surface-ground text-text-body rule-box flex flex-col print:shadow-none">
      {children}
    </article>
  )
}

/**
 * The recovery kit's page, shown once (brief T3, doc 81): a viewer (960 on
 * a desktop; below that width, the whole window). Overline RECOVERY KIT //
 * SHOWN ONCE, the title, one sentence, the core's notes; then the page, on
 * paper: the Secret Key and the recovery code in unicron's LargeTypeView
 * print variant (every character captioned, so O and 0 cannot be confused),
 * and "The page as text", shut on screen and printed whole. PRINT and SAVE
 * AS PDF… are secondary; I HAVE IT is the one primary.
 *
 * The caller decoded the bytes and zeroed them (`decodeAndWipe`); the
 * string lives in the caller's state only while this is open, and the
 * window is held out of screen captures meanwhile (D316). Lockdown keeps no
 * copy. Print goes through the host (the desktop's shell opens the
 * webview's own print panel; docs/plan/38-mac-fixes.md §3), and the page
 * prints alone. "Save as PDF…" sends the page back to the core, which lays
 * it out like this page and writes it only where the person picks (§4).
 * Where the host has no way to keep a file, the button is not there.
 */
export function OncePage({ page, onClose }: OncePageProps) {
  const open = page !== null
  const text = page?.text ?? null
  const paper = usePaper()
  const { print, printing } = usePrinting()
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useCaptureProtect(open)
  // A viewer opens at its top, the dialog itself, named by its title (brief 4.0): not at the first control far down the page.
  const viewerRef = useRef<HTMLDivElement>(null)
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
    <Dialog open={open} onOpenChange={(o) => !o && close()} closeOnOutsideClick={false}>
      <DialogContent data-print-page="" size="viewer" ref={viewerRef} initialFocus={viewerRef}>
        <DialogHeader>
          <DialogOverline>Recovery kit // shown once</DialogOverline>
          <DialogTitle>
            {page?.title}
          </DialogTitle>
          <DialogDescription size="body">Shown once and never kept. Print it, save it as a file or write it down.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-6">
          {page?.lines.length ? (
            <ul className="flex flex-col gap-1">
              {page.lines.map((l) => (
                <li key={l} className="type-caption text-text-secondary">
                  {l}
                </li>
              ))}
            </ul>
          ) : null}
          {(values.secretKey || values.recoveryCode) && (
            <OnPaper label="Your recovery kit page">
              {values.secretKey && <LargeTypeView print getSecret={secretKey} title="Secret Key" groupSize={4} />}
              {values.recoveryCode && <LargeTypeView print getSecret={recoveryCode} title="Recovery code" groupSize={4} />}
              {/* The page as written, on paper only: on screen it is the collapsible below. */}
              {text && (
                <pre hidden className="type-code-inline whitespace-pre-wrap print:block print:pt-6">
                  {text}
                </pre>
              )}
            </OnPaper>
          )}
          {text && (
            <div data-print-hide="">
              <SectionHeader level={3} title="The page as text" collapsible summary="Login, server and how to use it">
                {/* It scrolls sideways on a phone, so a keyboard can reach it (review R18). */}
                <pre tabIndex={0} role="region" aria-label="The page as text" className="type-code-inline overflow-x-auto py-3">
                  {text}
                </pre>
              </SectionHeader>
            </div>
          )}
          {/* What the file is, before anyone makes one; never on the paper. */}
          {paper.saveKitPdf && text && (
            <p data-print-hide="" className="type-caption text-text-secondary">
              A saved file holds everything needed to get into your account with your password. Keep it off this computer: on a USB drive, as a printed copy, or
              in a safe. Lockdown keeps no copy.
            </p>
          )}
          {saved && (
            <p data-print-hide="" role="status" className="type-caption text-text-body">
              {saved}
            </p>
          )}
          {error && (
            <p data-print-hide="" role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
        </div>
        <DialogFooter data-print-hide="" sticky>
          <Button onClick={() => void print()} variant="secondary" loading={printing}>
            Print
          </Button>
          {paper.saveKitPdf && text && (
            <Button onClick={() => void save()} variant="secondary" loading={saving}>
              Save as PDF…
            </Button>
          )}
          <Button onClick={close}>I have it</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
