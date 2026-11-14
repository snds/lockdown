import { createContext, useContext, useId, useLayoutEffect, useMemo, useState, type ReactNode } from "react"

import { AppFrameBottomRow, useAppFrame } from "@unicron/ui"

import { usePlaces } from "./places"

/*
 * The chrome of a pushed screen on a phone (brief 3.3, contract 5.1b #2,
 * doc 65): the screen says what goes in its top bar and its bottom row, and
 * the shell draws them where the frame puts them. The screen's content and
 * the chrome are in different places in the tree, so a screen hands its
 * buttons up with a hook rather than drawing them itself.
 */

/** What a pushed screen puts in its bottom row. */
export interface DetailBottomRowContent {
  /** Up to two neutral `FloatButton`s at the start. */
  start?: ReactNode
  /** One primary `FloatButton`, or an extended one when the yes shares, grants or approves (cross-app §10). */
  end?: ReactNode
}

type Slot = "bottom" | "topBar"
interface Held {
  bottom: { id: string; content: DetailBottomRowContent } | null
  topBar: { id: string; actions: ReactNode } | null
}

type Register = <S extends Slot>(slot: S, id: string, value: S extends "bottom" ? DetailBottomRowContent : ReactNode) => () => void

// Two contexts: the screens use only the setter, which never changes, so a screen
// handing up its buttons does not draw itself again for it.
const RegisterContext = createContext<Register | null>(null)
const HeldContext = createContext<Held>({ bottom: null, topBar: null })

/** The shell mounts one around the frame. The last screen to hand up a slot holds it until it goes. */
export function PushedChromeProvider({ children }: { children: ReactNode }) {
  const [held, setHeld] = useState<Held>({ bottom: null, topBar: null })
  const register = useMemo<Register>(
    () => (slot, id, value) => {
      setHeld((h) => ({ ...h, [slot]: slot === "bottom" ? { id, content: value } : { id, actions: value } }))
      return () => setHeld((h) => (h[slot]?.id === id ? { ...h, [slot]: null } : h))
    },
    [],
  )
  return (
    <RegisterContext.Provider value={register}>
      <HeldContext.Provider value={held}>{children}</HeldContext.Provider>
    </RegisterContext.Provider>
  )
}

/**
 * The screen is pushed on a phone: an open item or Settings section, or a
 * place that is not a section's root (Settings, Sharing). It has no tab bar;
 * it has a top bar with back, and its own bottom row when it has a primary.
 */
export function usePushed(): boolean {
  const frame = useAppFrame()
  const { nav } = usePlaces()
  return frame.phone && (frame.detailOpen || nav.area === "settings" || nav.area === "sharing")
}

/** Hands a slot up while the screen is there; what changes in it follows, and it goes with the screen. */
function useHandUp<S extends Slot>(slot: S, value: S extends "bottom" ? DetailBottomRowContent : ReactNode, shown: boolean) {
  const register = useContext(RegisterContext)
  const id = useId()
  useLayoutEffect(() => {
    if (!register || !shown) return
    return register(slot, id, value)
  }, [register, slot, id, value, shown])
}

/**
 * A pushed screen's bottom row on a phone (contract 5.1b #2): up to two
 * neutral float buttons at the start and one primary, or extended, at the
 * end, with the status strip above it. It is drawn only on a phone, on a
 * pushed screen, and not while the on-screen keyboard is up: a form keeps
 * its primary in the top bar for that reason (brief X6). On a short screen
 * (a phone on its side) the frame stands it in the trailing column.
 *
 * Returns whether the row is drawn, so the screen can leave out its own
 * buttons for the same actions (at 1440 they stay in the screen).
 *
 *   const inRow = useDetailBottomRow({
 *     start: <FloatButton label="Copy password" icon="copy" size="l" onClick={copy} />,
 *     end: <FloatButton label="Edit" icon="edit" kind="primary" size="l" onClick={edit} />,
 *   })
 *
 * Pass the same elements while nothing about them changes (`useMemo`), so
 * the row is not handed up again on every draw.
 */
export function useDetailBottomRow(content: DetailBottomRowContent): boolean {
  const frame = useAppFrame()
  const pushed = usePushed()
  const shown = pushed && !frame.keyboardOpen && Boolean(content.start || content.end)
  const value = useMemo(() => ({ start: content.start, end: content.end }), [content.start, content.end])
  useHandUp("bottom", value, shown)
  return shown
}

/** A pushed screen's top-bar actions on a phone: up to two neutral float buttons (edit, more). The shell's TopRow draws them. */
export function useDetailTopBarHandUp(actions: ReactNode): boolean {
  const pushed = usePushed()
  const shown = pushed && Boolean(actions)
  useHandUp("topBar", actions, shown)
  return shown
}

/** The top-bar actions a pushed screen handed up, for the shell's TopRow. */
export function useHeldTopBarActions(): ReactNode {
  return useContext(HeldContext).topBar?.actions ?? null
}

/** The shell's bottom row for a pushed screen: what the screen handed up, on unicron's detail row. */
export function PushedBottomRow() {
  const frame = useAppFrame()
  const pushed = usePushed()
  const held = useContext(HeldContext).bottom
  if (!pushed || frame.keyboardOpen || !held) return null
  return <AppFrameBottomRow variant="detail" start={held.content.start} end={held.content.end} />
}
