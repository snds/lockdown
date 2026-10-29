import type { ReactNode } from "react"

import { Button, Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle, Icon, cn, type IconName } from "@unicron/ui"

interface MasterDetailProps {
  /** Header for the list pane: title, search, actions. */
  header: ReactNode
  /**
   * The screen's title, the one the list pane's header shows as its `h1`. At
   * phone width an open detail hides the list pane, header and all, so the
   * detail's bar carries the same title as a heading only a screen reader
   * reads: every width has one `h1` (design review R18).
   */
  title: string
  list: ReactNode
  detail: ReactNode | null
  onBack: () => void
  emptyIcon: IconName
  emptyLabel: string
}

/**
 * The list → detail pattern (Mailbird's, kept neutral until the design
 * brief restyles it). Phone width: one pane at a time; selecting pushes
 * the detail and Back pops it. Wider: both panes side by side.
 */
export function MasterDetail({ header, title, list, detail, onBack, emptyIcon, emptyLabel }: MasterDetailProps) {
  const hasDetail = detail !== null
  return (
    <div className="flex min-h-0 flex-1">
      <section
        className={cn(
          "flex min-w-0 flex-1 flex-col bg-surface-chrome md:w-(--c-shell-list-width) md:flex-none md:border-r",
          hasDetail && "hidden md:flex",
        )}
      >
        {header}
        <div className="min-h-0 flex-1 overflow-y-auto">{list}</div>
      </section>
      <section className={cn("min-w-0 flex-1 flex-col bg-surface-canvas", hasDetail ? "flex" : "hidden md:flex")}>
        {hasDetail ? (
          <>
            <div className="flex h-12 shrink-0 items-center gap-1 px-2 md:hidden">
              <Button variant="ghost" size="icon" onClick={onBack} aria-label="Back">
                <Icon name="arrow_back" />
              </Button>
              <h1 className="sr-only">{title}</h1>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">{detail}</div>
          </>
        ) : (
          <Empty className="flex-1">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Icon name={emptyIcon} />
              </EmptyMedia>
              <EmptyTitle>Nothing selected</EmptyTitle>
              <EmptyDescription>{emptyLabel}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </section>
    </div>
  )
}

export function PaneHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="flex shrink-0 flex-col gap-3 px-4 pt-4 pb-3">
      <div className="flex items-center justify-between gap-2">
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        <div className="flex items-center gap-1">{children}</div>
      </div>
    </header>
  )
}
