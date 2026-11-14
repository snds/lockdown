import { createContext, useContext, type ReactNode } from "react"

import { AppFramePageHead, useAppFrame, type AppFramePageHeadProps, type PageHeadSwitcher } from "@unicron/ui"

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
}

const PageHeadContext = createContext<PageHeadFromShell | null>(null)

/** The shell mounts one around the content with the head of the place showing; `null` leaves a screen's head its own. */
export const PageHeadProvider = PageHeadContext.Provider

/**
 * A screen's page head, ready for `AppFramePageHead`: what the screen
 * says, with what the shell knows about the place laid over it (the
 * switcher, its overline, the chip row). Spread the result:
 * `<AppFramePageHead {...usePageHead({ title: "Codes", count })} />`.
 * The title is display/compact on a phone and the title role in a pane.
 */
export function usePageHead(own: PageHeadOwn): AppFramePageHeadProps {
  const shell = useContext(PageHeadContext)
  const frame = useAppFrame()
  return {
    size: frame.phone ? "md" : "sm",
    overline: shell?.overline ?? own.overline,
    title: shell?.title ?? own.title,
    count: shell?.count ?? own.count,
    meta: own.meta,
    actions: own.actions,
    switcher: shell?.switcher ?? own.switcher,
    below: shell?.below,
    gap: frame.phone || shell?.below ? "md" : "sm",
  }
}

/** A screen's head, drawn: `usePageHead` on `AppFramePageHead`. */
export function ScreenHead(props: PageHeadOwn) {
  return <AppFramePageHead {...usePageHead(props)} />
}
