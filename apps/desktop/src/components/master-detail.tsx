import { createContext, useContext, type ReactNode } from "react"

import { AppFrameColumn, AppFrameDetail, AppFrameInset, AppFrameList, EmptyState, useAppFrame, type IconName } from "@unicron/ui"

import { ScreenHead } from "@/app/page-head"

interface MasterDetailProps {
  /** Header for the list pane: title, search, actions. */
  header: ReactNode
  /**
   * The screen's title, the one the list pane's header shows as its `h1`.
   * While a detail is pushed over the list (a phone, medium width) the list
   * and its header are hidden, so the detail carries the same title as a
   * heading only a screen reader reads: every width has one `h1` (design
   * review R18).
   */
  title: string
  list: ReactNode
  detail: ReactNode | null
  onBack: () => void
  /** Kept for callers; the redesign's "Nothing open." state draws no icon (cross-app decisions §9). */
  emptyIcon?: IconName
  emptyLabel: string
}

/**
 * The list → detail pattern, on unicron's frame columns. At the expanded
 * width the list stands beside the detail, with "Nothing open." until a
 * row is chosen. Narrower, the list is the screen and a chosen row pushes
 * its detail over it; the way back is the shell's (the phone's top bar, the
 * status line's leading slot at medium width), which the frame hands
 * `onBack`. Each column scrolls by itself.
 */
export function MasterDetail({ header, title, list, detail, onBack, emptyLabel }: MasterDetailProps) {
  const frame = useAppFrame()
  const hasDetail = detail !== null
  return (
    <>
      <AppFrameList>
        {header}
        {list}
      </AppFrameList>
      <AppFrameDetail open={hasDetail} onBack={onBack}>
        {hasDetail ? (
          <>
            {!frame.split && <h1 className="sr-only">{title}</h1>}
            {detail}
          </>
        ) : (
          <AppFrameInset>
            <EmptyState overline={title} title="Nothing open." body={emptyLabel} />
          </AppFrameInset>
        )}
      </AppFrameDetail>
    </>
  )
}

const EmbeddedContext = createContext(false)

/**
 * A screen shown inside another's frame (Devices, Recovery and Status as
 * Settings sections): the frame already names it, so its own head keeps its
 * actions and draws no second title.
 */
export function Embedded({ children }: { children: ReactNode }) {
  return (
    <EmbeddedContext.Provider value={true}>
      <div className="[--c-page-gutter-narrow:0px] [--c-page-gutter:0px]">{children}</div>
    </EmbeddedContext.Provider>
  )
}

/** A screen's head: its title as the `h1`, its actions at the end. The shell lays the place's switcher and chips over it where they apply. */
export function PaneHeader({ title, children }: { title: string; children?: ReactNode }) {
  const embedded = useContext(EmbeddedContext)
  if (embedded) return children ? <div className="flex flex-wrap items-center justify-end gap-2">{children}</div> : null
  return <ScreenHead title={title} actions={children} />
}

/**
 * A screen that is one column (Settings, Sharing, Health, Status): the
 * frame's content column, so it scrolls under the phone's chrome and stands
 * beside the sidebar like every other screen.
 */
export function PageColumn({ children }: { children: ReactNode }) {
  return (
    <AppFrameColumn>
      {/* The column's own edge is the page's line, and the frame keeps the phone's 16: a screen adds no gutter of its own. */}
      <AppFrameInset>
        <div className="[--c-page-gutter-narrow:0px] [--c-page-gutter:0px]">{children}</div>
      </AppFrameInset>
    </AppFrameColumn>
  )
}
