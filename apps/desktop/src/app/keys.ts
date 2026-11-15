import { useCallback, useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent } from "react"

/*
 * The keys (brief X6, doc 65): the shell's own keys, the table the keys list
 * reads, and the row-menu keys every row with a menu takes. One file, so the
 * list a person reads and the keys that work cannot drift apart.
 */

/** One line of the keys list: the keys, as typed on this computer, and what they do. */
export interface Shortcut {
  id: ShortcutId
  /** One or more ways to press it; the list shows each as a key. */
  keys: string[]
  does: string
  /** A second line, when the key is not working as listed (the system shortcut is off). */
  note?: string
}

export type ShortcutId = "quick-access" | "quick-window" | "fill" | "search" | "settings" | "sidebar" | "keys" | "move" | "row-menu" | "escape"

/** The system-wide shortcut, as the core reports it; `null` where this build has no quick-access window (the web vault). */
export interface QuickWindowKey {
  label: string
  /** Why it does not work now (it is off, or this computer would not give it), in the core's words where it has some. */
  problem?: string
}

/**
 * Every key, in the order the keys list shows them (brief X6). On a Mac the
 * keys are the symbols on its keyboard; elsewhere they are spelled out.
 */
export function shortcuts({ mac, quickWindow }: { mac: boolean; quickWindow: QuickWindowKey | null }): Shortcut[] {
  const mod = (key: string) => (mac ? `⌘${key}` : `Ctrl+${key}`)
  const modShift = (key: string) => (mac ? `⌘⇧${key}` : `Ctrl+Shift+${key}`)
  const out: Shortcut[] = [{ id: "quick-access", keys: [mod("K")], does: "Quick access: items, places and Lock" }]
  if (quickWindow)
    out.push({
      id: "quick-window",
      keys: [quickWindow.label],
      does: "The quick-access window, from any app",
      note: quickWindow.problem,
    })
  out.push(
    { id: "fill", keys: [modShift("L")], does: "Fill in the browser, with the extension" },
    { id: "search", keys: ["/"], does: "Search" },
    { id: "settings", keys: [mod(",")], does: "Settings" },
    { id: "sidebar", keys: [mod("\\")], does: "Collapse or show the sidebar" },
    { id: "keys", keys: ["?"], does: "This list" },
    { id: "move", keys: ["↑ ↓", mac ? "↵" : "Enter"], does: "Move in a list; open the row" },
    { id: "row-menu", keys: mac ? ["⇧F10"] : ["Menu", "Shift+F10"], does: "The row's menu, as a right click opens it" },
    { id: "escape", keys: ["Esc"], does: "Close the top layer. In Search, clear the field, then leave" },
  )
  return out
}

/** Typing in a field: a key there is a letter, not a command. */
export const typing = (el: EventTarget | null) => el instanceof HTMLElement && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))

/** Inside a layer (a dialog, a sheet, a menu): the shell's keys wait until it closes, so they never open a layer under it. */
const inLayer = (el: EventTarget | null) => el instanceof Element && el.closest('[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]') !== null

interface AppKeys {
  search: () => void
  settings: () => void
  keys: () => void
}

/**
 * The shell's keys, from anywhere but a field or an open layer
 * (cross-app decisions §6, brief X6): `/` Search, ⌘, (Ctrl+,) Settings,
 * `?` the keys list. ⌘\ is the sidebar's own (unicron's `AppFrameSidebar`),
 * ⌘K the quick-access palette's.
 */
export function useAppKeys(actions: AppKeys): void {
  const latest = useRef(actions)
  latest.current = actions
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.altKey || inLayer(e.target)) return
      const plain = !e.metaKey && !e.ctrlKey
      if (e.key === "/" && plain && !typing(e.target)) {
        e.preventDefault()
        latest.current.search()
      } else if (e.key === "?" && plain && !typing(e.target)) {
        e.preventDefault()
        latest.current.keys()
      } else if (e.key === "," && (e.metaKey || e.ctrlKey) && !e.shiftKey) {
        e.preventDefault()
        latest.current.settings()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])
}

/** Where a row's menu opens from: the row, and the pointer's spot for a right click. */
export interface RowMenuFrom {
  row: HTMLElement
  /** `null` from a key: the menu opens at the row's overflow button. */
  point: { x: number; y: number } | null
}

/** What a row passes, each by name (a spread onto a unicron part is what the design-system ratchet counts). */
export interface RowMenuKeys {
  onKeyDown: (e: ReactKeyboardEvent<HTMLElement>) => void
  onContextMenu: (e: ReactMouseEvent<HTMLElement>) => void
  /** For `aria-keyshortcuts`: the keys that open the menu. */
  keyshortcuts: string
}

/** A key press and the `contextmenu` event some browsers send after it are one request. */
const SAME_PRESS_MS = 500

/**
 * The row-menu keys (cross-app decisions §10, brief 4.0): the Menu key,
 * Shift+F10 and a right click open the row's menu, the same menu as its
 * overflow button. A lane gives each row with a menu its own:
 *
 *   const menu = useRowMenuKeys(() => setMenuOpen(true))
 *   <ListRow onKeyDown={menu.onKeyDown} onContextMenu={menu.onContextMenu} aria-keyshortcuts={menu.keyshortcuts} … />
 *
 * A key another part already handled (`defaultPrevented`) is left alone.
 */
export function useRowMenuKeys(open: (from: RowMenuFrom) => void): RowMenuKeys {
  const latest = useRef(open)
  latest.current = open
  const keyed = useRef(0)
  const onKeyDown = useCallback((e: ReactKeyboardEvent<HTMLElement>) => {
    if (e.defaultPrevented) return
    const menuKey = e.key === "ContextMenu" && !e.shiftKey
    const shiftF10 = e.key === "F10" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey
    if (!menuKey && !shiftF10) return
    e.preventDefault()
    keyed.current = Date.now()
    latest.current({ row: e.currentTarget, point: null })
  }, [])
  const onContextMenu = useCallback((e: ReactMouseEvent<HTMLElement>) => {
    if (e.defaultPrevented) return
    e.preventDefault()
    // The browser's own event for the key just handled: one menu, not two.
    if (Date.now() - keyed.current < SAME_PRESS_MS) return
    // A keyboard's contextmenu carries no pointer spot.
    const point = e.clientX === 0 && e.clientY === 0 ? null : { x: e.clientX, y: e.clientY }
    latest.current({ row: e.currentTarget, point })
  }, [])
  return { onKeyDown, onContextMenu, keyshortcuts: "ContextMenu Shift+F10" }
}
