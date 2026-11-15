import type { ReactNode } from "react"

import { FloatButton, Sheet, SheetBody, SheetContent, TopBar } from "@unicron/ui"

interface FullScreenEntryProps {
  open: boolean
  /** Closing it: the top bar's close button, Escape. Not while `busy`. */
  onClose: () => void
  /** The screen's name, its one h1, small in the top bar ("New vault", "Add a code"). */
  title: string
  /** The yes, as the checkmark's name ("Create vault", "Add code"). It submits the form named by `formId`. */
  primary: string
  formId: string
  busy?: boolean
  children: ReactNode
}

/**
 * A short entry on a phone, as every form there is (briefs 3.5 and 4.0,
 * Mobile / 07): the whole screen, a top bar with close at its start and the
 * title, and one primary checkmark Float Button that submits the form. The
 * item editor draws the same layer for its longer forms. On a desktop the
 * caller keeps its dialog. (A Unicron gap: AdaptiveDialog has no full-screen
 * form on a phone; Vale lists the same.)
 */
export function FullScreenEntry({ open, onClose, title, primary, formId, busy = false, children }: FullScreenEntryProps) {
  return (
    <Sheet open={open} onOpenChange={(o) => !o && !busy && onClose()}>
      <SheetContent side="bottom" height="full" showCloseButton={false} aria-label={title}>
        <div className="px-4">
          <TopBar
            title={title}
            titleAs="h1"
            start={<FloatButton label="Cancel" icon="close" size="l" tooltip={false} onClick={onClose} disabled={busy} />}
            actions={<FloatButton label={primary} icon="checkmark" kind="primary" size="l" type="submit" form={formId} disabled={busy} />}
          />
        </div>
        <SheetBody>{children}</SheetBody>
      </SheetContent>
    </Sheet>
  )
}
