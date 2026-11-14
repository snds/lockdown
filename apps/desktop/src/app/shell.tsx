import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuGroup,
  AdaptiveMenuItem,
  AdaptivePopover,
  AppFrame,
  AppFrameMain,
  AppFrameSidebar,
  AppFrameStatusLine,
  AppLockup,
  FilterChip,
  FilterChipGroup,
  IconButton,
  NavItem,
  NavSection,
  useAppFrame,
  type AppFrameLayout,
  type NavMarkProp,
} from "@unicron/ui"

import type { ItemSummary } from "@/core/types"
import { OrganizationNav } from "@/features/organization/organization-nav"
import { inScope } from "@/features/organization/org-model"
import { useVaults } from "@/features/organization/vaults-context"
import { isMac } from "@/features/palette/palette-model"
import { ItemEditor, type EditorTarget } from "@/features/vault/item-editor"
import { VIEW_LABELS, countTypes, countViews, type ListView } from "@/features/vault/list-model"
import { NewItemMenu } from "@/features/vault/new-item-menu"
import { count as countWords, countName } from "@/lib/format"

import { BottomRow } from "./bottom-row"
import { CrumbProvider, useCrumbExtra } from "./crumbs"
import { PageHeadProvider, type PageHeadFromShell } from "./page-head"
import {
  ASIDE_VIEWS,
  MAIN_VIEWS,
  MORE_VIEWS,
  PlacesProvider,
  SECTIONS_BAR,
  crumbsOf,
  currentRow,
  navFor,
  sectionOf,
  usePlaces,
  vaultPlace,
  viewIcon,
  type Nav,
  type PlacesValue,
  type Section,
  type Surface,
} from "./places"
import { TopRow } from "./top-row"

export type { Area, Nav, SettingsSection, SharingTab, Surface } from "./places"

interface AppShellProps {
  /** Which build this is: the desktop app or the web vault. It picks the places. */
  surface?: Surface
  nav: Nav
  onNavigate: (nav: Nav) => void
  /** The item rows, for the counts; `null` until they load (a count is never `00` for "not known yet"). */
  rows: ItemSummary[] | null
  onLock: () => void
  /** How many things Health has to fix (the cached report and the Needs attention list). */
  toFix?: number
  /** What waits on the person in Sharing. */
  sharingMark?: number
  /** The update mark on the way into Settings (cross-app decisions §3); found-5 fills it. */
  settingsMark?: NavMarkProp
  /** An item was made or changed in the shell's editor: its id, and whether the core generated a value to copy. */
  onSaved: (id: string, generated: boolean) => void
  onNotice?: (text: string) => void
  /** A second tap on the active tab, after the shell went to its root and scrolled to the top: fetch again. */
  onRefresh?: () => void
  /** The end of the status line: up to two `StatusItem`s. found-5 fills it. */
  status?: ReactNode
  /** Pins the shell instead of taking the one the width picks: a test, a preview. */
  layout?: AppFrameLayout
  children: ReactNode
}

/** What the shell does for any screen. */
interface ShellValue {
  /** Opens the editor: a new item from a template, or an item to change. */
  edit: (target: EditorTarget) => void
}

const ShellContext = createContext<ShellValue>({ edit: () => {} })

/** The shell's editor, from any screen: `useShell().edit({ kind: "login", item: null, website })`. */
export function useShell(): ShellValue {
  return useContext(ShellContext)
}

/** A choice kept on this device, outside the vault (the sidebar's shape is not a secret and not a setting of the account). */
function useDeviceFlag(key: string, initial: boolean): [boolean, (next: boolean) => void] {
  const [value, setValue] = useState(() => {
    try {
      const stored = window.localStorage.getItem(key)
      return stored === null ? initial : stored === "1"
    } catch {
      return initial
    }
  })
  const set = useCallback(
    (next: boolean) => {
      setValue(next)
      try {
        window.localStorage.setItem(key, next ? "1" : "0")
      } catch {
        // Kept for this session only.
      }
    },
    [key],
  )
  return [value, set]
}

const typing = (el: EventTarget | null) => el instanceof HTMLElement && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))

/**
 * Lockdown's shell, as one tree of unicron's frame parts (redesign brief §3
 * and X1, doc 64). The frame picks the shell from the window's width, so
 * nothing here is a media query:
 *
 *   compact  (under 600)   the phone's floating rows: the lockup and the
 *                          profile button on top; Search, the tab bar of
 *                          Items, Codes and Health, and create at the foot
 *   medium   (600 to 1023) the sidebar as its rail, one content column, a
 *                          pushed detail with its way back in the status line
 *   expanded (1024 up)     the sidebar, the status line, list beside detail
 *
 * Where the sidebar's groups don't show (the phone, the rail) the page
 * head is the switcher and the chip row picks the view. Every place is in
 * `places.ts`; everything here reads that table.
 */
export function AppShell({ surface = "desktop", nav, onNavigate, rows, onLock, toFix = 0, sharingMark = 0, settingsMark, onSaved, onNotice, onRefresh, status, layout, children }: AppShellProps) {
  const [editor, setEditor] = useState<EditorTarget | null>(null)
  const [creating, setCreating] = useState(false)
  // The sidebar as its rail by choice (⌘\), kept on this device.
  const [collapsed, setCollapsed] = useDeviceFlag(SIDEBAR_KEY, false)
  const { vaults } = useVaults()

  const go = useCallback(
    (id: string) => {
      const to = navFor(id)
      if (to) onNavigate(to)
    },
    [onNavigate],
  )
  const places = useMemo<PlacesValue>(() => ({ nav, surface, go, navigate: onNavigate }), [nav, surface, go, onNavigate])
  const shell = useMemo<ShellValue>(() => ({ edit: setEditor }), [])

  // The section the person was last in: where Back goes from Search, Sharing and Settings on a phone.
  const section = sectionOf(nav)
  const lastSection = useRef<Nav>({ area: "vault", view: "all" })
  useEffect(() => {
    if (section) lastSection.current = nav
  }, [section, nav])
  const leave = useCallback(() => onNavigate(lastSection.current), [onNavigate])

  // `/` opens Search and ⌘, (Ctrl+,) opens Settings, from anywhere but a field (cross-app decisions §6).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.altKey) return
      if (e.key === "/" && !e.metaKey && !e.ctrlKey && !typing(e.target)) {
        e.preventDefault()
        go("search")
      } else if (e.key === "," && (e.metaKey || e.ctrlKey) && !e.shiftKey) {
        e.preventDefault()
        go("settings")
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [go])

  // A new item is filed where the list is looking (the scope when New item was pressed).
  const create = (kind: string) =>
    setEditor({
      kind,
      item: null,
      vaultId: nav.scope?.vault ?? vaults.find((v) => v.folders.some((f) => f.id === nav.scope?.folder))?.id,
      folder: nav.scope?.folder,
      tags: nav.scope?.tag ? [nav.scope.tag] : undefined,
    })

  const reselect = (id: Section) => {
    go(SECTIONS_BAR.find((s) => s.id === id)?.place ?? "all")
    const column = document.querySelector("[data-column][data-shown]")
    if (column) column.scrollTop = 0
    onRefresh?.()
  }

  return (
    <PlacesProvider value={places}>
      <ShellContext.Provider value={shell}>
        <CrumbProvider>
          <AppFrame layout={layout}>
            <Sidebar collapsed={collapsed} onCollapsedChange={setCollapsed} rows={rows} toFix={toFix} sharingMark={sharingMark} settingsMark={settingsMark} onLock={onLock} onCreate={() => setCreating(true)} />
            <StatusLine surface={surface}>{status}</StatusLine>
            <TopRow onLeave={leave} onLock={onLock} settingsMark={settingsMark} />
            <AppFrameMain>
              <ItemsHead collapsed={collapsed} rows={rows} sharingMark={sharingMark}>
                {children}
              </ItemsHead>
            </AppFrameMain>
            <BottomRow toFix={toFix} onCreate={() => setCreating(true)} onQuickCreate={create} onReselect={reselect} />
            <CreateMenu open={creating} onOpenChange={setCreating} onPick={create} onNotice={onNotice} />
          </AppFrame>
          <ItemEditor
            target={editor}
            onClose={() => setEditor(null)}
            onSaved={(id, generated) => {
              setEditor(null)
              onSaved(id, generated)
            }}
          />
        </CrumbProvider>
      </ShellContext.Provider>
    </PlacesProvider>
  )
}

/** The sidebar's groups show: the expanded width with the sidebar not collapsed. Otherwise the page head switches and the chips pick the view. */
function useSidebarGroups(collapsed: boolean): boolean {
  const frame = useAppFrame()
  return frame.layout === "expanded" && !collapsed && !frame.sidebarRail
}

const SIDEBAR_KEY = "lockdown.sidebar.collapsed"
const MORE_KEY = "lockdown.sidebar.more"

interface SidebarProps {
  collapsed: boolean
  onCollapsedChange: (collapsed: boolean) => void
  rows: ItemSummary[] | null
  toFix: number
  sharingMark: number
  settingsMark?: NavMarkProp
  onLock: () => void
  onCreate: () => void
}

/**
 * The desktop's navigation column (brief 3.2), 204 wide: the lockup, NEW
 * ITEM, Search, ITEMS, VAULTS, TOOLS, and Settings pinned at the foot
 * beside Lock. Exactly one row is current: the deepest real place. At
 * medium width, or collapsed by choice (⌘\), the frame draws it as its rail
 * of Search, Items, Codes and Health; the rest is behind the page head.
 */
function Sidebar({ collapsed, onCollapsedChange, rows, toFix, sharingMark, settingsMark, onLock, onCreate }: SidebarProps) {
  const { nav, go, navigate } = usePlaces()
  const frame = useAppFrame()
  const [drawer, setDrawer] = useState(false)
  const [moreOpen, setMoreOpen] = useDeviceFlag(MORE_KEY, false)
  // The whole sidebar shows: at the expanded width, and in the drawer the rail opens at medium width.
  const groups = useSidebarGroups(collapsed) || (drawer && !frame.phone)
  const current = currentRow(nav)
  const counts = useMemo(() => (rows ? countViews(rows) : null), [rows])
  const types = useMemo(() => (rows ? countTypes(rows) : null), [rows])
  const mark = (n: number | undefined, one = "item", many?: string): NavMarkProp | undefined => (n ? { count: n, label: countName(n, one, many) } : undefined)
  const row = (v: ListView, level = 0) => (
    <NavItem key={v} label={VIEW_LABELS[v]} icon={viewIcon(v)} level={level} mark={mark(counts?.[v])} current={current === v} onClick={() => go(v)} />
  )
  const more = MORE_VIEWS.filter((v) => (counts?.[v] ?? 0) > 0)
  // More is open when the person left it open, and always while the place showing is inside it.
  const inMore = [...MORE_VIEWS, ...ASIDE_VIEWS].includes(current as ListView)
  const health = mark(toFix, "to fix", "to fix")
  const section = sectionOf(nav)

  return (
    <AppFrameSidebar
      label="Sidebar"
      lockup={<AppLockup name="Lockdown" />}
      action={{ label: "New item", icon: "add", onClick: onCreate }}
      footer={<NavItem label="Settings" icon="settings" current={current === "settings"} mark={settingsMark} onClick={() => go("settings")} />}
      footerControl={<IconButton label="Lock" icon="locked" variant="ghost" onClick={onLock} />}
      collapsed={collapsed}
      onCollapsedChange={onCollapsedChange}
      drawerOpen={drawer}
      onDrawerOpenChange={setDrawer}
      // No `leadingInset`: the desktop window keeps the system's title bar (src-tauri sets no overlay), so the
      // macOS window buttons are above the lockup, not over it. It is this one prop when the title bar becomes the app's.
    >
      <NavItem label="Search" icon="search" current={current === "search"} mark={{ keys: "/" }} onClick={() => go("search")} />
      {groups ? (
        <>
          <NavSection title={types === null ? "Items" : `Items // ${countWords(types)} ${types === 1 ? "type" : "types"}`} tree>
            {row("all")}
            {MAIN_VIEWS.map((v) => row(v))}
            {rows && rows.length > 0 && <NavItem label="More" icon="overflow-menu--horizontal" expanded={moreOpen || inMore} onExpandedChange={setMoreOpen} />}
            {rows && rows.length > 0 && (moreOpen || inMore) && [...more, ...ASIDE_VIEWS].map((v) => row(v, 1))}
          </NavSection>
          <OrganizationNav rows={rows ?? []} nav={nav} onNavigate={navigate} sharingMark={sharingMark} />
          <NavSection title="Tools">
            <NavItem label="Health" icon="checkmark--outline" current={current === "health"} mark={health} onClick={() => go("health")} />
          </NavSection>
        </>
      ) : (
        // The rail: the three sections. Views, vaults and Sharing are behind the page head there.
        SECTIONS_BAR.map((s) => (
          <NavItem key={s.id} label={s.label} icon={s.icon} current={section === s.id && current !== "search"} mark={s.id === "health" ? health : undefined} onClick={() => go(s.place)} />
        ))
      )}
    </AppFrameSidebar>
  )
}

/**
 * The desktop's top line: the breadcrumb as links (brief 3.2), with what
 * the open screen added through `useCrumb`. At medium width a pushed
 * detail's back button takes its leading slot; the frame does that.
 */
function StatusLine({ surface, children }: { surface: Surface; children?: ReactNode }) {
  const { nav, go } = usePlaces()
  const { vaults } = useVaults()
  const extra = useCrumbExtra()
  // Written in capitals here: a crumb that is a button does not take the line's capitals (a unicron gap, doc 64 §7).
  const crumbs = crumbsOf(nav, vaults, surface, isMac(), extra).map((c) => ({ label: c.label.toUpperCase(), onClick: c.place ? () => go(c.place as string) : undefined }))
  return <AppFrameStatusLine crumbs={crumbs}>{children}</AppFrameStatusLine>
}

/** The control the frame drew that a layer stands by: the sidebar's primary (full width, or the rail's icon), found, not held. */
const sidebarCreate = () => [...document.querySelectorAll<HTMLElement>('[data-slot="app-frame-sidebar-head"] button')].findLast((el) => el.offsetParent !== null) ?? null
const pageHead = () => [...document.querySelectorAll<HTMLElement>('[data-slot="app-frame-page-head"] [data-slot="page-head"]')].find((el) => el.offsetParent !== null) ?? null

/**
 * Where a control the frame drew is, while a layer that belongs to it is
 * open. A menu or a popover needs a trigger to stand by, and the frame
 * draws these controls itself; so an unseen trigger is laid over the
 * control while the layer is open.
 */
function useBoxOf(find: () => HTMLElement | null, open: boolean): CSSProperties {
  const [box, setBox] = useState<CSSProperties>({})
  useLayoutEffect(() => {
    if (!open) return
    const r = find()?.getBoundingClientRect()
    setBox(r ? { left: r.left, top: r.top, width: r.width, height: r.height } : {})
    // `find` reads the document; it is the same question every time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  return { position: "fixed", pointerEvents: "none", opacity: 0, ...box }
}

/**
 * The list of every kind to make, opened by the sidebar's NEW ITEM, the
 * rail's create button and the phone's: a menu under the button on a
 * desktop, a sheet on a phone.
 */
function CreateMenu({ open, onOpenChange, onPick, onNotice }: { open: boolean; onOpenChange: (open: boolean) => void; onPick: (template: string) => void; onNotice?: (text: string) => void }) {
  const box = useBoxOf(sidebarCreate, open)
  return <NewItemMenu open={open} onOpenChange={onOpenChange} onPick={onPick} onNotice={onNotice} trigger={<button type="button" aria-hidden="true" tabIndex={-1} style={box} />} />
}

/**
 * The Items head where the sidebar's groups don't show (brief X1): the
 * shell lays the scope and type count over the screen's own head as its
 * overline, makes the whole head the switcher, and puts the chip row under
 * it. At the expanded width with the sidebar open, a screen's head is its own.
 */
function ItemsHead({ collapsed, rows, sharingMark, children }: { collapsed: boolean; rows: ItemSummary[] | null; sharingMark: number; children: ReactNode }) {
  const { nav, go, navigate } = usePlaces()
  const frame = useAppFrame()
  const { vaults } = useVaults()
  const groups = useSidebarGroups(collapsed)
  const [switching, setSwitching] = useState(false)
  const items = nav.area === "vault" && sectionOf(nav) === "items"
  const box = useBoxOf(pageHead, switching)
  const scoped = useMemo(() => (rows && nav.scope ? inScope(rows, nav.scope, vaults) : rows), [rows, nav.scope, vaults])
  const counts = useMemo(() => (scoped ? countViews(scoped) : null), [scoped])
  const types = useMemo(() => (scoped ? countTypes(scoped) : null), [scoped])

  if (groups || !items) return <PageHeadProvider value={null}>{children}</PageHeadProvider>

  // The scope as the breadcrumb names it, without its group: `FAMILY // FINANCE`, `#WORK`.
  const scopeWords = nav.scope
    ? crumbsOf({ area: "vault", view: "all", scope: nav.scope }, vaults, "desktop", false)
        .slice(1)
        .map((c) => c.label)
        .join(" // ")
    : ""
  const scopeName = scopeWords || "All vaults"
  // A sideways swipe (or an arrow key on the head) steps the scope: All vaults, then each vault.
  const stops = ["all", ...vaults.map((v) => vaultPlace(v.id))]
  const at = Math.max(0, stops.indexOf(nav.scope?.vault ? vaultPlace(nav.scope.vault) : "all"))
  const step = (by: number) => {
    const next = stops[(at + by + stops.length) % stops.length]
    if (next) go(next)
  }
  const pick = (view: ListView) => navigate({ ...nav, area: "vault", view })
  // A chip whose view is empty here is left out of the row (brief X1) and listed under MORE instead, so no
  // view the sidebar offers is out of reach where the sidebar's groups don't show. Codes is its own tab.
  const main = MAIN_VIEWS.filter((v) => v !== "codes")
  const chips: ListView[] = ["all", ...main.filter((v) => (counts?.[v] ?? 0) > 0)]
  const more = [...main.filter((v) => !chips.includes(v)), ...MORE_VIEWS.filter((v) => (counts?.[v] ?? 0) > 0)]
  // A view from More joins the row while it is the one showing.
  if (!chips.includes(nav.view)) chips.push(nav.view)

  const head: PageHeadFromShell = {
    overline: types === null ? scopeName : `${scopeName} // ${countWords(types)} ${types === 1 ? "type" : "types"}`,
    title: nav.view === "all" ? "Items" : VIEW_LABELS[nav.view],
    count: counts ? counts[nav.view] : undefined,
    switcher: { label: "Switch vault", "aria-haspopup": "dialog", onClick: () => setSwitching(true), onSwipe: (direction) => step(direction === "next" ? 1 : -1) },
    below: (
      <FilterChipGroup
        type="single"
        scroll
        label="Show"
        value={nav.view}
        onValueChange={(v) => pick(v as ListView)}
        action={
          <AdaptiveMenu
            title="Show"
            overline={`Items // ${scopeName}`}
            align="end"
            trigger={
              <FilterChip kind="trigger" aria-label="More kinds and lists">
                More
              </FilterChip>
            }
          >
            {more.length > 0 && (
              <AdaptiveMenuGroup label={`Types ${countWords(more.length)}`}>
                {more.map((v) => (
                  <AdaptiveMenuItem key={v} icon={viewIcon(v)} count={counts?.[v]} current={nav.view === v} onSelect={() => pick(v)}>
                    {VIEW_LABELS[v]}
                  </AdaptiveMenuItem>
                ))}
              </AdaptiveMenuGroup>
            )}
            <AdaptiveMenuGroup label="Kept aside">
              {ASIDE_VIEWS.map((v) => (
                <AdaptiveMenuItem key={v} icon={viewIcon(v)} count={counts?.[v]} current={nav.view === v} onSelect={() => pick(v)}>
                  {VIEW_LABELS[v]}
                </AdaptiveMenuItem>
              ))}
            </AdaptiveMenuGroup>
          </AdaptiveMenu>
        }
      >
        {chips.map((v) => (
          // Only the active chip shows its count, as drawn.
          <FilterChip key={v} value={v} count={nav.view === v && counts ? countWords(counts[v]) : undefined}>
            {v === "all" ? "All" : VIEW_LABELS[v]}
          </FilterChip>
        ))}
      </FilterChipGroup>
    ),
  }

  return (
    <PageHeadProvider value={head}>
      {children}
      <AdaptivePopover
        open={switching}
        onOpenChange={setSwitching}
        overline={`Vaults // ${countWords(vaults.length)}`}
        title="Switch vault"
        align="start"
        height={frame.phone ? "content" : undefined}
        trigger={<button type="button" aria-hidden="true" tabIndex={-1} style={box} />}
      >
        <div className="flex flex-col gap-6">
          <OrganizationNav variant="switcher" rows={rows ?? []} nav={nav} onNavigate={navigate} sharingMark={sharingMark} onPicked={() => setSwitching(false)} />
        </div>
      </AdaptivePopover>
    </PageHeadProvider>
  )
}

