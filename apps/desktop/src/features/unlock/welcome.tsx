import { useEffect, useRef, useState, type ReactNode } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@unicron/ui"

/**
 * The welcome layout (docs/plan/39-first-run.md §1.9, §1.12): the first
 * run, joining, the migration, a forgotten password and the unlock screen
 * all draw in one roomy column, the same at either density, from the
 * `--c-welcome-*` knobs in `app-extension.css`. One `h1` per screen, and
 * focus moves to it when the screen changes, so a screen reader hears
 * each step.
 */
export function WelcomeColumn({ children, locked = false }: { children: ReactNode; locked?: boolean }) {
  return (
    <main
      data-welcome=""
      className={`flex min-h-full justify-center px-(--c-page-gutter-narrow) py-(--c-welcome-pad-block) sm:px-(--c-page-gutter) ${locked ? "bg-surface-locked/70" : "bg-background"}`}
    >
      <div className="flex w-full max-w-(--c-welcome-column) flex-col gap-(--c-welcome-gap) self-center">{children}</div>
    </main>
  )
}

/** The Lockdown mark: a padlock on the brand square, in the theme's own ink. */
export function LockdownMark({ unlock = false }: { unlock?: boolean }) {
  return (
    <svg
      viewBox="0 0 32 32"
      aria-hidden="true"
      className={`self-center text-foreground ${unlock ? "size-(--c-welcome-mark-unlock)" : "size-(--c-welcome-mark)"}`}
    >
      <rect width="32" height="32" rx="7" fill="currentColor" />
      <path
        className="fill-background"
        d="M11 14v-3a5 5 0 0 1 10 0v3h1.5a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 8 23.5v-8a1.5 1.5 0 0 1 1.5-1.5H11zm2.5 0h5v-3a2.5 2.5 0 0 0-5 0v3z"
      />
    </svg>
  )
}

interface ScreenProps {
  /** "Step 1 of 4"; absent on Welcome, the proof and the side paths. */
  step?: [number, number]
  title: string
  sentence: ReactNode
  /** The Learn more dialog: its title and paragraphs. */
  learnMore?: { title: string; paragraphs: string[] }
  /** The mark above the heading (Welcome, unlock). */
  mark?: "welcome" | "unlock"
  /** Centre the heading block (Welcome, unlock). */
  centered?: boolean
  children?: ReactNode
}

/**
 * One screen of the welcome layout: the step line, the heading, the one
 * sentence with Learn more, then the screen's own groups.
 */
export function WelcomeScreen({ step, title, sentence, learnMore, mark, centered = false, children }: ScreenProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    heading.current?.focus()
  }, [title])
  return (
    <>
      {mark && <LockdownMark unlock={mark === "unlock"} />}
      <div className={`flex flex-col gap-(--c-welcome-gap-tight) ${centered ? "text-center" : ""}`}>
        {step && (
          <p className="text-sm text-muted-foreground">
            Step {step[0]} of {step[1]}
          </p>
        )}
        <h1 ref={heading} tabIndex={-1} className="text-3xl font-semibold tracking-tight outline-none">
          {title}
        </h1>
        <p className="text-lg text-muted-foreground [&>[data-slot=button]]:text-foreground [&>[data-slot=button]]:underline [&>[data-slot=button]]:underline-offset-4">
          {sentence}
          {learnMore && (
            <>
              {" "}
              <Button variant="link" size="inline" onClick={() => setOpen(true)}>
                Learn more
              </Button>
            </>
          )}
        </p>
      </div>
      {children}
      {learnMore && (
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{learnMore.title}</DialogTitle>
              <DialogDescription>{learnMore.paragraphs[0]}</DialogDescription>
            </DialogHeader>
            <div className="flex flex-col gap-3 text-sm">
              {learnMore.paragraphs.slice(1).map((p) => (
                <p key={p}>{p}</p>
              ))}
            </div>
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
  return <div className="flex flex-col gap-(--c-welcome-gap-tight) text-base">{children}</div>
}

/** The actions: full width, primary first, stacked. */
export function WelcomeActions({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-(--c-welcome-gap-tight) [&>[data-slot=button]]:w-full">{children}</div>
}
