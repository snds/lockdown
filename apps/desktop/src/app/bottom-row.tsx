import { useState } from "react"

import { AppFrameBottomRow, DropdownMenu, DropdownMenuContent, DropdownMenuTrigger, FloatButton, TabBar, TabBarItem, useAppFrame } from "@unicron/ui"

import { QuickCreateItems } from "@/features/vault/new-item-menu"
import { countName } from "@/lib/format"

import { SECTIONS_BAR, sectionOf, usePlaces, type Section } from "./places"

interface BottomRowProps {
  /** How many things Health has to fix: its tab's count mark. */
  toFix: number
  /** Opens the list of every kind to make (a sheet on a phone). */
  onCreate: () => void
  /** Makes one of quick create's four kinds at once. */
  onQuickCreate: (template: string) => void
  /** A second tap on the active tab: back to its root, at the top, fetched again. */
  onReselect: (section: Section) => void
}

/**
 * The phone's bottom row (brief 3.3, Mobile / 01, States / 03): Search,
 * the tab bar of Items, Codes and Health, and create. Scrolled, the frame
 * shrinks the bar to the active tab. Search keeps the row with its button
 * current and no tab active. A pushed screen (an item, Sharing, Settings)
 * has no tab bar; found-4 gives an open item its own row.
 */
export function BottomRow({ toFix, onCreate, onQuickCreate, onReselect }: BottomRowProps) {
  const frame = useAppFrame()
  const { nav, go } = usePlaces()
  const [quick, setQuick] = useState(false)
  if (!frame.phone || frame.detailOpen || nav.area === "settings" || nav.area === "sharing") return null
  const searching = nav.area === "search"
  return (
    <AppFrameBottomRow
      variant="tabs"
      start={<FloatButton label="Search" icon="search" size="l" current={searching ? "page" : undefined} onClick={() => go("search")} />}
      tabs={
        <TabBar
          label="Sections"
          value={searching ? undefined : (sectionOf(nav) ?? undefined)}
          onValueChange={(id) => go(SECTIONS_BAR.find((s) => s.id === id)?.place ?? "all")}
          onReselect={(id) => onReselect(id as Section)}
        >
          {SECTIONS_BAR.map((s) => (
            <TabBarItem key={s.id} value={s.id} icon={s.icon} label={s.label} mark={s.id === "health" && toFix > 0 ? { count: toFix, label: countName(toFix, "to fix", "to fix") } : undefined} />
          ))}
        </TabBar>
      }
      end={
        <DropdownMenu
          open={quick}
          // A press makes an item from the full list; only a long press (Shift+Enter, the Menu key, a right click) opens quick create.
          onOpenChange={(next, details) => (next && details.reason === "trigger-press" ? undefined : setQuick(next))}
        >
          <DropdownMenuTrigger
            render={
              <FloatButton
                label={quick ? "Close quick create" : "New item"}
                icon={quick ? "close" : "add"}
                kind="primary"
                size="l"
                onClick={quick ? undefined : onCreate}
                onLongPress={() => setQuick(true)}
                longPressLabel="Quick create"
              />
            }
          />
          {/* On a short screen the button stands in the trailing column, so the menu opens beside it. */}
          <DropdownMenuContent width="auto" side={frame.layout === "rail" ? "left" : "top"} align="end" sideOffset={8} scrim aria-label="Quick create">
            <QuickCreateItems onPick={onQuickCreate} />
          </DropdownMenuContent>
        </DropdownMenu>
      }
    />
  )
}
