import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@unicron/ui"

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

/**
 * The recovery kit's page, shown once. The caller decoded the bytes and
 * zeroed them (`decodeAndWipe`); the string lives in the caller's state
 * only while this dialog is open. Lockdown keeps no copy; the person
 * prints or writes it down.
 */
export function OncePage({ page, onClose }: OncePageProps) {
  const open = page !== null
  const text = page?.text ?? null
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-xl">
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
