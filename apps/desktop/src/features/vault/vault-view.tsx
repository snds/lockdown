import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuRadioGroup,
  AdaptiveMenuRadioItem,
  AppFrameInset,
  AppFramePageHead,
  Banner,
  Button,
  EmptyState,
  FilterChip,
  FilterChipGroup,
  IconButton,
  SearchField,
  SectionHeader,
  Skeleton,
  useAppFrame,
} from "@unicron/ui"

import { savedWords } from "@/app/notify"
import { usePageHead } from "@/app/page-head"
import { usePlaces } from "@/app/places"
import { useSettings } from "@/app/settings"
import { useTemplates } from "@/app/templates"
import { MasterDetail } from "@/components/master-detail"
import { useClient } from "@/core/client"
import type { Finding, ItemSummary } from "@/core/types"
import { groupByExpiry } from "@/features/organization/expiry"
import { OrganizePrompt } from "@/features/organization/organize-prompt"
import { folderPaths, inScope, rowMenuOffers, scopeTitle, targetOf, type OrganizeTarget, type Scope } from "@/features/organization/org-model"
import { RowMenuItems } from "@/features/organization/row-menu"
import { useRowMenuLayers } from "@/features/organization/row-menu-layers"
import { useVaults } from "@/features/organization/vaults-context"
import { count, countName } from "@/lib/format"

import { kindWord } from "./detail-model"
import { DetailPane } from "./detail-pane"
import { ItemEditor, type EditorTarget } from "./item-editor"
import { ItemRow } from "./item-row"
import { NewItemMenu } from "./new-item-menu"
import {
  DEFAULT_SORT,
  VIEW_LABELS,
  buildList,
  defaultGroup,
  findingWords,
  groupFor,
  isKindView,
  rowWords,
  scopeChips,
  type GroupBy,
  type ListView,
  type RowGroup,
  type SortKey,
} from "./list-model"
import { rememberWhere, searchWith } from "./search-words"

interface VaultViewProps {
  view: ListView
  rows: ItemSummary[] | null
  error: string | null
  /** Reloads the list: after a change, and TRY AGAIN after a failed read. */
  onChanged: () => void
  onNotice: (text: string) => void
  /** The open item: kept by the app so the quick-access palette can open one. */
  selected: string | null
  onSelect: (id: string | null) => void
  /** Kept for callers: Quick access is ⌘K now, with no button on the list (D111, brief X4). */
  onPalette: () => void
  /** A vault, folder or tag the list is narrowed to (the sidebar's Vaults and Tags). */
  scope?: Scope
}

/** A list longer than this draws only the rows in view (brief X6). */
export const WINDOW_FROM = 200

// --- sort and group, kept per device ----------------------------------------------------------

const ORDER_KEY = "lockdown.list-order"

/** The person's sort and group, kept on this device for every list (brief V1); the grouping starts by the device's width. */
function useListOrder(phone: boolean): { sort: SortKey; group: GroupBy; setSort: (s: SortKey) => void; setGroup: (g: GroupBy) => void } {
  const [kept, setKept] = useState<{ sort?: SortKey; group?: GroupBy }>(() => {
    try {
      return JSON.parse(window.localStorage.getItem(ORDER_KEY) ?? "{}") as { sort?: SortKey; group?: GroupBy }
    } catch {
      return {}
    }
  })
  const keep = useCallback((next: { sort?: SortKey; group?: GroupBy }) => {
    setKept((was) => {
      const now = { ...was, ...next }
      try {
        window.localStorage.setItem(ORDER_KEY, JSON.stringify(now))
      } catch {
        // Kept for this session only.
      }
      return now
    })
  }, [])
  return {
    sort: kept.sort ?? DEFAULT_SORT,
    group: kept.group ?? defaultGroup(phone),
    setSort: (sort) => keep({ sort }),
    setGroup: (group) => keep({ group }),
  }
}

/** The "Sort and group" menu (brief V1): a menu on a desktop, a sheet on a phone. By letter only with A to Z. */
function SortMenu({ trigger, sort, group, letters, onSort, onGroup }: { trigger: React.ReactElement<Record<string, unknown>>; sort: SortKey; group: GroupBy; letters: boolean; onSort: (s: SortKey) => void; onGroup: (g: GroupBy) => void }) {
  return (
    <AdaptiveMenu title="Sort and group" overline="Items" align="end" trigger={trigger}>
      <AdaptiveMenuRadioGroup label="Sort" value={sort} onValueChange={(v) => onSort(v as SortKey)}>
        <AdaptiveMenuRadioItem value="title">A to Z</AdaptiveMenuRadioItem>
        <AdaptiveMenuRadioItem value="changed">Recently changed</AdaptiveMenuRadioItem>
        <AdaptiveMenuRadioItem value="used">Recently used</AdaptiveMenuRadioItem>
      </AdaptiveMenuRadioGroup>
      <AdaptiveMenuRadioGroup label="Group" value={groupFor(sort, group)} onValueChange={(v) => onGroup(v as GroupBy)}>
        {letters && sort === "title" && <AdaptiveMenuRadioItem value="letter">By letter</AdaptiveMenuRadioItem>}
        <AdaptiveMenuRadioItem value="kind">By type</AdaptiveMenuRadioItem>
        <AdaptiveMenuRadioItem value="none">None</AdaptiveMenuRadioItem>
      </AdaptiveMenuRadioGroup>
    </AdaptiveMenu>
  )
}

// --- the words of each empty list (brief 5.2) ---------------------------------------------------

/** "Cards" → "cards", "SSH keys" stays: a plural in a sentence. */
const inSentence = (words: string) => (/^[A-Z][a-z]/.test(words) ? words.charAt(0).toLowerCase() + words.slice(1) : words)

interface EmptyWords {
  overline: string
  title: string
  body?: string
  /** The template a NEW button starts, or `menu` for the whole New item menu. */
  create?: { label: string; template: string | "menu" }
}

function emptyWords(view: ListView, scopeName: string | null, trashDays: number | null): EmptyWords {
  const name = VIEW_LABELS[view]
  const kind = isKindView(view) ? view : view === "wi-fi" ? "wi-fi" : view === "contacts" ? "identity/contact" : null
  const single = kind === "identity/contact" ? "contact" : kind ? inSentence(kindWord(kind)) : ""
  if (view === "deleted")
    return {
      overline: `${scopeName ? `${scopeName} // ` : ""}Recently deleted // ${count(0)}`,
      title: "Nothing here.",
      body: trashDays === null ? "Each vault keeps deleted items for its own time." : `Deleted items stay here for ${trashDays} days, then go for good.`,
    }
  if (scopeName && view === "all") return { overline: `${scopeName} // ${count(0)}`, title: "Nothing here.", body: "Nothing is filed here yet.", create: { label: "New item", template: "menu" } }
  if (scopeName && kind) return { overline: `${scopeName} // ${name} // ${count(0)}`, title: `No ${inSentence(name)} in ${scopeName}.`, create: { label: `New ${single}`, template: kind } }
  if (scopeName) return { overline: `${scopeName} // ${name} // ${count(0)}`, title: "Nothing here." }
  switch (view) {
    case "all":
      return { overline: `Items // ${count(0)}`, title: "Nothing saved yet.", body: "Save a login from your browser, or add one here.", create: { label: "New item", template: "menu" } }
    case "favourites":
      return { overline: `Favorites // ${count(0)}`, title: "No favorites yet.", body: "Star an item to keep it here." }
    case "passkeys":
      return { overline: `Passkeys // ${count(0)}`, title: "No passkeys yet.", body: "Save one with Lockdown when a site offers it in your browser." }
    case "codes":
      return { overline: `Codes // ${count(0)}`, title: "No codes yet.", body: "Add one from the set-up link a site gives you." }
    case "contacts":
      return { overline: `Contacts // ${count(0)}`, title: "No contacts yet.", body: "Contacts are never used to fill a form.", create: { label: "New contact", template: "identity/contact" } }
    case "archived":
      return { overline: `Archived // ${count(0)}`, title: "Nothing archived.", body: "Archive an item to keep it without it getting in your way." }
    case "expiring":
      return { overline: `Expiring // ${count(0)}`, title: "Nothing expiring.", body: "Items with an expiry date within 30 days show here." }
    case "attention":
      return { overline: `Needs attention // ${count(0)}`, title: "Nothing needs you.", body: "Conflict copies, expiring items and codes that protect this account itself show here." }
    default:
      return { overline: `${name} // ${count(0)}`, title: `No ${inSentence(name)} yet.`, body: "Add one and it shows here.", create: kind ? { label: `New ${single}`, template: kind } : undefined }
  }
}

// --- a long list draws only the rows in view (brief X6) -----------------------------------------

/** The nearest ancestor that scrolls: the frame's list column. */
function scrollerOf(el: HTMLElement | null): HTMLElement | null {
  for (let p = el?.parentElement ?? null; p; p = p.parentElement) {
    const y = getComputedStyle(p).overflowY
    if (y === "auto" || y === "scroll") return p
  }
  return null
}

/** Without a layout (a test, a first paint), how many rows a windowed group draws. */
const FIRST_PAINT = 24
const OVERSCAN = 8

/**
 * Which of a group's rows to draw: all of them when the list is short; in
 * a long one, those in the scroller's view and a few either side. Rows
 * outside are stood in for by space of their height, so the scroll bar and
 * the sticky letter headers stay true.
 */
function useWindow(enabled: boolean, rowCount: number, rowHeight: number): { ref: React.RefObject<HTMLUListElement | null>; start: number; end: number } {
  const ref = useRef<HTMLUListElement | null>(null)
  const [range, setRange] = useState<[number, number]>(() => [0, Math.min(rowCount, FIRST_PAINT)])
  useLayoutEffect(() => {
    if (!enabled) return
    const list = ref.current
    const scroller = scrollerOf(list)
    if (!list || !scroller) return
    let frame = 0
    const measure = () => {
      frame = 0
      const view = scroller.getBoundingClientRect()
      const top = list.getBoundingClientRect().top
      if (view.height === 0) {
        setRange([0, Math.min(rowCount, FIRST_PAINT)])
        return
      }
      const first = Math.max(0, Math.floor((view.top - top) / rowHeight) - OVERSCAN)
      const last = Math.min(rowCount, Math.ceil((view.bottom - top) / rowHeight) + OVERSCAN)
      setRange((was) => (was[0] === first && was[1] === Math.max(first, last) ? was : [first, Math.max(first, last)]))
    }
    const later = () => {
      if (!frame) frame = requestAnimationFrame(measure)
    }
    measure()
    scroller.addEventListener("scroll", later, { passive: true })
    window.addEventListener("resize", later)
    return () => {
      scroller.removeEventListener("scroll", later)
      window.removeEventListener("resize", later)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [enabled, rowCount, rowHeight])
  return enabled ? { ref, start: range[0], end: Math.min(range[1], rowCount) } : { ref, start: 0, end: rowCount }
}

interface GroupProps {
  group: RowGroup
  /** The kind of header: letters stick while their rows scroll (SectionHeader `index`). */
  header: "letter" | "section" | "none"
  label: string
  windowed: boolean
  rowHeight: number
  /** Where the group's first row sits in the whole list, and the list's length. */
  offset: number
  total: number
  inset: boolean
  row: (r: ItemSummary, position: { index: number; of: number } | undefined) => ReactNode
}

/** One group of the list: a named region ("S, 4 items") with its header and its rows. */
function RowsGroup({ group, header, label, windowed, rowHeight, offset, total, inset, row }: GroupProps) {
  const { ref, start, end } = useWindow(windowed, group.rows.length, rowHeight)
  const shown = group.rows.slice(start, end)
  return (
    <section aria-label={label}>
      {header !== "none" && <SectionHeader title={group.label} count={group.rows.length} variant={header === "letter" ? "index" : "default"} inset={inset} />}
      <ul ref={ref} role="list" className="flex flex-col">
        {windowed && start > 0 && <li aria-hidden="true" style={{ height: start * rowHeight }} />}
        {shown.map((r, i) => row(r, windowed ? { index: offset + start + i, of: total } : undefined))}
        {windowed && end < group.rows.length && <li aria-hidden="true" style={{ height: (group.rows.length - end) * rowHeight }} />}
      </ul>
    </section>
  )
}

/** Each item's worst finding from the cached report (desktop only; brief V1). Never the network checks. */
function useFindings(on: boolean, rows: ItemSummary[] | null): Map<string, string> {
  const client = useClient()
  const [findings, setFindings] = useState<Finding[] | null>(null)
  useEffect(() => {
    if (!on || !rows) return
    let live = true
    client
      .securityReport()
      .then((r) => live && setFindings(r.findings))
      .catch(() => live && setFindings(null))
    return () => {
      live = false
    }
  }, [client, on, rows])
  return useMemo(() => (on ? findingWords(findings) : new Map<string, string>()), [on, findings])
}

/** A row's height in the list's window: unicron's ListRow with plain meta, 92, or 56 compact. */
const ROW_HEIGHT = { comfortable: 92, compact: 56 } as const

/**
 * The vault's lists (brief V1): All items, every view, kind and scope, as
 * one calm list beside the item it opens. On a desktop: All items has the
 * search field (typing opens Search) and letter headers; a view or a kind
 * has a Section Header with its count and SORT; a vault, folder or tag has
 * its header and a chip row of the kinds it holds. On a phone and at medium
 * width the shell's page head is the switcher, with its chips; the list adds
 * its sort button. Rows are unicron's ListRow (`ItemRow`) with a ⋯ menu;
 * on a phone they swipe (`SwipeRow`), every swipe action also in that menu.
 * Loading draws skeleton rows; a failed read keeps what loaded with a
 * critical Banner and TRY AGAIN, or says so in a critical Empty State.
 */
export function VaultView({ view, rows, error, onChanged, onNotice, selected, onSelect, scope }: VaultViewProps) {
  const { vaults } = useVaults()
  const { navigate, nav, go } = usePlaces()
  const { settings } = useSettings()
  const { templates } = useTemplates()
  const frame = useAppFrame()
  const [organizing, setOrganizing] = useState<OrganizeTarget | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const paths = useMemo(() => folderPaths(vaults), [vaults])
  const scoped = useMemo(() => (rows && scope ? inScope(rows, scope, vaults) : rows), [rows, scope, vaults])
  const scopeName = scope ? scopeTitle(scope, vaults) : null
  const scopeVault = scope?.vault ? vaults.find((v) => v.id === scope.vault) : scope?.folder ? vaults.find((v) => v.folders.some((f) => f.id === scope.folder)) : undefined
  const unverified = scope?.vault ? vaults.find((v) => v.id === scope.vault)?.unverified : null
  const paneTitle = scopeName ?? VIEW_LABELS[view]
  const order = useListOrder(frame.phone)
  const setSelected = onSelect
  const [version, setVersion] = useState(0)
  const [editor, setEditor] = useState<EditorTarget | null>(null)
  const findings = useFindings(!frame.phone, rows)
  // Search opened from here offers this list as its second chip, and Escape comes back (brief X4).
  const whereKey = JSON.stringify(nav)
  useEffect(() => {
    rememberWhere(JSON.parse(whereKey) as typeof nav)
    return () => rememberWhere(null)
  }, [whereKey])

  // Letter headers belong to All items, as Desktop / 01 draws it; a view or a scope is one list (Desktop / 03) unless grouped by type.
  const letters = view === "all" && !scope
  const group: GroupBy = order.group === "letter" && !letters ? "none" : order.group
  const groups = useMemo(() => {
    if (!scoped) return []
    // The Expiring view reads soonest first, in sections by how soon (expired, a week, a month, later).
    if (view === "expiring" && order.sort === DEFAULT_SORT) return groupByExpiry(buildList(scoped, view, "", order.sort, "none", paths).flatMap((g) => g.rows))
    return buildList(scoped, view, "", order.sort, group, paths)
  }, [scoped, view, order.sort, group, paths])
  const inView = useMemo(() => groups.reduce((n, g) => n + g.rows.length, 0), [groups])
  const grouping = view === "expiring" && order.sort === DEFAULT_SORT ? "kind" : groupFor(order.sort, group)

  const bump = () => {
    setVersion((v) => v + 1)
    onChanged()
  }
  const changed = (text: string, moved?: string, id?: string) => {
    setActionError(null)
    if (moved && selected === id) setSelected(moved)
    bump()
    if (text) onNotice(text)
  }
  const menuLayers = useRowMenuLayers({ onChanged: (text) => changed(text), onError: setActionError })
  const create = (template: string) => setEditor({ kind: template, item: null, vaultId: scopeVault?.id, folder: scope?.folder, tags: scope?.tag ? [scope.tag] : undefined })
  const creatable = (template: string) => templates?.kinds.some((t) => (t.id ?? t.kind) === template && t.creatable) ?? false

  // --- the head ------------------------------------------------------------------------------
  const sortTrigger = <IconButton label="Sort and group" icon="settings--adjust" variant="ghost" />
  const sortMenu = (trigger: React.ReactElement<Record<string, unknown>>) => <SortMenu trigger={trigger} sort={order.sort} group={group} letters={letters} onSort={order.setSort} onGroup={order.setGroup} />
  const head = usePageHead({
    title: paneTitle,
    count: rows ? inView : undefined,
    actions: (
      <>
        {sortMenu(sortTrigger)}
      </>
    ),
  })
  // What the shell adds to every head of Items ("Check again"), for the desktop's own head.
  const shellActions = usePageHead({ title: paneTitle }).actions
  // The shell lays its switcher over the head where the sidebar's groups don't show (a phone, medium width).
  const shellHead = head.switcher !== undefined
  const trashCaption =
    view === "deleted" && rows ? (scopeVault ? `Kept for ${scopeVault.trash_days} days, then deleted for good.` : vaults.length > 1 ? "Each vault keeps deleted items for its own time." : vaults[0] ? `Kept for ${vaults[0].trash_days} days, then deleted for good.` : null) : null
  const chips = scope && !shellHead && scoped ? scopeChips(scoped) : []

  let header: ReactNode
  if (shellHead) {
    header = (
      <>
        <AppFramePageHead size={head.size} overline={head.overline} title={head.title} count={head.count} meta={head.meta} actions={head.actions} switcher={head.switcher} below={head.below} gap={head.gap} />
      </>
    )
  } else if (view === "all" && !scope) {
    // All items, as Desktop / 01 draws it: no visible head, the search field first. Typing opens Search.
    header = (
      <AppFrameInset>
        <h1 className="sr-only">{paneTitle}</h1>
        <div className="flex flex-col gap-4 pb-6">
          <SearchField
            aria-label="Search"
            placeholder={rows ? `Search ${countName(inView, "item")}` : "Search"}
            value=""
            onValueChange={(words) => {
              if (!words) return
              searchWith(words)
              go("search")
            }}
            trailing={
              <>
                {sortMenu(sortTrigger)}
                {shellActions}
              </>
            }
          />
          {head.below}
        </div>
      </AppFrameInset>
    )
  } else {
    header = (
      <AppFrameInset>
        <h1 className="sr-only">{paneTitle}</h1>
        <div className="flex flex-col gap-4 pb-2">
          {head.below}
          <SectionHeader
            title={paneTitle}
            count={rows ? inView : undefined}
            action={
              <>
                {sortMenu(<Button variant="ghost" size="sm">Sort</Button>)}
                {shellActions}
              </>
            }
          />
          {chips.length > 0 && (
            <FilterChipGroup type="single" scroll label="Show" value={view} onValueChange={(v) => navigate({ ...nav, area: "vault", view: v as ListView })}>
              {["all" as ListView, ...chips].map((v) => (
                <FilterChip key={v} value={v} count={view === v && scoped ? count(inView) : undefined}>
                  {v === "all" ? "All" : VIEW_LABELS[v]}
                </FilterChip>
              ))}
            </FilterChipGroup>
          )}
        </div>
      </AppFrameInset>
    )
  }

  // --- the list ------------------------------------------------------------------------------
  const empty = emptyWords(view, scopeName, view === "deleted" ? (scopeVault?.trash_days ?? (vaults.length === 1 ? (vaults[0]?.trash_days ?? null) : null)) : null)
  const emptyAction = empty.create ? (
    empty.create.template === "menu" ? (
      // Beside the sidebar its NEW ITEM is the one way in; a second button of the same name would only repeat it.
      !shellHead ? undefined : <NewFromMenu label={empty.create.label} onPick={create} onNotice={onNotice} />
    ) : creatable(empty.create.template) ? (
      <Button variant="secondary" onClick={() => create(empty.create!.template)}>
        {empty.create.label}
      </Button>
    ) : undefined
  ) : undefined
  const windowed = inView > WINDOW_FROM
  const rowHeight = ROW_HEIGHT[settings.density === "compact" ? "compact" : "comfortable"]
  const oneVault = Boolean(scopeVault) || vaults.length < 2
  // A phone's rows swipe (brief V1, States / 01): the same calls and toasts as the row's menu.
  const swipe = frame.phone ? { onChanged: (text: string) => changed(text), onError: setActionError, onNotice } : null
  const drawRow = (r: ItemSummary, position: { index: number; of: number } | undefined) => (
    <ItemRow
      key={r.id}
      row={r}
      selected={selected === r.id}
      onSelect={setSelected}
      position={position}
      words={rowWords(r, { view, vaults, oneVault, findings })}
      swipe={swipe}
      actions={
        rowMenuOffers(r) ? (
          <RowMenuItems
            row={r}
            onOrganize={(row) => setOrganizing(targetOf(row))}
            onHistory={menuLayers.onHistory}
            onPurge={menuLayers.onPurge}
            onChanged={(text, moved) => changed(text, moved, r.id)}
            onError={setActionError}
            swiped={Boolean(swipe)}
          />
        ) : undefined
      }
    />
  )
  // Where each group's first row sits in the whole list, for a long list's positions.
  const offsets = groups.reduce<number[]>((at, _g, i) => [...at, i === 0 ? 0 : at[i - 1]! + groups[i - 1]!.rows.length], [])
  const list = (
    <div className="flex flex-col gap-4 pb-6">
      {shellHead && frame.layout === "rail" && (
        // A phone on its side draws the page head collapsed, without its actions (a Unicron gap): the sort and the
        // shell's Check again stay reachable here (cross-app §8: every pull to refresh has a plain Check again).
        <AppFrameInset>
          <div className="flex justify-end gap-2">
            {sortMenu(sortTrigger)}
            {shellActions}
          </div>
        </AppFrameInset>
      )}
      {(unverified || (rows && error) || actionError || trashCaption) && (
        <AppFrameInset>
          <div className="flex flex-col gap-4">
            {rows && error && (
              // A reload that failed over rows that loaded before: the rows stay (cross-app decisions §9).
              <Banner
                tone="critical"
                title="Couldn't load your items."
                body={error}
                action={
                  <Button variant="secondary" size="sm" onClick={onChanged}>
                    Try again
                  </Button>
                }
              />
            )}
            {actionError && <Banner tone="critical" size="compact" title={actionError} onDismiss={() => setActionError(null)} />}
            {unverified && (
              // A shared vault some of whose recent changes didn't verify: nothing to press (D573, docs/plan/49-shared-vault-quiet.md).
              <div role="note">
                <Banner tone="info" size="compact" body={unverified} />
              </div>
            )}
            {trashCaption && inView > 0 && <p className="type-caption text-muted-foreground">{trashCaption}</p>}
          </div>
        </AppFrameInset>
      )}
      {!rows && error && (
        <AppFrameInset>
          <EmptyState tone="critical" overline="Items // Not loaded" title="Couldn't load your items." body={error} actionLabel="Try again" onAction={onChanged} />
        </AppFrameInset>
      )}
      {!rows && !error && (
        // A status with its words in it (review R18): the skeleton rows of States / 04, and no count.
        <div role="status" className="flex flex-col px-4">
          <span className="sr-only">Loading items</span>
          {[0, 1, 2, 3, 4].map((i) => (
            <Skeleton key={i} variant="row" />
          ))}
        </div>
      )}
      {rows && inView === 0 && (
        <AppFrameInset>
          <EmptyState overline={empty.overline} title={empty.title} body={empty.body} action={emptyAction} />
        </AppFrameInset>
      )}
      <div className="flex flex-col">
      {groups.map((g, i) => {
        const at = offsets[i] ?? 0
        return (
          <RowsGroup
            key={g.key}
            group={g}
            header={!g.label ? "none" : grouping === "letter" ? "letter" : "section"}
            label={g.label ? `${g.label}, ${countName(g.rows.length, "item")}` : paneTitle}
            windowed={windowed}
            rowHeight={rowHeight}
            offset={at}
            total={inView}
            inset={frame.phone}
            row={drawRow}
          />
        )
      })}
      </div>
    </div>
  )

  return (
    <>
      <MasterDetail
        header={header}
        title={paneTitle}
        list={list}
        detail={
          selected ? (
            <DetailPane
              key={`${selected}:${version}`}
              id={selected}
              rows={rows ?? []}
              onEdit={(item) => setEditor({ kind: item.template ?? item.kind, item })}
              onTrashed={() => {
                setSelected(null)
                // The pane says it itself, with UNDO (Q14).
                bump()
              }}
              onChanged={(text) => {
                bump()
                onNotice(text)
              }}
              onReplaced={(id, text) => {
                setSelected(id)
                bump()
                onNotice(text)
              }}
              onNotice={onNotice}
            />
          ) : null
        }
        onBack={() => setSelected(null)}
        emptyLabel="Pick an item to see it here."
      />
      <ItemEditor
        target={editor}
        onClose={() => setEditor(null)}
        onSaved={(id, generated) => {
          const title = editor?.item ? (editor.item.fields.find((f) => f.field_id === "title")?.value ?? null) : null
          setEditor(null)
          setSelected(id)
          bump()
          onNotice(savedWords(title, generated))
        }}
      />
      <OrganizePrompt
        target={organizing}
        rows={rows ?? []}
        onClose={() => setOrganizing(null)}
        onSaved={() => {
          const title = organizing?.title
          setOrganizing(null)
          bump()
          onNotice(title ? `Saved ${title}` : "Saved")
        }}
      />
      {menuLayers.layers}
    </>
  )
}

/** NEW ITEM in an empty list: the shell's New item menu (every template), behind a secondary button. */
function NewFromMenu({ label, onPick, onNotice }: { label: string; onPick: (template: string) => void; onNotice: (text: string) => void }) {
  return <NewItemMenu onPick={onPick} onNotice={onNotice} trigger={<Button variant="secondary">{label}</Button>} />
}
