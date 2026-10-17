import { useMemo, useState } from "react"

import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
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
  Input,
  ItemGroup,
  Skeleton,
  ToggleGroup,
  ToggleGroupItem,
} from "@unicron/ui"

import { MasterDetail, PaneHeader } from "@/components/master-detail"
import type { ItemDetail, ItemSummary } from "@/core/types"

import { DetailPane } from "./detail-pane"
import { ItemRow } from "./item-row"
import { DEFAULT_GROUP, DEFAULT_SORT, VIEW_LABELS, buildList, kindIcon, type GroupBy, type ListView, type SortKey } from "./list-model"
import { LoginEditor } from "./login-editor"

interface VaultViewProps {
  view: ListView
  rows: ItemSummary[] | null
  error: string | null
  onChanged: () => void
  onAddCode: () => void
  onNotice: (text: string) => void
}

const EMPTY: Record<ListView, [string, string]> = {
  all: ["Nothing saved yet", "Add a login, or import from another manager in Settings."],
  passkeys: ["No passkeys yet", "Passkeys arrive with the sign-in flows; a Login keeps its own."],
  codes: ["No codes here", "Add a code from the Codes screen."],
  "wi-fi": ["No Wi-Fi networks", "Wi-Fi items come with the next item kinds."],
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
export function VaultView({ view, rows, error, onChanged, onAddCode, onNotice }: VaultViewProps) {
  const [query, setQuery] = useState("")
  const [sort, setSort] = useState<SortKey>(DEFAULT_SORT)
  const [groupBy, setGroupBy] = useState<GroupBy>(DEFAULT_GROUP)
  const [selected, setSelected] = useState<string | null>(null)
  const [version, setVersion] = useState(0)
  const [editor, setEditor] = useState<{ open: boolean; item: ItemDetail | null }>({ open: false, item: null })

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
      <DropdownMenu>
        <DropdownMenuTrigger render={<Button size="sm" />}>
          <Icon name="add" data-icon="inline-start" />
          New
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={() => setEditor({ open: true, item: null })}>
            <Icon name="key" data-icon="inline-start" />
            Login
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onAddCode}>
            <Icon name="timer" data-icon="inline-start" />
            One-time code
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
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
          <Skeleton className="h-(--c-row-height)" />
          <Skeleton className="h-(--c-row-height)" />
          <Skeleton className="h-(--c-row-height)" />
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
              <ItemRow key={r.id} row={r} selected={selected === r.id} onSelect={setSelected} />
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
              onEdit={(item) => setEditor({ open: true, item })}
              onTrashed={() => {
                setSelected(null)
                bump()
                onNotice("Moved to the trash.")
              }}
              onNotice={onNotice}
            />
          ) : null
        }
        onBack={() => setSelected(null)}
        emptyIcon="key"
        emptyLabel="Pick an item to see its fields. Hidden values stay masked until you reveal them."
      />
      <LoginEditor
        open={editor.open}
        item={editor.item}
        onClose={() => setEditor({ open: false, item: null })}
        onSaved={(id) => {
          setEditor({ open: false, item: null })
          setSelected(id)
          bump()
        }}
      />
    </>
  )
}
