import type { ReactNode } from "react"

import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverline,
  DialogTitle,
  FloatButton,
  Sheet,
  SheetBody,
  SheetContent,
  SheetFooter,
  TopBar,
  useAdaptiveSheet,
  useViewport,
} from "@unicron/ui"

/** Whether a sharing form is a full screen now: a phone, or a phone turned sideways (brief I7, S4, S5: "the Forms are full screens"). */
export function useFullScreenForm(): boolean {
  const sheet = useAdaptiveSheet()
  const { short } = useViewport()
  return sheet || short
}

interface ShareLayerProps {
  open: boolean
  /** Closing by Escape, the scrim (where allowed) or the close control. */
  onClose: () => void
  overline: ReactNode
  title: string
  description?: ReactNode
  children: ReactNode
  /** The desktop's actions, the one primary last (unicron's dialog footer). */
  footer?: ReactNode
  /** On a phone: the yes at the end, an extended float button with its word ("Create link"). Left out: none. */
  phoneAction?: ReactNode
  /**
   * On a phone: the top bar's leading control. Left out: a close float button.
   * The link shown once puts DONE here as a text button (brief I7).
   */
  phoneStart?: ReactNode
  /** A press outside closes it; off for a form with something typed. */
  closeOnOutsideClick?: boolean
}

/**
 * The layer every sharing form sits in (briefs I7, S4, S5): a form-width
 * dialog on a desktop and a full screen on a phone, with the screen's name
 * in the top bar, close at its start, and the yes at the end of the screen
 * as an extended float button with its word (Q19). It draws only unicron's
 * dialog, sheet, top bar and float button.
 */
export function ShareLayer({ open, onClose, overline, title, description, children, footer, phoneAction, phoneStart, closeOnOutsideClick = false }: ShareLayerProps) {
  const full = useFullScreenForm()
  if (full)
    return (
      <Sheet open={open} onOpenChange={(o) => !o && onClose()} closeOnOutsideClick={closeOnOutsideClick}>
        <SheetContent side="bottom" height="full" showCloseButton={false} aria-label={title}>
          <div className="px-4">
            <TopBar title={title} titleAs="h1" start={phoneStart ?? <FloatButton label="Close" icon="close" size="l" tooltip={false} onClick={onClose} />} />
          </div>
          <SheetBody>
            <div className="flex flex-col gap-4 pb-6">
              <p className="type-overline text-text-label">{overline}</p>
              {description && <p className="type-body text-text-secondary">{description}</p>}
              {children}
            </div>
          </SheetBody>
          {phoneAction && (
            <SheetFooter>
              <div className="flex w-full justify-end">{phoneAction}</div>
            </SheetFooter>
          )}
        </SheetContent>
      </Sheet>
    )
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} closeOnOutsideClick={closeOnOutsideClick}>
      <DialogContent size="form">
        <DialogHeader>
          <DialogOverline>{overline}</DialogOverline>
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-col gap-4">{children}</div>
        </DialogBody>
        {footer && <DialogFooter>{footer}</DialogFooter>}
      </DialogContent>
    </Dialog>
  )
}
