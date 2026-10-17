import { useMemo, useState } from "react"

import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icon,
  IconButton,
  Input,
  ItemGroup,
  Skeleton,
  ToggleGroup,
  ToggleGroupItem,
} from "@unicron/ui"

import { MasterDetail, PaneHeader } from "@/components/master-detail"
import { isMac } from "@/features/palette/palette-model"
import type { ItemSummary } from "@/core/types"

import { DetailPane } from "./detail-pane"
import { ItemRow } from "./item-row"
import { DEFAULT_GROUP, DEFAULT_SORT, VIEW_LABELS, buildList, kindIcon, type GroupBy, type ListView, type SortKey } from "./list-model"
import { ItemEditor, type EditorTarget } from "./item-editor"
import { NewItemMenu } from "./new-item-menu"
import { useQuickCopy } from "./quick-copy"

interface VaultViewProps {
  view: ListView
  rows: ItemSummary[] | null
  error: string | null
  onChanged: () => void
  onNotice: (text: string) => void
  /** The open item: kept by the app so the quick-access palette can open one. */
  selected: string | null
  onSelect: (id: string | null) => void
  /** Opens the quick-access palette (also ⌘K / Ctrl+K anywhere in the app). */
  onPalette: () => void
}

const EMPTY: Record<ListView, [string, string]> = {
  all: ["Nothing saved yet", "Add one with New item, or import from another manager in Settings."],
  passkeys: ["No passkeys yet", "Passkeys arrive with the sign-in flows; a Login keeps its own."],
  codes: ["No codes here", "Add a code from the Codes screen."],
  "wi-fi": ["No Wi-Fi networks", "Add one with New item → Wi-Fi network."],
  expiring: ["Nothing expiring", "Items with an expiry date within thirty days show here."],
  attention: ["Nothing needs you", "Conflict copies, expiring items and codes that protect this account itself show here."],
  deleted: ["The trash is empty", "Trashed items stay readable here through their grace period."],
}

/**
 * The vault (brief §3): a list on the left, one item on the right. The
 * list is one smart view, searched, sorted and grouped as D93 decided:
 * title A–Z, grouped by kind with counts, with a sort menu (title, recently
 * changed) and a group toggle (by kind, none). A Login keeps its passkeys
 * and codes inside its own row; the method glyphs say which it holds.
 */
export function VaultView({ view, rows, error, onChanged, onNotice, selected, onSelect, onPalette }: VaultViewProps) {
  const [query, setQuery] = useState("")
  const [sort, setSort] = useState<SortKey>(DEFAULT_SORT)
  const [groupBy, setGroupBy] = useState<GroupBy>(DEFAULT_GROUP)
  const setSelected = onSelect
  const [version, setVersion] = useState(0)
  const [editor, setEditor] = useState<EditorTarget | null>(null)
  const quickCopy = useQuickCopy(onNotice)

  const groups = useMemo(() => (rows ? buildList(rows, view, query, sort, groupBy) : []), [rows, view, query, sort, groupBy])
  const inView = useMemo(() => groups.reduce((n, g) => n + g.rows.length, 0), [groups])
  const [emptyTitle, emptyText] = EMPTY[view]

  const header = (
    <PaneHeader title={VIEW_LABELS[view]}>
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button variant="ghost" size="sm" aria-label="Sort and group" />}>
          <Icon name="sort" data-icon="inline-start" />
          Sort
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuLabel>Sort by</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={sort} onValueChange={(v) => setSort(v as SortKey)}>
            <DropdownMenuRadioItem value="title">Title</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="changed">Recently changed</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
          <DropdownMenuSeparator />
          <DropdownMenuLabel>Group</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={groupBy} onValueChange={(v) => setGroupBy(v as GroupBy)}>
            <DropdownMenuRadioItem value="kind">By kind</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="none">No groups</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      <IconButton label="Quick access" icon="bolt" shortcut={isMac() ? "⌘K" : "Ctrl+K"} onClick={onPalette} />
      <NewItemMenu onPick={(kind) => setEditor({ kind, item: null })} />
    </PaneHeader>
  )

  const list = (
    <div className="flex flex-col gap-2 px-2 pb-4">
      <div className="flex items-center gap-2 px-2">
        <Input type="search" placeholder="Search titles, sites, usernames" aria-label="Search" value={query} onChange={(e) => setQuery(e.target.value)} autoFocus />
        <ToggleGroup aria-label="Group rows" value={[groupBy]} onValueChange={(v) => v[0] && setGroupBy(v[0] as GroupBy)} size="sm">
          <ToggleGroupItem value="kind" aria-label="Group by kind">
            <Icon name="view_agenda" />
          </ToggleGroupItem>
          <ToggleGroupItem value="none" aria-label="No groups">
            <Icon name="view_list" />
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      {error && (
        <p role="alert" className="px-2 text-sm text-destructive">
          {error}
        </p>
      )}
      {!rows && !error && (
        <div className="flex flex-col gap-2 px-2" aria-busy="true" aria-label="Loading">
          <Skeleton variant="row" />
          <Skeleton variant="row" />
          <Skeleton variant="row" />
        </div>
      )}
      {rows && inView === 0 && (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Icon name={view === "all" ? "key" : view === "attention" ? "check" : kindIcon(view === "deleted" ? "custom" : view)} />
            </EmptyMedia>
            <EmptyTitle>{query ? "No matches" : emptyTitle}</EmptyTitle>
            <EmptyDescription>{query ? "Try fewer letters." : emptyText}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
      {groups.map((g) => (
        <section key={g.key} aria-label={g.label || VIEW_LABELS[view]}>
          {g.label && (
            <h2 data-group-header="" className="flex items-baseline justify-between px-3 pb-1 text-xs font-medium text-muted-foreground uppercase">
              <span>{g.label}</span>
              <span aria-label={`${g.rows.length} ${g.rows.length === 1 ? "item" : "items"}`}>{g.rows.length}</span>
            </h2>
          )}
          <ItemGroup role="list">
            {g.rows.map((r) => (
              <ItemRow key={r.id} row={r} selected={selected === r.id} onSelect={setSelected} onCopy={quickCopy} />
            ))}
          </ItemGroup>
        </section>
      ))}
    </div>
  )

  const bump = () => {
    setVersion((v) => v + 1)
    onChanged()
  }

  return (
    <>
      <MasterDetail
        header={header}
        list={list}
        detail={
          selected ? (
            <DetailPane
              key={`${selected}:${version}`}
              id={selected}
              rows={rows ?? []}
              onEdit={(item) => setEditor({ kind: item.kind, item })}
              onTrashed={() => {
                setSelected(null)
                bump()
                onNotice("Moved to the trash.")
              }}
              onChanged={(text) => {
                bump()
                onNotice(text)
              }}
              onNotice={onNotice}
            />
          ) : null
        }
        onBack={() => setSelected(null)}
        emptyIcon="key"
        emptyLabel="Pick an item to see its fields. Hidden values stay masked until you reveal them."
      />
      <ItemEditor
        target={editor}
        onClose={() => setEditor(null)}
        onSaved={(id, generated) => {
          setEditor(null)
          setSelected(id)
          bump()
          onNotice(generated ? "Saved. The new password is in the item; copy it from there." : "Saved.")
        }}
      />
    </>
  )
}
