import { useLayoutEffect, useRef, type ReactNode } from "react"

import { EmptyState, useAppFrame } from "@unicron/ui"

/**
 * One Sharing tab's list and detail inside the page's one column (brief
 * S1): side by side at the expanded width, the list a column of the
 * frame's list width and the detail after a rule; narrower, the list is
 * the tab and a chosen row pushes its detail over the whole page. A pushed
 * detail tells the frame so, the way unicron's detail column does
 * (`setDetailOpen`, `setBack`): the phone's top bar and the status line's
 * leading slot at medium width then carry the way back to the list.
 *
 * App.tsx mounts Sharing in one `PageColumn`, so the frame's own list and
 * detail columns are not available here (doc 90 §5, gap 1).
 */
export function SharingPanes({ name, list, detail, onBack, backLabel, emptyBody }: {
  /** The tab's name: the list's region name and the overline of "Nothing open." */
  name: string
  list: ReactNode
  /** What is open, or null. */
  detail: ReactNode | null
  onBack: () => void
  /** Where Back goes, in words ("People"). */
  backLabel: string
  /** "Pick a person to see them here." */
  emptyBody: string
}) {
  const frame = useAppFrame()
  const open = detail !== null
  usePushedDetail(!frame.split && open, onBack, backLabel)
  if (!frame.split)
    return open ? (
      <section aria-label={`${name}: open`} className="flex min-w-0 flex-col gap-6">
        {detail}
      </section>
    ) : (
      <section aria-label={name} className="flex min-w-0 flex-col gap-6">
        {list}
      </section>
    )
  return (
    <div className="grid grid-cols-[var(--c-app-frame-list)_minmax(0,1fr)] items-start gap-x-(--c-app-frame-split-gap)">
      <section aria-label={name} className="flex min-w-0 flex-col gap-6">
        {list}
      </section>
      <section aria-label={`${name}: open`} className="rule-l flex min-w-0 flex-col gap-6 pl-6">
        {detail ?? <EmptyState overline={name} title="Nothing open." body={emptyBody} />}
      </section>
    </div>
  )
}

/** Whether a detail is pushed over the page now (a phone, or medium width): the page head steps aside. */
export function useSharingPushed(open: boolean): boolean {
  const frame = useAppFrame()
  return !frame.split && open
}

/** Says to the frame that a detail is pushed, and how to go back, for as long as it is. */
function usePushedDetail(pushed: boolean, onBack: () => void, label: string) {
  const frame = useAppFrame()
  const { setDetailOpen, setBack } = frame
  const latest = useRef(onBack)
  useLayoutEffect(() => {
    latest.current = onBack
  })
  useLayoutEffect(() => {
    if (!pushed) return
    setDetailOpen(true)
    setBack({ label, onBack: () => latest.current() })
    return () => {
      setDetailOpen(false)
      setBack(null)
    }
  }, [pushed, label, setDetailOpen, setBack])
}

/** The head of an open detail: its overline and its title, a level under the page's own. */
export function paneHeading(overline: ReactNode, title: ReactNode, caption?: ReactNode) {
  return (
    <header className="flex flex-col gap-1">
      <p className="type-overline text-text-label">{overline}</p>
      <h2 className="type-title text-text-body break-words">{title}</h2>
      {caption && <p className="type-body text-text-secondary">{caption}</p>}
    </header>
  )
}
