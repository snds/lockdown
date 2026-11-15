import { createContext, useContext, type ReactNode } from "react"

import { AppFramePageHead, isExpandedWidth, useAppFrame, type AppFramePageHeadProps, type PageHeadSwitcher } from "@unicron/ui"

import { useNeedsYouUnderHead } from "./needs-you"

/** What a screen says about its own head. */
export interface PageHeadOwn {
  /** The mono caps line above, `//` between its parts. */
  overline?: ReactNode
  /** The page's name: its `h1`. */
  title: string
  /** How many, beside the title. */
  count?: number | string
  /** A fact beside the title ("THU 11.12"). */
  meta?: ReactNode
  /** Controls at the end of the head. */
  actions?: ReactNode
  /** The whole head is a button that switches something. */
  switcher?: PageHeadSwitcher
}

/**
 * What the shell says about the head of the place that shows. At compact
 * and medium width the Items head is the switcher (brief X1): the shell
 * gives it the scope and type count as its overline, the view's name, the
 * count in view, the switcher itself and the chip row under it.
 */
export interface PageHeadFromShell {
  overline?: ReactNode
  title?: string
  count?: number | string
  switcher?: PageHeadSwitcher
  below?: ReactNode
  /** Controls the place adds after the screen's own: "Check again" on Items, Codes and Sharing (brief 3.3). */
  actions?: ReactNode
}

const PageHeadContext = createContext<PageHeadFromShell | null>(null)

/** The shell mounts one around the content with the head of the place showing; `null` leaves a screen's head its own. */
export const PageHeadProvider = PageHeadContext.Provider

/**
 * A screen's page head, ready for `AppFramePageHead`: what the screen
 * says, with what the shell knows about the place laid over it (the
 * switcher, its overline, the chip row). `ScreenHead` draws it; a screen
 * that needs more of `AppFramePageHead` reads the values and passes them by
 * name (never as a spread: the design-system ratchet counts one).
 * The title is display/compact on a phone and at medium width, and the
 * display role on a desktop page (Desktop / 06 and 12 draw it 68 px high).
 */
export function usePageHead(own: PageHeadOwn): AppFramePageHeadProps & { title: string } {
  const shell = useContext(PageHeadContext)
  const frame = useAppFrame()
  // On a page (Health, Sharing, Codes), what waits on the person sits under the head, above anything else (brief X3).
  const needsYou = useNeedsYouUnderHead()
  const below =
    needsYou && shell?.below ? (
      <div className="flex flex-col gap-4">
        {shell.below}
        {needsYou}
      </div>
    ) : (needsYou ?? shell?.below)
  return {
    size: pageHeadSize(frame),
    overline: shell?.overline ?? own.overline,
    title: shell?.title ?? own.title,
    count: shell?.count ?? own.count,
    meta: own.meta,
    actions:
      own.actions && shell?.actions ? (
        <>
          {own.actions}
          {shell.actions}
        </>
      ) : (own.actions ?? shell?.actions),
    switcher: shell?.switcher ?? own.switcher,
    below,
    gap: frame.phone || below ? "md" : "sm",
  }
}

/** The page head's size at this width: display on an expanded desktop page, display/compact narrower. */
export function pageHeadSize(frame: { phone: boolean; width: Parameters<typeof isExpandedWidth>[0] }): "lg" | "md" {
  return !frame.phone && isExpandedWidth(frame.width) ? "lg" : "md"
}

/**
 * A screen's head, drawn: `usePageHead` on `AppFramePageHead`. Each prop is
 * passed by name: a spread onto a unicron part is something the design-system
 * ratchet counts.
 */
export function ScreenHead(props: PageHeadOwn) {
  const head = usePageHead(props)
  return <AppFramePageHead size={head.size} overline={head.overline} title={head.title} count={head.count} meta={head.meta} actions={head.actions} switcher={head.switcher} below={head.below} gap={head.gap} />
}
