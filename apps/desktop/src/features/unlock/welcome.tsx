import { useEffect, useRef, useState, type ReactNode } from "react"

import { Button, Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle, useAppFrame } from "@unicron/ui"

import { ClosedFrame } from "./closed-frame"

/**
 * The first run's steps, written as the focus frame's folio says them
 * (brief F2; cross-app decisions §4): `Step 01 // 04`. A flow of one step,
 * Welcome and the proof show none.
 */
export function stepFolio([n, of]: [number, number]): string {
  const two = (x: number) => String(x).padStart(2, "0")
  return `Step ${two(n)} // ${two(of)}`
}

/**
 * The frame every screen before the vault draws in (brief F2, F3, F4;
 * cross-app decisions §4): Unicron's focus frame, one 432 column with the
 * lockup at its top and the step folio at the end of the lockup's row. The
 * first run, joining, the migration and a forgotten password share it with
 * the lock screen (`ClosedFrame`). Nothing animates in; nothing of the vault
 * is drawn.
 */
export function WelcomeColumn({ children, folio }: { children: ReactNode; folio?: ReactNode; /** Kept for callers; the wash is gone (Q15). */ locked?: boolean }) {
  return <ClosedFrame folio={folio}>{children}</ClosedFrame>
}

interface ScreenProps {
  title: string
  sentence: ReactNode
  /** The Learn more dialog (D94): its title and short paragraphs. */
  learnMore?: { title: string; paragraphs: string[] }
  children?: ReactNode
}

/**
 * One screen's heading (brief F2): the title in the title role (the screen's
 * one `h1`, which takes the focus when the step changes, so a screen reader
 * hears each step), then one sentence, then `Learn more`, a text button that
 * opens a question dialog of short paragraphs with `Got it` (D94).
 */
export function WelcomeScreen({ title, sentence, learnMore, children }: ScreenProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    heading.current?.focus()
  }, [title])
  return (
    <>
      <div className="flex flex-col gap-4">
        <h1 ref={heading} tabIndex={-1} className="type-title text-text-body outline-none">
          {title}
        </h1>
        <p className="type-body text-text-secondary">{sentence}</p>
        {learnMore && (
          // A text button hangs its word on the column's edge, as the title does.
          <div className="-ms-4 -mt-2">
            <Button variant="ghost" onClick={() => setOpen(true)}>
              Learn more
            </Button>
          </div>
        )}
      </div>
      {children}
      {learnMore && (
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogContent size="question">
            <DialogHeader>
              <DialogTitle>{learnMore.title}</DialogTitle>
            </DialogHeader>
            <DialogBody>
              {learnMore.paragraphs.map((p) => (
                <p key={p} className="type-body text-text-body">
                  {p}
                </p>
              ))}
            </DialogBody>
            <DialogFooter>
              <Button onClick={() => setOpen(false)}>Got it</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </>
  )
}

/** A group of controls inside a screen: label, field, help, tight together. */
export function WelcomeGroup({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-3">{children}</div>
}

/**
 * The screen's buttons (brief F2): full column width and labelled at every
 * width, one primary, then a secondary, then a text button. On a phone they
 * sit at the foot of the column.
 */
export function WelcomeActions({ children }: { children: ReactNode }) {
  const { phone } = useAppFrame()
  return <div className={`flex flex-col gap-3 [&>[data-slot=button]]:w-full ${phone ? "mt-auto" : ""}`}>{children}</div>
}

/** On a phone a screen's form fills the column, so its buttons sit at the foot (first run, joining, the migration, forgot). */
export function useFormFill(): string {
  const { phone } = useAppFrame()
  return `flex flex-col gap-8 ${phone ? "grow" : ""}`
}
