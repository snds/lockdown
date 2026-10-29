import type { ComponentProps, ReactNode } from "react"

import { Button, DialogFooter } from "@unicron/ui"

/** What an action does; it sets the button's look, as unicron's `DialogAction` kinds do. */
export type LabelledActionKind = "confirm" | "cancel" | "destructive"

const VARIANT = { confirm: "default", cancel: "outline", destructive: "ghost-destructive" } as const

/**
 * A dialog's actions, with their words at every width (design review R1;
 * brief §6 rule 5: nothing security-relevant rides on an icon alone).
 *
 * unicron's `DialogActions` turns each action into a round icon button below
 * `sm` (× and ✓, the words kept only as the accessible name), and has no prop
 * or variant that keeps the words. This is the smallest local replacement: the
 * same footer and the same button looks, from `sm` up one row aligned right as
 * before (a destructive action written first leads, apart from the rest; one
 * written last, as the dialog's yes, stays at the right with the others,
 * where unicron's rule pulled the whole row left); below `sm` the
 * actions stack full width in their source order, so the primary, written
 * last, sits last, nearest the thumb. The unicron request is recorded in
 * docs/plan/31-design-fixes-1.md.
 */
export function LabelledActions({ children }: { children: ReactNode }) {
  return (
    <DialogFooter>
      <div
        data-slot="labelled-actions"
        className="flex flex-col gap-2 max-sm:*:w-full sm:flex-1 sm:flex-row sm:items-center sm:justify-end sm:[&>[data-kind=destructive]:first-child]:mr-auto"
      >
        {children}
      </div>
    </DialogFooter>
  )
}

/**
 * One action in `LabelledActions`: a text button, never an icon alone.
 * `data-kind` stays on it, so unicron's dialog still hides its corner ×
 * on phones when a Cancel is in the row.
 */
export function LabelledAction({
  kind = "confirm",
  type,
  form,
  disabled,
  onClick,
  children,
}: {
  kind?: LabelledActionKind
  type?: "button" | "submit"
  form?: string
  disabled?: boolean
  onClick?: ComponentProps<typeof Button>["onClick"]
  children: ReactNode
}) {
  return (
    <Button data-kind={kind} variant={VARIANT[kind]} type={type} form={form} disabled={disabled} onClick={onClick}>
      {children}
    </Button>
  )
}
