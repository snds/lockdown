import { createContext, useContext, type ReactNode } from "react"

import { AppFrameColumn, AppFrameDetail, AppFrameInset, AppFrameList, EmptyState, useAppFrame, type IconName } from "@unicron/ui"

import { useDetailTopBarHandUp } from "@/app/detail-bottom-row"
import { ScreenHead } from "@/app/page-head"
import { useShellRefresh } from "@/app/status"

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
 *
 * On a book-pose fold (a foldable held open like a book) the frame splits
 * list and detail at the hinge whatever the width, each in one half, and
 * "Nothing open." stands in the far half until a row is chosen (brief X6,
 * contract 5.1b #10). A phone on its side keeps the pushed detail; its bars
 * stand in the trailing column.
 *
 * A pushed detail on a phone hands its top-bar actions up with
 * `useDetailTopBar` and its bottom row with `useDetailBottomRow`
 * (`app/detail-bottom-row.tsx`).
 */
export function MasterDetail({ header, title, list, detail, onBack, emptyLabel }: MasterDetailProps) {
  const frame = useAppFrame()
  // A pull down on the phone's list runs the place's refresh (brief 3.3); its plain path is "Check again".
  const refresh = useShellRefresh()
  const hasDetail = detail !== null
  return (
    <>
      <AppFrameList onRefresh={refresh}>
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

/**
 * A pushed screen's top-bar actions on a phone (brief 3.3): up to two
 * neutral `FloatButton`s (size `l`) after the small centred title, such as
 * Edit and More. Returns whether the top bar draws them, so the screen can
 * leave out its own buttons for the same actions; beside the list they stay
 * in the screen. Pass the same element while nothing about it changes.
 */
export function useDetailTopBar(actions: ReactNode): boolean {
  return useDetailTopBarHandUp(actions)
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
  const refresh = useShellRefresh()
  return (
    <AppFrameColumn onRefresh={refresh}>
      {/* The column's own edge is the page's line, and the frame keeps the phone's 16: a screen adds no gutter of its own. */}
      <AppFrameInset>
        <div className="[--c-page-gutter-narrow:0px] [--c-page-gutter:0px]">{children}</div>
      </AppFrameInset>
    </AppFrameColumn>
  )
}
